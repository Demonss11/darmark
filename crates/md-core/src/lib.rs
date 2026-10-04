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
    sanitize_html(&out)
}

// ─── Санитайзер HTML-вывода pulldown-cmark ────────────────────────────
//
// pulldown-cmark не санитайзит вывод — он только парсит Markdown и генерирует HTML.
// Сырой HTML из CommonMark (например `<div>` внутри текста) и вредоносные атрибуты
// (onerror, javascript: в ссылках) остаются. Здесь мы их чистим.

/// Простой санитайзер: удаляет <script>, on*-атрибуты, javascript:/data:text/html URI.
fn sanitize_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            // Начало тега или комментария
            let saved: String = chars.clone().take(40).collect();
            let saved_lower = saved.to_ascii_lowercase();

            if saved_lower.starts_with("script") || saved_lower.starts_with("/script") {
                // Пропускаем весь <script>...</script>
                let _ = skip_script_block(&mut chars);
                continue;
            }

            if saved_lower.starts_with("!--") {
                // Пропускаем открывающие "!--", затем ищем закрывающие "-->".
                for _ in 0..3 {
                    chars.next();
                }
                if skip_comment(&mut chars) {
                    continue;
                }
                // Если закрывающих не нашли — дошли до конца, выводить нечего.
            }

            // Обычный тег — ищем вредоносные атрибуты
            out.push_str(&sanitize_tag(&mut chars));
        } else {
            out.push(c);
        }
    }
    out
}

/// Пропускает содержимое до закрывающего </script> (case-insensitive).
/// Возвращает true, если тег был закрыт.
fn skip_script_block(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    // Пропускаем содержимое скрипта
    let mut depth = 1;
    loop {
        let c = match chars.next() {
            Some(c) => c,
            None => return false,
        };
        if c == '<' {
            let rest: String = chars.clone().take(8).collect();
            if rest.eq_ignore_ascii_case("/script") {
                depth -= 1;
                if depth <= 0 {
                    // Пропускаем закрывающий тег
                    let _ = chars.next(); // >
                    return true;
                }
            }
        }
    }
}

/// Пропускает HTML-комментарий <!-- ... --> целиком. Возвращает true, если найден.
fn skip_comment(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    // Уже съели <!--, ищем закрывающие --> и съедаем их полностью,
    // чтобы в выводе не осталось «хвоста» из '>'.
    while let Some(c) = chars.next() {
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next(); // второй '-'
            if chars.peek() == Some(&'>') {
                chars.next(); // '>'
            }
            return true;
        }
    }
    false
}

