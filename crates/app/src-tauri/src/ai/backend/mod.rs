//! Реализации шва [`crate::ai::transport::AiTransport`].
//!
//! В MVP — единственная реализация `reqwest`-бэкенда в процессе GUI. TLS —
//! системный, SChannel, без OpenSSL.

pub mod reqwest;
