//! Тесты модуля `mcp`: конверт события `log-entry` и разрешение каталога плагинов.
//!
//! `AppHandle` без Tauri-runtime не создать, поэтому прод-сток параметризован трейтом
//! [`LogEmitter`](super::LogEmitter): тест подставляет собирающий mock и проверяет ровно
//! тот конверт `{level, category, message}`, что уходит в продакшене (TZ-MCP R2/R4).
//! [`catalog_repo`](super::catalog_repo) тоже не зависит от Tauri: база каталога — аргумент.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::catalog_repo;
use super::{app_log_sink, LogEmitter};

/// Готовит валидный по форме каталог `<base>/plugins/index.json` и возвращает каталог плагинов.
///
/// `discover_repo` проверяет только наличие `index.json`, но тест держит каталог
/// согласованным с продуктом (без повторного парсинга индекса).
fn write_plugins_index(base: &Path) -> PathBuf {
    let repo = base.join("plugins");
    std::fs::create_dir_all(&repo).expect("каталог plugins");
    std::fs::write(repo.join("index.json"), b"{}").expect("index.json");
    repo
}

/// Меняет `DARMARK_PLUGIN_REPO` на время замыкания и восстанавливает прежнее значение.
///
/// Переменные процесса общие: доступ сериализован общим env-локом `ai` (как в других тестах).
fn with_env_override<R>(value: Option<&Path>, f: impl FnOnce() -> R) -> R {
    let _guard = crate::ai::history::env_lock();
    let previous = std::env::var_os("DARMARK_PLUGIN_REPO");
    match value {
        Some(path) => std::env::set_var("DARMARK_PLUGIN_REPO", path),
        None => std::env::remove_var("DARMARK_PLUGIN_REPO"),
    }
    let result = f();
    match previous {
        Some(prev) => std::env::set_var("DARMARK_PLUGIN_REPO", prev),
        None => std::env::remove_var("DARMARK_PLUGIN_REPO"),
    }
    result
}

/// Восстанавливает CWD при выходе из теста — даже при панике.
struct CwdGuard(PathBuf);

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// Mock-эмиттер: захватывает пары `(event, payload)`, проверки идут после вызова стока.
///
/// Разделяемое состояние за `Arc`: экземпляр передаётся в сток по значению, а проверяющая
/// половина остаётся у теста.
#[derive(Clone, Default)]
struct MockEmitter {
    entries: Arc<Mutex<Vec<(String, Value)>>>,
}

impl LogEmitter for MockEmitter {
    fn emit(&self, event: &str, payload: Value) -> Result<(), String> {
        self.entries
            .lock()
            .expect("журнал теста не отравлен")
            .push((event.to_string(), payload));
        Ok(())
    }
}

#[test]
fn mcp_log_entry_has_valid_level_category_message() {
    let emitter = MockEmitter::default();
    let sink = app_log_sink(emitter.clone());

    sink("MCP markdown_render: 12 символов — ok (1 мс)");

    let entries = emitter.entries.lock().expect("журнал не отравлен");
    assert_eq!(
        entries.len(),
        1,
        "каждый вызов стока — одна запись: {entries:?}"
    );
    let (event, payload) = &entries[0];
    assert_eq!(event, "log-entry", "имя события журнала");
    assert!(payload["level"].is_string(), "level — строка: {payload}");
    assert_eq!(
        payload["level"], "info",
        "MCP-записи идут уровнем info: {payload}"
    );
    assert!(
        payload["category"].is_string(),
        "category — строка: {payload}"
    );
    assert_eq!(
        payload["category"], "system",
        "категория MCP-записей: {payload}"
    );
    assert!(
        payload["message"].is_string(),
        "message — строка: {payload}"
    );
    assert_eq!(
        payload["message"], "MCP markdown_render: 12 символов — ok (1 мс)",
        "сообщение дословно из стока: {payload}"
    );
}

/// Env `DARMARK_PLUGIN_REPO` важнее и автопоиска, и fallback.
#[test]
fn catalog_repo_env_override_wins() {
    let app_dir = tempfile::tempdir().unwrap();
    write_plugins_index(app_dir.path());

    let override_dir = tempfile::tempdir().unwrap();
    let resolved = with_env_override(Some(override_dir.path()), || catalog_repo(app_dir.path()));

    assert_eq!(
        resolved,
        override_dir.path(),
        "env-каталог имеет приоритет над app_data_dir"
    );
}

/// Без env автопоиск от `app_data_dir` находит `<app_data_dir>/plugins`.
#[test]
fn catalog_repo_discovers_plugins_under_app_data_dir() {
    let app_dir = tempfile::tempdir().unwrap();
    let repo = write_plugins_index(app_dir.path());

    let resolved = with_env_override(None, || catalog_repo(app_dir.path()));

    assert_eq!(resolved, repo, "автопоиск нашёл plugins под app_data_dir");
}

/// Без env и без каталога — fallback `<app_data_dir>/plugins`, и CWD на результат не влияет.
#[test]
fn catalog_repo_ignores_cwd_when_no_catalog_found() {
    let app_dir = tempfile::tempdir().unwrap();

    // В CWD — свой каталог с `plugins/index.json`: старая (CWD-based) реализация вернула бы его.
    let cwd_dir = tempfile::tempdir().unwrap();
    write_plugins_index(cwd_dir.path());

    let original = std::env::current_dir().expect("текущий каталог");
    std::env::set_current_dir(cwd_dir.path()).expect("перейти в cwd-каталог");
    let _cwd_guard = CwdGuard(original);

    let resolved = with_env_override(None, || catalog_repo(app_dir.path()));

    assert_eq!(
        resolved,
        app_dir.path().join("plugins"),
        "fallback считается от app_data_dir, а не от CWD"
    );
    assert_ne!(
        resolved,
        cwd_dir.path().join("plugins"),
        "каталог рядом с CWD не используется"
    );
}
