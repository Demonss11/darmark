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

    // Удаляем on*-атрибуты и нейтрализуем опасные URL в href/src
    rewrite_tag(&tag)
}

/// Символы, допустимые в имени атрибута/тега (до разделителя или конца).
fn is_name_char(c: char) -> bool {
    !(c.is_whitespace() || c == '=' || c == '>' || c == '/')
}

/// Переписывает один тег: выкидывает on*-атрибуты и нейтрализует
/// `javascript:`/`data:text/html` в `href`/`src`.
///
/// Работает по Vec<char>, а не по байтовым индексам: в предыдущей реализации
/// `find`/`as_bytes` давали байтовые позиции, а вывод шёл через `chars().nth(i)`,
/// из-за чего кириллица в атрибуте (`title="заголовок"`) уводила индекс
/// в середину многобайтового символа и роняла процесс (panic «not a char boundary»).
fn rewrite_tag(tag: &str) -> String {
    let ch: Vec<char> = tag.chars().collect();
    let n = ch.len();

    // Не тег (нет ведущего '<') — возвращаем как есть.
    if n == 0 || ch[0] != '<' {
        return tag.to_string();
    }

    let mut out = String::with_capacity(tag.len());
    out.push('<');
    let mut i = 1;

    // Закрывающий слэш: </tag>
    if i < n && ch[i] == '/' {
        out.push('/');
        i += 1;
    }

    // Имя тега.
    while i < n && is_name_char(ch[i]) {
        out.push(ch[i]);
        i += 1;
    }

    // Атрибуты.
    while i < n {
        let mut ws = String::new();
        while i < n && ch[i].is_whitespace() {
            ws.push(ch[i]);
            i += 1;
        }
        if i >= n {
            break;
        }
        if ch[i] == '>' {
            out.push('>');
            break;
        }
        // Самозакрывающийся конец: ... />
        if ch[i] == '/' {
            out.push_str(&ws);
            out.push('/');
            i += 1;
            while i < n && ch[i].is_whitespace() {
                i += 1;
            }
            if i < n && ch[i] == '>' {
                out.push('>');
            }
            break;
        }

        // Имя атрибута.
        let name_start = i;
        while i < n && is_name_char(ch[i]) {
            i += 1;
        }
        let name: String = ch[name_start..i].iter().collect();
        let name_lower = name.to_lowercase();

        // Необязательное =значение (пробелы вокруг '=' допустимы).
        let save = i;
        let mut ws_eq = String::new();
        while i < n && ch[i].is_whitespace() {
            ws_eq.push(ch[i]);
            i += 1;
        }
        let has_eq = i < n && ch[i] == '=';
        if has_eq {
            i += 1;
            while i < n && ch[i].is_whitespace() {
                i += 1;
            }
        } else {
            i = save;
            ws_eq.clear();
        }

        // Значение: в кавычках или без.
        let mut quote: Option<char> = None;
        let mut value = String::new();
        if has_eq {
            if i < n && (ch[i] == '"' || ch[i] == '\'') {
                quote = Some(ch[i]);
                i += 1;
                while i < n && ch[i] != quote.unwrap() {
                    value.push(ch[i]);
                    i += 1;
                }
                if i < n {
                    i += 1; // закрывающая кавычка
                }
            } else {
                while i < n && !(ch[i].is_whitespace() || ch[i] == '>' || ch[i] == '/') {
                    value.push(ch[i]);
                    i += 1;
                }
            }
        }

        let is_event = name_lower.starts_with("on")
            && name_lower
                .chars()
                .nth(2)
                .is_some_and(|c| c.is_ascii_alphabetic());
        if is_event {
            // on*-обработчик — выбрасываем вместе с разделителем.
            continue;
        }

        let is_url_attr = name_lower == "href" || name_lower == "src";
        let dangerous = is_url_attr && {
            let lv = value.trim_start().to_lowercase();
            lv.starts_with("javascript:") || lv.starts_with("data:text/html")
        };

        out.push_str(&ws);
        out.push_str(&name);
        if has_eq {
            out.push_str(&ws_eq);
            out.push('=');
            if dangerous {
                // Нейтрализовано: пустое значение вместо опасного URL.
                out.push_str("\"\"");
            } else if let Some(q) = quote {
                out.push(q);
                out.push_str(&value);
                out.push(q);
            } else {
                out.push_str(&value);
            }
        }
    }

    // Незакрытый тег (не дошли до '>') — оставляем исходник как есть.
    if !out.ends_with('>') {
        return tag.to_string();
    }
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

    // ─── P0.2 (regression): не-ASCII в атрибутах не должен ронять санитайзер ───
    //
    // Старая реализация считала позиции в байтах (find/as_bytes), а выводила
    // символы через chars().nth(i): кириллица в title/alt уводила индекс в середину
    // многобайтового символа → panic «is not a char boundary» на валидном документе.

    #[test]
    fn cyrillic_link_title_survives() {
        let html = to_html(r#"[текст](http://example.com "заголовок")"#);
        assert!(html.contains("http://example.com"), "html: {html}");
        assert!(html.contains("заголовок"), "title потерян: {html}");
        assert!(html.contains("текст"), "html: {html}");
    }

    #[test]
    fn cyrillic_img_alt_survives() {
        let html = to_html(r#"![подпись](http://example.com/картинка.png "описание")"#);
        assert!(html.contains("подпись"), "alt потерян: {html}");
        assert!(html.contains("описание"), "title потерян: {html}");
    }

    #[test]
    fn emoji_in_attribute_survives() {
        let html = to_html(r#"[ссылка](http://example.com "🚀 ракета")"#);
        assert!(html.contains("🚀"), "emoji потерян: {html}");
        assert!(html.contains("http://example.com"), "html: {html}");
    }

    #[test]
    fn event_handler_after_cyrillic_is_still_stripped() {
        let html = to_html(r#"<img src="загрузчик.png" onerror="alert(1)">"#);
        assert!(!html.contains("onerror"), "html: {html}");
        assert!(!html.contains("alert"), "html: {html}");
        assert!(html.contains("загрузчик.png"), "html: {html}");
    }

    #[test]
    fn javascript_href_with_cyrillic_context_neutralized() {
        let html = to_html(r#"<a href="javascript:alert('привет')" title="кириллица">клик</a>"#);
        let low = html.to_lowercase();
        assert!(!low.contains("javascript:"), "html: {html}");
        assert!(html.contains("кириллица"), "html: {html}");
    }
}
