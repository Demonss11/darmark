//! md-core — чистое ядро Markdown → HTML без зависимостей от UI.
//!
//! Используется Tauri-командой `render_markdown`, а в будущем — CLI/TUI/harness.
//! Точность HTML обеспечивает pulldown-cmark (compliant CommonMark) с расширениями:
//! - GFM Tables (пайповые таблицы `| a | b |`)
//! - Strikethrough (`~~text~~`)
//! - Task lists (`- [ ]` / `- [x]`)
//! - Footnotes (`[^1]`)
//! - Heading IDs (`# Title {#custom-id}`)

use pulldown_cmark::{html, Options, Parser};

/// Все расширения, включённые по умолчанию в приложении.
pub const DEFAULT_OPTIONS: Options = Options::empty()
    .union(Options::ENABLE_TABLES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_HEADING_ATTRIBUTES);

/// Рендерит markdown в HTML-фрагмент с расширениями по умолчанию.
///
/// Синтаксические ошибки отсутствуют по построению: невалидный markdown
/// рендерится как обычный текст (CommonMark fallback), поэтому Result не нужен.
pub fn to_html(markdown: &str) -> String {
    to_html_with(markdown, DEFAULT_OPTIONS)
}

/// То же, но с произвольным набором опций (для тестов и будущего harness).
pub fn to_html_with(markdown: &str, options: Options) -> String {
    let parser = Parser::new_ext(markdown, options);
    let mut out = String::with_capacity(markdown.len() + 64);
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_paragraph_and_heading() {
        let html = to_html("# Title\n\nSome **bold** text.");
        assert!(html.contains("<h1"));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn renders_gfm_table() {
        let md = "| Name | Qty |\n|------|----:|\n| Apple | 3 |\n| Pear | 12 |";
        let html = to_html(md);
        assert!(html.contains("<table>"), "html: {html}");
        assert!(html.contains("<th>Name</th>"));
        assert!(
            html.contains("<td style=\"text-align: right\">3</td>"),
            "html: {html}"
        );
        assert!(html.contains("Pear"));
    }

    #[test]
    fn table_without_extension_is_plain_text() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |";
        let html = to_html_with(md, Options::empty());
        assert!(!html.contains("<table>"));
    }

    #[test]
    fn renders_strikethrough_and_tasklist() {
        let html = to_html("~~gone~~\n\n- [x] done\n- [ ] todo");
        assert!(html.contains("<del>gone</del>"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("checked"));
    }

    #[test]
    fn renders_footnotes() {
        let html = to_html("Text[^1]\n\n[^1]: note body");
        assert!(html.contains("footnote"), "html: {html}");
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert_eq!(to_html(""), "");
    }
}
