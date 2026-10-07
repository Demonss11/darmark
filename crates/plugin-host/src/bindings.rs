//! Функции, доступные Lua-плагину (Фаза 1 TZ-H2, §6.3 DESIGN_DOC).
//!
//! `host.*` документные вызовы идут через [`HostApi`] — в child это RPC к GUI-хосту
//! (`ToHost::HostCall` ↔ `ToChild::Reply`), в тестах — фейк. `md.*` и `json.*` считаются
//! **нативно** в child: `md-core` линкуется в child, JSON-конвертация — модуль [`json_conv`].

use std::fmt;
use std::sync::Arc;

use mlua::{Lua, Result, Table, Value, Variadic};
use plugin_proto::envelope::PluginError;
use serde_json::json;

/// Хост-API за границей процесса. Реализация на стороне child шлёт [`ToHost`](plugin_proto::envelope::ToHost)
/// и ждёт reply; проверка permissions и доступ к документу — забота GUI-хоста.
pub trait HostApi: Send + Sync + 'static {
    /// Синхронный host-call. `Err` — отказ (например `permission_denied`), не Lua-исключение на хосте.
    fn call(
        &self,
        method: &str,
        args: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, PluginError>;
    /// Канонический канал лога (`ToHost::Log`).
    fn log(&self, level: &str, message: &str);
    /// Событие жизненного цикла (`ToHost::Event`).
    fn emit(&self, kind: &str, payload: serde_json::Value);
}

/// Логирует через [`HostApi`] с префиксом plugin_id.
struct Logged {
    plugin_id: String,
    api: Arc<dyn HostApi>,
}

impl fmt::Debug for Logged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Logged")
            .field("plugin_id", &self.plugin_id)
            .finish_non_exhaustive()
    }
}

impl Logged {
    fn log(&self, level: &str, message: &str) {
        self.api
            .log(level, &format!("{}: {message}", self.plugin_id));
    }
}

fn host_err(e: PluginError) -> mlua::Error {
    mlua::Error::RuntimeError(e.to_string())
}

/// Превращает результат host-call в Lua-контракт `(value, err)`:
/// успех → `(value, nil)`, отказ → `(nil, {code, message, permission?})`.
///
/// Permission-отказ приходит плагину **значением**, а не Lua-исключением (§4.4 TZ-H2 / §8
/// DESIGN_DOC): `on_activate` не прерывается, плагин сам решает, что делать.
fn defuse(
    lua: &Lua,
    result: std::result::Result<serde_json::Value, PluginError>,
) -> Result<(Value, Value)> {
    match result {
        Ok(value) => Ok((json_to_lua(lua, &value)?, Value::Nil)),
        Err(error) => Ok((Value::Nil, error_to_lua(lua, &error)?)),
    }
}

/// Таблица ошибки host-call для Lua: `{ code, message, permission?, data? }`.
fn error_to_lua(lua: &Lua, error: &PluginError) -> Result<Value> {
    let table = lua.create_table()?;
    table.set("code", error.code.clone())?;
    table.set("message", error.message.clone())?;
    if let Some(permission) = &error.permission {
        table.set("permission", permission.clone())?;
    }
    if let Some(data) = &error.data {
        table.set("data", json_conv::json_to_lua(lua, data).map_err(host_err)?)?;
    }
    Ok(Value::Table(table))
}

/// Регистрирует глобальные таблицы `host`, `md`, `json` и переопределяет `print`.
pub fn register(lua: &Lua, plugin_id: &str, api: Arc<dyn HostApi>) -> Result<()> {
    register_host(lua, plugin_id, Arc::clone(&api))?;
    register_md(lua)?;
    register_json(lua)?;
    Ok(())
}

