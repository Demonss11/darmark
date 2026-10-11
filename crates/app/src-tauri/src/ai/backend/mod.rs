//! Реализации шва [`crate::ai::transport::AiTransport`].
//!
//! В MVP — единственная реализация `reqwest`-бэкенда в процессе GUI (ADR-0034 §2.1). Сетевой
//! слой собирается **только на Windows** (ADR-0033 §2.1): системный TLS — SChannel, без OpenSSL.

#[cfg(windows)]
pub mod reqwest;
