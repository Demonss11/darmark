//! frontmatter.rs — рендер YAML-шапки Markdown в HTML-таблицу («таблица-таблиц»).
//!
//! Поддержано осознанно узкое подмножество YAML: mapping, sequence, скаляры,
//! одинарные/двойные кавычки, полные строки-комментарии, вложенность по
//! отступам и простые inline-коллекции `[…]` / `{…}`. Экзотика (block scalars
//! `|` и `>`, anchors/aliases, multi-doc, многострочные незакавыченные скаляры)
//! намеренно не реализована: при любой неоднозначности парсер возвращает ошибку
//! и шапка показывается как исходник в `<pre><code>` — предпросмотр обязан
//! деградировать, а не молча искажать данные.
//!
//! HTML здесь **не** санитайзится: значения экранируются при сборке, а финальный
//! проход выполнит `sanitize_html` ядра (все генерируемые теги и атрибуты — из
//! его белого списка).

/// Максимальная глубина вложенности — защита от «бомбы» отступов.
const MAX_DEPTH: usize = 32;

/// Разобранное значение YAML-подмножества.
#[derive(Debug, PartialEq, Eq)]
enum Yaml {
    Scalar(String),
    Null,
    Map(Vec<(String, Yaml)>),
    Seq(Vec<Yaml>),
}

/// Строка после нормализации: значимый отступ и содержимое без ведущих пробелов.
#[derive(Debug, Clone)]
struct Line {
    indent: usize,
    content: String,
}

/// Рендерит тело YAML-шапки (то, что между `---`) в HTML-фрагмент.
///
/// Возвращает таблицу/список либо `<pre>` с исходником, если подмножество YAML
/// распарсить не удалось.
pub(crate) fn render_frontmatter(yaml: &str) -> String {
    match parse_document(yaml) {
        Some(node) => render_node(&node, 0),
        None => render_source(yaml),
    }
}

/// Fallback: исходник шапки как экранированный текст в `<pre><code>`.
fn render_source(yaml: &str) -> String {
    let mut out = String::from("<pre class=\"md-frontmatter-source\"><code>");
    escape_html_text_into(&mut out, yaml);
    out.push_str("</code></pre>");
    out
}

// ─── Парсер подмножества YAML ──────────────────────────────────────────

/// Парсит документ целиком. `None` — ошибка/неподдерживаемая конструкция.
fn parse_document(yaml: &str) -> Option<Yaml> {
    let lines = normalize_lines(yaml)?;
    if lines.is_empty() {
        return Some(Yaml::Map(Vec::new()));
    }
    let base = lines[0].indent;
    let mut i = 0usize;
    let node = parse_block(&lines, &mut i, base)?;
    // Лишние строки означают конструкцию, которую мы не разобрали (например,
    // многострочный скаляр) — честнее отдать исходник, чем терять данные.
    if i != lines.len() {
        return None;
    }
    Some(node)
}

/// Разбивает вход на значимые строки, выбрасывая пустые и полные комментарии.
/// `None`, если в отступе встретился таб (не поддерживаем смешанные отступы).
fn normalize_lines(yaml: &str) -> Option<Vec<Line>> {
    let mut out = Vec::new();
    for raw in yaml.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if line[..indent].contains('\t') {
            return None;
        }
        out.push(Line {
            indent,
            content: trimmed.to_string(),
        });
    }
    Some(out)
}

/// Разбирает узел, начинающийся с строки `i` на отступе `indent`.
fn parse_block(lines: &[Line], i: &mut usize, indent: usize) -> Option<Yaml> {
    let line = lines.get(*i)?;
    if line.indent != indent {
        return None;
    }
    if seq_marker(&line.content).is_some() {
        parse_seq(lines, i, indent)
    } else if split_map_entry(&line.content).is_some() {
        parse_map(lines, i, indent)
    } else {
        *i += 1;
        parse_scalar(&line.content)
    }
}

fn parse_map(lines: &[Line], i: &mut usize, indent: usize) -> Option<Yaml> {
    let mut entries = Vec::new();
    while let Some(line) = lines.get(*i) {
        if line.indent < indent {
            break;
        }
        // Более глубокий отступ без «родителя» или смешение map и seq на
        // одном уровне — ошибка подмножества.
        if line.indent > indent || seq_marker(&line.content).is_some() {
            return None;
        }
        let (key_raw, val_raw) = split_map_entry(&line.content)?;
        let key = unquote(key_raw.trim())?;
        if key.is_empty() {
            return None;
        }
        *i += 1;
        let val = if val_raw.is_empty() {
            match lines.get(*i) {
                Some(next) if next.indent > indent => parse_block(lines, i, next.indent)?,
                _ => Yaml::Null,
            }
        } else {
            parse_scalar(val_raw)?
        };
        entries.push((key, val));
    }
    Some(Yaml::Map(entries))
}