fn register_host(lua: &Lua, plugin_id: &str, api: Arc<dyn HostApi>) -> Result<()> {
    let log = Arc::new(Logged {
        plugin_id: plugin_id.to_string(),
        api: Arc::clone(&api),
    });

    let host = lua.create_table()?;
    host.set("plugin_id", plugin_id)?;

    let log_for_fn = Arc::clone(&log);
    host.set(
        "log",
        lua.create_function(move |_, (level, message): (String, String)| {
            log_for_fn.log(&level, &message);
            Ok(())
        })?,
    )?;

    let api_show = Arc::clone(&api);
    host.set(
        "show_message",
        lua.create_function(move |lua, text: String| {
            defuse(lua, api_show.call("show_message", json!({ "text": text })))
        })?,
    )?;

    let api_len = Arc::clone(&api);
    host.set(
        "get_document_len",
        lua.create_function(move |lua, doc_id: String| {
            defuse(
                lua,
                api_len.call("get_document_len", json!({ "doc_id": doc_id })),
            )
        })?,
    )?;

    let api_ver = Arc::clone(&api);
    host.set(
        "get_document_version",
        lua.create_function(move |lua, doc_id: String| {
            defuse(
                lua,
                api_ver.call("get_document_version", json!({ "doc_id": doc_id })),
            )
        })?,
    )?;

    let api_range = Arc::clone(&api);
    host.set(
        "get_document_range",
        lua.create_function(move |lua, (doc_id, start, len): (String, i64, i64)| {
            defuse(
                lua,
                api_range.call(
                    "get_document_range",
                    json!({ "doc_id": doc_id, "start": start, "len": len }),
                ),
            )
        })?,
    )?;

    let api_text = Arc::clone(&api);
    host.set(
        "get_document_text",
        lua.create_function(move |lua, doc_id: String| {
            defuse(
                lua,
                api_text.call("get_document_text", json!({ "doc_id": doc_id })),
            )
        })?,
    )?;

    let api_edit = Arc::clone(&api);
    host.set(
        "apply_edit",
        lua.create_function(
            move |lua, (doc_id, start, stop, text): (String, i64, i64, String)| {
                defuse(
                    lua,
                    api_edit.call(
                        "apply_edit",
                        json!({ "doc_id": doc_id, "start": start, "stop": stop, "text": text }),
                    ),
                )
            },
        )?,
    )?;

    let api_get_setting = Arc::clone(&api);
    host.set(
        "get_setting",
        lua.create_function(move |lua, key: String| {
            defuse(
                lua,
                api_get_setting.call("get_setting", json!({ "key": key })),
            )
        })?,
    )?;

    let api_set_setting = Arc::clone(&api);
    host.set(
        "set_setting",
        lua.create_function(move |lua, (key, value): (String, Value)| {
            let encoded = match json_conv::lua_to_json(&value) {
                Ok(encoded) => encoded,
                Err(error) => return defuse(lua, Err(error)),
            };
            defuse(
                lua,
                api_set_setting.call("set_setting", json!({ "key": key, "value": encoded })),
            )
        })?,
    )?;

    // Тест/dev-утилита изоляции отказов: аварийно завершает child-процесс.
    // Только в dev-сборке (не входит в §6.3 DESIGN_DOC и не должна уезжать в релизный child).
    #[cfg(debug_assertions)]
    host.set(
        "crash",
        lua.create_function(|_, ()| -> Result<()> { std::process::abort() })?,
    )?;

    lua.globals().set("host", host)?;

    // print перенаправляем в лог: иначе вывод плагина уходит в stdout хоста (§6.1).
    let tostring: mlua::Function = lua.globals().get("tostring")?;
    let log_for_print = Arc::clone(&log);
    lua.globals().set(
        "print",
        lua.create_function(move |_, args: Variadic<Value>| {
            let mut parts = Vec::with_capacity(args.len());
            for value in args {
                parts.push(tostring.call::<mlua::LuaString>(value)?.to_string_lossy());
            }
            log_for_print.log("info", &parts.join("\t"));
            Ok(())
        })?,
    )?;

    Ok(())
}

