//! Бюджеты и лимиты, общие для GUI-хоста и child-процесса (Фаза 0 плагинной системы).
//!
//! Живут в `plugin-core` (единственный крейт, видимый и хосту, и child): константы watchdog
//! потребляет Supervisor в `src-tauri`, который по правилу границ (§2 плагинной системы) **не** линкует
//! `plugin-host`. In-process лимиты прототипа (memory-limit +
//! instruction-hook) не переносятся — в child их роль выполняют Job Object + watchdog

pub const PROGRESS_TIMEOUT_MS: u64 = 400;

pub const DEADLINE_MS: u64 = 1500;

pub const MAX_PLUGIN_SOURCE_BYTES: u64 = 1024 * 1024;

pub const DEFAULT_MEM_BYTES: usize = 64 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_budget_is_smaller_than_deadline() {
        let progress = PROGRESS_TIMEOUT_MS;
        let deadline = DEADLINE_MS;
        assert!(progress < deadline);
    }
}
