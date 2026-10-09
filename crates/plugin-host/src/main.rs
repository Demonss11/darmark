//! Child-процесс плагинного хоста darmark (Фазы 1 и 6 плагинной системы).
//!
//! По умолчанию работает как stdio-хост: читает кадры `ToChild` (host → child) из stdin,
//! шлёт `ToHost` (child → host) в stdout. Host-call'ы плагина (`host.get_document_*`,
//! `apply_edit`, `show_message`) идут честным round-trip'ом: `ToHost::HostCall` →
//! ожидание `ToChild::Reply` (аргумент/ответ передаются только как дельта/окно, §4.2 плагинной системы).
//!
//! Режим `--self-test <path>` — dev-проверка песочницы без GUI-хоста.
//!
//! Режим `--serve-plugin <path>` (Фаза 6, §7.1 «Dev»): `<path>` — каталог плагина
//! (`plugin.json` + entry) **или** отдельный `.lua`-файл. Child читает манифест (если каталог)
//! и исходник, делает `load` + `on_activate`, затем входит в **тот же** stdio-цикл конверта,
//! что и без аргументов. stdio здесь **назначено разработчиком** (обычно унаследовано от
//! `darmark --debug-plugin <path>`): host-call'ы плагина обслуживает разработчик-хост, а не GUI.
//! Ошибка загрузки/активации → сообщение в stderr и ненулевой код.
//!
//! GUI-хост (`src-tauri`) **не** линкует этот крейт: child спавнится как внешний exe (D6/ADR-0021).

mod bindings;
mod sandbox;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use mlua::{Function, Table};
use plugin_proto::envelope::{self, PluginError, ToChild, ToHost};
use plugin_proto::manifest::Manifest;
use plugin_proto::MAX_PLUGIN_SOURCE_BYTES;
use serde_json::{json, Value};

use bindings::HostApi;

/// Заглушка plugin_id до Фазы 3 (id берётся из манифеста): хост всегда передаёт его в `load`.
const DEFAULT_PLUGIN_ID: &str = "plugin";

// ─── stdio-транспорт ────────────────────────────────────────────────────

/// Разделяемый stdio-endpoint. За ним сериализуются host-call'ы плагина и служебные кадры.
struct ChildIo {
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    next_id: u32,
}

/// `HostApi` поверх stdio: каждый вызов — `ToHost::HostCall` + блокирующее ожидание `Reply`.
struct RpcHost {
    io: Arc<Mutex<ChildIo>>,
}

impl HostApi for RpcHost {
    fn call(&self, method: &str, args: Value) -> Result<Value, PluginError> {
        // Держим блокировку на весь round-trip: во время host-call верхний цикл не читает stdin,
        // так что следующий кадр гарантированно — ответ хоста.
        let mut io = self.io.lock().unwrap();
        let id = io.next_id;
        io.next_id = io.next_id.wrapping_add(1);
        envelope::write_to_host(
            &mut io.writer,
            &ToHost::HostCall {
                id,
                method: method.to_string(),
                args,
            },
        )
        .map_err(|e| PluginError::protocol(format!("не отправить host-call: {e}")))?;

        match envelope::read_to_child(&mut io.reader) {
            Ok(Ok(ToChild::Reply { id: rid, result })) if rid == id => result,
            Ok(Ok(other)) => Err(PluginError::protocol(format!(
                "ожидался Reply #{id}, получено {other:?}"
            ))),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(PluginError::crashed(format!("stdio-граница закрыта: {e}"))),
        }
    }

    fn log(&self, level: &str, message: &str) {
        let mut io = self.io.lock().unwrap();
        let _ = envelope::write_to_host(
            &mut io.writer,
            &ToHost::Log {
                level: level.to_string(),
                message: message.to_string(),
            },
        );
    }

    fn emit(&self, kind: &str, payload: Value) {
        let mut io = self.io.lock().unwrap();
        let _ = envelope::write_to_host(
            &mut io.writer,
            &ToHost::Event {
                kind: kind.to_string(),
                payload,
            },
        );
    }
}

/// Загруженный плагин: `plugin_id` и единый `ctx` на весь жизненный цикл.
///
/// `ctx` создаётся один раз при `load` и переиспользуется в `on_deactivate`: состояние,
/// накопленное в `on_activate`, должно доживать до деактивации (§6.2 DESIGN_DOC).
struct Loaded {
    plugin_id: String,
    ctx: Table,
}

