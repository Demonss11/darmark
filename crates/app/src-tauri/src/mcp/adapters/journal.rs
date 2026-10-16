use super::LogSink;






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





pub(super) fn log_mcp_call(sink: &LogSink, message: &str) {
    sink(message);
}
