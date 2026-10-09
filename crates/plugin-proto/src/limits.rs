//! Бюджеты и лимиты, общие для GUI-хоста и child-процесса (Фаза 0 плагинной системы).
//!
//! Живут в `plugin-proto` (единственный крейт, видимый и хосту, и child): константы watchdog
//! потребляет Supervisor в `src-tauri`, который по правилу границ (§2 плагинной системы) **не** линкует
//! `plugin-host`. In-process лимиты прототипа (`lua-proto/src/limits.rs`: memory-limit +
//! instruction-hook, F9/F14) не переносятся — в child их роль выполняют Job Object + watchdog
//! (ADR-0021 §2.4).

/// Прогресс-таймаут invocation, мс (F33): нет **любого** события от child за бюджет → снятие
/// (`TerminateJobObject`). Сбрасывается каждым кадром child. Референс из прогонов.
pub const PROGRESS_TIMEOUT_MS: u64 = 400;

/// Абсолютный wall-clock дедлайн invocation, мс (F35): **не** сбрасывается событиями, ловит
/// «chatty»-плагин, бесконечно дёргающий быстрые host-вызовы.
pub const DEADLINE_MS: u64 = 1500;

/// Лимит размера `.lua`-исходника перед загрузкой (§10.1 DESIGN_DOC).
pub const MAX_PLUGIN_SOURCE_BYTES: u64 = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_budget_is_smaller_than_deadline() {
        // Через локальные привязки: clippy ругается на ассерты по константам напрямую.
        let progress = PROGRESS_TIMEOUT_MS;
        let deadline = DEADLINE_MS;
        assert!(progress < deadline);
    }
}
