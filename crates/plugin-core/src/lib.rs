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


pub mod job;



pub const HOST_API_VERSION: u32 = 1;







pub const REPORT_SCHEMA_VERSION: u32 = 1;


pub use limits::MAX_PLUGIN_SOURCE_BYTES;
