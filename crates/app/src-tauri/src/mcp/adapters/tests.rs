use super::journal::mcp_log_message;
use super::privacy::{path_name, sanitize_paths, sanitize_validation};
use super::*;
use std::time::Duration;

use std::path::Path;

use crate::ai::types::AiValidationResult;

#[test]
fn path_name_returns_last_component() {
    let path = Path::new(r"C:\Users\Demon\AppData\Roaming\darmark\plugins\word-count");
    assert_eq!(path_name(path), "word-count");
}

#[test]
fn path_name_falls_back_without_component() {
    assert_eq!(path_name(Path::new("")), "-");
}

#[test]
fn log_message_has_no_full_path_and_reports_ok() {
    let message = mcp_log_message(
        "plugin_test",
        "word-count",
        Duration::from_millis(12),
        true,
        &[],
    );
    assert!(message.contains("word-count"), "msg: {message}");
    assert!(!message.contains(":\\"), "нет полного пути: {message}");
    assert!(message.contains("ok"), "msg: {message}");
}

#[test]
fn log_message_lists_errors_without_full_path() {
    let errors = vec!["manifest: missing id".to_string()];
    let message = mcp_log_message(
        "plugin_validate",
        "broken",
        Duration::from_millis(3),
        false,
        &errors,
    );
    assert!(message.contains("broken"), "msg: {message}");
    assert!(message.contains("missing id"), "msg: {message}");
    assert!(!message.contains(":\\"), "нет полного пути: {message}");
}

#[test]
fn log_message_carries_counts_for_pathless_tools() {
    let catalog = mcp_log_message(
        "plugin_catalog_list",
        "4 плагинов",
        Duration::from_millis(1),
        true,
        &[],
    );
    assert!(catalog.contains("plugin_catalog_list"), "{catalog}");
    assert!(catalog.contains("4 плагинов"), "{catalog}");

    let render = mcp_log_message(
        "markdown_render",
        "120 символов",
        Duration::from_millis(1),
        true,
        &[],
    );
    assert!(render.contains("markdown_render"), "{render}");
    assert!(render.contains("120 символов"), "{render}");
}

#[test]
fn path_schema_requires_path() {
    let schema = path_schema();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"][0], "path");
}



#[test]
fn sanitize_validation_strips_full_path_from_errors_and_report() {
    let dir = Path::new(r"C:\Users\Demon\AppData\Roaming\darmark\plugins\word-count");
    let mut result = AiValidationResult {
        ok: false,
        errors: vec![
            format!("manifest: {}\\plugin.json: нет id", dir.display()),
            format!("entry: {}\\main.lua: слишком большой", dir.display()),
        ],
        report: Some(json!({
            "path": dir.display().to_string(),
            "checks": [{
                "name": "manifest",
                "status": "fail",
                "message": format!("{}: нет id", dir.join("plugin.json").display()),
            }],
        })),
    };

    sanitize_validation(&mut result, dir);

    let errors = result.errors.join("; ");
    assert!(
        errors.contains("word-count"),
        "имя каталога сохранено: {errors}"
    );
    assert!(!errors.contains(":\\"), "нет полного пути: {errors}");
    assert!(!errors.contains("AppData"), "нет полного пути: {errors}");

    let report = result.report.expect("отчёт сохранён");
    assert_eq!(report["path"], "word-count", "path → имя каталога");
    let rendered = report.to_string();
    assert!(
        !rendered.contains(":\\"),
        "в отчёте нет полного пути: {rendered}"
    );
    assert!(
        !rendered.contains("AppData"),
        "в отчёте нет полного пути: {rendered}"
    );
}


#[test]
fn sanitize_paths_handles_verbatim_prefix() {
    let dir = Path::new(r"\\?\C:\Users\Demon\AppData\Roaming\darmark\plugins\word-count");
    let text = format!("{}\\plugin.json: ошибка", dir.display());
    let sanitized = sanitize_paths(&text, dir);
    assert!(sanitized.contains("word-count"), "{sanitized}");
    assert!(!sanitized.contains(":\\"), "нет полного пути: {sanitized}");
    assert!(
        !sanitized.contains(r"\\?\"),
        "нет verbatim-префикса: {sanitized}"
    );
}

/// Ошибка каталога (`index.json`) тоже чистится перед журналом и результатом.
#[test]
fn sanitize_paths_replaces_repo_dir_with_name() {
    let repo = Path::new(r"C:\repo\examples\plugins");
    let text = format!("{}\\index.json: не разобран JSON", repo.display());
    let sanitized = sanitize_paths(&text, repo);
    assert!(sanitized.starts_with(r"plugins\index.json"), "{sanitized}");
    assert!(!sanitized.contains(":\\"), "нет полного пути: {sanitized}");
}

/// Замена привязана к границам сегмента пути: вариант затирается только целиком.
#[test]
fn sanitize_paths_replaces_only_whole_path_segment() {
    let base = Path::new(r"C:\repo");

    // Границей справа служит разделитель → замена происходит.
    assert_eq!(
        sanitize_paths(r"C:\repo\index.json: ошибка", base),
        r"repo\index.json: ошибка"
    );
}

/// Вариант как префикс другого пути не должен затираться (границы сегмента нет).
#[test]
fn sanitize_paths_keeps_variant_as_prefix_of_another_path() {
    let base = Path::new(r"C:\repo");
    for text in [
        r"C:\repo2\index.json: ошибка",
        r"C:\repo.backup\index.json: ошибка",
    ] {
        assert_eq!(
            sanitize_paths(text, base),
            text,
            "префикс другого пути не затирается: {text}"
        );
    }
}

/// Вариант как суффикс сегмента (слева символ имени) не затирается.
#[test]
fn sanitize_paths_keeps_variant_as_segment_suffix() {
    let base = Path::new(r"C:\repo");
    let text = r"backupC:\repo\index.json";
    assert_eq!(
        sanitize_paths(text, base),
        text,
        "суффикс внутри сегмента не затирается"
    );
}

/// Вариант как подстрока внутри сегмента (обе стороны — символы имени) не затирается.
#[test]
fn sanitize_paths_keeps_variant_as_substring_inside_segment() {
    let base = Path::new(r"C:\repo");
    let text = r"xC:\repoy";
    assert_eq!(
        sanitize_paths(text, base),
        text,
        "подстрока внутри сегмента не затирается"
    );
}
