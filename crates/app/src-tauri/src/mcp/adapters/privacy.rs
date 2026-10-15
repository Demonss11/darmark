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

/// Символ, продолжающий сегмент пути (буква/цифра/`.`/`_`/`-`), а не границу между сегментами.
///
/// Разделители (`\`, `/`), кавычки, пробелы и прочие символы границей считаются, поэтому
/// вариант пути совпадает только целиком, а не как подстрока внутри сегмента.
fn is_path_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// Заменяет вариант пути на `name`, только если он совпадает как **целый сегмент**: и слева,
/// и справа от вхождения стоит граница пути (начало/конец строки или не-имя-сегмента).
///
/// Так `C:\a` не затирает `C:\ab` или `C:\a.backup` (префикс другого пути), а также не
/// срабатывает на варианте внутри более длинного имени.
fn replace_path_segment(text: &str, variant: &str, name: &str) -> String {
    if variant.is_empty() {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(variant) {
        let start = search_from + rel;
        let end = start + variant.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_path_name_char(c));
        let after_ok = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_path_name_char(c));
        if before_ok && after_ok {
            result.push_str(&text[copied..start]);
            result.push_str(name);
            copied = end;
            search_from = end;
        } else {
            search_from = start + variant.chars().next().map_or(1, char::len_utf8);
        }
    }
    result.push_str(&text[copied..]);
    result
}

/// Заменяет вхождения вариантов пути на `name` с учётом границ сегмента пути.
fn sanitize_text(text: &str, variants: &[String], name: &str) -> String {
    variants.iter().fold(text.to_string(), |acc, variant| {
        replace_path_segment(&acc, variant, name)
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