/// Санитайзит один HTML-тег (от < до >), возвращая чистый тег.
/// Работает с chars, которые уже съели '<'.
fn sanitize_tag(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut tag = String::new();
    tag.push('<');

    // Собираем весь тег
    let mut in_quote: Option<char> = None;
    let mut in_script = false;

    // Определяем, это ли <script> тег
    let tag_name: String = chars
        .clone()
        .take_while(|&c| c.is_whitespace() || c.is_alphabetic())
        .collect();
    if tag_name.eq_ignore_ascii_case("script") {
        in_script = true;
    }

    // Определяем, это ли </script>
    let is_closing_script = tag_name.eq_ignore_ascii_case("/script");

    // Собираем до закрывающего >
    let mut all_chars = Vec::new();
    loop {
        let c = match chars.next() {
            Some(c) => c,
            None => {
                // Тег не закрыт — выводим как есть
                for ch in &all_chars {
                    tag.push(*ch);
                }
                return tag;
            }
        };
        tag.push(c);
        all_chars.push(c);

        if in_quote.is_some() {
            if c == in_quote.unwrap() {
                in_quote = None;
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c == '>' {
            break;
        }
    }

    // Если это <script> или </script> — не выводим ничего
    if in_script || is_closing_script {
        return String::new();
    }

    // Удаляем on*-атрибуты и sanitize URLs
    let cleaned = remove_event_handlers(&tag);
    let cleaned = sanitize_urls_in_tag(&cleaned);
    cleaned
}

/// Удаляет on*-атрибуты из тега.
fn remove_event_handlers(tag: &str) -> String {
    let mut result = String::with_capacity(tag.len());
    let tag_lower = tag.to_lowercase();
    let mut i = 0;

    while i < tag.len() {
        // Ищем "on" за пробелом
        if let Some(pos) = tag_lower[i..].find(" on") {
            let abs = i + pos + 1; // +1 для пропуска пробела перед "on"
            let after = &tag[abs + 2..]; // после "on"
            let first_char = after.chars().next();

            if first_char.map_or(false, |c| c.is_ascii_alphabetic()) {
                // Это on*-атрибут! Пропускаем его
                result.push_str(&tag[i..abs]);

                // Пропускаем имя атрибута
                let name_end = after
                    .find(|c: char| c.is_whitespace() || c == '>' || c == '=')
                    .unwrap_or(after.len());
                let pos_after_name = abs + 2 + name_end;

                // Если есть =, пропускаем значение
                if pos_after_name < tag.len() && tag.as_bytes()[pos_after_name] == b'=' {
                    let eq_pos = pos_after_name;
                    let val_start = eq_pos + 1;
                    if val_start < tag.len() {
                        let quote = tag.as_bytes()[val_start];
                        if quote == b'"' || quote == b'\'' {
                            // Пропускаем до закрывающей кавычки
                            let rest = &tag[val_start + 1..];
                            if let Some(close) = rest.find(quote as char) {
                                i = val_start + 1 + close + 1;
                            } else {
                                i = tag.len();
                            }
                        } else {
                            // Значение без кавычек — до пробела или >
                            let rest = &tag[val_start..];
                            let end = rest
                                .find(|c: char| c.is_whitespace() || c == '>')
                                .unwrap_or(rest.len());
                            i = val_start + end;
                        }
                    } else {
                        i = pos_after_name;
                    }
                } else {
                    i = pos_after_name;
                }
                continue;
            }
        }
        result.push(tag.chars().nth(i).unwrap());
        i += 1;
    }

    result
}

/// Санитайзит javascript: и data:text/html в href/src атрибутах тега.
fn sanitize_urls_in_tag(tag: &str) -> String {
    let mut result = String::with_capacity(tag.len());
    let tag_lower = tag.to_lowercase();
    let mut i = 0;

    while i < tag.len() {
        // Ищем href или src
        let target = if let Some(pos) = tag_lower[i..].find("href") {
            Some(("href", i + pos))
        } else if let Some(pos) = tag_lower[i..].find("src") {
            // Проверяем, что это не часть href
            Some(("src", i + pos))
        } else {
            None
        };

        if let Some((attr_name, abs_pos)) = target {
            // Проверяем, что это атрибут (перед ним пробел, после — =)
            let after_attr = &tag_lower[abs_pos + attr_name.len()..];
            if after_attr.starts_with('=') {
                let eq_pos = abs_pos + attr_name.len();
                let val_start = eq_pos + 1;

                if val_start < tag.len() {
                    let quote = tag.as_bytes()[val_start];
                    if quote == b'"' || quote == b'\'' {
                        let quote_char = quote as char;
                        let rest = &tag[val_start + 1..];
                        if let Some(val_end) = rest.find(quote_char) {
                            let url = &rest[..val_end];
                            let url_lower = url.to_lowercase();

                            if url_lower.starts_with("javascript:")
                                || url_lower.starts_with("data:text/html")
                            {
                                result.push_str(&tag[i..val_start]);
                                result.push('\'');
                                result.push('\'');
                                // Пропускаем до закрывающей кавычки
                                i = val_start + 1 + val_end + 1;
                                continue;
                            }
                        }
                    }
                }
            }
        }

        result.push(tag.chars().nth(i).unwrap());
        i += 1;
    }

    result
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

    // ─── P0.2: XSS-санитайзер ──────────────────────────────────────────

    #[test]
    fn strips_img_onerror() {
        let html = to_html("<img src=\"x\" onerror=\"alert(1)\">");
        assert!(!html.contains("onerror"), "html: {html}");
    }

    #[test]
    fn strips_script_tags() {
        let html = to_html("<script>alert('xss')</script>");
        assert!(!html.contains("<script"), "html: {html}");
    }

    #[test]
    fn neutralizes_javascript_href() {
        let md = "[click](javascript:alert(1))";
        let html = to_html(md);
        assert!(!html.to_lowercase().contains("javascript:"), "html: {html}");
    }

    #[test]
    fn neutralizes_data_html_href() {
        let md = "[click](data:text/html,<script>alert(1)</script>)";
        let html = to_html(md);
        assert!(
            !html.to_lowercase().contains("data:text/html"),
            "html: {html}"
        );
    }

    #[test]
    fn preserves_normal_links() {
        let md = "[Tauri](https://tauri.app)";
        let html = to_html(md);
        assert!(html.contains("https://tauri.app"), "html: {html}");
    }

    #[test]
    fn preserves_normal_images() {
        let md = "![alt](https://example.com/img.png)";
        let html = to_html(md);
        assert!(html.contains("https://example.com/img.png"), "html: {html}");
    }

    #[test]
    fn preserves_normal_attributes() {
        let md = r#"[link](http://example.com "Title")"#;
        let html = to_html(md);
        assert!(html.contains("http://example.com"), "html: {html}");
        assert!(html.contains("Title"), "html: {html}");
    }

    #[test]
    fn removes_html_comments_without_leaving_artifacts() {
        let html = to_html("before <!-- hidden --> after");
        assert!(!html.contains("hidden"), "html: {html}");
        assert!(!html.contains("-->"), "html: {html}");
        assert!(
            html.contains("before") && html.contains("after"),
            "html: {html}"
        );
    }
}