fn parse_seq(lines: &[Line], i: &mut usize, indent: usize) -> Option<Yaml> {
    let mut items = Vec::new();
    while let Some(line) = lines.get(*i) {
        if line.indent != indent {
            break;
        }
        let Some((rest, _offset)) = seq_marker(&line.content) else {
            break;
        };
        *i += 1;
        let rest = rest.trim();
        if rest.is_empty() {
            match lines.get(*i) {
                Some(next) if next.indent > indent => {
                    items.push(parse_block(lines, i, next.indent)?)
                }
                _ => items.push(Yaml::Null),
            }
        } else if split_map_entry(rest).is_some() {
            // «- key: value» — элемент-объект. Колонку `key` берём из позиции
            // остатка в исходной строке, чтобы продолжения (`  role: …`) легли
            // на тот же отступ.
            let item_indent = indent + line.content.find(rest).unwrap_or(2);
            let mut sub = vec![Line {
                indent: item_indent,
                content: rest.to_string(),
            }];
            while let Some(next) = lines.get(*i) {
                if next.indent > indent {
                    sub.push(next.clone());
                    *i += 1;
                } else {
                    break;
                }
            }
            let mut j = 0usize;
            let node = parse_block(&sub, &mut j, item_indent)?;
            if j != sub.len() {
                return None;
            }
            items.push(node);
        } else {
            items.push(parse_scalar(rest)?);
        }
    }
    Some(Yaml::Seq(items))
}

/// Разбирает скаляр (или inline-коллекцию). `None` — ошибка.
fn parse_scalar(raw: &str) -> Option<Yaml> {
    let s = raw.trim();
    if s.is_empty() {
        return Some(Yaml::Null);
    }
    if let Some(inner) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return parse_inline_seq(inner);
    }
    if let Some(inner) = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        return parse_inline_map(inner);
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return Some(Yaml::Scalar(s[1..s.len() - 1].replace("''", "'")));
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Some(Yaml::Scalar(unescape_double(&s[1..s.len() - 1])));
    }
    // Незакрытая кавычка — почти всегда опечатка; не гадаем.
    if s.starts_with('\'') || s.starts_with('"') {
        return None;
    }
    let val = match s.find(" #") {
        Some(p) => s[..p].trim_end(),
        None => s,
    };
    if val.is_empty() || val == "null" || val == "~" {
        return Some(Yaml::Null);
    }
    Some(Yaml::Scalar(val.to_string()))
}

fn parse_inline_seq(inner: &str) -> Option<Yaml> {
    let mut items = Vec::new();
    for part in split_top_level(inner, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        items.push(parse_scalar(part)?);
    }
    Some(Yaml::Seq(items))
}

fn parse_inline_map(inner: &str) -> Option<Yaml> {
    let mut entries = Vec::new();
    for part in split_top_level(inner, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (k, v) = split_map_entry(part)?;
        entries.push((unquote(k.trim())?, parse_scalar(v)?));
    }
    Some(Yaml::Map(entries))
}

/// Распознаёт маркер элемента последовательности. Возвращает остаток строки
/// и его байтовое смещение в исходном содержимом.
fn seq_marker(content: &str) -> Option<(&str, usize)> {
    if content == "-" {
        return Some(("", 1));
    }
    content.strip_prefix("- ").map(|rest| (rest, 2))
}

/// Ищет разделитель map-записи: `:` вне кавычек, за которым пробел/таб/конец.
/// Скаляры вида `http://…` и время `12:30` не расщепляются.
fn split_map_entry(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => {
                if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b':' {
                    match bytes.get(i + 1) {
                        None => return Some((&s[..i], "")),
                        Some(&c) if c == b' ' || c == b'\t' => {
                            return Some((&s[..i], s[i + 1..].trim()));
                        }
                        _ => {}
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Делит строку по `sep` вне кавычек и вложенных `[]`/`{}`.
fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut quote: Option<char> = None;
    let mut start = 0usize;
    for (idx, c) in s.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ if c == sep && depth == 0 => {
                    out.push(&s[start..idx]);
                    start = idx + c.len_utf8();
                }
                _ => {}
            },
        }
    }
    out.push(&s[start..]);
    out
}

/// Снимает обрамляющие кавычки с ключа. `None` — незакрытая кавычка.
fn unquote(s: &str) -> Option<String> {
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return Some(s[1..s.len() - 1].replace("''", "'"));
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Some(unescape_double(&s[1..s.len() - 1]));
    }
    if s.starts_with('\'') || s.starts_with('"') {
        return None;
    }
    Some(s.to_string())
}

/// Базовая разборка escape-последовательностей двойных кавычек.
fn unescape_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ─── Отрисовка дерева в HTML ───────────────────────────────────────────

