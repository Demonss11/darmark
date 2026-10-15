//! Harness документа (ADR-0034): минимальный дифф предложенного текста и его применение
//! через `DocumentStore` (D5, ADR-0028).
//!
//! Дифф считается **на Rust-стороне** (TS не парсит Markdown/текст): практичная контракция
//! по общему префиксу/суффиксу **строк** — изменённый блок становится одним
//! [`AiEditOp`](crate::ai::types::AiEditOp) с байтовыми смещениями UTF-8. Применение
//! защищено проверкой ревизии (rev-guard, ADR-0034): устаревший `base_rev` → `ai_stale_edit`,
//! правка не применяется.

mod apply;
mod commands;
mod diff;

#[cfg(test)]
mod tests;

pub use apply::apply_with_guard;
pub use commands::{ai_apply_edit, ai_diff};
pub use diff::{checked_diff_ops, diff_ops, MAX_DIFF_LINES};

pub use commands::{
    __cmd__ai_apply_edit, __cmd__ai_diff, __tauri_command_name_ai_apply_edit,
    __tauri_command_name_ai_diff,
};
