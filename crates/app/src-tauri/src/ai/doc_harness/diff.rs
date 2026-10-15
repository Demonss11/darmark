use crate::ai::types::AiEditOp;
use crate::error::{CommandError, ErrorCode};

/// Верхний предел числа строк изменённого блока (ADR-0032): защита от гигантских правок.
pub const MAX_DIFF_LINES: usize = 10_000;

/// Считает минимальный дифф `original → modified` (байтовые смещения UTF-8).
///
/// Возвращает **один** [`AiEditOp`] на изменённый блок строк (или пустой список, если тексты
/// совпадают). Общий префикс/суффикс считается по строкам, поэтому смещения всегда лежат на
/// границах UTF-8 (символ не режется).
pub fn diff_ops(original: &str, modified: &str) -> Vec<AiEditOp> {
    if original == modified {
        return Vec::new();
    }
    let old_lines = line_spans(original);
    let new_lines = line_spans(modified);
    let mut prefix = 0;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && line(original, old_lines[prefix]) == line(modified, new_lines[prefix])
    {
        prefix += 1;
    }
    let max_suffix = (old_lines.len() - prefix).min(new_lines.len() - prefix);
    let mut suffix = 0;
    while suffix < max_suffix
        && line(original, old_lines[old_lines.len() - 1 - suffix])
            == line(modified, new_lines[new_lines.len() - 1 - suffix])
    {
        suffix += 1;
    }

    let start = line_start(original, &old_lines, prefix);
    let end = if suffix == 0 {
        original.len()
    } else {
        old_lines[old_lines.len() - suffix].0
    };
    let new_start = line_start(modified, &new_lines, prefix);
    let new_end = if suffix == 0 {
        modified.len()
    } else {
        new_lines[new_lines.len() - suffix].0
    };

    vec![AiEditOp {
        start,
        end,
        text: modified[new_start..new_end].to_string(),
    }]
}

/// Дифф с проверкой лимита размера (ADR-0032): изменённый блок больше [`MAX_DIFF_LINES`]
/// строк → `ai_bad_request`. Дифф считается всегда **одним** [`AiEditOp`], поэтому
/// проверяется единственный блок (заменённый фрагмент и его новая версия).
pub fn checked_diff_ops(original: &str, modified: &str) -> Result<Vec<AiEditOp>, CommandError> {
    let ops = diff_ops(original, modified);
    if let Some(op) = ops.first() {
        let added = op.text.lines().count();
        let removed = original[op.start..op.end].lines().count();
        if added.max(removed) > MAX_DIFF_LINES {
            return Err(CommandError::new(
                ErrorCode::AiBadRequest,
                format!("изменённый блок больше {MAX_DIFF_LINES} строк"),
            ));
        }
    }
    Ok(ops)
}

/// Байтовые границы строк `[start, end)` (end — за `\n`, если он есть).
///
/// Пустой текст даёт одну пустую строку; текст с завершающим `\n` — виртуальную пустую
/// строку в конце (симметрично для оригинала и правки — суффикс сходится корректно).
fn line_spans(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            spans.push((start, index + 1));
            start = index + 1;
        }
    }
    spans.push((start, bytes.len()));
    spans
}

/// Начало строки с индексом `index` (или конец текста, если индекс за пределами).
fn line_start(text: &str, spans: &[(usize, usize)], index: usize) -> usize {
    if index < spans.len() {
        spans[index].0
    } else {
        text.len()
    }
}

/// Срез строки по её байтовым границам.
fn line(text: &str, span: (usize, usize)) -> &str {
    &text[span.0..span.1]
}
