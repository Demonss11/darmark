use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::state::{DocumentId, DocumentStore};

use crate::plugins::services::DocumentServices;
use crate::plugins::supervisor::Supervisor;

/// Процессные тесты конкурируют за CPU (chatty/hang крутят цикл) и искажают замеры watchdog,
/// поэтому выполняем их строго по одному. Заодно глушим WER-диалоги Windows: иначе `abort`
/// child'а может висеть до ручного закрытия и «крах» превратится в `timeout`.
pub(super) fn serial() -> std::sync::MutexGuard<'static, ()> {
    suppress_windows_error_dialogs();
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn suppress_windows_error_dialogs() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        unsafe {
            SetErrorMode(0x0001 | 0x0002);
        }
    });
}

pub(super) fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("plugins")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("фикстура {}: {e}", path.display()))
}

/// Корень репозитория: `CARGO_MANIFEST_DIR` = `crates/app/src-tauri`, три `parent()` до `<repo>`.
pub(super) fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("корень workspace")
        .to_path_buf()
}

/// Путь к child-бинарнику. Всегда пересобирает его debug-профилем: `host.crash`
/// существует только в debug (`#[cfg(debug_assertions)]`), а mtime-проверка
/// пропускала release-бинарник — тесты молча гоняли его и получали `lua_error`
/// вместо `crashed`. Инкрементальная debug-сборка дешёвая (~0.1 s), корректность
/// важнее скорости.
///
/// Env-override `DARMARK_PLUGIN_HOST_EXE` пропускает сборку (осознанное
/// исключение): путь используется как есть — отвечаете за его профиль сами.
pub(super) fn child_exe() -> PathBuf {
    if let Ok(path) = std::env::var("DARMARK_PLUGIN_HOST_EXE") {
        return PathBuf::from(path);
    }
    let root = workspace_root();
    let status = std::process::Command::new(env!("CARGO"))
        .args(["build", "-p", "plugin-host"])
        .current_dir(&root)
        .status()
        .expect("запуск cargo build -p plugin-host");
    assert!(status.success(), "не удалось собрать plugin-host");
    Supervisor::resolve_child_exe().expect("child exe после сборки")
}

/// Стор с одним документом `doc-1` (id плагины адресуют строкой).
pub(super) fn store_with_doc(text: &str) -> Arc<Mutex<DocumentStore>> {
    let mut store = DocumentStore::default();
    let snap = store.create(text.to_string());
    assert_eq!(snap.id.as_str(), "doc-1");
    Arc::new(Mutex::new(store))
}

pub(super) fn services(store: &Arc<Mutex<DocumentStore>>) -> Arc<dyn crate::plugins::HostServices> {
    Arc::new(DocumentServices::new(Arc::clone(store)))
}

pub(super) fn views() -> Arc<Mutex<crate::plugins::views::PluginViews>> {
    Arc::new(Mutex::new(crate::plugins::views::PluginViews::new()))
}

pub(super) fn doc_rev(store: &Arc<Mutex<DocumentStore>>) -> u64 {
    store
        .lock()
        .unwrap()
        .get(&DocumentId::new("doc-1"))
        .unwrap()
        .rev
}

pub(super) fn valid_manifest(id: &str, api: u32, perms: &[&str]) -> String {
    let perms_json = serde_json::to_string(perms).unwrap();
    format!(
        r#"{{"id":"{id}","name":"{id}","version":"1.0.0","api_version":{api},"entry":"main.lua","permissions":{perms_json}}}"#
    )
}

pub(super) fn write_plugin(root: &Path, id: &str, manifest: &str, lua: Option<&str>) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("plugin.json"), manifest).unwrap();
    if let Some(lua) = lua {
        fs::write(dir.join("main.lua"), lua).unwrap();
    }
}

pub(super) fn doc_text(store: &Arc<Mutex<DocumentStore>>, id: &str) -> String {
    store
        .lock()
        .unwrap()
        .get(&DocumentId::new(id))
        .map(|doc| doc.text.clone())
        .unwrap_or_default()
}

pub(super) fn view_contrib(
    kind: &str,
    title: &str,
    tier: u8,
) -> plugin_core::manifest::ViewContrib {
    plugin_core::manifest::ViewContrib {
        kind: kind.to_string(),
        title: title.to_string(),
        tier,
    }
}

/// Колбэк, пишущий имя сигнала в общий журнал (F41/S2): позволяет проверить, какой именно
/// канал эмитится в каждом сценарии.
pub(super) fn push_log(
    log: &Arc<Mutex<Vec<&'static str>>>,
    event: &'static str,
) -> Arc<dyn Fn() + Send + Sync> {
    let log = Arc::clone(log);
    Arc::new(move || log.lock().unwrap().push(event))
}
