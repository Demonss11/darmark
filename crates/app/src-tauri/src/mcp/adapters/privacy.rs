use std::path::Path;

use serde_json::Value;

use crate::ai::types::AiValidationResult;

/// Префикс verbatim-пути Windows (`\\?\`), который возвращает `canonicalize`.
const VERBATIM_PREFIX: &str = r"\\?\";

/// Имя каталога (последний компонент пути) — приватность §4.6 (не полный путь).
pub(super) fn path_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "-".to_string())
}

/// Варианты строкового представления каталога для замены (verbatim и «обычный»).
///
/// `canonicalize` на Windows отдаёт verbatim-путь (`\\?\C:\…`), но в отчётах/ошибках CLI
/// может встретиться и форма без префикса — чистим обе. Длинные варианты идут первыми:
/// иначе замена короткого оставила бы префикс `\\?\` в тексте.
fn dir_path_variants(dir: &Path) -> Vec<String> {
    let mut variants = vec![dir.to_string_lossy().into_owned()];
    if let Some(stripped) = variants[0].strip_prefix(VERBATIM_PREFIX) {
        variants.push(stripped.to_string());
    }
    variants.retain(|variant| !variant.is_empty());
    variants.sort_by_key(|variant| std::cmp::Reverse(variant.len()));
    variants
}

/// Заменяет все вхождения вариантов пути на `name`.
fn sanitize_text(text: &str, variants: &[String], name: &str) -> String {
    variants.iter().fold(text.to_string(), |acc, variant| {
        acc.replace(variant.as_str(), name)
    })
}

/// Убирает полный путь базового каталога из текста (приватность R2/AC-MCP-13).
pub(super) fn sanitize_paths(text: &str, base: &Path) -> String {
    sanitize_text(text, &dir_path_variants(base), &path_name(base))
}

/// Убирает полный путь каталога из ошибок валидации.
pub(super) fn sanitize_validation(result: &mut AiValidationResult, dir: &Path) {
    let variants = dir_path_variants(dir);
    let name = path_name(dir);
    for error in &mut result.errors {
        *error = sanitize_text(error, &variants, &name);
    }
    if let Some(report) = result.report.as_mut() {
        sanitize_value(report, &variants, &name);
    }
}

/// Рекурсивно чистит строки JSON-значения (отчёт CLI несёт путь в `path` и `checks[].message`).
fn sanitize_value(value: &mut Value, variants: &[String], name: &str) {
    match value {
        Value::String(text) => *text = sanitize_text(text, variants, name),
        Value::Array(items) => {
            for item in items.iter_mut() {
                sanitize_value(item, variants, name);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                sanitize_value(item, variants, name);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
