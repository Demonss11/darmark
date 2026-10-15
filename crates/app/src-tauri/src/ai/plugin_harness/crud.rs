use std::path::{Path, PathBuf};

use serde_json::Value;

use super::paths::{is_reparse_point, ENTRY_FILE, MANIFEST_FILE};
use super::validate::{check_plugin_limits, validate_manifest_id, validate_plugin_id};
use crate::ai::types::AiValidationResult;
use crate::error::{CommandError, ErrorCode};

/// Записывает `plugin.json` + `main.lua` в каталог (создавая его).
pub fn write_plugin_files(dir: &Path, manifest: &Value, source: &str) -> Result<(), CommandError> {
    std::fs::create_dir_all(dir).map_err(io_error)?;
    let manifest_bytes = serde_json::to_vec_pretty(manifest).map_err(|e| {
        CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("манифест не сериализуется: {e}"),
        )
    })?;
    std::fs::write(dir.join(MANIFEST_FILE), manifest_bytes).map_err(io_error)?;
    std::fs::write(dir.join(ENTRY_FILE), source.as_bytes()).map_err(io_error)?;
    Ok(())
}

/// Уникальный временный каталог для валидации (не трогает реальный профиль).
pub(super) fn temp_validate_dir(id: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "darmark-ai-validate-{}-{id}-{sequence}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn io_error(e: std::io::Error) -> CommandError {
    CommandError::new(ErrorCode::AiIo, e.to_string())
}

/// Сводка ошибок валидации для кода `ai_plugin_invalid`.
fn errors_summary(errors: &[String]) -> String {
    if errors.is_empty() {
        "плагин не прошёл валидацию".to_string()
    } else {
        errors.join("; ")
    }
}

/// Создание: безопасный id/манифест/лимиты → каталог не существует → запись → валидация →
/// регистрация.
///
/// Невалидный плагин откатывается (каталог удаляется), чтобы не блокировать повторную попытку.
/// Ошибка регистрации (в т.ч. `no_host`) тоже откатывает каталог — «сиротских» каталогов нет.
pub fn create_plugin_at(
    host: &crate::plugins::host::PluginHost,
    root: &Path,
    id: &str,
    manifest: &Value,
    source: &str,
    validate: impl Fn(&Path) -> AiValidationResult,
) -> Result<crate::PluginInfo, CommandError> {
    validate_plugin_id(id)?;
    validate_manifest_id(manifest, id)?;
    check_plugin_limits(manifest, source)?;
    let dir = root.join(id);
    if dir.exists() {
        return Err(CommandError::new(
            ErrorCode::AiPluginExists,
            format!("плагин уже существует: {id}"),
        ));
    }
    write_plugin_files(&dir, manifest, source)?;
    let result = validate(&dir);
    if !result.ok {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            errors_summary(&result.errors),
        ));
    }
    match host.register_plugin_dir(&dir) {
        Ok(info) => Ok(info),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&dir);
            Err(map_register_error(error))
        }
    }
}

/// Обновление: безопасный id/манифест/лимиты → каталог существует (и не reparse point) →
/// валидация предложенного контента во временном каталоге → перезапись реальных файлов →
/// `reload_plugin`.
///
/// **Рабочий плагин не затирается:** при `!ok` реальные файлы не трогаются (ADR-0034/ADR-0026).
pub fn update_plugin_at(
    host: &crate::plugins::host::PluginHost,
    root: &Path,
    id: &str,
    manifest: &Value,
    source: &str,
    validate: impl Fn(&Path) -> AiValidationResult,
) -> Result<crate::PluginInfo, CommandError> {
    validate_plugin_id(id)?;
    validate_manifest_id(manifest, id)?;
    check_plugin_limits(manifest, source)?;
    let dir = root.join(id);
    if !dir.exists() {
        return Err(CommandError::new(
            ErrorCode::AiPluginNotFound,
            format!("плагин не найден: {id}"),
        ));
    }
    if is_reparse_point(&dir) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("каталог плагина — symlink/junction, запись запрещена: {id}"),
        ));
    }
    let temp = temp_validate_dir(id);
    let result = match write_plugin_files(&temp, manifest, source) {
        Ok(()) => validate(&temp),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(e);
        }
    };
    let _ = std::fs::remove_dir_all(&temp);
    if !result.ok {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            errors_summary(&result.errors),
        ));
    }
    write_plugin_files(&dir, manifest, source)?;
    host.reload(id)
        .map_err(|error| CommandError::new(ErrorCode::AiPluginNotFound, error.message))?;
    host.list_plugins()
        .into_iter()
        .find(|info| info.id == id)
        .ok_or_else(|| {
            CommandError::new(
                ErrorCode::AiPluginNotFound,
                format!("плагин не найден: {id}"),
            )
        })
}

/// Удаление через существующий `PluginHost::remove`.
pub fn delete_plugin_at(
    host: &crate::plugins::host::PluginHost,
    id: &str,
) -> Result<(), CommandError> {
    validate_plugin_id(id)?;
    host.remove(id)
        .map_err(|error| CommandError::new(ErrorCode::AiPluginNotFound, error.message))
}

/// Маппинг ошибки регистрации на коды IPC: «уже существует» → `ai_plugin_exists`,
/// «нет сайдкара-хоста» → `plugin` (проблема окружения, не контента), «не появился в
/// реестре» → `ai_plugin_register_failed`, иначе → `ai_plugin_invalid`.
pub(super) fn map_register_error(error: plugin_core::envelope::PluginError) -> CommandError {
    let code = match error.code.as_str() {
        "plugin_exists" => ErrorCode::AiPluginExists,
        "no_host" => ErrorCode::Plugin,
        "register_failed" => ErrorCode::AiPluginRegisterFailed,
        _ => ErrorCode::AiPluginInvalid,
    };
    CommandError::new(code, error.message)
}
