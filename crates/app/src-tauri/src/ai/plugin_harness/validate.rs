use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::paths::{ENTRY_FILE, MANIFEST_FILE};
use super::proc::{
    resolve_sidecar, run_with_timeout, tail, CLI_ENV, CLI_EXE, HOST_ENV, HOST_EXE,
    VALIDATION_TIMEOUT,
};
use crate::ai::types::AiValidationResult;
use crate::error::{CommandError, ErrorCode};

/// Версия схемы JSON-отчёта CLI задаётся в `plugin-core` (ADR-0034 §2.4): писатель
/// (`plugin-cli`) и читатель (harness) обязаны свериться с одним значением.
use plugin_core::REPORT_SCHEMA_VERSION;

/// Максимальная длина id плагина (защита от гигантских имён каталогов).
pub(super) const MAX_PLUGIN_ID_LEN: usize = 64;

/// Безопасный id плагина: непустой, ≤ [`MAX_PLUGIN_ID_LEN`], только `[a-z0-9-_]`.
///
/// Отклоняет traversal (`..`), разделители пути и спецсимволы — до любой записи на диск.
pub fn validate_plugin_id(id: &str) -> Result<(), CommandError> {
    let safe = !id.is_empty()
        && id.len() <= MAX_PLUGIN_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if safe {
        Ok(())
    } else {
        Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!(
                "недопустимый id плагина: {id:?} (ожидается [a-z0-9-_], ≤ {MAX_PLUGIN_ID_LEN})"
            ),
        ))
    }
}

/// `manifest.id` обязан совпадать с параметром `id` (create/update): иначе плагин записался
/// бы под чужим/неожиданным id. Ошибка → `ai_plugin_invalid` **до** любой записи на диск.
pub fn validate_manifest_id(manifest: &Value, id: &str) -> Result<(), CommandError> {
    match manifest.get("id").and_then(Value::as_str) {
        Some(manifest_id) if manifest_id == id => Ok(()),
        Some(manifest_id) => Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("id манифеста {manifest_id:?} не совпадает с id {id:?}"),
        )),
        None => Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "в манифесте нет строкового поля id",
        )),
    }
}

/// Лимиты размера **до** записи на диск (ADR-0034 §2.4/ADR-0032): исходник `main.lua` и
/// сериализованный манифест. Нарушение → `ai_plugin_invalid` без записи файлов.
pub fn check_plugin_limits(manifest: &Value, source: &str) -> Result<(), CommandError> {
    if let Err(e) = plugin_core::manifest::validate_source_len(source.len() as u64) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("entry: {e}"),
        ));
    }
    let manifest_len = serde_json::to_vec(manifest)
        .map_err(|e| {
            CommandError::new(
                ErrorCode::AiPluginInvalid,
                format!("манифест не сериализуется: {e}"),
            )
        })?
        .len() as u64;
    let max = plugin_core::MAX_PLUGIN_SOURCE_BYTES;
    if manifest_len > max {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("манифест больше {max} байт"),
        ));
    }
    Ok(())
}

/// Разбирает JSON-отчёт `darmark-plugin test --json`.
///
/// `None` — отчёту нельзя доверять: нет `schema_version` или неизвестный мажор → harness
/// уходит в fallback (ADR-0034 §2.4). Иначе — `ok`/`errors` (проваленные `checks[]`) и сырой отчёт.
pub fn parse_cli_report(stdout: &str) -> Option<AiValidationResult> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    let schema = value.get("schema_version")?.as_u64()?;
    if schema != u64::from(REPORT_SCHEMA_VERSION) {
        return None;
    }
    let ok = value.get("ok").and_then(Value::as_bool)?;
    let errors = value
        .get("checks")
        .and_then(Value::as_array)
        .map(|checks| {
            checks
                .iter()
                .filter(|check| check.get("status").and_then(Value::as_str) == Some("fail"))
                .map(|check| {
                    let name = check.get("name").and_then(Value::as_str).unwrap_or("check");
                    let message = check.get("message").and_then(Value::as_str).unwrap_or("");
                    format!("{name}: {message}")
                })
                .collect()
        })
        .unwrap_or_default();
    Some(AiValidationResult {
        ok,
        errors,
        report: Some(value),
    })
}

