//! Регистрация host-функций, доступных плагину.
//!
//! В P1 реализован минимум: `host.log` и перенаправление `print`.
//! Документные/View-функции и проверка permissions — предметы P5.

use mlua::{Function, Lua, LuaString, Result, Value, Variadic};

/// Регистрирует таблицу `host` и переопределяет `print`.
pub fn register(lua: &Lua, plugin_id: &str) -> Result<()> {
    let host = lua.create_table()?;
    host.set("plugin_id", plugin_id)?;
    host.set(
        "log",
        lua.create_function(|_, (level, message): (String, String)| {
            log_message(&level, &message);
            Ok(())
        })?,
    )?;
    lua.globals().set("host", host)?;

    // print перенаправляем в host.log: иначе вывод плагина уходит в stdout хоста (DESIGN §6.1).
    let tostring: Function = lua.globals().get("tostring")?;
    lua.globals().set(
        "print",
        lua.create_function(move |_, args: Variadic<Value>| {
            let mut parts = Vec::with_capacity(args.len());
            for value in args {
                parts.push(tostring.call::<LuaString>(value)?.to_string_lossy());
            }
            log_message("info", &parts.join("\t"));
            Ok(())
        })?,
    )?;

    Ok(())
}

/// Пишет сообщение плагина в stderr хоста (DESIGN §6.3).
pub fn log_message(level: &str, message: &str) {
    eprintln!("[{level}] {message}");
}
