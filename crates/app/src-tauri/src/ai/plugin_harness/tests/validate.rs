use super::super::validate::{manifest_errors, MAX_PLUGIN_ID_LEN};
use super::super::*;
use crate::error::ErrorCode;

#[test]
fn safe_ids_are_accepted() {
    for id in ["word-count", "my_plugin", "a1", "x"] {
        assert!(validate_plugin_id(id).is_ok(), "принят: {id}");
    }
}

#[test]
fn unsafe_ids_are_rejected() {
    for id in [
        "",
        "..",
        "../evil",
        "a/b",
        "a\\b",
        "A",
        "with space",
        "спец",
        "id!",
        "a.b",
    ] {
        let err = validate_plugin_id(id).unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginInvalid, "отклонён: {id:?}");
    }
    let too_long = "a".repeat(MAX_PLUGIN_ID_LEN + 1);
    assert_eq!(
        validate_plugin_id(&too_long).unwrap_err().code,
        ErrorCode::AiPluginInvalid
    );
}

#[test]
fn cli_report_with_schema_version_is_trusted() {
    let stdout = r#"{
            "schema_version": 1,
            "path": "C:/tmp/p",
            "ok": false,
            "checks": [
                {"name": "manifest", "status": "ok", "message": "ok"},
                {"name": "sandbox", "status": "fail", "message": "on_activate упал"}
            ],
            "sandbox_logs": []
        }"#;
    let result = parse_cli_report(stdout).expect("отчёт со schema_version=1 доверенный");
    assert!(!result.ok);
    assert_eq!(result.errors, vec!["sandbox: on_activate упал".to_string()]);
    assert!(result.report.is_some());
}

#[test]
fn cli_report_without_or_unknown_schema_falls_back() {
    assert!(parse_cli_report(r#"{"ok": true, "checks": []}"#).is_none());
    assert!(parse_cli_report(r#"{"schema_version": 2, "ok": true, "checks": []}"#).is_none());
    assert!(parse_cli_report("не json").is_none());
}

#[test]
fn fallback_manifest_validation_flags_errors() {
    let good = serde_json::json!({
        "id": "word-count",
        "name": "Word Count",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua"
    });
    assert!(manifest_errors(&good, "function on_activate(ctx) end").is_empty());

    let bad_perm = serde_json::json!({
        "id": "word-count",
        "name": "Word Count",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua",
        "permissions": ["quantum:teleport"]
    });
    let errors = manifest_errors(&bad_perm, "function on_activate(ctx) end");
    assert!(
        errors.iter().any(|e| e.contains("quantum:teleport")),
        "{errors:?}"
    );

    let bad_id = serde_json::json!({
        "id": "../evil",
        "name": "X",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua"
    });
    assert!(!manifest_errors(&bad_id, "x").is_empty());
}

/// Единая точка статической валидации (TZ-MCP R4/AC-MCP-6): принимает валидный
/// каталог и сообщает `manifest`/`entry`-ошибки при отсутствии файлов.
#[test]
fn static_validation_accepts_valid_plugin_and_reports_missing_files() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("p");
    let manifest = serde_json::json!({
        "id": "p",
        "name": "P",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua"
    });
    write_plugin_files(&dir, &manifest, "function on_activate(ctx) end").unwrap();
    let valid = validate_plugin_static(&dir);
    assert!(valid.ok, "валидный плагин: {:?}", valid.errors);
    assert!(valid.errors.is_empty());

    let empty = root.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let missing = validate_plugin_static(&empty);
    assert!(!missing.ok);
    assert!(
        missing.errors.iter().any(|e| e.starts_with("manifest:")),
        "{:?}",
        missing.errors
    );
    assert!(
        missing.errors.iter().any(|e| e.starts_with("entry:")),
        "{:?}",
        missing.errors
    );
}

#[test]
fn manifest_id_must_match_param() {
    let manifest = serde_json::json!({
        "id": "p",
        "name": "P",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua"
    });
    assert!(validate_manifest_id(&manifest, "p").is_ok());
    let err = validate_manifest_id(&manifest, "q").unwrap_err();
    assert_eq!(err.code, ErrorCode::AiPluginInvalid, "id != параметр id");

    let missing = serde_json::json!({"name": "P"});
    assert_eq!(
        validate_manifest_id(&missing, "p").unwrap_err().code,
        ErrorCode::AiPluginInvalid,
        "нет строкового id"
    );
}

#[test]
fn source_and_manifest_limits_are_checked() {
    let manifest = serde_json::json!({
        "id": "p",
        "name": "P",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua"
    });
    assert!(check_plugin_limits(&manifest, "function on_activate(ctx) end").is_ok());

    let max = plugin_core::MAX_PLUGIN_SOURCE_BYTES as usize;
    let big_source = "a".repeat(max + 1);
    assert_eq!(
        check_plugin_limits(&manifest, &big_source)
            .unwrap_err()
            .code,
        ErrorCode::AiPluginInvalid,
        "исходник сверх лимита"
    );

    let big_manifest = serde_json::json!({
        "id": "p",
        "name": "P",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua",
        "description": "x".repeat(max + 1)
    });
    assert_eq!(
        check_plugin_limits(&big_manifest, "x").unwrap_err().code,
        ErrorCode::AiPluginInvalid,
        "манифест сверх лимита"
    );
}
