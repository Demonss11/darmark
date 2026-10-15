//! Host-call'и `DocumentServices`: экспорт HTML, статусбар, контент view, правка и выдача текста.

use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::plugins::services::{DocumentServices, Exporter, StatusSink};
use crate::plugins::HostServices;

use super::common::{services, store_with_doc, view_contrib, views};

#[test]
fn export_html_success_returns_true() {
    let store = store_with_doc("x");
    let captured = Arc::new(Mutex::new(String::new()));
    let captured_for = Arc::clone(&captured);
    let exporter: Exporter = Arc::new(move |html: &str| {
        *captured_for.lock().unwrap() = html.to_string();
        Ok(true)
    });
    let services = DocumentServices::new(store).with_exporter(exporter);

    let result = services
        .handle("export_html", json!({ "html": "<p>x</p>" }))
        .unwrap();
    assert_eq!(result, json!(true));
    assert_eq!(*captured.lock().unwrap(), "<p>x</p>");
}

#[test]
fn export_html_cancel_returns_false() {
    let store = store_with_doc("x");
    let exporter: Exporter = Arc::new(|_html: &str| Ok(false));
    let services = DocumentServices::new(store).with_exporter(exporter);

    let result = services
        .handle("export_html", json!({ "html": "x" }))
        .unwrap();
    assert_eq!(result, json!(false), "отмена диалога → false, не ошибка");
}

#[test]
fn export_html_io_error_is_value() {
    let store = store_with_doc("x");
    let exporter: Exporter = Arc::new(|_html: &str| Err("диск полон".to_string()));
    let services = DocumentServices::new(store).with_exporter(exporter);

    let err = services
        .handle("export_html", json!({ "html": "x" }))
        .unwrap_err();
    assert_eq!(err.code, "export_failed");
    assert!(
        err.message.contains("диск полон"),
        "message: {}",
        err.message
    );
}

#[test]
fn export_html_without_exporter_is_error() {
    let store = store_with_doc("x");
    let services = DocumentServices::new(store);
    let err = services
        .handle("export_html", json!({ "html": "x" }))
        .unwrap_err();
    assert_eq!(err.code, "no_exporter");
}

#[test]
fn show_message_calls_status_sink() {
    let store = store_with_doc("x");
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen_for = Arc::clone(&seen);
    let sink: StatusSink = Arc::new(move |plugin_id: &str, text: &str| {
        seen_for
            .lock()
            .unwrap()
            .push((plugin_id.to_string(), text.to_string()));
    });
    let services = DocumentServices::new(store).with_status_sink(sink);

    let result = services
        .handle(
            "show_message",
            json!({ "text": "Слова: 5", "_plugin_id": "word-count" }),
        )
        .unwrap();
    assert_eq!(result, json!(true), "успех show_message — значение true");
    assert_eq!(
        *seen.lock().unwrap(),
        vec![("word-count".to_string(), "Слова: 5".to_string())]
    );
}

#[test]
fn show_message_without_plugin_id_is_rejected() {
    let store = store_with_doc("x");
    let services = DocumentServices::new(store);
    let err = services
        .handle("show_message", json!({ "text": "x" }))
        .unwrap_err();
    assert_eq!(err.code, "bad_args");
}

#[test]
fn document_services_set_view_content_sanitizes_and_notifies() {
    let store = store_with_doc("x");
    let views = views();
    views
        .lock()
        .unwrap()
        .register("p", &[view_contrib("main", "Main", 1)]);

    let notified = Arc::new(Mutex::new(0u32));
    let notified_for = Arc::clone(&notified);
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        *notified_for.lock().unwrap() += 1;
    });
    let services = DocumentServices::new(Arc::clone(&store))
        .with_views(Arc::clone(&views))
        .with_notify(notify);

    let result = services
        .handle(
            "set_view_content",
            json!({
                "view_id": "p:main",
                "html": "<img src=x onerror=alert(1)>",
                "_plugin_id": "p",
            }),
        )
        .unwrap();
    assert_eq!(result, json!(true));
    assert_eq!(*notified.lock().unwrap(), 1, "notify обязан сработать");
    let html = views.lock().unwrap().snapshot()[0].html.clone();
    assert!(!html.contains("onerror"), "html не санитизирован: {html}");
}

#[test]
fn set_view_content_rejects_foreign_plugin() {
    let store = store_with_doc("x");
    let views = views();
    views
        .lock()
        .unwrap()
        .register("p", &[view_contrib("main", "Main", 1)]);
    let services = DocumentServices::new(Arc::clone(&store)).with_views(Arc::clone(&views));

    let err = services
        .handle(
            "set_view_content",
            json!({ "view_id": "other:main", "html": "x", "_plugin_id": "p" }),
        )
        .unwrap_err();
    assert_eq!(err.code, "bad_view_id");
}

#[test]
fn set_view_content_requires_plugin_id() {
    let store = store_with_doc("x");
    let views = views();
    views
        .lock()
        .unwrap()
        .register("p", &[view_contrib("main", "Main", 1)]);
    let services = DocumentServices::new(Arc::clone(&store)).with_views(Arc::clone(&views));

    let err = services
        .handle(
            "set_view_content",
            json!({ "view_id": "p:main", "html": "x" }),
        )
        .unwrap_err();
    assert_eq!(err.code, "bad_args", "без _plugin_id вызов отвергается");
}

#[test]
fn apply_edit_defers_document_changed() {
    let store = store_with_doc("# A");
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let services =
        DocumentServices::new(Arc::clone(&store)).with_pending_changed(Arc::clone(&pending));

    services
        .handle(
            "apply_edit",
            json!({ "doc_id": "doc-1", "start": 0, "stop": 0, "text": "X" }),
        )
        .unwrap();
    let queued = pending.lock().unwrap().clone();
    assert_eq!(
        queued,
        vec![("doc-1".to_string(), 1)],
        "правка → отложенный changed"
    );
    services
        .handle(
            "apply_edit",
            json!({ "doc_id": "doc-1", "start": 0, "stop": 1, "text": "X" }),
        )
        .unwrap();
    assert_eq!(pending.lock().unwrap().len(), 1, "эхо не порождает событие");
}

/// F40/S1: ответ `get_document_text` — JSON-строка; при обилии управляющих символов
/// экранирование раздувает кадр host→child (до 6×) сверх `MAX_HOST_FRAME_BYTES`. Вместо
/// `protocol`-сбоя хост обязан вернуть **явный** `too_large` (основной путь — окна).
#[test]
fn get_document_text_rejects_escaping_overflow() {
    let dense = "\u{0001}".repeat(3 * 1024 * 1024);
    let store = store_with_doc(&dense);
    let services = services(&store);

    let err = services
        .handle("get_document_text", json!({ "doc_id": "doc-1" }))
        .unwrap_err();
    assert_eq!(
        err.code, "too_large",
        "плотный текст → явный отказ, не protocol"
    );
}

/// F40/S1 (контроль): текст, умещающийся в кадр даже после экранирования, отдаётся целиком —
/// гард не должен резать легитимные документы.
#[test]
fn get_document_text_allows_text_within_frame() {
    let text = "\"".repeat(1024 * 1024);
    let store = store_with_doc(&text);
    let services = services(&store);

    let value = services
        .handle("get_document_text", json!({ "doc_id": "doc-1" }))
        .unwrap();
    assert_eq!(
        value.as_str().map(str::len),
        Some(text.len()),
        "текст в пределах кадра отдаётся целиком"
    );
}
