//! Реализации шва [`crate::ai::transport::AiTransport`].
//!
//! В MVP — единственная реализация `reqwest`-бэкенда в процессе GUI (ADR-0034 §2.1). TLS —
//! системный, SChannel, без OpenSSL (ADR-0033 §2.1).

pub mod reqwest;