fn render_node(node: &Yaml, depth: usize) -> String {
    let mut out = String::new();
    match node {
        Yaml::Map(entries) if entries.is_empty() => {
            out.push_str("<span class=\"fm-empty\">—</span>");
        }
        Yaml::Map(entries) => {
            if depth > MAX_DEPTH {
                return "<span class=\"fm-deep\">…</span>".to_string();
            }
            out.push_str("<table class=\"md-frontmatter\"><tbody>");
            for (key, val) in entries {
                out.push_str("<tr><th class=\"fm-key\">");
                escape_html_text_into(&mut out, key);
                out.push_str("</th><td class=\"fm-val\">");
                out.push_str(&render_node(val, depth + 1));
                out.push_str("</td></tr>");
            }
            out.push_str("</tbody></table>");
        }
        Yaml::Seq(items) if items.is_empty() => {
            out.push_str("<span class=\"fm-empty\">—</span>");
        }
        Yaml::Seq(items) => {
            if depth > MAX_DEPTH {
                return "<span class=\"fm-deep\">…</span>".to_string();
            }
            out.push_str("<ul class=\"fm-list\">");
            for item in items {
                out.push_str("<li>");
                out.push_str(&render_node(item, depth + 1));
                out.push_str("</li>");
            }
            out.push_str("</ul>");
        }
        Yaml::Scalar(s) => escape_html_text_into(&mut out, s),
        Yaml::Null => out.push_str("<span class=\"fm-null\">—</span>"),
    }
    out
}

/// Экранирует текст для вставки в HTML-содержимое.
fn escape_html_text_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_map_is_table() {
        let html = render_frontmatter("title: Мой документ\ndate: 2026-10-07\n");
        assert!(html.contains("<table class=\"md-frontmatter\">"), "{html}");
        assert!(html.contains("<th class=\"fm-key\">title</th>"), "{html}");
        assert!(html.contains("Мой документ"), "{html}");
        assert!(html.contains("2026-10-07"), "{html}");
    }

    #[test]
    fn nested_map_becomes_nested_table() {
        let html = render_frontmatter("author:\n  name: Иван\n  email: ivan@example.com\n");
        assert_eq!(html.matches("<table").count(), 2, "{html}");
        assert!(html.contains("author"), "{html}");
        assert!(html.contains("ivan@example.com"), "{html}");
    }

    #[test]
    fn sequence_becomes_list() {
        let html = render_frontmatter("tags:\n  - markdown\n  - editor\n");
        assert!(html.contains("<ul class=\"fm-list\">"), "{html}");
        assert!(html.contains("<li>markdown</li>"), "{html}");
        assert!(html.contains("<li>editor</li>"), "{html}");
    }

    #[test]
    fn sequence_of_maps_becomes_tables_in_list() {
        let html = render_frontmatter("authors:\n  - name: A\n    role: X\n  - name: B\n");
        assert!(html.contains("<ul class=\"fm-list\">"), "{html}");
        // Внешняя таблица + вложенная таблица на каждый элемент-объект.
        assert_eq!(html.matches("<table").count(), 3, "{html}");
        assert!(html.contains("role"), "{html}");
        assert!(
            !html.contains("name: A"),
            "маркер списка не должен попасть: {html}"
        );
    }

    #[test]
    fn quotes_and_comments() {
        let html = render_frontmatter("a: 'x # y'\nb: \"line\\nbreak\"\n# comment\nc: 1 # tail\n");
        assert!(html.contains("x # y"), "{html}");
        assert!(html.contains("line\nbreak"), "{html}");
        assert!(
            html.contains(">1<") || html.contains(">1</th>"),
            "c=1: {html}"
        );
    }

    #[test]
    fn inline_collections() {
        let html = render_frontmatter("tags: [a, b, c]\nmeta: {x: 1, y: 2}\n");
        assert!(html.contains("<li>a</li>"), "{html}");
        assert!(html.contains(">x<") || html.contains("x</th>"), "{html}");
    }

    #[test]
    fn null_and_empty() {
        let html = render_frontmatter("a:\nb: null\nc: ~\n");
        assert_eq!(html.matches("fm-null").count(), 3, "{html}");
    }

    #[test]
    fn multiline_scalar_falls_back_to_source() {
        let html = render_frontmatter("description: first\n  second line\n");
        assert!(
            html.contains("<pre class=\"md-frontmatter-source\">"),
            "{html}"
        );
        assert!(html.contains("second line"), "{html}");
    }

    #[test]
    fn block_scalar_falls_back_to_source() {
        let html = render_frontmatter("body: |\n  line one\n  line two\n");
        assert!(
            html.contains("<pre class=\"md-frontmatter-source\">"),
            "{html}"
        );
    }

    #[test]
    fn tab_indent_falls_back() {
        let html = render_frontmatter("a:\n\tb: 1\n");
        assert!(html.contains("md-frontmatter-source"), "{html}");
    }

    #[test]
    fn values_are_escaped() {
        let html = render_frontmatter("x: <script>alert(1)</script>\n");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn empty_frontmatter_is_empty_map() {
        let html = render_frontmatter("");
        assert!(html.contains("fm-empty"), "{html}");
    }

    #[test]
    fn cyrillic_and_emoji_keys() {
        let html = render_frontmatter("название: 🚀 ракета\n");
        assert!(html.contains("название"), "{html}");
        assert!(html.contains("🚀 ракета"), "{html}");
    }
}