/// Валидация манифеста и исходника **без** CLI (fallback, ADR-0034/ADR-0026): те же проверки через
/// `plugin_core::manifest`. Возвращает понятные ошибки (пусто — валидно).
///
/// Продовая точка статической проверки «манифест/entry» (TZ-MCP R4/AC-MCP-6): переиспользуется
/// [`validate_plugin_static`] и как диагностический fallback, когда отчёт CLI недоступен.
pub fn manifest_errors(manifest: &Value, source: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let manifest_check = serde_json::to_vec(manifest)
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            plugin_core::manifest::Manifest::from_json(&bytes)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
    if let Err(message) = manifest_check {
        errors.push(format!("manifest: {message}"));
    }
    if let Err(e) = plugin_core::manifest::validate_source_len(source.len() as u64) {
        errors.push(format!("entry: {e}"));
    }
    errors
}

/// Полная валидация каталога плагина: CLI `test --json`, при недоступности/недоверии —
/// fallback (`manifest` + `plugin-host --self-test`). Без паник: всё кодируется в результат.
pub fn validate_plugin(dir: &Path) -> AiValidationResult {
    if let Some(cli) = resolve_sidecar(CLI_ENV, CLI_EXE) {
        let mut command = Command::new(cli);
        command.arg("test").arg(dir).arg("--json");
        if let Ok(output) = run_with_timeout(command, VALIDATION_TIMEOUT) {
            if let Some(result) = parse_cli_report(&output.stdout) {
                return result;
            }
        }
    }
    fallback_validate(dir)
}

/// Статическая валидация каталога плагина: манифест + entry **без** песочницы.
///
/// Единая точка проверки «манифест/entry» (TZ-MCP R4/AC-MCP-6): переиспользуется
/// fallback-валидацией и MCP-адаптером `plugin_validate`. Файлы читаются независимо
/// (ошибка одного не скрывает проверку другого), а содержимое проверяет [`manifest_errors`],
/// поэтому логика валидации не дублируется. Без паник: отсутствие/невалидность файлов
/// кодируются в `errors`.
pub fn validate_plugin_static(dir: &Path) -> AiValidationResult {
    let mut errors = Vec::new();

    let manifest = match std::fs::read(dir.join(MANIFEST_FILE)) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => Some(value),
            Err(e) => {
                errors.push(format!("manifest: {e}"));
                None
            }
        },
        Err(e) => {
            errors.push(format!("manifest: {e}"));
            None
        }
    };
    let source = match std::fs::read_to_string(dir.join(ENTRY_FILE)) {
        Ok(source) => Some(source),
        Err(e) => {
            errors.push(format!("entry: {e}"));
            None
        }
    };

    match (manifest, source) {
        (Some(manifest), Some(source)) => errors.extend(manifest_errors(&manifest, &source)),
        (Some(manifest), None) => errors.extend(manifest_errors(&manifest, "")),
        (None, Some(source)) => {
            if let Err(e) = plugin_core::manifest::validate_source_len(source.len() as u64) {
                errors.push(format!("entry: {e}"));
            }
        }
        (None, None) => {}
    }

    AiValidationResult {
        ok: errors.is_empty(),
        errors,
        report: None,
    }
}

/// Fallback-валидация (ADR-0034 §2.4): манифест/entry через `plugin_core` + песочница child-хостом.
fn fallback_validate(dir: &Path) -> AiValidationResult {
    let mut result = validate_plugin_static(dir);
    if !result.ok {
        return result;
    }
    let source_path = dir.join(ENTRY_FILE);
    match resolve_sidecar(HOST_ENV, HOST_EXE) {
        Some(host) => {
            let mut command = Command::new(host);
            command.arg("--self-test").arg(&source_path);
            match run_with_timeout(command, VALIDATION_TIMEOUT) {
                Ok(output) if output.success => {}
                Ok(output) => result
                    .errors
                    .push(format!("sandbox: {}", tail(&output.stderr))),
                Err(e) => result.errors.push(format!("sandbox: {}", e.message)),
            }
        }
        None => result
            .errors
            .push("sandbox: darmark-plugin-host не найден рядом с darmark.exe".to_string()),
    }
    result.ok = result.errors.is_empty();
    result
}
