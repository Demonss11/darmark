//! Продуктовое ядро плагинной подсистемы darmark: транспорт (кадры), serde-конверт
//! хост ↔ child, Job Object, карантин, манифест и формулировки границы изоляции.
//!
//! **Правило границ (плагинная система §2):** этот крейт **не** знает про Tauri и про `mlua`. Он
//! линкуется и в GUI-хост, и в child-процесс. Живой Lua (`mlua`) допустим только в
//! `plugin-host`.

pub mod envelope;
pub mod frame;
pub mod limits;
pub mod manifest;
pub mod notices;
pub mod quarantine;

// Job Object (лимиты RSS/CPU/время) — изоляция child-процесса плагина.
pub mod job;

/// Версия контракта host ↔ плагин (§7.2 DESIGN_DOC). Манифест с `api_version > HOST_API_VERSION`
/// отвергается; удаление/переименование host-функций поднимает мажор.
pub const HOST_API_VERSION: u32 = 1;

/// Версия схемы JSON-отчёта `darmark-plugin test --json` (корневое поле `schema_version`).
///
/// Аддитивные поля не повышают мажор. Константа живёт здесь, потому что её читают **оба**
/// участника контракта: `plugin-cli` пишет поле, GUI-harness сверяет его и при расхождении
/// уходит в fallback (ADR-0034 §2.4). Дубли константы в двух крейтах означали бы тихую
/// рассинхронизацию схемы.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// Лимит размера `.lua`-исходника перед загрузкой (§10.1 DESIGN_DOC).
pub use limits::MAX_PLUGIN_SOURCE_BYTES;
