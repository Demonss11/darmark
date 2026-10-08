//! Песочница: Lua-состояние с allowlist стандартных библиотек (D17/§6.1 DESIGN_DOC).
//!
//! Набор задаётся явно: без `OS`/`IO` (нет ФС и времени), без `PACKAGE` (нет `require`),
//! без `DEBUG` (hook ставится через C API в прототипе) и без `COROUTINE` (корутина обошла бы
//! watchdog). Базовая библиотека в `mlua` 0.12 грузится **неявно** и флага `BASE` нет (F7),
//! поэтому [`harden_base`] чистит уже загруженную базу.
//!
//! Перенесено из `lua-proto/src/sandbox.rs`, дополнено чисткой `rawget`/`rawset` (D17).

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

/// Чистка базы: убираем динамическую загрузку кода, ручной сборщик мусора и доступ к
/// «сырым» операциям таблиц (D17 — `rawget`/`rawset` позволяют обойти метаметоды-защиту).
fn harden_base(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    for name in [
        "load",
        "loadfile",
        "dofile",
        "collectgarbage",
        "rawget",
        "rawset",
    ] {
        globals.set(name, Value::Nil)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_nil(lua: &Lua, name: &str) {
        let value: Value = lua.globals().get(name).unwrap();
        assert_eq!(value, Value::Nil, "глобал {name} должен быть удалён");
    }

    #[test]
    fn forbidden_globals_are_nil() {
        let lua = create().unwrap();
        for name in [
            "os",
            "io",
            "require",
            "package",
            "debug",
            "coroutine",
            "load",
            "loadfile",
            "dofile",
            "collectgarbage",
            "rawget",
            "rawset",
        ] {
            assert_nil(&lua, name);
        }
    }

    #[test]
    fn allowed_libraries_are_present() {
        let lua = create().unwrap();
        let string_lib: Value = lua.globals().get("string").unwrap();
        let table_lib: Value = lua.globals().get("table").unwrap();
        let math_lib: Value = lua.globals().get("math").unwrap();
        assert!(matches!(string_lib, Value::Table(_)));
        assert!(matches!(table_lib, Value::Table(_)));
        assert!(matches!(math_lib, Value::Table(_)));
    }

    #[test]
    fn sandbox_can_evaluate_expression() {
        let lua = create().unwrap();
        let sum: i64 = lua.load("return 2 + 3").eval().unwrap();
        assert_eq!(sum, 5);
    }
}