/// Публикует результат `Invoke` как `ToHost::Event`: `done` при успехе, `error` при отказе.
/// Ответ child'а на `Invoke` — именно событие (у `ToChild::Reply` обратное направление:
/// это хост отвечает на `HostCall`).
fn announce_result(host: &Arc<dyn HostApi>, id: u32, result: Result<Value, PluginError>) {
    match result {
        Ok(value) => host.emit("done", json!({ "id": id, "result": value })),
        Err(error) => host.emit("error", json!({ "id": id, "error": error })),
    }
}

/// Обрабатывает один `Invoke`. Возвращает результат, который публикуется как `ToHost::Event`.
fn handle_invoke(
    lua: &mlua::Lua,
    host: &Arc<dyn HostApi>,
    loaded: &mut Option<Loaded>,
    method: &str,
    args: &Value,
) -> Result<Value, PluginError> {
    match method {
        "load" => {
            let source = args
                .get("source")
                .and_then(Value::as_str)
                .ok_or_else(|| PluginError::protocol("load: отсутствует string-поле source"))?;
            if source.len() as u64 > MAX_PLUGIN_SOURCE_BYTES {
                return Err(PluginError::protocol(format!(
                    "исходник {0} байт превышает лимит {MAX_PLUGIN_SOURCE_BYTES}",
                    source.len()
                )));
            }
            let plugin_id = args
                .get("plugin_id")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_PLUGIN_ID)
                .to_string();

            // Регистрация глобалов выполняется ровно один раз на процесс (перезагрузка = новый child).
            bindings::register(lua, &plugin_id, Arc::clone(host))
                .map_err(|e| PluginError::lua(format!("не зарегистрировать host: {e}")))?;
            lua.load(source)
                .set_name(plugin_id.as_str())
                .exec()
                .map_err(|e| PluginError::lua(format!("{plugin_id}: ошибка загрузки: {e}")))?;

            let ctx = bindings::make_context(lua, &plugin_id)
                .map_err(|e| PluginError::lua(format!("не создать ctx: {e}")))?;
            *loaded = Some(Loaded { plugin_id, ctx });
            host.emit("ready", json!(null));
            Ok(Value::Null)
        }
        "activate" => {
            let loaded = loaded
                .as_ref()
                .ok_or_else(|| PluginError::protocol("activate до load"))?;
            let activate: Function = lua.globals().get("on_activate").map_err(|_| {
                PluginError::lua(format!("{}: отсутствует on_activate", loaded.plugin_id))
            })?;
            activate
                .call::<()>(loaded.ctx.clone())
                .map_err(|e| PluginError::lua(format!("{}: on_activate: {e}", loaded.plugin_id)))?;
            Ok(Value::Null)
        }
        "deactivate" => {
            if let Some(loaded) = loaded.as_ref() {
                if let Ok(deactivate) = lua.globals().get::<Function>("on_deactivate") {
                    deactivate.call::<()>(loaded.ctx.clone()).map_err(|e| {
                        PluginError::lua(format!("{}: on_deactivate: {e}", loaded.plugin_id))
                    })?;
                }
            }
            Ok(Value::Null)
        }
        "event" => {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let payload = args.get("payload").cloned().unwrap_or(Value::Null);
            // Подписчики ctx._subs[name] + глобальный on_event (если объявлен).
            bindings::dispatch_event(lua, loaded.as_ref().map(|l| &l.ctx), &name, &payload)
                .map_err(|e| PluginError::lua(format!("event {name}: {e}")))?;
            Ok(Value::Null)
        }
        "action" => {
            // Обратная маршрутизация из тир-1 view (§9.4): клик по `data-p-*` на
            // контейнере → хост → сюда. Возвращаемый плагином HTML уходит обратно
            // через host.set_view_content и санитизируется на GUI-хосте (D7/§11.1).
            let view_id = args
                .get("view_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let payload = args.get("payload").cloned().unwrap_or(Value::Null);
            if let Ok(handler) = lua.globals().get::<Function>("on_action") {
                let lua_payload = bindings::json_to_lua(lua, &payload)
                    .map_err(|e| PluginError::lua(e.to_string()))?;
                handler
                    .call::<()>((view_id, action, lua_payload))
                    .map_err(|e| PluginError::lua(format!("on_action: {e}")))?;
            }
            Ok(Value::Null)
        }
        other => Err(PluginError::protocol(format!("неизвестный метод: {other}"))),
    }
}