fn register_md(lua: &Lua) -> Result<()> {
    let md = lua.create_table()?;
    md.set(
        "to_html",
        lua.create_function(|lua, (text, _opts): (String, Option<Value>)| {
            lua.create_string(md_core::to_html(&text))
        })?,
    )?;
    md.set(
        "to_html_mapped",
        lua.create_function(|lua, (text, _opts): (String, Option<Value>)| {
            lua.create_string(md_core::to_html_mapped(&text))
        })?,
    )?;
    lua.globals().set("md", md)?;
    Ok(())
}

fn register_json(lua: &Lua) -> Result<()> {
    let json = lua.create_table()?;
    json.set(
        "encode",
        lua.create_function(|_, value: Value| {
            let encoded = json_conv::lua_to_json(&value).map_err(host_err)?;
            serde_json::to_string(&encoded).map_err(|e| mlua::Error::RuntimeError(e.to_string()))
        })?,
    )?;
    json.set(
        "decode",
        lua.create_function(|lua, text: String| {
            let parsed: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| mlua::Error::RuntimeError(format!("json.decode: {e}")))?;
            json_conv::json_to_lua(lua, &parsed).map_err(host_err)
        })?,
    )?;
    lua.globals().set("json", json)?;
    Ok(())
}

/// Строит `ctx` для `on_activate`/`on_deactivate`. В Фазе 1 `subscribe`/`unsubscribe` —
/// заглушки (событийная шина — Фаза 4), но вызовы не падают, чтобы эталонный `word-count`
/// грузился уже сейчас.
pub fn make_context(lua: &Lua, plugin_id: &str) -> Result<Table> {
    let ctx = lua.create_table()?;
    ctx.set("plugin_id", plugin_id)?;
    ctx.set("api_version", plugin_proto::HOST_API_VERSION)?;
    ctx.set(
        "subscribe",
        lua.create_function(|_, _args: Variadic<Value>| Ok(()))?,
    )?;
    ctx.set(
        "unsubscribe",
        lua.create_function(|_, _args: Variadic<Value>| Ok(()))?,
    )?;
    Ok(ctx)
}

/// Конвертация JSON → Lua (обёртка над внутренним конвертером для вызывающих извне модуля).
pub fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> Result<Value> {
    json_conv::json_to_lua(lua, value).map_err(host_err)
}

/// Конвертация Lua ↔ JSON без `mlua`-фичи `serialize` (она не включена ради размера child).
mod json_conv {
    use mlua::{Lua, Table, Value};
    use plugin_proto::envelope::PluginError;
    use serde_json::Value as Json;

    /// Lua → JSON. Функции/потоки/userdata не кодируются — ошибка `lua_error`.
    pub fn lua_to_json(value: &Value) -> std::result::Result<Json, PluginError> {
        Ok(match value {
            Value::Nil => Json::Null,
            Value::Boolean(b) => Json::Bool(*b),
            Value::Integer(i) => Json::Number((*i).into()),
            Value::Number(n) => serde_json::Number::from_f64(*n)
                .map(Json::Number)
                .unwrap_or(Json::Null),
            Value::String(s) => Json::String(s.to_string_lossy()),
            Value::Table(t) => table_to_json(t)?,
            _ => {
                return Err(PluginError::lua(
                    "значение нельзя закодировать в JSON (функция/поток/userdata)",
                ))
            }
        })
    }

