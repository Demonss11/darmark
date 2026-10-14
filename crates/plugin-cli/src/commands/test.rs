//! `test`: манифест, entry, запуск в песочнице.

use std::io::Read;
use std::path::{Path, PathBuf};

use plugin_core::manifest::Manifest;
use plugin_host::selftest;

use crate::report::{Check, TestReport};
use crate::templates::MANIFEST_FILE;

/// Лимит размера `plugin.json` (совпадает с GUI-сканированием и самопроверкой child).
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// Проверки идут по порядку; при провале ранней (манифест/entry) последующие не выполняются —
/// без валидного входа песочницу запускать бессмысленно.
pub fn run_test(path: &Path, verbose: bool) -> TestReport {
    let mut checks = Vec::new();
    let mut sandbox_logs = Vec::new();

    if verbose {
        eprintln!("путь: {}", path.display());
        eprintln!(
            "тип: {}",
            if path.is_dir() {
                "каталог плагина"
            } else {
                ".lua-файл"
            }
        );
    }

    if !path.exists() {
        checks.push(Check::fail(
            "path",
            format!("{}: путь не существует", path.display()),
        ));
        return TestReport {
            path: path.to_path_buf(),
            checks,
            sandbox_logs,
        };
    }

    let is_dir = path.is_dir();

    if is_dir {
        match check_manifest(path) {
            Ok(message) => checks.push(Check::ok("manifest", message)),
            Err(message) => {
                checks.push(Check::fail("manifest", message));
                return TestReport {
                    path: path.to_path_buf(),
                    checks,
                    sandbox_logs,
                };
            }
        }
    }

    match check_entry(path) {
        Ok(message) => checks.push(Check::ok("entry", message)),
        Err(message) => {
            checks.push(Check::fail("entry", message));
            return TestReport {
                path: path.to_path_buf(),
                checks,
                sandbox_logs,
            };
        }
    }

    match selftest::self_test(path) {
        Ok(report) => {
            sandbox_logs = report.logs;
            checks.push(Check::ok(
                "sandbox",
                format!(
                    "{}: активирован ({} байт)",
                    report.plugin_id, report.source_bytes
                ),
            ));
        }
        Err(message) => checks.push(Check::fail("sandbox", message)),
    }

    TestReport {
        path: path.to_path_buf(),
        checks,
        sandbox_logs,
    }
}

fn check_manifest(dir: &Path) -> Result<String, String> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let bytes = read_capped(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest =
        Manifest::from_json(&bytes).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let permissions = if manifest.permissions.is_empty() {
        "нет".to_string()
    } else {
        manifest.permissions.join(", ")
    };
    Ok(format!(
        "id={}, version={}, api_version={}, permissions: {}",
        manifest.id, manifest.version, manifest.api_version, permissions
    ))
}

/// Путь к entry-файлу: для каталога — из манифеста, для `.lua` — сам файл.
fn entry_path_of(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        let manifest_path = path.join(MANIFEST_FILE);
        let bytes = read_capped(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest =
            Manifest::from_json(&bytes).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        Ok(path.join(&manifest.entry))
    } else {
        Ok(path.to_path_buf())
    }
}

fn check_entry(path: &Path) -> Result<String, String> {
    let entry_path = entry_path_of(path)?;
    if !entry_path.is_file() {
        return Err(format!("{}: файл не найден", entry_path.display()));
    }

    let bytes = std::fs::read(&entry_path)
        .map_err(|e| format!("{}: не читается: {e}", entry_path.display()))?;
    plugin_core::manifest::validate_source_len(bytes.len() as u64)
        .map_err(|e| format!("{}: {e}", entry_path.display()))?;
    let source =
        String::from_utf8(bytes).map_err(|e| format!("{}: не UTF-8: {e}", entry_path.display()))?;

    if !has_on_activate(&source) {
        return Err(format!(
            "{}: в исходнике не найдена функция on_activate",
            entry_path.display()
        ));
    }

    Ok(format!(
        "{}: {} байт, UTF-8, on_activate найден",
        entry_path.display(),
        source.len()
    ))
}