/// stdio-endpoint процесса: назначенные stdin/stdout (в dev-режиме их держит разработчик).
fn stdio_io() -> Arc<Mutex<ChildIo>> {
    Arc::new(Mutex::new(ChildIo {
        reader: Box::new(std::io::stdin()),
        writer: Box::new(std::io::stdout()),
        next_id: 1,
    }))
}

/// Штатный stdio-цикл конверта `ToChild` ↔ `ToHost`.
///
/// Общий для двух режимов: без аргументов (`loaded = None` — хост сам пришлёт `load`/`activate`)
/// и `--serve-plugin` (`loaded = Some` — плагин уже загружен и активирован).
fn run_loop(
    io: Arc<Mutex<ChildIo>>,
    host: Arc<dyn HostApi>,
    lua: mlua::Lua,
    mut loaded: Option<Loaded>,
) -> ExitCode {
    loop {
        let frame = {
            let mut guard = io.lock().unwrap();
            envelope::read_to_child(&mut guard.reader)
        };
        let invoke = match frame {
            Ok(Ok(msg)) => msg,
            Ok(Err(protocol)) => {
                // Host прислал неразбираемый конверт: сообщаем и завершаемся (не паникуем).
                host.emit("error", json!({ "message": protocol.message }));
                return ExitCode::FAILURE;
            }
            // EOF: хост закрыл пайп — штатное завершение.
            Err(_) => return ExitCode::SUCCESS,
        };

        match invoke {
            ToChild::Invoke { id, method, args } => {
                let reply = handle_invoke(&lua, &host, &mut loaded, &method, &args);
                announce_result(&host, id, reply);
            }
            // Reply на верхнем уровне не ожидается: ответы разбирает `RpcHost::call`.
            ToChild::Reply { id, .. } => {
                host.emit(
                    "error",
                    json!({ "message": format!("неожиданный Reply #{id} на верхнем уровне") }),
                );
            }
        }
    }
}

fn run_stdio() -> ExitCode {
    let io = stdio_io();
    let host: Arc<dyn HostApi> = Arc::new(RpcHost {
        io: Arc::clone(&io),
    });

    let lua = match sandbox::create() {
        Ok(lua) => lua,
        Err(e) => {
            eprintln!("[child] не создать Lua-состояние: {e}");
            return ExitCode::FAILURE;
        }
    };

    run_loop(io, host, lua, None)
}

// ─── self-test (dev) ────────────────────────────────────────────────────

/// Хост-заглушка для `--self-test`: лог в stderr, документных вызовов нет.
struct StderrHost;

impl HostApi for StderrHost {
    fn call(&self, method: &str, _args: Value) -> Result<Value, PluginError> {
        Err(PluginError::protocol(format!(
            "self-test: host-call {method} недоступен"
        )))
    }

    fn log(&self, level: &str, message: &str) {
        eprintln!("[{level}] {message}");
    }

    fn emit(&self, _kind: &str, _payload: Value) {}
}

fn plugin_id_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| DEFAULT_PLUGIN_ID.to_string())
}