    fn table_to_json(t: &Table) -> std::result::Result<Json, PluginError> {
        let len = t.raw_len();
        // Непустая таблица с ключами 1..len и без «дыр» — массив; иначе объект.
        if len > 0 {
            let mut arr = Vec::with_capacity(len);
            let mut is_array = true;
            for i in 1..=len {
                let v: Value = t
                    .raw_get(i as i64)
                    .map_err(|e| PluginError::lua(e.to_string()))?;
                if v == Value::Nil {
                    is_array = false;
                    break;
                }
                arr.push(lua_to_json(&v)?);
            }
            if is_array {
                return Ok(Json::Array(arr));
            }
        }
        let mut map = serde_json::Map::new();
        for pair in t.pairs::<Value, Value>() {
            let (k, v) = pair.map_err(|e| PluginError::lua(e.to_string()))?;
            let key = match k {
                Value::String(s) => s.to_string_lossy(),
                Value::Integer(i) => i.to_string(),
                Value::Number(n) => n.to_string(),
                _ => {
                    return Err(PluginError::lua(
                        "ключ таблицы должен быть строкой или числом",
                    ))
                }
            };
            map.insert(key, lua_to_json(&v)?);
        }
        Ok(Json::Object(map))
    }

    /// JSON → Lua.
    pub fn json_to_lua(lua: &Lua, value: &Json) -> std::result::Result<Value, PluginError> {
        Ok(match value {
            Json::Null => Value::Nil,
            Json::Bool(b) => Value::Boolean(*b),
            Json::Number(n) => match n.as_i64() {
                Some(i) => Value::Integer(i),
                None => Value::Number(n.as_f64().unwrap_or(0.0)),
            },
            Json::String(s) => Value::String(
                lua.create_string(s)
                    .map_err(|e| PluginError::lua(e.to_string()))?,
            ),
            Json::Array(items) => {
                let t = lua
                    .create_table()
                    .map_err(|e| PluginError::lua(e.to_string()))?;
                for (i, item) in items.iter().enumerate() {
                    let v = json_to_lua(lua, item)?;
                    t.raw_set((i + 1) as i64, v)
                        .map_err(|e| PluginError::lua(e.to_string()))?;
                }
                Value::Table(t)
            }
            Json::Object(map) => {
                let t = lua
                    .create_table()
                    .map_err(|e| PluginError::lua(e.to_string()))?;
                for (k, item) in map {
                    let v = json_to_lua(lua, item)?;
                    t.raw_set(k.as_str(), v)
                        .map_err(|e| PluginError::lua(e.to_string()))?;
                }
                Value::Table(t)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeHost {
        logs: Mutex<Vec<(String, String)>>,
        calls: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl HostApi for FakeHost {
        fn call(
            &self,
            method: &str,
            args: serde_json::Value,
        ) -> std::result::Result<serde_json::Value, PluginError> {
            self.calls
                .lock()
                .unwrap()
                .push((method.to_string(), args.clone()));
            Ok(match method {
                "get_document_len" => json!(42),
                "get_document_version" => json!(3),
                "get_document_range" => json!("hello"),
                "get_document_text" => json!("full text"),
                "apply_edit" => json!(true),
                "get_setting" => json!(true),
                _ => serde_json::Value::Null,
            })
        }

        fn log(&self, level: &str, message: &str) {
            self.logs
                .lock()
                .unwrap()
                .push((level.to_string(), message.to_string()));
        }

        fn emit(&self, _kind: &str, _payload: serde_json::Value) {}
    }

    fn lua_with_host() -> (Lua, Arc<FakeHost>) {
        let lua = crate::sandbox::create().unwrap();
        let host = Arc::new(FakeHost::default());
        register(&lua, "test-plugin", host.clone()).unwrap();
        (lua, host)
    }

    #[test]
    fn plugin_id_is_exposed() {
        let (lua, _) = lua_with_host();
        let id: String = lua.load("return host.plugin_id").eval().unwrap();
        assert_eq!(id, "test-plugin");
    }

    #[test]
    fn print_and_host_log_route_to_sink() {
        let (lua, host) = lua_with_host();
        lua.load(r#"host.log("warn", "прямо"); print("через", "print")"#)
            .exec()
            .unwrap();
        let logged = host.logs.lock().unwrap();
        assert!(logged
            .iter()
            .any(|(l, m)| l == "warn" && m.contains("прямо")));
        assert!(logged
            .iter()
            .any(|(l, m)| l == "info" && m.contains("через\tprint")));
    }

    #[test]
    fn md_to_html_is_native() {
        let (lua, _) = lua_with_host();
        let html: String = lua
            .load(r##"return md.to_html("# Заголовок")"##)
            .eval()
            .unwrap();
        assert!(html.contains("<h1"), "html: {html}");
    }

    #[test]
    fn document_host_calls_roundtrip() {
        let (lua, host) = lua_with_host();
        let (len, version, range, text, edited): (i64, i64, String, String, bool) = lua
            .load(
                r#"
                local len = host.get_document_len("d")
                local version = host.get_document_version("d")
                local range = host.get_document_range("d", 0, 5)
                local text = host.get_document_text("d")
                local edited = host.apply_edit("d", 0, 1, "x")
                return len, version, range, text, edited
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!((len, version), (42, 3));
        assert_eq!(range, "hello");
        assert_eq!(text, "full text");
        assert!(edited);
        let calls = host.calls.lock().unwrap();
        assert!(calls.iter().any(|(m, _)| m == "apply_edit"));
    }

    /// Хост, всегда отклоняющий host-call (эмулирует отсутствие permission).
    struct DenyingHost;

    impl HostApi for DenyingHost {
        fn call(
            &self,
            _method: &str,
            _args: serde_json::Value,
        ) -> std::result::Result<serde_json::Value, PluginError> {
            Err(PluginError::permission_denied("document:write"))
        }

        fn log(&self, _level: &str, _message: &str) {}
        fn emit(&self, _kind: &str, _payload: serde_json::Value) {}
    }

    #[test]
    fn permission_denied_is_value_not_exception() {
        let lua = crate::sandbox::create().unwrap();
        register(&lua, "p", Arc::new(DenyingHost)).unwrap();

        // Callback продолжается: ошибка приходит вторым значением, а не исключением Lua.
        let (ok, code, permission): (Value, String, String) = lua
            .load(
                r#"
                local ok, err = host.apply_edit("d", 0, 1, "x")
                return ok, err.code, err.permission
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(ok, Value::Nil);
        assert_eq!(code, "permission_denied");
        assert_eq!(permission, "document:write");

        // Обработчик жив: следующий вызов тоже отдаёт значение.
        let second: String = lua
            .load(r#"local _, e = host.get_document_range("d", 0, 1); return e.code"#)
            .eval()
            .unwrap();
        assert_eq!(second, "permission_denied");
    }

    #[test]
    fn json_encode_decode_roundtrip() {
        let (lua, _) = lua_with_host();
        let encoded: String = lua
            .load(r#"return json.encode({ n = 1, list = { "a", "b" }, ok = true })"#)
            .eval()
            .unwrap();
        let decoded: (i64, String, bool) = lua
            .load(format!(
                r#"local t = json.decode({encoded:?}); return t.n, t.list[2], t.ok"#
            ))
            .eval()
            .unwrap();
        assert_eq!(decoded, (1, "b".to_string(), true));
    }

    #[test]
    fn json_encode_rejects_function() {
        let (lua, _) = lua_with_host();
        let err = lua
            .load(r#"return json.encode(print)"#)
            .eval::<String>()
            .unwrap_err();
        assert!(err.to_string().contains("JSON"), "err: {err}");
    }

    #[test]
    fn context_has_version_and_subscribe_noop() {
        let lua = crate::sandbox::create().unwrap();
        let ctx = make_context(&lua, "p").unwrap();
        lua.globals().set("ctx", ctx).unwrap();
        let version: u32 = lua
            .load("ctx.subscribe('document:changed', function() end); return ctx.api_version")
            .eval()
            .unwrap();
        assert_eq!(version, plugin_proto::HOST_API_VERSION);
    }
}