/// Эвристика объявления **глобальной** точки входа `on_activate`.
///
/// Признаются только две формы:
/// - `function on_activate(...)` — голое имя, `function` и `on_activate` разделены
///   любыми whitespace (≥1);
/// - `on_activate = ...` — голое имя, между `on_activate` и `=` любые whitespace.
///
/// Отвергаются: поля/методы (`M.on_activate = ...`, `function M.on_activate(...)`),
/// локальные объявления (`local function on_activate(...)`, `local on_activate = ...`),
/// а также `on_activate` как значение или часть другого идентификатора.
///
/// Известные ограничения:
/// - Lua-комментарии (`--`, `--[[ ]]`) не обрабатываются;
/// - явные глобальные формы `_G.on_activate = ...` / `_ENV.on_activate = ...`
///   отвергаются (перед именем стоит `.`), хотя семантически они глобальные.
fn has_on_activate(source: &str) -> bool {
    const NEEDLE: &str = "on_activate";

    fn is_ws(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0B}' | '\u{0C}')
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// Перед позицией `pos` (после отбрасывания trailing whitespace) не должно быть слова
    /// `local` — иначе объявление локальное, а не глобальная точка входа.
    fn no_local_before(source: &str, pos: usize) -> bool {
        let before = source[..pos].trim_end_matches(is_ws);
        if before.ends_with("local") {
            let local_start = before.len() - "local".len();
            if local_start == 0
                || source[..local_start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_word_char(c))
            {
                return false;
            }
        }
        true
    }

    /// То же про маркер поля/метода `.` или `:` перед позицией.
    fn no_field_marker_before(source: &str, pos: usize) -> bool {
        let before = source[..pos].trim_end_matches(is_ws);
        !before.ends_with('.') && !before.ends_with(':')
    }

    let mut from = 0;
    while let Some(offset) = source[from..].find(NEEDLE) {
        let start = from + offset;
        let end = start + NEEDLE.len();

        // Соседние буквы/цифры/подчёркивания отсекают on_activate2, my_on_activate и т.п.
        let before_ok = source[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = source[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c));

        if before_ok && after_ok {
            // Присваивание: `on_activate` [ws]* `=`.
            if no_field_marker_before(source, start) && no_local_before(source, start) {
                let after = source[end..].trim_start_matches(is_ws);
                if after.starts_with('=') {
                    return true;
                }
            }

            // Объявление функции: `function` [ws]+ `on_activate`. Пробел обязателен —
            // иначе `function M.on_activate` прошёл бы как глобальное объявление.
            let before = &source[..start];
            if let Some(func_rel) = before.rfind("function") {
                let func_start = func_rel;
                let func_end = func_start + "function".len();
                let func_before_ok = source[..func_start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_word_char(c));
                let between = &before[func_end..];
                let between_ok = !between.is_empty() && between.chars().all(is_ws);
                if func_before_ok && between_ok && no_local_before(source, func_start) {
                    return true;
                }
            }
        }

        from = end;
    }
    false
}