/// Читает исходник плагина, **сначала** проверяя фактическую длину прочитанного буфера
/// (защита от роста файла между проверкой и чтением — TOCTOU).
fn read_plugin_source(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("не читается {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_PLUGIN_SOURCE_BYTES {
        return Err(format!(
            "плагин больше {} КБ ({} байт)",
            MAX_PLUGIN_SOURCE_BYTES / 1024,
            bytes.len()
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("{}: не UTF-8: {e}", path.display()))
}

fn load_and_activate(path: &Path) -> Result<(), String> {
    let source = read_plugin_source(path)?;
    let plugin_id = plugin_id_of(path);

    let lua = sandbox::create().map_err(|e| format!("не создать Lua-состояние: {e}"))?;
    bindings::register(&lua, &plugin_id, Arc::new(StderrHost))
        .map_err(|e| format!("не зарегистрировать host: {e}"))?;

    lua.load(source.as_str())
        .set_name(plugin_id.as_str())
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

    eprintln!("[child] {plugin_id}: on_activate вызван, sandbox активен");
    Ok(())
}

// ─── dev-режим `--serve-plugin` (Фаза 6) ────────────────────────────────

/// Разбирает `<path>` для `--serve-plugin`: каталог плагина (с `plugin.json`) или `.lua`-файл.
///
/// Возвращает `(plugin_id, source)`. Для каталога манифест валидируется
/// ([`Manifest::from_json`]) **до** чтения `.lua`; `entry` безопасен (его проверяет сам манифест).
fn read_plugin_manifest_source(path: &Path) -> Result<(String, String), String> {
    if path.is_dir() {
        let manifest_path = path.join("plugin.json");
        let bytes = std::fs::read(&manifest_path)
            .map_err(|e| format!("не читается {}: {e}", manifest_path.display()))?;
        let manifest =
            Manifest::from_json(&bytes).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        // `manifest.entry` — безопасное имя `.lua` (валидировано манифестом).
        let entry_path = path.join(&manifest.entry);
        let source = read_plugin_source(&entry_path)?;
        Ok((manifest.id, source))
    } else if path.is_file() {
        let source = read_plugin_source(path)?;
        Ok((plugin_id_of(path), source))
    } else {
        Err(format!(
            "{}: ожидается каталог плагина с plugin.json или .lua-файл",
            path.display()
        ))
    }
}

/// `--serve-plugin <path>`: загрузка + активация плагина с диска и вход в штатный stdio-цикл.
///
/// Назначенный stdio обслуживает разработчик-хост: host-call'ы плагина (`get_document_*`,
/// `apply_edit`, …) уходят в stdin/stdout этого процесса, а не в GUI (§7.1 «Dev»).
fn serve_plugin(path: &Path) -> ExitCode {
    let io = stdio_io();
    let host: Arc<dyn HostApi> = Arc::new(RpcHost {
        io: Arc::clone(&io),
    });

    let (plugin_id, source) = match read_plugin_manifest_source(path) {
        Ok(pair) => pair,
        Err(message) => {
            eprintln!("[child] {message}");
            return ExitCode::FAILURE;
        }
    };

    let lua = match sandbox::create() {
        Ok(lua) => lua,
        Err(e) => {
            eprintln!("[child] не создать Lua-состояние: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Единый диспатч с обычным циклом: `load` регистрирует `host.*` и исполняет исходник,
    // `activate` вызывает `on_activate`. Их host-call'ы уже идут в назначенный stdio.
    let mut loaded: Option<Loaded> = None;
    let load = handle_invoke(
        &lua,
        &host,
        &mut loaded,
        "load",
        &json!({ "source": source, "plugin_id": plugin_id.clone() }),
    );
    if let Err(error) = load {
        eprintln!("[child] {plugin_id}: загрузка не удалась: {error}");
        return ExitCode::FAILURE;
    }
    if let Err(error) = handle_invoke(&lua, &host, &mut loaded, "activate", &Value::Null) {
        eprintln!("[child] {plugin_id}: активация не удалась: {error}");
        return ExitCode::FAILURE;
    }
    eprintln!("[child] {plugin_id}: on_activate вызван, dev-режим serve-plugin");

    run_loop(io, host, lua, loaded)
}

// ─── точка входа ────────────────────────────────────────────────────────

enum Mode {
    Stdio,
    SelfTest(PathBuf),
    ServePlugin(PathBuf),
}

fn parse_mode() -> Result<Mode, String> {
    let mut args = std::env::args().skip(1);
    let mut self_test = None;
    let mut serve_plugin = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--self-test" | "--plugin" => {
                let path = args.next().ok_or_else(|| format!("{arg} требует путь"))?;
                self_test = Some(PathBuf::from(path));
            }
            "--serve-plugin" => {
                let path = args
                    .next()
                    .filter(|p| !p.starts_with('-'))
                    .ok_or_else(|| "--serve-plugin требует путь".to_string())?;
                serve_plugin = Some(PathBuf::from(path));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("неизвестный аргумент: {other}")),
        }
    }
    Ok(match (serve_plugin, self_test) {
        (Some(path), _) => Mode::ServePlugin(path),
        (None, Some(path)) => Mode::SelfTest(path),
        (None, None) => Mode::Stdio,
    })
}

fn print_usage() {
    eprintln!("darmark-plugin-host — child-хост Lua-плагинов darmark");
    eprintln!("  (без аргументов)       stdio-режим: конверт хост ↔ child");
    eprintln!("  --self-test <path>     загрузить и активировать плагин (лог в stderr)");
    eprintln!(
        "  --serve-plugin <path>  каталог плагина (plugin.json) или .lua-файл: load + on_activate,\n\
         \x20                        затем штатный stdio-цикл конверта. stdio назначает разработчик:\n\
         \x20                        host-call'ы плагина обслуживает dev-хост (см. darmark --debug-plugin)"
    );
}

fn main() -> ExitCode {
    match parse_mode() {
        Ok(Mode::Stdio) => run_stdio(),
        Ok(Mode::SelfTest(path)) => match load_and_activate(&path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("[child] ошибка: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Mode::ServePlugin(path)) => serve_plugin(&path),
        Err(message) => {
            eprintln!("[child] ошибка аргументов: {message}");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn self_test_runs_hello_fixture() {
        load_and_activate(&fixture("hello.lua")).expect("hello.lua должен активироваться");
    }

    #[test]
    fn missing_on_activate_is_reported() {
        let err = load_and_activate(&fixture("no-activate.lua")).unwrap_err();
        assert!(err.contains("on_activate"), "err: {err}");
    }

    #[test]
    fn plugin_id_from_stem_for_self_test() {
        assert_eq!(plugin_id_of(Path::new("word-count/main.lua")), "main");
        assert_eq!(plugin_id_of(Path::new("word-count.lua")), "word-count");
    }

    #[test]
    fn serve_plugin_reads_manifest_directory() {
        let (id, source) =
            read_plugin_manifest_source(&fixture("serve-dir")).expect("каталог плагина");
        assert_eq!(id, "serve-dir", "id берётся из манифеста");
        assert!(
            source.contains("on_activate"),
            "исходник прочитан: {source}"
        );
    }

    #[test]
    fn serve_plugin_reads_lua_file() {
        let (id, source) = read_plugin_manifest_source(&fixture("hello.lua")).expect(".lua-файл");
        assert_eq!(id, "hello", "id — из имени файла");
        assert!(source.contains("on_activate"));
    }

    #[test]
    fn serve_plugin_missing_path_is_error() {
        let err = read_plugin_manifest_source(&fixture("нет-такого")).unwrap_err();
        assert!(
            err.contains("plugin.json") || err.contains(".lua"),
            "err: {err}"
        );
    }

    /// Готовит child-состояние с загруженным плагином из `source`.
    fn loaded_lua(source: &str) -> (mlua::Lua, Arc<dyn HostApi>, Option<Loaded>) {
        let lua = sandbox::create().unwrap();
        let host: Arc<dyn HostApi> = Arc::new(StderrHost);
        let mut loaded = None;
        handle_invoke(
            &lua,
            &host,
            &mut loaded,
            "load",
            &json!({ "source": source, "plugin_id": "p" }),
        )
        .expect("load");
        (lua, host, loaded)
    }

    #[test]
    fn action_invokes_on_action_handler() {
        let (lua, host, mut loaded) = loaded_lua(
            "function on_activate(ctx) end\n\
             function on_action(view_id, action, payload) seen = {view_id, action, payload} end",
        );
        handle_invoke(
            &lua,
            &host,
            &mut loaded,
            "action",
            &json!({ "view_id": "p:main", "action": "inc", "payload": { "n": 1 } }),
        )
        .expect("action");

        let (view_id, action, n): (String, String, i64) = lua
            .load("return seen[1], seen[2], seen[3].n")
            .eval()
            .unwrap();
        assert_eq!(view_id, "p:main");
        assert_eq!(action, "inc");
        assert_eq!(n, 1);
    }

    #[test]
    fn action_without_handler_is_noop() {
        let (lua, host, mut loaded) = loaded_lua("function on_activate(ctx) end");
        let value = handle_invoke(
            &lua,
            &host,
            &mut loaded,
            "action",
            &json!({ "view_id": "p:main", "action": "x", "payload": null }),
        )
        .expect("action");
        assert_eq!(value, Value::Null);
    }

    #[test]
    fn event_reaches_ctx_subscriber() {
        let (lua, host, mut loaded) = loaded_lua(
            "function on_activate(ctx)\n\
               ctx.subscribe('document:changed', function(name, payload) hits = (hits or 0) + 1 end)\n\
             end",
        );
        handle_invoke(&lua, &host, &mut loaded, "activate", &Value::Null).expect("activate");
        handle_invoke(
            &lua,
            &host,
            &mut loaded,
            "event",
            &json!({ "name": "document:changed", "payload": { "doc_id": "d", "rev": 1 } }),
        )
        .expect("event");

        let hits: i64 = lua.globals().get("hits").unwrap();
        assert_eq!(hits, 1, "подписчик ctx обязан получить событие");
    }
}
