//! Инструмент `plugin_read`: исходник **установленного** плагина по `id` (TASK-003 §2).
//!
//! Отдельным модулем, а не соседом в [`super::tools`]: контракт крупнее прочих адаптеров —
//! два файла с потолками чтения, `entry` из манифеста и подгонка ответа под лимит контекста
//! модели, — а `tools.rs` с этими правками перешагнул бы порог 400 строк (ADR-0039 §2.1).

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcp_core::handlers::{ToolError, ToolHandler};
use serde_json::{json, Value};

use super::journal::{log_mcp_call, mcp_log_message};
use super::privacy::sanitize_paths;
use super::{id_schema, LogSink};
use crate::ai::tool_loop::{truncate_chars, MAX_TOOL_RESULT_CHARS};
use crate::plugins::scan::{read_capped, read_source, MAX_MANIFEST_BYTES};

/// Имя манифеста в каталоге установленного плагина (контракт `plugin_read`).
const READ_MANIFEST_FILE: &str = "plugin.json";

/// Имя entry-файла по умолчанию: берётся, когда манифест не разобран или невалиден.
const READ_ENTRY_FILE: &str = "main.lua";

/// Запас бюджета ответа: служебные поля payload и разметка R7, которую транспорт добавляет
/// вокруг текста **после** его обрезки (`tool_message_content`).
const READ_PAYLOAD_RESERVE_CHARS: usize = 512;

/// Бюджет ответа в символах: транспорт отдаёт модели `content[0].text` = `payload.to_string()`
/// и режет его до [`MAX_TOOL_RESULT_CHARS`], поэтому payload обязан уложиться сам.
const READ_PAYLOAD_BUDGET: usize = MAX_TOOL_RESULT_CHARS - READ_PAYLOAD_RESERVE_CHARS;

/// Инструмент чтения исходника **установленного** плагина по `id`.
///
/// Принимает только `id`: путь собирается здесь из доверенного
/// [`plugins_root`](crate::ai::plugin_harness::plugins_root) и проходит тот же белый список
/// [`ensure_plugin_path_allowed`](crate::ai::plugin_harness::ensure_plugin_path_allowed), что
/// `plugin_validate`/`plugin_test`, — произвольный путь от модели не принимается ни прямо, ни
/// внутри `id`. Отдаёт **ровно** `plugin.json` и объявленный в манифесте `entry` (fallback —
/// `main.lua`): ни настроек, ни документа (TASK-003 §6.3). Не верификатор: вердикта не несёт,
/// поэтому payload без `isError` (ADR-0041 §2.2 п.15).
///
/// Отказ на `id` или на пути — до входа в инструмент (класс C, ADR-0041 §1.1): записи журнала
/// нет. Любая ошибка после разрешения каталога (чтение, потолок, `entry`, лимит модели) несёт
/// запись `ok = false`, поэтому карточка вызова и журнал не могут разойтись.
///
/// Поле `source` может прийти урезанным под лимит модели — тогда payload объявляет это полем
/// `truncated: true`: модель не должна судить о невидимой части кода.
pub(super) struct PluginReadTool {
    pub(super) sink: LogSink,
}

impl ToolHandler for PluginReadTool {
    fn name(&self) -> &str {
        "plugin_read"
    }

    fn description(&self) -> &str {
        "Исходник установленного плагина по id: plugin.json и entry из манифеста"
    }

    fn input_schema(&self) -> Value {
        id_schema()
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(id) = params.get("id").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется id"));
        };
        crate::ai::plugin_harness::validate_plugin_id(id)
            .map_err(|e| ToolError::invalid_params(e.message))?;
        let root = crate::ai::plugin_harness::plugins_root()
            .map_err(|e| ToolError::internal(e.message))?;
        let dir = crate::ai::plugin_harness::ensure_plugin_path_allowed(&root.join(id))
            .map_err(|e| ToolError::invalid_params(e.message))?;

        let started = Instant::now();
        match read_payload(&dir) {
            Ok(payload) => {
                log_mcp_call(
                    &self.sink,
                    &mcp_log_message("plugin_read", id, started.elapsed(), true, &[]),
                );
                Ok(payload)
            }
            Err(message) => {
                log_mcp_call(
                    &self.sink,
                    &mcp_log_message(
                        "plugin_read",
                        id,
                        started.elapsed(),
                        false,
                        std::slice::from_ref(&message),
                    ),
                );
                Err(ToolError::internal(message))
            }
        }
    }
}

