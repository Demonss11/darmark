//! Песочница: создание Lua-состояния с allowlist стандартных библиотек.
//!
//! Набор задаётся явно (DESIGN_DOC §6.1): без `OS`/`IO` (нет ФС и времени),
//! без `PACKAGE` (нет `require`), без `DEBUG` (hook ставится через C API),
//! без `COROUTINE` (hook ставится на поток — корутина обошла бы лимит инструкций).
//! Базовая библиотека загружается всегда; из неё убираем опасные входы.

use mlua::{Lua, LuaOptions, Result, StdLib, Value};

/// Создаёт изолированное Lua-состояние с урезанным набором библиотек.
pub fn create() -> Result<Lua> {
    let lua = Lua::new_with(
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )?;
    harden_base(&lua)?;
    Ok(lua)
}

/// Чистка `BASE`: убираем динамическую загрузку кода и ручной сборщик мусора.
fn harden_base(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    for name in ["load", "loadfile", "dofile", "collectgarbage"] {
        globals.set(name, Value::Nil)?;
    }
    Ok(())
}
