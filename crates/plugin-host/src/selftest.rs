//! Самопроверка плагина в изолированной песочнице (без GUI-хоста).
//!
//! Используется бинарником `darmark-plugin-host --self-test` и CLI `darmark-plugin test`:
//! читает плагин (каталог с манифестом или `.lua`-файл), загружает его в песочницу,
//! вызывает `on_activate`/`on_deactivate` с хостом-заглушкой (host-call'ы отклоняются,
//! логи/события копятся в отчёте).

use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

use mlua::Function;
use plugin_proto::envelope::PluginError;
use serde_json::Value;

use crate::{bindings, sandbox};

/// Лимит размера `plugin.json` при самопроверке (совпадает с GUI-сканированием, §6.6).
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// Имя файла манифеста в каталоге плагина.
const MANIFEST_FILE: &str = "plugin.json";

/// Отчёт о самопроверке плагина.
#[derive(Debug, Clone, PartialEq)]
pub struct SelfTestReport {
    /// id плагина (из манифеста или из имени `.lua`-файла).
    pub plugin_id: String,
    /// Размер исходника в байтах.
    pub source_bytes: usize,
    /// `on_activate` вызван без ошибок.
    pub activated: bool,
    /// Логи и события песочницы (host.log/print/emit).
    pub logs: Vec<String>,
}

/// Хост-заглушка: host-call'ы отклоняются, логи/события копятся в `logs`.
struct LogHost {
    logs: Arc<Mutex<Vec<String>>>,
}

impl bindings::HostApi for LogHost {
    fn call(&self, method: &str, _args: Value) -> Result<Value, PluginError> {
        Err(PluginError::protocol(format!(
            "self-test: host-call {method} недоступен"
        )))
    }

    fn log(&self, level: &str, message: &str) {
        self.logs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(format!("[{level}] {message}"));
    }

    fn emit(&self, kind: &str, payload: Value) {
        let line = format!(
            "[event:{kind}] {}",
            serde_json::to_string(&payload).unwrap_or_else(|_| "?".to_string())
        );
        self.logs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line);
    }
}

/// Проверяет плагин в песочнице: чтение (манифест/entry), `load`, `on_activate`,
/// `on_deactivate` (если объявлена). host-call'ы отклоняются с понятным сообщением.
pub fn self_test(path: &Path) -> Result<SelfTestReport, String> {
    let (plugin_id, source_bytes, source) = resolve_source(path)?;

    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let host: Arc<dyn bindings::HostApi> = Arc::new(LogHost {
        logs: Arc::clone(&logs),
    });

    let lua = sandbox::create().map_err(|e| format!("не создать Lua-состояние: {e}"))?;
    bindings::register(&lua, &plugin_id, host)
        .map_err(|e| format!("не зарегистрировать host: {e}"))?;

    lua.load(&source)
        .set_name(&plugin_id)
        .exec()
        .map_err(|e| format!("ошибка загрузки {plugin_id}: {e}"))?;

    let activate: Function = lua
        .globals()
        .get("on_activate")
        .map_err(|_| format!("{plugin_id}: отсутствует on_activate"))?;
    let ctx =
        bindings::make_context(&lua, &plugin_id).map_err(|e| format!("не создать ctx: {e}"))?;
    // Один ctx на весь цикл: состояние из on_activate доживает до on_deactivate (§6.2).
    activate
        .call::<()>(ctx.clone())
        .map_err(|e| format!("{plugin_id}: on_activate упал: {e}"))?;

    if let Ok(deactivate) = lua.globals().get::<Function>("on_deactivate") {
        deactivate
            .call::<()>(ctx)
            .map_err(|e| format!("{plugin_id}: on_deactivate упал: {e}"))?;
    }

    // Забираем логи: Arc может быть подержан замыканиями в Lua, поэтому клонируем содержимое.
    let logs = logs.lock().unwrap_or_else(|e| e.into_inner()).clone();

    Ok(SelfTestReport {
        plugin_id,
        source_bytes,
        activated: true,
        logs,
    })
}

/// Читает исходник плагина (каталог → манифест + entry; `.lua`-файл → сам файл).
/// Возвращает `(plugin_id, source_bytes, source)`.
fn resolve_source(path: &Path) -> Result<(String, usize, String), String> {
    if path.is_dir() {
        let manifest_path = path.join(MANIFEST_FILE);
        let bytes = read_capped(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest = plugin_proto::manifest::Manifest::from_json(&bytes)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let entry_path = path.join(&manifest.entry);
        let source = crate::read_plugin_source(&entry_path)?;
        Ok((manifest.id, source.len(), source))
    } else if path.is_file() {
        let source = crate::read_plugin_source(path)?;
        Ok((crate::plugin_id_of(path), source.len(), source))
    } else {
        Err(format!(
            "{}: ожидается каталог плагина с plugin.json или .lua-файл",
            path.display()
        ))
    }
}

/// Читает файл с жёстким потолком размера (защита от TOCTOU и OOM).
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
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn self_test_activates_hello_fixture() {
        let report = self_test(&fixture("hello.lua")).expect("hello.lua должен активироваться");
        assert!(report.activated);
        assert_eq!(report.plugin_id, "hello");
        assert!(report.source_bytes > 0);
    }

    #[test]
    fn self_test_rejects_missing_on_activate() {
        let err = self_test(&fixture("no-activate.lua")).unwrap_err();
        assert!(err.contains("on_activate"), "err: {err}");
    }
}