/// Читает манифест и объявленный в нём `entry`. Единственная точка отказа: любое `Err` уходит
/// в журнал записью `ok = false`, поэтому сообщение обязано быть очищено от полного пути —
/// `std::io` и harness несут его в тексте ошибки.
fn read_payload(dir: &Path) -> Result<Value, String> {
    read_payload_raw(dir).map_err(|error| sanitize_paths(&error, dir))
}

/// Тело [`read_payload`] без чистки путей (её делает вызывающий ровно один раз).
fn read_payload_raw(dir: &Path) -> Result<Value, String> {
    let manifest = read_manifest(dir)?;
    let entry_path = declared_entry_path(dir, &manifest)?;
    let source = read_source(&entry_path)?;
    fit_payload(&manifest, &source)
}

/// Читает текст `plugin.json` под потолком [`MAX_MANIFEST_BYTES`].
///
/// Потолок проверяется **до** чтения: гигантский манифест иначе уронил бы хост по памяти
/// (проверка размера после `read_to_string` бесполезна — аллокация уже произошла).
fn read_manifest(dir: &Path) -> Result<String, String> {
    let bytes = read_capped(&dir.join(READ_MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    String::from_utf8(bytes).map_err(|e| format!("{READ_MANIFEST_FILE}: не UTF-8: {e}"))
}

/// Путь к исходнику, объявленному манифестом.
///
/// Имя берётся через `Manifest::from_json` (разбор **и** валидация), а не голой десериализацией:
/// только валидированный манифест гарантирует безопасное имя файла — без разделителей пути,
/// `:` и `..` ([`plugin_core::manifest::ManifestError::InvalidEntry`]), поэтому `dir.join(entry)`
/// не может вывести чтение за каталог плагина. Неразобранный или невалидный манифест откатывается
/// на [`READ_ENTRY_FILE`]: отладке нужен ответ, а не отказ на каждом устаревшем манифесте.
///
/// Объявленное имя проходит reparse-гейт тем же вызовом, что и `plugin.json`/`main.lua`
/// (AC-MCP-12): иначе symlink-`entry` читался бы вслед за ссылкой за пределы `plugins/`.
fn declared_entry_path(dir: &Path, manifest: &str) -> Result<PathBuf, String> {
    let entry = plugin_core::manifest::Manifest::from_json(manifest.as_bytes())
        .map(|manifest| manifest.entry)
        .unwrap_or_else(|_| READ_ENTRY_FILE.to_string());
    crate::ai::plugin_harness::ensure_plugin_entry_allowed(dir, &entry).map_err(|e| e.message)
}

/// Укладывает ответ в бюджет модели. Если исходник не влезает — режется **он** (манифест нужен
/// целиком: без него отладка невозможна) и усечение объявляется полем `truncated: true`.
///
/// `Err` — гигантский манифест, при котором не влезает даже пустой исходник: отдать такой payload
/// значит получить от транспорта молча обрезанный JSON, поэтому честнее явная ошибка.
fn fit_payload(manifest: &str, source: &str) -> Result<Value, String> {
    if payload_chars(manifest, source, false) <= READ_PAYLOAD_BUDGET {
        return Ok(json!({
            "manifest": manifest,
            "source": source,
            "truncated": false,
        }));
    }
    let Some(fitted) = fit_source(manifest, source) else {
        return Err(format!(
            "{READ_MANIFEST_FILE}: ответ не укладывается в лимит модели ({MAX_TOOL_RESULT_CHARS} символов)"
        ));
    };
    Ok(json!({
        "manifest": manifest,
        "source": fitted,
        "truncated": true,
    }))
}

/// Наибольший префикс исходника, при котором payload укладывается в бюджет.
///
/// Бинарный поиск по числу символов: JSON-экранирование нелинейно, поэтому длина измеряется
/// сериализацией ([`payload_chars`]), а не арифметикой. `None` — не влезает даже пустой исходник.
fn fit_source(manifest: &str, source: &str) -> Option<String> {
    let fits = |chars: usize| {
        payload_chars(manifest, &truncate_chars(source, chars), true) <= READ_PAYLOAD_BUDGET
    };
    if !fits(0) {
        return None;
    }

    // Инвариант «префикс длины k влезает → влезает и меньший» держится: добавление символа не
    // меняет экранирование предыдущих, поэтому длина payload монотонна по длине префикса.
    let mut low = 0;
    let mut high = source.chars().count();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Some(truncate_chars(source, low))
}

/// Длина ответа в символах — ровно то, что видит транспорт: `Value::to_string()` в
/// `content[0].text` и обрезка `truncate_chars` считают символы, а не байты.
fn payload_chars(manifest: &str, source: &str, truncated: bool) -> usize {
    json!({
        "manifest": manifest,
        "source": source,
        "truncated": truncated,
    })
    .to_string()
    .chars()
    .count()
}
