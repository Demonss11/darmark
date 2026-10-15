use super::permissions::with_plugin_id;
use super::*;

#[test]
fn permission_denied_for_write_without_grant() {
    let err = check_permission(&["document:read".to_string()], "apply_edit").unwrap_err();
    assert_eq!(err.code, "permission_denied");
    assert_eq!(err.permission.as_deref(), Some("document:write"));
}

#[test]
fn granted_permission_allows_call() {
    assert!(check_permission(&["document:write".to_string()], "apply_edit").is_ok());
    assert!(check_permission(&[], "get_setting").is_ok());
}

#[test]
fn read_calls_require_document_read() {
    let err = check_permission(&[], "get_document_range").unwrap_err();
    assert_eq!(err.permission.as_deref(), Some("document:read"));
}

#[test]
fn set_view_content_requires_view_modify() {
    let err = check_permission(&["view:create".to_string()], "set_view_content").unwrap_err();
    assert_eq!(err.code, "permission_denied");
    assert_eq!(err.permission.as_deref(), Some("view:modify"));
    assert!(check_permission(&["view:modify".to_string()], "set_view_content").is_ok());
}

#[test]
fn export_html_requires_no_permission() {
    assert_eq!(required_permission("export_html"), None);
    assert!(
        check_permission(&[], "export_html").is_ok(),
        "нативный диалог — согласие пользователя, permission не нужен"
    );
}

#[test]
fn show_message_requires_statusbar() {
    assert_eq!(required_permission("show_message"), Some("ui:statusbar"));
    let err = check_permission(&[], "show_message").unwrap_err();
    assert_eq!(err.code, "permission_denied");
    assert_eq!(err.permission.as_deref(), Some("ui:statusbar"));
    assert!(check_permission(&["ui:statusbar".to_string()], "show_message").is_ok());
}

#[test]
fn plugin_id_is_injected_into_host_call_args() {
    let args = serde_json::json!({ "view_id": "p:main", "html": "<b>x</b>" });
    let injected = with_plugin_id(&args, "p");
    assert_eq!(injected["_plugin_id"], "p");
    assert_eq!(injected["view_id"], "p:main");
    assert!(args.get("_plugin_id").is_none());
}

#[test]
fn plugin_id_injection_keeps_non_object_args() {
    let injected = with_plugin_id(&Value::Null, "p");
    assert_eq!(injected, Value::Null);
}
