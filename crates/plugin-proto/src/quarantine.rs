//! Состояние карантина одного плагина (ADR-0021 §3, §10.2 DESIGN_DOC).
//!
//! После `threshold` **подряд** неудач плагин автоотключается до ручного включения. Успешный
//! запуск сбрасывает серию. Состояние per-plugin живёт в хосте (плагин stateless, §6.2).
//!
//! Перенесено из `lua-rpc-spike/src/lib.rs` (F37) в отдельный модуль с тестами.

/// Карантин: счётчик последовательных неудач и флаг отключения.
#[derive(Debug, Clone)]
pub struct Quarantine {
    threshold: u32,
    consecutive_failures: u32,
    disabled: bool,
}

impl Quarantine {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            consecutive_failures: 0,
            disabled: false,
        }
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Успешный запуск: серия неудач прервана.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Неудача запуска (краш, зависание, отказ): при накоплении `threshold` — карантин.
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.threshold {
            self.disabled = true;
        }
    }

    /// Ручное включение после карантина: счётчик и флаг сбрасываются.
    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
        self.disabled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disables_after_threshold() {
        let mut q = Quarantine::new(3);
        q.record_failure();
        q.record_failure();
        assert!(!q.is_disabled());
        assert_eq!(q.consecutive_failures(), 2);
        q.record_failure();
        assert!(q.is_disabled(), "после 3 неудач подряд — карантин");
    }

    #[test]
    fn success_resets_series() {
        let mut q = Quarantine::new(3);
        q.record_failure();
        q.record_failure();
        q.record_success();
        assert_eq!(q.consecutive_failures(), 0);
        q.record_failure();
        assert!(!q.is_disabled(), "серия прервана успехом");
    }

    #[test]
    fn manual_reset_reenables() {
        let mut q = Quarantine::new(2);
        q.record_failure();
        q.record_failure();
        assert!(q.is_disabled());
        q.reset();
        assert!(!q.is_disabled());
        assert_eq!(q.consecutive_failures(), 0);
    }

    #[test]
    fn zero_threshold_clamped_to_one() {
        let mut q = Quarantine::new(0);
        q.record_failure();
        assert!(q.is_disabled());
    }
}
