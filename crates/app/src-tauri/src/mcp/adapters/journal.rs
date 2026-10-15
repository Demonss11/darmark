use super::LogSink;

/// Текст записи журнала для вызова MCP tool (TZ-MCP R2).
///
/// `subject` — имя каталога/файла либо иная краткая характеристика вызова
/// (количество плагинов, объём текста) — без полного пути и содержимого документа.
/// Чистая функция — тестируется без Tauri (AC-MCP-13: в записи нет полного пути).
pub(super) fn mcp_log_message(
    tool: &str,
    subject: &str,
    duration: std::time::Duration,
    ok: bool,
    errors: &[String],
) -> String {
    if ok {
        format!("MCP {tool}: {subject} — ok ({} мс)", duration.as_millis())
    } else {
        format!(
            "MCP {tool}: {subject} — ошибка: {} ({} мс)",
            errors.join("; "),
            duration.as_millis()
        )
    }
}

/// Публикует запись журнала через инъецированный сток (best-effort, TZ-MCP R2).
///
/// Категория `system` и конверт события задаются стоком (`mcp::app_log_sink`): TS-тип
/// `LogCategory` (`plugin`/`document`/`system`) не трогаем.
pub(super) fn log_mcp_call(sink: &LogSink, message: &str) {
    sink(message);
}