/// Чтение с жёстким потолком размера: `metadata` обгоняет файл между `stat` и `read`
/// (TOCTOU), поэтому лимит проверяется ещё раз по фактически прочитанному.
fn read_capped(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if metadata.len() > max {
        return Err(format!(
            "{}: превышает лимит {max} Б ({} Б)",
            path.display(),
            metadata.len()
        ));
    }
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if buf.len() as u64 > max {
        return Err(format!("{}: превышает лимит {max} Б", path.display()));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{report_to_json, CheckStatus};
    use std::fs;

    fn write_plugin(dir: &Path, manifest: &str, lua: Option<&str>) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(MANIFEST_FILE), manifest).unwrap();
        if let Some(lua) = lua {
            fs::write(dir.join("main.lua"), lua).unwrap();
        }
    }

    fn manifest(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","name":"{id}","version":"1.0.0","api_version":1,"entry":"main.lua"}}"#
        )
    }

    fn find<'a>(report: &'a TestReport, name: &str) -> &'a Check {
        report
            .checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("нет проверки {name}: {:?}", report.checks))
    }

    #[test]
    fn test_valid_plugin_ok() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("valid");
        write_plugin(
            &dir,
            &manifest("valid"),
            Some("function on_activate(ctx) end\nfunction on_deactivate(ctx) end\n"),
        );
        let report = run_test(&dir, false);
        assert!(report.ok(), "checks: {:?}", report.checks);
        assert_eq!(find(&report, "manifest").status, CheckStatus::Ok);
        assert_eq!(find(&report, "entry").status, CheckStatus::Ok);
        assert_eq!(find(&report, "sandbox").status, CheckStatus::Ok);
    }

    #[test]
    fn test_unknown_permission_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("bad-perm");
        write_plugin(
            &dir,
            r#"{"id":"bad-perm","name":"Bad","version":"1.0.0","api_version":1,"entry":"main.lua","permissions":["quantum:teleport"]}"#,
            Some("function on_activate(ctx) end"),
        );
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "manifest");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(
            check.message.contains("quantum:teleport"),
            "msg: {}",
            check.message
        );
    }

    #[test]
    fn test_missing_entry_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("no-entry");
        write_plugin(&dir, &manifest("no-entry"), None);
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "entry");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("main.lua"), "msg: {}", check.message);
    }

    #[test]
    fn test_source_without_on_activate_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("no-activate");
        write_plugin(
            &dir,
            &manifest("no-activate"),
            Some("host.log(\"info\", \"нет активации\")"),
        );
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "entry");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(
            check.message.contains("on_activate"),
            "msg: {}",
            check.message
        );
    }

    #[test]
    fn test_oversized_source_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("big");
        // MAX_PLUGIN_SOURCE_BYTES = 1 МиБ; пишем заведомо больше.
        let big = "function on_activate(ctx) end\n".repeat(70_000);
        write_plugin(&dir, &manifest("big"), Some(&big));
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "entry");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("лимит"), "msg: {}", check.message);
    }

    #[test]
    fn test_smoke_word_count_plugin() {
        // plugin-cli → crates → root → examples/plugins/word-count.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("examples")
            .join("plugins")
            .join("word-count");
        let report = run_test(&path, false);
        assert!(report.ok(), "checks: {:?}", report.checks);
        assert_eq!(find(&report, "sandbox").status, CheckStatus::Ok);
    }

    #[test]
    fn report_json_has_schema_version_and_shape() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("valid");
        write_plugin(
            &dir,
            &manifest("valid"),
            Some("function on_activate(ctx) end\n"),
        );
        let report = run_test(&dir, false);
        let json = report_to_json(&report);
        assert_eq!(
            json["schema_version"], 1,
            "версионированный отчёт (ADR-0034 §2.4)"
        );
        assert_eq!(json["ok"], true);
        assert!(json["checks"].is_array(), "checks[] присутствует");
        assert!(
            json["sandbox_logs"].is_array(),
            "sandbox_logs[] присутствует"
        );
        assert!(json["path"].is_string(), "path присутствует");
    }

    #[test]
    fn has_on_activate_variants() {
        // Базовые случаи.
        assert!(has_on_activate("function on_activate(ctx) end"));
        assert!(has_on_activate("on_activate = function(ctx) end"));
        assert!(has_on_activate("on_activate   = function(ctx) end"));
        assert!(has_on_activate("on_activate=function"));
        assert!(!has_on_activate("function on_event() end"));

        // Разные варианты whitespace между function и on_activate.
        assert!(has_on_activate("function  on_activate(ctx) end"));
        assert!(has_on_activate("function\ton_activate(ctx) end"));
        assert!(has_on_activate("function\non_activate(ctx) end"));
        assert!(has_on_activate("function\r\non_activate(ctx) end"));

        // Локальные объявления — не глобальная точка входа.
        assert!(!has_on_activate("local function on_activate(ctx) end"));
        assert!(!has_on_activate("local on_activate = function(ctx) end"));

        // Границы слова: on_activate2 и my_on_activate — не on_activate.
        assert!(!has_on_activate("on_activate2"));
        assert!(!has_on_activate("my_on_activate"));
        assert!(!has_on_activate("local on_activate_handler = 1"));

        // `on_activate` как значение, а не точка входа — не объявление.
        assert!(!has_on_activate("local f = on_activate"));
        assert!(!has_on_activate("return on_activate"));
        assert!(!has_on_activate("print(on_activate)"));
        assert!(!has_on_activate("host.log('info', on_activate)"));

        // Известное ограничение: комментарии не обрабатываются.
        // Следующая строка будет ложно распознана как объявление:
        // assert!(has_on_activate("-- function on_activate"));
    }

    #[test]
    fn has_on_activate_word_boundaries_around_function() {
        // `function` как часть более длинного идентификатора — не объявление.
        assert!(!has_on_activate("notfunction on_activate(ctx) end"));
        assert!(!has_on_activate("myfunction on_activate(ctx) end"));
        // Продолжение имени после on_activate — другой идентификатор.
        assert!(!has_on_activate("function on_activatex(ctx) end"));
        assert!(!has_on_activate("function on_activate_v2(ctx) end"));
        assert!(!has_on_activate("on_activate_v2 = function(ctx) end"));
    }

    #[test]
    fn has_on_activate_mixed_whitespace_variants() {
        // Смешанные пробелы и переводы строк между function и on_activate.
        assert!(has_on_activate("function \t\n on_activate(ctx) end"));
        assert!(has_on_activate("function\n\non_activate(ctx) end"));
        // Whitespace перед `=`, включая перевод строки.
        assert!(has_on_activate("on_activate   \n = function(ctx) end"));
        assert!(has_on_activate("on_activate\r\n= function(ctx) end"));
    }

    #[test]
    fn has_on_activate_declaration_without_parens() {
        // Эвристика не парсит синтаксис Lua: `function on_activate` без скобок
        // и с пробелом перед `(` распознаётся — невалидный синтаксис поймает песочница.
        assert!(has_on_activate("function on_activate end"));
        assert!(has_on_activate("function on_activate (ctx) end"));
    }

    #[test]
    fn has_on_activate_unicode_spaces_are_not_separators() {
        // Lua-лексер считает разделителями только ASCII-пробелы, \t, \n, \r:
        // NBSP и другие юникод-пробелы не разрывают токены, эвристика это учитывает.
        assert!(!has_on_activate("function\u{00A0}on_activate(ctx) end"));
        assert!(!has_on_activate("on_activate\u{00A0}= function(ctx) end"));
        assert!(!has_on_activate("function\u{2028}on_activate(ctx) end"));
    }

    #[test]
    fn has_on_activate_vertical_tab_and_form_feed() {
        // Lua допускает \v и \f как разделители токенов — эвристика их учитывает.
        assert!(has_on_activate("function\u{0B}on_activate(ctx) end"));
        assert!(has_on_activate("function\u{0C}on_activate(ctx) end"));
        // Форма с \v перед `=`.
        assert!(has_on_activate("on_activate\u{0B}= function(ctx) end"));
    }

    #[test]
    fn has_on_activate_method_form_is_not_global_entry() {
        // plugin-host берёт on_activate из globals (selftest.rs: `globals().get`):
        // `function M.on_activate` создаёт метод таблицы M, а не глобальную точку
        // входа. Между `function` и `on_activate` стоит `M.` — не чистый whitespace,
        // поэтому эвристика его уже отвергает (фиксируем корректное поведение).
        assert!(!has_on_activate("function M.on_activate(ctx) end"));
        assert!(!has_on_activate("function M:on_activate(ctx) end"));
    }

    #[test]
    fn has_on_activate_field_assignment_is_not_global_entry() {
        // plugin-host берёт on_activate из globals (selftest.rs: `globals().get`):
        // `M.on_activate = function` создаёт поле таблицы M, а не глобальную точку
        // входа, — эвристика не должна его принимать.
        assert!(!has_on_activate("M.on_activate = function(ctx) end"));
    }

    #[test]
    fn check_entry_accepts_double_space_declaration() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("double-space");
        write_plugin(
            &dir,
            &manifest("double-space"),
            Some("function  on_activate(ctx) end\n"),
        );
        let message = check_entry(&dir).expect("двойной пробел допускается");
        assert!(message.contains("on_activate найден"), "msg: {message}");
    }

    #[test]
    fn check_entry_rejects_method_form() {
        // `function M.on_activate` не создаёт глобальную on_activate,
        // которую ждёт plugin-host, — entry должен быть отклонён.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("method-form");
        write_plugin(
            &dir,
            &manifest("method-form"),
            Some("function M.on_activate(ctx) end\n"),
        );
        let err = check_entry(&dir).unwrap_err();
        assert!(err.contains("on_activate"), "err: {err}");
    }

    #[test]
    fn check_entry_rejects_field_assignment_form() {
        // `M.on_activate = function` создаёт поле таблицы M, а не глобальную
        // on_activate, которую ждёт plugin-host, — entry должен быть отклонён.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("field-assignment");
        write_plugin(
            &dir,
            &manifest("field-assignment"),
            Some("M.on_activate = function(ctx) end\n"),
        );
        let err = check_entry(&dir).unwrap_err();
        assert!(err.contains("on_activate"), "err: {err}");
    }
}
