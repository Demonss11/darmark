//! Ресурсные лимиты: память и число инструкций (DESIGN_DOC §10.1).
//!
//! Память: `set_memory_limit` даёт `Error::MemoryError` вместо падения.
//! Инструкции: hook ставится на текущий поток, поэтому `COROUTINE` не загружается.
//! Жизненный цикл hook обёрнут в RAII-гвард [`InstructionLimitGuard`]: он снимает
//! hook на любом выходе (включая ошибку и разворачивание) и сбрасывает счётчик
//! на каждый вызов плагина.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use mlua::{debug::HookTriggers, ffi::lua_State, Error, Lua, Result, VmState};

/// Шаг счётчика инструкций по умолчанию. Компромисс между точностью лимита и оверхедом (M6).
pub const DEFAULT_HOOK_STEP: u32 = 10_000;

extern "C" {
    fn proto_set_limit(limit: u64);
    fn proto_install_hook(state: *mut lua_State, step: i32);
    fn proto_remove_hook(state: *mut lua_State);
    fn proto_install_safe_pcall(state: *mut lua_State);
    fn proto_restore_safe_pcall(state: *mut lua_State);
}

/// Ставит лимит памяти в мегабайтах. `0` — без лимита.
pub fn set_memory_limit_mb(lua: &Lua, mb: u64) -> Result<()> {
    if mb == 0 {
        return Ok(());
    }
    let bytes = (mb as usize).saturating_mul(1024 * 1024);
    lua.set_memory_limit(bytes)?;
    Ok(())
}

/// Текущая занятая память Lua-кучи в килобайтах (метрика M3).
pub fn used_memory_kb(lua: &Lua) -> f64 {
    lua.used_memory() as f64 / 1024.0
}

#[derive(Clone, Copy)]
enum HookKind {
    None,
    Mlua,
    C(*mut lua_State),
}

/// RAII-гвард лимита инструкций.
///
/// `arm_mlua` — штатный `Lua::set_hook` (для контроля пути A / профиля unwind).
/// `arm_c` — нативный C-hook, совместимый с `panic = "abort"`.
///
/// Пока гвард жив, hook взведён; `Drop` снимает его. Счётчик обнуляется при
/// установке, поэтому лимит считается на один вызов плагина.
pub struct InstructionLimitGuard<'a> {
    lua: &'a Lua,
    kind: HookKind,
}

impl<'a> InstructionLimitGuard<'a> {
    /// Ставит mlua-hook (`Lua::set_hook`).
    pub fn arm_mlua(lua: &'a Lua, max_insns: u64, step: u32) -> Result<Self> {
        if max_insns == 0 {
            return Ok(Self {
                lua,
                kind: HookKind::None,
            });
        }
        let step = step.max(1);
        let executed = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&executed);
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(step),
            move |_lua, _debug| {
                let total = counter.fetch_add(step as usize, Ordering::Relaxed) + step as usize;
                if total as u64 > max_insns {
                    return Err(Error::RuntimeError(
                        "instruction_limit_exceeded".to_string(),
                    ));
                }
                Ok(VmState::Continue)
            },
        )?;
        Ok(Self {
            lua,
            kind: HookKind::Mlua,
        })
    }

    /// Ставит нативный C-hook (`src/proto_hook.c`): ошибка поднимается из C-фрейма,
    /// поэтому не пересекает Rust-фрейм и не абортит процесс под `panic = "abort"`.
    pub fn arm_c(lua: &'a Lua, max_insns: u64, step: u32) -> Result<Self> {
        if max_insns == 0 {
            return Ok(Self {
                lua,
                kind: HookKind::None,
            });
        }
        let mut state: *mut lua_State = std::ptr::null_mut();
        unsafe {
            proto_set_limit(max_insns);
            lua.exec_raw::<()>((), |s| {
                state = s;
                proto_install_hook(s, step.max(1) as i32);
                // Scoped safe-pcall: плагин не сможет проглотить лимит через pcall.
                proto_install_safe_pcall(s);
            })?;
        }
        Ok(Self {
            lua,
            kind: HookKind::C(state),
        })
    }
}

impl Drop for InstructionLimitGuard<'_> {
    fn drop(&mut self) {
        match self.kind {
            HookKind::None => {}
            HookKind::Mlua => self.lua.remove_hook(),
            // Снимаем напрямую через C: не заходим в Lua API из Drop,
            // чтобы не аллоцировать и не трогать стек на пути ошибки.
            HookKind::C(state) => unsafe {
                proto_remove_hook(state);
                proto_restore_safe_pcall(state);
            },
        }
    }
}
