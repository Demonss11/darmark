//! md-core — чистое ядро Markdown → HTML без зависимостей от UI.
//!
//! Используется Tauri-командой `render_markdown`, а в будущем — CLI/TUI/harness.
//! Точность HTML обеспечивает pulldown-cmark (compliant CommonMark) с расширениями:
//! - GFM Tables (пайповые таблицы `| a | b |`)
//! - Strikethrough (`~~text~~`)
//! - Task lists (`- [ ]` / `- [x]`)
//! - Footnotes (`[^1]`)
//! - Heading IDs (`# Title {#custom-id}`)

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

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

/// Рендерит markdown с пометкой топ-блоков: каждый блок оборачивается в
/// `<div class="md-block" data-md="{start},{end}">`, где `start`/`end` — байтовые
/// смещения блока в исходном markdown (start включительно, end исключительно).
///
/// Документ парсится **один раз**: события `OffsetIter` группируются по
/// топ-блокам и каждая группа прогоняется через `html::push_html`. Это
/// принципиально — ссылочные определения (`[foo]: /url`) и сноски (`[^1]`)
/// разрешаются единым парсером на весь документ, поэтому повторная сборка
/// вырезанных срезов (прошлая реализация) их не теряет.
///
/// Инварианты:
/// * детерминированность — одинаковый вход даёт одинаковый выход;
/// * содержимое блоков **без всех `data-md`-атрибутов** (и без обёрток) побайтово
///   совпадает с [`to_html`] (тот же `DEFAULT_OPTIONS` + тот же санитайзер);
/// * `data-md` содержит только цифры и запятую — безопасен для HTML;
/// * границы блоков — UTF-8 char-boundary (гарантия `OffsetIter`);
/// * пустые блоки не создаются (проверка по итоговому HTML, а не по срезу).
///
/// Для блоков-таблиц дополнительно размечается вложенная структура: `<tr>`,
/// `<th>`, `<td>` получают собственный `data-md="{start},{end}"`. Строка заголовка
/// (`<thead>`) размечается только на уровне ячеек `<th>` — отдельного `tr` у неё
/// нет (это снимает неоднозначность «есть ли `TableRow` внутри `TableHead`» между
/// версиями pulldown). Диапазоны берутся из парсера, поэтому экранированный `\|`
/// не сбивает маппинг. Диапазон ячейки обрезается до содержимого (без
/// обрамляющих пробелов; пустая ячейка сохраняет диапазон парсера), диапазон
/// строки — без завершающего `\n`/`\r`. Инъекция выполняется **после**
/// санитайзера, иначе `data-*` был бы срезан.
pub fn to_html_mapped(markdown: &str) -> String {
    // Группы событий по топ-блокам: (start, end, события блока, структурные
    // диапазоны таблиц). Структурные диапазоны собираем на этапе обхода, пока
    // доступны байтовые смещения `OffsetIter` — в `Event` их уже нет.
    let mut blocks: Vec<(usize, usize, Vec<Event<'_>>, Vec<StructRange>)> = Vec::new();
    let mut group: Vec<Event<'_>> = Vec::new();
    let mut group_struct: Vec<StructRange> = Vec::new();
    let mut group_start: Option<usize> = None;
    let mut depth: usize = 0;
    let mut in_head = false;

    for (event, range) in Parser::new_ext(markdown, DEFAULT_OPTIONS).into_offset_iter() {
        collect_struct_range(
            markdown,
            &event,
            range.start,
            range.end,
            &mut in_head,
            &mut group_struct,
        );

        match &event {
            Event::Start(_) => {
                if depth == 0 {
                    group_start = Some(range.start);
                }
                depth += 1;
                group.push(event);
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                group.push(event);
                if depth == 0 {
                    let start = group_start.take().unwrap_or(range.start);
                    blocks.push((
                        start,
                        range.end,
                        std::mem::take(&mut group),
                        std::mem::take(&mut group_struct),
                    ));
                }
            }
            _ => {
                if depth == 0 {
                    // Одиночные события верхнего уровня, не образующие контейнер:
                    // `Rule` (`<hr>`), HTML-блоки, «висячий» текст. Каждое —
                    // самостоятельный блок (иначе `<hr>` выпал бы из подсветки).
                    blocks.push((range.start, range.end, vec![event], Vec::new()));
                } else {
                    group.push(event);
                }
            }
        }
    }

    // Ссылочные определения не порождают событий, поэтому их байты оказываются
    // «между» блоками. Приклеиваем такие непустые промежутки к следующему блоку
    // (а хвостовой — к последнему), чтобы покрытие T-5 не рвалось.
    let mut prev_end = 0;
    for block in blocks.iter_mut() {
        let start = block.0;
        let end = block.1;
        if start > prev_end {
            if let Some(non_ws) = first_non_ws(markdown, prev_end, start) {
                block.0 = non_ws;
            }
        }
        prev_end = end;
    }
    if let Some(last) = blocks.last_mut() {
        if let Some(end) = last_non_ws_end(markdown, last.1, markdown.len()) {
            last.1 = end;
        }
    }

    let mut result = String::with_capacity(markdown.len() + 64);
    for (start, end, events, struct_ranges) in blocks {
        // Хвостовые пробелы/пустые строки не относятся к блоку: подсветка в
        // редакторе не должна захватывать «воздух» между абзацами.
        let end = trim_end_ws(markdown, start, end);
        if end <= start {
            continue;
        }
        let mut html_out = String::with_capacity((end - start) + 32);
        html::push_html(&mut html_out, events.into_iter());
        let mut sanitized = sanitize_html(&html_out);
        // Санитайзер срезал бы `data-*`, поэтому вложенную разметку таблицы
        // добавляем строго после него.
        if !struct_ranges.is_empty() {
            sanitized = inject_structural_data_md(&sanitized, &struct_ranges);
        }
        // HTML-комментарий/опасный контейнер непуст по исходнику, но после
        // санитайзера остаётся лишь пробельный хвост — обёртку не создаём
        // (T-2.5), но хвост сохраняем, чтобы конкатенация совпадала с to_html.
        if sanitized.trim().is_empty() {
            result.push_str(&sanitized);
            continue;
        }
        result.push_str("<div class=\"md-block\" data-md=\"");
        result.push_str(&start.to_string());
        result.push(',');
        result.push_str(&end.to_string());
        result.push_str("\">");
        result.push_str(&sanitized);
        result.push_str("</div>");
    }
    result
}

// ─── Структурная разметка таблиц (TZ-inspect-tables, §3.1) ─────────────

/// Тип структурного элемента таблицы, получающего собственный `data-md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructTag {
    Row,
    Th,
    Td,
}

/// Байтовый диапазон структурного элемента таблицы в исходном markdown.
#[derive(Debug, Clone, Copy)]
struct StructRange {
    tag: StructTag,
    start: usize,
    end: usize,
}

/// Собирает диапазон структурного тега таблицы из очередного события.
///
/// Порядок добавления совпадает с порядком следования HTML-тегов, поэтому
/// инъекция может сопоставлять их курсором. Строка заголовка (`TableRow`
/// внутри `TableHead`) намеренно не размечается: у шапки интерактивны `th`,
/// а отдельный `tr` там только запутал бы сопоставление.
fn collect_struct_range(
    markdown: &str,
    event: &Event<'_>,
    start: usize,
    end: usize,
    in_head: &mut bool,
    out: &mut Vec<StructRange>,
) {
    match event {
        Event::Start(Tag::TableHead) => *in_head = true,
        Event::End(TagEnd::TableHead) => *in_head = false,
        Event::Start(Tag::TableRow) => {
            if !*in_head {
                let end = trim_end_ws(markdown, start, end);
                if end > start {
                    out.push(StructRange {
                        tag: StructTag::Row,
                        start,
                        end,
                    });
                }
            }
        }
        Event::Start(Tag::TableCell) => {
            let (start, end) = trim_cell(markdown, start, end);
            let tag = if *in_head {
                StructTag::Th
            } else {
                StructTag::Td
            };
            out.push(StructRange { tag, start, end });
        }
        _ => {}
    }
}

/// Обрезает диапазон ячейки до её содержимого (без обрамляющих пробелов).
/// Если после обрезки содержимого нет (ячейка из пробелов) — возвращает
/// исходный диапазон парсера, чтобы не потерять место в структуре.
fn trim_cell(markdown: &str, start: usize, end: usize) -> (usize, usize) {
    match (
        first_non_ws(markdown, start, end),
        last_non_ws_end(markdown, start, end),
    ) {
        (Some(s), Some(e)) if s < e => (s, e),
        _ => (start, end),
    }
}

/// Вставляет `data-md="start,end"` в открывающие теги `<tr>`/`<th>`/`<td>`
/// в порядке `ranges`. Чистая функция над уже санитизированным HTML.
///
/// Состояние: `in_head` (меняется по `<thead>`/`</thead>`/`<tbody>`) и
/// `cell_depth` (глубина вложенности ячеек) — оба учитываются только на
/// внешнем уровне (`cell_depth == 0`), что защищает от сырой вложенной
/// таблицы внутри ячейки. При несовпадении типа тега и диапазона тег
/// пропускается без сдвига курсора (defensive).
fn inject_structural_data_md(html: &str, ranges: &[StructRange]) -> String {
    let mut out = String::with_capacity(html.len() + ranges.len() * 24);
    let mut cursor = 0usize;
    let mut cell_depth: usize = 0;
    let mut in_head = false;

    let bytes = html.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            // Копируем текст до следующего '<' одним куском — дешевле посимвольно.
            match html[i..].find('<') {
                Some(off) => {
                    out.push_str(&html[i..i + off]);
                    i += off;
                }
                None => {
                    out.push_str(&html[i..]);
                    break;
                }
            }
            continue;
        }

        let tag_end = find_tag_end(html, i);
        let raw = &html[i..=tag_end];
        let name = tag_name(raw);
        let closing = raw.as_bytes().get(1) == Some(&b'/');

        let mut injected = false;
        if !closing {
            match name.as_str() {
                "thead" if cell_depth == 0 => in_head = true,
                "tbody" | "tfoot" if cell_depth == 0 => in_head = false,
                "th" => {
                    if cell_depth == 0 && ranges.get(cursor).map(|r| r.tag) == Some(StructTag::Th) {
                        push_with_data_md(&mut out, raw, ranges[cursor]);
                        cursor += 1;
                        injected = true;
                    }
                    cell_depth += 1;
                }
                "td" => {
                    if cell_depth == 0 && ranges.get(cursor).map(|r| r.tag) == Some(StructTag::Td) {
                        push_with_data_md(&mut out, raw, ranges[cursor]);
                        cursor += 1;
                        injected = true;
                    }
                    cell_depth += 1;
                }
                "tr" if cell_depth == 0
                    && !in_head
                    && ranges.get(cursor).map(|r| r.tag) == Some(StructTag::Row) =>
                {
                    push_with_data_md(&mut out, raw, ranges[cursor]);
                    cursor += 1;
                    injected = true;
                }
                "tr" => {}
                _ => {}
            }
        } else if cell_depth == 0 && (name == "thead" || name == "tbody" || name == "tfoot") {
            in_head = false;
        } else if name == "th" || name == "td" {
            cell_depth = cell_depth.saturating_sub(1);
        }

        if !injected {
            out.push_str(raw);
        }
        i = tag_end + 1;
    }
    out
}

/// Индекс `>` открывающего/закрывающего тега, начиная с `<` по `start`,
/// с учётом кавычек внутри тега (`style="text-align: right"`).
fn find_tag_end(html: &str, start: usize) -> usize {
    let bytes = html.as_bytes();
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
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
                } else if b == b'>' {
                    return i;
                }
            }
        }
        i += 1;
    }
    bytes.len().saturating_sub(1)
}

/// Имя тега в нижнем регистре из сырого `<...>` (без закрывающего слэша).
fn tag_name(raw: &str) -> String {
    raw.trim_start_matches('<')
        .trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ':')
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Дописывает тег `raw` в `out`, вставив ` data-md="s,e"` перед `>`.
fn push_with_data_md(out: &mut String, raw: &str, range: StructRange) {
    let inner = raw.strip_suffix('>').unwrap_or(raw);
    out.push_str(inner);
    out.push_str(" data-md=\"");
    out.push_str(&range.start.to_string());
    out.push(',');
    out.push_str(&range.end.to_string());
    out.push_str("\">");
}

/// Байтовый индекс первого непробельного символа в `markdown[from..to]`.
fn first_non_ws(markdown: &str, from: usize, to: usize) -> Option<usize> {
    let slice = markdown.get(from..to)?;
    slice
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| from + i)
}

/// Байтовый индекс сразу после последнего непробельного символа в
/// `markdown[from..to]`.
fn last_non_ws_end(markdown: &str, from: usize, to: usize) -> Option<usize> {
    let slice = markdown.get(from..to)?;
    slice
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(i, c)| from + i + c.len_utf8())
        .next_back()
}

/// Усекает `end`, убирая хвостовые пробельные символы в `markdown[start..end]`.
fn trim_end_ws(markdown: &str, start: usize, end: usize) -> usize {
    match markdown.get(start..end) {
        Some(slice) => start + slice.trim_end().len(),
        None => end,
    }
}

// ─── Санитайзер HTML-вывода pulldown-cmark (v2: белые списки) ──────────
//
// pulldown-cmark не санитайзит вывод: сырой HTML из документа проходит как есть.
// Идеи взяты из ammonia (без зависимости от html5ever):
//   * белый список тегов вместо чёрного — неизвестный тег «разворачиваем»;
//   * содержимое опасных контейнеров (script/style/iframe/...) выкидываем целиком;
//   * белый список атрибутов (глобальные + по тегу) вместо «удалить on*» —
//     это автоматически убирает on*, srcdoc, formaction и т.п.;
//   * схемы URL проверяем по белому списку, а не по префиксу;
//   * значения атрибутов декодируем от сущностей и заново экранируем, чтобы
//     `&#34;` не мог «разорвать» кавычки и создать новый атрибут.

/// Разрешённые теги (аналог `ammonia::Builder::tags`).
const ALLOWED_TAGS: &[&str] = &[
    "a",
    "abbr",
    "b",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "dd",
    "del",
    "details",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "input",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "pre",
    "q",
    "s",
    "samp",
    "section",
    "small",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "u",
    "ul",
    "var",
];

/// Теги, которые вместе с содержимым удаляются целиком (аналог
/// `clean_content_tags`). Только элементы с закрывающим тегом — иначе
/// незакрытый тег «съел» бы весь остаток документа.
const DROP_CONTENT_TAGS: &[&str] = &[
    "script", "style", "iframe", "object", "form", "template", "svg", "math", "noscript",
    "textarea", "select", "button", "xmp", "title",
];

/// Атрибуты, разрешённые на любом теге (аналог `generic_attributes`).
const GLOBAL_ATTRS: &[&str] = &["class", "id", "title", "lang", "dir", "role"];

/// Разрешённые схемы URL (аналог `url_schemes`).
const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];

/// Атрибуты, значение которых — URL (аналог `is_url_attr`).
const URL_ATTRS: &[&str] = &[
    "href",
    "src",
    "action",
    "formaction",
    "data",
    "poster",
    "ping",
    "xlink:href",
];

fn is_allowed_tag(name: &str) -> bool {
    ALLOWED_TAGS.contains(&name)
}

fn is_drop_content_tag(name: &str) -> bool {
    DROP_CONTENT_TAGS.contains(&name)
}

/// Атрибуты, разрешённые конкретному тегу (сверх глобальных).
fn tag_attrs(tag: &str) -> &'static [&'static str] {
    match tag {
        "a" => &["href", "target", "rel", "hreflang", "type"],
        "img" => &["src", "alt", "width", "height", "loading"],
        "input" => &["type", "checked", "disabled", "value"],
        "ol" => &["start", "reversed", "type"],
        "li" => &["value"],
        "td" | "th" => &[
            "colspan", "rowspan", "align", "valign", "scope", "headers", "style",
        ],
        "table" => &["summary", "width"],
        "col" | "colgroup" => &["span", "width"],
        "blockquote" | "q" => &["cite"],
        "del" | "ins" => &["cite", "datetime"],
        "time" => &["datetime"],
        "details" => &["open"],
        _ => &[],
    }
}

fn is_allowed_attr(tag: &str, attr: &str) -> bool {
    GLOBAL_ATTRS.contains(&attr) || tag_attrs(tag).contains(&attr)
}

fn is_url_attr(attr: &str) -> bool {
    URL_ATTRS.contains(&attr)
}

/// Декодирует числовые (`&#106;`, `&#x61;`) и несколько «опасных» именованных
/// сущностей. Неизвестные `&name;` оставляем как есть — их заэкранирует вывод.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        if let Some(semi) = after.find(';').filter(|&p| p <= 10) {
            if let Some(ch) = decode_entity_name(&after[..semi]) {
                out.push(ch);
                rest = &after[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[pos + 1..];
    }
    out.push_str(rest);
    out
}

fn decode_entity_name(name: &str) -> Option<char> {
    if let Some(digits) = name.strip_prefix('#') {
        let code = if let Some(hex) = digits
            .strip_prefix('x')
            .or_else(|| digits.strip_prefix('X'))
        {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            digits.parse::<u32>().ok()?
        };
        return char::from_u32(code);
    }
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "colon" => Some(':'),
        "sol" => Some('/'),
        "num" => Some('#'),
        "Tab" | "tab" => Some('\t'),
        "NewLine" | "newline" => Some('\n'),
        "nbsp" => Some('\u{00A0}'),
        _ => None,
    }
}

/// Экранирует значение атрибута для вывода в двойных кавычках.
fn escape_html_attr(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// Проверяет URL-значение по белому списку схем. Схема извлекается из
/// декодированного значения; управляющие символы и пробелы убираются, чтобы
/// `java\tscript:` не проскочил. Относительные пути разрешены.
fn url_is_safe(attr: &str, value: &str) -> bool {
    let cleaned: String = value
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    match cleaned.find(':') {
        Some(colon) => {
            let scheme = &cleaned[..colon];
            // ':' внутри относительного пути — не схема.
            if scheme.contains('/') || scheme.contains('?') || scheme.contains('#') {
                return true;
            }
            if !is_scheme(scheme) {
                return false;
            }
            if attr == "src" && scheme == "data" {
                return cleaned.starts_with("data:image/");
            }
            ALLOWED_SCHEMES.contains(&scheme)
        }
        None => true,
    }
}

/// `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` по RFC 3986.
fn is_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Оставляем `style` только для выравнивания ячеек (то, что генерирует pulldown).
fn style_is_safe(value: &str) -> bool {
    match value.trim().to_ascii_lowercase().split_once(':') {
        Some(("text-align", rest)) => {
            matches!(rest.trim(), "left" | "right" | "center" | "justify")
        }
        _ => false,
    }
}

/// Проходит по HTML и применяет политику белых списков.
fn sanitize_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '<' {
            out.push(c);
            continue;
        }

        // Комментарий <!-- ... --> вырезаем целиком.
        let saved: String = chars.clone().take(40).collect();
        if saved.to_ascii_lowercase().starts_with("!--") {
            for _ in 0..3 {
                chars.next();
            }
            skip_comment(&mut chars);
            continue;
        }

        // Опасный контейнер — выкидываем вместе с содержимым.
        if let Some((name, closing)) = peek_tag_name(&chars) {
            if !closing && is_drop_content_tag(&name) {
                skip_element_block(&mut chars, &name);
                continue;
            }
        }

        out.push_str(&rewrite_tag(&collect_tag(&mut chars)));
    }
    out
}

/// Читает имя тега, не потребляя поток. Возвращает (имя в нижнем регистре,
/// является ли тег закрывающим).
fn peek_tag_name(chars: &std::iter::Peekable<std::str::Chars<'_>>) -> Option<(String, bool)> {
    let mut it = chars.clone().peekable();
    let closing = it.peek() == Some(&'/');
    if closing {
        it.next();
    }
    let mut name = String::new();
    while let Some(&c) = it.peek() {
        if c.is_ascii_alphanumeric() || c == '-' || c == ':' {
            name.push(c.to_ascii_lowercase());
            it.next();
        } else {
            break;
        }
    }
    if name.is_empty() {
        None
    } else {
        Some((name, closing))
    }
}

/// Пропускает содержимое контейнера до закрывающего `</name>` (включительно).
// Используем `while let … chars.next()` (а не `for c in chars.by_ref()`), потому
// что внутри тела нужны `chars.clone()` и `chars.by_ref()` — `for` занял бы поток.
#[allow(clippy::while_let_on_iterator)]
fn skip_element_block(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, name: &str) -> bool {
    let close = format!("/{name}");
    while let Some(c) = chars.next() {
        if c != '<' {
            continue;
        }
        let saved: String = chars.clone().take(close.len() + 1).collect();
        let low = saved.to_ascii_lowercase();
        if low.starts_with(&close) {
            let boundary = match low[close.len()..].chars().next() {
                None => true,
                Some(c) => c == '>' || c.is_whitespace() || c == '/',
            };
            if boundary {
                for ch in chars.by_ref() {
                    if ch == '>' {
                        break;
                    }
                }
                return true;
            }
        }
    }
    false
}

/// Пропускает HTML-комментарий после уже съеденного `<!--`.
fn skip_comment(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    while let Some(c) = chars.next() {
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            if chars.peek() == Some(&'>') {
                chars.next();
            }
            return true;
        }
    }
    false
}

/// Собирает сырой тег от текущей позиции (уже после `<`) до `>` вне кавычек.
fn collect_tag(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut raw = String::from("<");
    let mut in_quote: Option<char> = None;
    for c in chars.by_ref() {
        raw.push(c);
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c == '>' {
            break;
        }
    }
    raw
}

/// Символы, допустимые в имени атрибута/тега (до разделителя или конца).
fn is_name_char(c: char) -> bool {
    !(c.is_whitespace() || c == '=' || c == '>' || c == '/')
}

/// Переписывает один тег по политике белых списков.
///
/// Работает по `Vec<char>`, а не по байтовым индексам: кириллица в атрибуте
/// (`title="заголовок"`) не должна уводить индекс в середину символа.
fn rewrite_tag(tag: &str) -> String {
    let ch: Vec<char> = tag.chars().collect();
    let n = ch.len();
    if n < 3 || ch[0] != '<' || ch[n - 1] != '>' {
        return tag.to_string();
    }

    let mut i = 1;
    let closing = ch[i] == '/';
    if closing {
        i += 1;
    }
    let name_start = i;
    while i < n - 1 && is_name_char(ch[i]) {
        i += 1;
    }
    let name: String = ch[name_start..i]
        .iter()
        .collect::<String>()
        .to_ascii_lowercase();
    if name.is_empty() {
        return tag.to_string();
    }

    if closing {
        return if is_allowed_tag(&name) {
            format!("</{name}>")
        } else {
            String::new()
        };
    }

    if !is_allowed_tag(&name) {
        // Неизвестный тег «разворачиваем»: сам тег убираем, содержимое остаётся.
        return String::new();
    }

    let self_closing = ch[n - 2] == '/';
    let body_end = if self_closing { n - 2 } else { n - 1 };

    let mut out = String::with_capacity(tag.len());
    out.push('<');
    out.push_str(&name);

    let mut i = skip_ws(&ch, i, body_end);
    while i < body_end {
        let name_at = i;
        while i < body_end && is_name_char(ch[i]) {
            i += 1;
        }
        if i == name_at {
            i += 1; // неожиданный символ — пропускаем
            continue;
        }
        let attr: String = ch[name_at..i]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();

        let mut j = skip_ws(&ch, i, body_end);
        let mut has_eq = false;
        let mut value = String::new();
        if j < body_end && ch[j] == '=' {
            has_eq = true;
            j = skip_ws(&ch, j + 1, body_end);
            if j < body_end && (ch[j] == '"' || ch[j] == '\'') {
                let q = ch[j];
                j += 1;
                while j < body_end && ch[j] != q {
                    value.push(ch[j]);
                    j += 1;
                }
                if j < body_end {
                    j += 1;
                }
            } else {
                while j < body_end && !ch[j].is_whitespace() {
                    value.push(ch[j]);
                    j += 1;
                }
            }
            i = j;
        }

        if !is_allowed_attr(&name, &attr) {
            continue;
        }
        let decoded = decode_entities(&value);
        if is_url_attr(&attr) && !url_is_safe(&attr, &decoded) {
            continue;
        }
        if attr == "style" && !style_is_safe(&decoded) {
            continue;
        }

        out.push(' ');
        out.push_str(&attr);
        if has_eq {
            out.push('=');
            out.push('"');
            escape_html_attr(&mut out, &decoded);
            out.push('"');
        }
    }

    if name == "a" {
        out.push_str(" rel=\"noopener noreferrer\"");
    }

    out.push_str(if self_closing { " />" } else { ">" });
    out
}

/// Пропускает пробелы в диапазоне `[i, end)`.
fn skip_ws(ch: &[char], mut i: usize, end: usize) -> usize {
    while i < end && ch[i].is_whitespace() {
        i += 1;
    }
    i
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

    // ─── P0.2: XSS-санитайзер (v2, белые списки) ───────────────────────

    #[test]
    fn strips_img_onerror() {
        let html = to_html("<img src=\"x\" onerror=\"alert(1)\">");
        assert!(!html.contains("onerror"), "html: {html}");
    }

    #[test]
    fn strips_script_tags() {
        let html = to_html("<script>alert('xss')</script>");
        assert!(!html.contains("<script"), "html: {html}");
        assert!(!html.contains("alert"), "html: {html}");
    }

    #[test]
    fn strips_style_element_and_content() {
        let html = to_html("<style>body{color:red}</style>text");
        assert!(!html.contains("<style"), "html: {html}");
        assert!(!html.contains("color:red"), "html: {html}");
        assert!(html.contains("text"), "html: {html}");
    }

    #[test]
    fn strips_iframe_form_object() {
        for vector in [
            r#"<iframe src="https://evil.example"></iframe>"#,
            r#"<form action="https://evil.example"><input name="x"></form>"#,
            r#"<object data="https://evil.example"></object>"#,
        ] {
            let html = to_html(vector);
            assert!(!html.contains("evil.example"), "html: {html}");
        }
    }

    #[test]
    fn unknown_tag_unwrapped_but_text_kept() {
        let html = to_html("<video>movie</video>");
        assert!(!html.to_lowercase().contains("<video"), "html: {html}");
        assert!(html.contains("movie"), "html: {html}");
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
    fn neutralizes_entity_obfuscated_javascript() {
        let html = to_html(r#"<a href="javascript&#58;alert(1)">x</a>"#);
        assert!(!html.to_lowercase().contains("javascript"), "html: {html}");
    }

    #[test]
    fn prevents_entity_attribute_breakout() {
        // `&#34;` декодируется в кавычку и должна быть заэкранирована,
        // иначе атрибут «порвётся» и создаст onerror.
        let html = to_html(r#"<img alt='&#34; onerror=alert(1)'>"#);
        assert!(
            html.contains("alt=\"&quot; onerror=alert(1)\""),
            "html: {html}"
        );
    }

    #[test]
    fn adds_rel_noopener_to_links() {
        let html = to_html("[x](https://example.com)");
        assert!(html.contains("rel=\"noopener noreferrer\""), "html: {html}");
    }

    #[test]
    fn drops_style_attribute_outside_cells() {
        let html = to_html(r#"<div style="background:url(https://evil.example)">x</div>"#);
        assert!(!html.contains("evil.example"), "html: {html}");
        assert!(!html.contains("style="), "html: {html}");
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

    // ─── P0.2: табличный harness (техдолг #7) ──────────────────────────
    //
    // Точечные тесты выше проверяют отдельные случаи; здесь единый инвариант
    // «ни один XSS-вектор не выживает» прогоняется по таблице контекстов.
    // Расширять нужно именно эти таблицы, чтобы новое правило не «протекало».

    /// Проверяет, что ни один из запрещённых маркеров (без учёта регистра)
    /// не встречается в HTML, отрендеренном из входа.
    fn assert_clean(input: &str, forbidden: &[&str]) {
        let html = to_html(input);
        let low = html.to_lowercase();
        for marker in forbidden {
            assert!(
                !low.contains(&marker.to_lowercase()),
                "маркер {marker:?} просочился\n  вход: {input}\n  HTML: {html}"
            );
        }
    }

    /// Контексты с событийными атрибутами: не должно остаться ни `on*`, ни тела.
    const EVENT_HANDLER_VECTORS: &[&str] = &[
        r#"<img src="x" onerror="alert(1)">"#,
        r#"<img src="x" ONERROR="alert(1)">"#,
        r#"<img src=x onerror=alert(1)>"#,
        r#"<div onclick="alert(1)">text</div>"#,
        r#"<body onload="alert(1)">"#,
        r#"<svg onload="alert(1)"></svg>"#,
        r##"<a href="#" onmouseover="alert(1)">x</a>"##,
        r#"<video src="x" onplay="alert(1)"></video>"#,
        r#"<p onfocus='alert(1)'>x</p>"#,
    ];

    /// Контексты с опасными URI в `href`/`src`.
    const DANGEROUS_URL_VECTORS: &[&str] = &[
        r#"<a href="javascript:alert(1)">x</a>"#,
        r#"<a href='JavaScript:alert(1)'>x</a>"#,
        r#"<a href="  javascript:alert(1)">x</a>"#,
        r#"<a href="javascript&#58;alert(1)">x</a>"#,
        r#"<a href="jav&#x61;script:alert(1)">x</a>"#,
        r#"<iframe src="javascript:alert(1)"></iframe>"#,
        r#"<img src="data:text/html,<b>hi</b>">"#,
        r#"[x](javascript:alert(1))"#,
    ];

    #[test]
    fn table_no_event_handler_survives_any_context() {
        for vector in EVENT_HANDLER_VECTORS {
            assert_clean(
                vector,
                &[
                    "onerror",
                    "onload",
                    "onclick",
                    "onmouseover",
                    "onplay",
                    "onfocus",
                    "alert(1)",
                    "<script",
                ],
            );
        }
    }

    #[test]
    fn table_no_dangerous_uri_survives_any_context() {
        for vector in DANGEROUS_URL_VECTORS {
            assert_clean(
                vector,
                &["javascript:", "data:text/html", "<script", "alert(1)"],
            );
        }
    }

    /// Обратный инвариант: безобидное содержимое harness обязан сохранять.
    #[test]
    fn table_benign_content_survives() {
        let cases: &[(&str, &[&str])] = &[
            ("[Tauri](https://tauri.app)", &["https://tauri.app"]),
            (
                "![alt](https://example.com/img.png)",
                &["https://example.com/img.png"],
            ),
            (r#"[x](http://e.com "Title")"#, &["http://e.com", "Title"]),
            (
                r#"<img src="photo.png" alt="картинка">"#,
                &["photo.png", "картинка"],
            ),
        ];
        for (input, expected) in cases {
            let html = to_html(input);
            for want in *expected {
                assert!(
                    html.contains(want),
                    "ожидался {want:?}\n  вход: {input}\n  HTML: {html}"
                );
            }
        }
    }

    // ─── Точечные тесты внутренних функций v2 ──────────────────────────

    #[test]
    fn decode_entities_handles_numeric_and_named() {
        assert_eq!(decode_entities("&#106;&#x61;va"), "java");
        assert_eq!(decode_entities("&colon;"), ":");
        assert_eq!(decode_entities("a&amp;b"), "a&b");
        assert_eq!(decode_entities("&unknown;"), "&unknown;");
    }

    #[test]
    fn url_scheme_check_rejects_obfuscation() {
        assert!(!url_is_safe("href", "javascript:alert(1)"));
        assert!(!url_is_safe("href", "java\tscript:alert(1)"));
        assert!(!url_is_safe("href", "data:text/html,x"));
        assert!(url_is_safe("href", "https://x/?a=1&b=2"));
        assert!(url_is_safe("href", "/relative/path"));
        assert!(url_is_safe("src", "data:image/png;base64,AAAA"));
        assert!(!url_is_safe("src", "data:text/html,x"));
    }

    #[test]
    fn style_allows_only_text_align() {
        assert!(style_is_safe("text-align: right"));
        assert!(style_is_safe("text-align:center"));
        assert!(!style_is_safe("background:red"));
        assert!(!style_is_safe("position:fixed"));
    }

    // ─── Режим инспектора: to_html_mapped (T-1…T-5) ────────────────────

    /// Извлекает `(start, end)`-пары из `data-md` **обёрток блоков**. Сужено
    /// намеренно: после разметки ячеек/строк таблиц (`tr`/`th`/`td`) `data-md`
    /// в документе уже не уникален, а блочные тесты должны видеть только
    /// `.md-block`.
    fn mapped_ranges(md: &str) -> Vec<(usize, usize)> {
        const WRAP: &str = "<div class=\"md-block\" data-md=\"";
        let html = to_html_mapped(md);
        let mut ranges = Vec::new();
        let mut rest = html.as_str();
        while let Some(pos) = rest.find(WRAP) {
            let after = &rest[pos + WRAP.len()..];
            let Some(quote) = after.find('"') else { break };
            let raw = &after[..quote];
            if let Some((s, e)) = raw.split_once(',') {
                if let (Ok(a), Ok(b)) = (s.parse::<usize>(), e.parse::<usize>()) {
                    ranges.push((a, b));
                }
            }
            rest = &after[quote..];
        }
        ranges
    }

    #[test]
    fn heading_and_paragraph_two_blocks() {
        let md = "# H\n\ntext";
        assert_eq!(mapped_ranges(md), vec![(0, 3), (5, 9)]);
        let html = to_html_mapped(md);
        assert_eq!(html.matches("md-block").count(), 2);
        assert!(html.contains("<h1"), "html: {html}");
        assert!(html.contains("<p>text</p>"), "html: {html}");
    }

    #[test]
    fn gfm_table_one_block() {
        let md = "| A |\n|---|\n| 1 |";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges, vec![(0, md.len())], "ranges: {ranges:?}");
        let html = to_html_mapped(md);
        assert_eq!(html.matches("md-block").count(), 1);
        assert!(html.contains("<table>"), "html: {html}");
    }

    #[test]
    fn nested_list_one_block() {
        let md = "- item\n  - sub\n  - sub2";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges, vec![(0, md.len())], "ranges: {ranges:?}");
        assert_eq!(to_html_mapped(md).matches("md-block").count(), 1);
    }

    #[test]
    fn blockquote_with_nested() {
        let md = "> quote\n> - item";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges, vec![(0, md.len())], "ranges: {ranges:?}");
        assert_eq!(to_html_mapped(md).matches("md-block").count(), 1);
    }

    #[test]
    fn fenced_code_block() {
        let md = "```\n# not heading\n```";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges, vec![(0, md.len())], "ranges: {ranges:?}");
        let html = to_html_mapped(md);
        assert_eq!(html.matches("md-block").count(), 1);
        assert!(
            html.contains("<pre><code># not heading\n</code></pre>"),
            "html: {html}"
        );
    }

    #[test]
    fn footnote_definition_separate_block() {
        let md = "Text[^1]\n\n[^1]: note";
        let html = to_html_mapped(md);
        // Основной абзац со ссылкой на сноску + определение сноски.
        assert_eq!(html.matches("md-block").count(), 2, "html: {html}");
        let ranges = mapped_ranges(md);
        assert_eq!(ranges.len(), 2, "ranges: {ranges:?}");
        assert_eq!(ranges[0], (0, 8), "ranges: {ranges:?}");
        assert_eq!(ranges[1], (10, 20), "ranges: {ranges:?}");
        assert!(html.contains("footnote"), "html: {html}");
    }

    #[test]
    fn setext_heading() {
        let md = "Title\n=====";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges, vec![(0, md.len())], "ranges: {ranges:?}");
        let html = to_html_mapped(md);
        assert!(html.contains("<h1>Title</h1>"), "html: {html}");
    }

    #[test]
    fn empty_input() {
        assert_eq!(to_html_mapped(""), "");
        assert_eq!(to_html_mapped("\n\n"), "");
    }

    #[test]
    fn trimmed_no_trailing_blank_lines() {
        let md = "# H\n\n\n\n";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges, vec![(0, 3)], "ranges: {ranges:?}");
    }

    #[test]
    fn utf8_boundary() {
        let md = "# Привет 🌍";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges.len(), 1, "ranges: {ranges:?}");
        let (s, e) = ranges[0];
        assert!(md.is_char_boundary(s), "start {s} не на границе символа");
        assert!(md.is_char_boundary(e), "end {e} не на границе символа");
        // Диапазон покрывает весь кириллический заголовок с эмодзи.
        assert_eq!(&md[s..e], "# Привет 🌍");
    }

    #[test]
    fn crlf_bytes() {
        let md = "# H\r\n\r\ntext\r\n";
        let ranges = mapped_ranges(md);
        // Байты: "# H"(0..3) \r\n \r\n "text"(7..11) \r\n.
        // trim_end не захватывает хвостовой `\r`.
        assert_eq!(ranges, vec![(0, 3), (7, 11)], "ranges: {ranges:?}");
        assert_eq!(&md[ranges[0].0..ranges[0].1], "# H");
        assert_eq!(&md[ranges[1].0..ranges[1].1], "text");
    }

    /// Снимает с mapped-HTML обёртки блоков, оставляя только содержимое.
    /// Учитывает вложенные `<div>` (например, `footnote-definition`).
    fn strip_mapped_wrappers(html: &str) -> String {
        const OPEN: &str = "<div class=\"md-block\" data-md=\"";
        let mut out = String::new();
        let mut rest = html;
        while let Some(pos) = rest.find(OPEN) {
            out.push_str(&rest[..pos]);
            let after = &rest[pos + OPEN.len()..];
            // после `data-md="…"` идёт `>` — пропускаем до него
            let Some(gt) = after.find('>') else { break };
            let inner = &after[gt + 1..];
            match find_matching_div_close(inner) {
                Some(end) => {
                    out.push_str(&inner[..end]);
                    rest = &inner[end + "</div>".len()..];
                }
                None => {
                    rest = inner;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Индекс парного `</div>` в `s` для уже открытого `<div>` (глубина = 1).
    fn find_matching_div_close(s: &str) -> Option<usize> {
        let bytes = s.as_bytes();
        let mut depth = 1usize;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(b"<div") {
                depth += 1;
                i += 4;
            } else if bytes[i..].starts_with(b"</div>") {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 6;
            } else {
                i += 1;
            }
        }
        None
    }

    /// Удаляет все атрибуты `data-md="…"` из HTML (вместе с предшествующим
    /// пробелом). Используется инвариантом «без разметки == to_html».
    fn strip_all_data_md(html: &str) -> String {
        const ATTR: &str = "data-md=\"";
        let mut out = String::with_capacity(html.len());
        let mut rest = html;
        while let Some(pos) = rest.find(ATTR) {
            // Срезаем один пробел-разделитель перед атрибутом, если он есть.
            let cut = if pos > 0 && rest.as_bytes()[pos - 1] == b' ' {
                pos - 1
            } else {
                pos
            };
            out.push_str(&rest[..cut]);
            let after = &rest[pos + ATTR.len()..];
            match after.find('"') {
                Some(q) => rest = &after[q + 1..],
                None => {
                    rest = after;
                    break;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Диапазоны `data-md` **на структурных тегах таблицы** (`tr`/`th`/`td`)
    /// в порядке документа: `(tag, start, end)`. Обёртки `.md-block` (тег `div`)
    /// игнорируются.
    fn table_ranges(html: &str) -> Vec<(String, usize, usize)> {
        let bytes = html.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'<' {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            if j < bytes.len() && bytes[j] == b'/' {
                j += 1;
            }
            let name_start = j;
            while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
                j += 1;
            }
            let name = html[name_start..j].to_ascii_lowercase();
            if matches!(name.as_str(), "tr" | "th" | "td") {
                // Конец открывающего тега (атрибуты могут содержать кавычки, но
                // `data-md` у нас всегда числовой и не содержит `>`).
                let mut k = j;
                while k < bytes.len() && bytes[k] != b'>' {
                    k += 1;
                }
                let tag = &html[i..k.min(bytes.len())];
                if let Some((_, tail)) = tag.split_once("data-md=\"") {
                    if let Some(q) = tail.find('"') {
                        if let Some((s, e)) = tail[..q].split_once(',') {
                            if let (Ok(a), Ok(b)) = (s.parse::<usize>(), e.parse::<usize>()) {
                                out.push((name, a, b));
                            }
                        }
                    }
                }
                i = k + 1;
                continue;
            }
            i += 1;
        }
        out
    }

    /// Содержимое блоков (без обёрток) обязано побайтово совпадать с `to_html`.
    /// Проверяем на документах, задевающих все ветки парсера: ссылочные
    /// определения, картинки, сноски, `<hr>`, HTML-комментарии.
    #[test]
    fn mapped_matches_to_html_regression() {
        for md in [
            "# H\n\ntext\n\n| A |\n|---|\n| 1 |\n\n> quote",
            "[foo]: /url\n\nUse [foo] here.",
            "![img][logo]\n\n[logo]: /a.png",
            "Text[^1]\n\n[^1]: note",
            "text\n\n***\n\nmore",
            "before\n\n<!-- hidden -->\n\nafter",
            // Таблицы: вложенная разметка tr/th/td не должна менять содержимое.
            "| A | B |\n|---|---|\n| 1 | 2 |",
            "| Файл | Размер |\n|------|-------:|\n| README.md | 2 КБ |",
            "| A\\|B | C |\n|---|---|\n| a | b |",
            "| 🚀 | Привет 🎯 |\n|---|---|\n| a | b |",
            "| **жирный** | `код` |\n|---|---|\n| a | b |",
        ] {
            let mapped = to_html_mapped(md);
            let plain = to_html(md);
            let content = strip_all_data_md(&strip_mapped_wrappers(&mapped));
            assert_eq!(content, plain, "несовпадение для входа {md:?}");
        }
    }

    #[test]
    fn reference_definition_resolves_link() {
        let md = "[foo]: /url\n\nUse [foo] here.";
        let html = to_html_mapped(md);
        assert!(
            html.contains("href=\"/url\""),
            "ссылка не разрешилась: {html}"
        );
        assert!(!html.contains("[foo]"), "осталась сырая ссылка: {html}");
    }

    #[test]
    fn reference_definition_resolves_image() {
        let md = "![img][logo]\n\n[logo]: /a.png";
        let html = to_html_mapped(md);
        assert!(
            html.contains("src=\"/a.png\""),
            "картинка не разрешилась: {html}"
        );
    }

    #[test]
    fn footnote_reference_resolves_and_labels_stay() {
        let md = "Text[^1]\n\n[^1]: note";
        let html = to_html_mapped(md);
        assert!(
            html.contains("footnote-reference"),
            "нет ссылки на сноску: {html}"
        );
        assert!(
            html.contains("footnote-definition-label\">1<"),
            "метка определения испорчена: {html}"
        );
    }

    #[test]
    fn rule_is_indexed() {
        let md = "text\n\n***\n\nmore";
        let ranges = mapped_ranges(md);
        assert_eq!(ranges, vec![(0, 4), (6, 9), (11, 15)], "ranges: {ranges:?}");
        let html = to_html_mapped(md);
        assert_eq!(html.matches("<hr").count(), 1, "html: {html}");
        assert_eq!(html.matches("md-block").count(), 3, "html: {html}");
    }

    #[test]
    fn html_comment_yields_no_empty_block() {
        let md = "before\n\n<!-- hidden -->\n\nafter";
        let html = to_html_mapped(md);
        assert_eq!(
            html.matches("md-block").count(),
            2,
            "пустая обёртка от комментария: {html}"
        );
        assert!(!html.contains("hidden"), "комментарий просочился: {html}");
        assert!(
            !html.contains("data-md=\"\"") && !html.contains("></div>"),
            "html: {html}"
        );
    }

    fn assert_blocks_cover_nonempty(md: &str) {
        let ranges = mapped_ranges(md);

        // Диапазоны не пересекаются и упорядочены.
        for pair in ranges.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "наложение: {pair:?}");
        }

        // Каждый диапазон покрывает только «содержательные» байты.
        for (s, e) in &ranges {
            assert!(md.is_char_boundary(*s) && md.is_char_boundary(*e));
            assert!(!md[*s..*e].trim().is_empty(), "пустой блок {s}..{e}");
        }

        // Каждая непустая строка целиком попадает хотя бы в один диапазон.
        // (Диапазон может захватывать и пустые строки — например, ссылочное
        // определение, приклеенное к соседнему блоку.)
        let mut pos = 0usize;
        for segment in md.split_inclusive('\n') {
            let no_nl = segment.strip_suffix('\n').unwrap_or(segment);
            let content = no_nl.strip_suffix('\r').unwrap_or(no_nl);
            if !content.trim().is_empty() {
                let line_start = pos;
                let line_end = pos + content.len();
                assert!(
                    ranges
                        .iter()
                        .any(|&(s, e)| s <= line_start && line_end <= e),
                    "строка {content:?} не покрыта: {ranges:?} в {md:?}"
                );
            }
            pos += segment.len();
        }
    }

    #[test]
    fn mapped_blocks_cover_nonempty_without_overlap() {
        for md in [
            "# A\n\ntext\n\n| X |\n|---|\n| 1 |\n\n- a\n- b",
            "text\n\n***\n\nmore",
            "[foo]: /url\n\nUse [foo] here.",
            "![img][logo]\n\n[logo]: /a.png",
            "Text[^1]\n\n[^1]: note",
        ] {
            assert_blocks_cover_nonempty(md);
        }
    }

    // ─── Режим инспектора: гранулярность таблиц (TZ-inspect-tables) ─────
    //
    // Ядро должно разметить tr/th/td атрибутом data-md (байтовые диапазоны
    // парсера). Тесты красные до реализации §3.1 ТЗ — это TDD-порядок.

    /// Порядок и содержимое вложенной разметки: тег → исходный фрагмент.
    #[test]
    fn table_rows_and_cells_are_mapped() {
        let md = "| A | B |\n|---|---|\n| a1 | b1 |\n| a2 | b2 |";
        let html = to_html_mapped(md);
        let got: Vec<(String, String)> = table_ranges(&html)
            .into_iter()
            .map(|(tag, s, e)| (tag, md[s..e].to_string()))
            .collect();
        let want: Vec<(String, String)> = [
            ("th", "A"),
            ("th", "B"),
            ("tr", "| a1 | b1 |"),
            ("td", "a1"),
            ("td", "b1"),
            ("tr", "| a2 | b2 |"),
            ("td", "a2"),
            ("td", "b2"),
        ]
        .into_iter()
        .map(|(t, v)| (t.to_string(), v.to_string()))
        .collect();
        assert_eq!(got, want, "html: {html}");
    }

    /// Вложенные диапазоны не выходят за пределы обёртки `.md-block`.
    #[test]
    fn table_inner_ranges_nested_in_block() {
        let md = "| A |\n|---|\n| a |";
        let html = to_html_mapped(md);
        let blocks = mapped_ranges(md);
        assert_eq!(blocks.len(), 1, "blocks: {blocks:?}");
        let (bs, be) = blocks[0];
        let inner = table_ranges(&html);
        assert!(!inner.is_empty(), "нет разметки ячеек: {html}");
        for (tag, s, e) in &inner {
            assert!(
                bs <= *s && *e <= be,
                "диапазон {tag} {s}..{e} вне блока {bs}..{be}"
            );
        }
    }

    /// Диапазоны ячеек/строк — на границах UTF-8 (кириллица, эмодзи).
    #[test]
    fn table_ranges_on_char_boundaries() {
        let md = "| 🚀 | Привет 🎯 |\n|---|---|\n| a | b |";
        let html = to_html_mapped(md);
        let ranges = table_ranges(&html);
        assert!(!ranges.is_empty(), "html: {html}");
        for (tag, s, e) in &ranges {
            assert!(
                md.is_char_boundary(*s) && md.is_char_boundary(*e),
                "граница {tag} {s}..{e} не на символе"
            );
            assert!(md.get(*s..*e).is_some(), "срез {tag} {s}..{e} невалиден");
        }
    }

    /// Пустая ячейка не ломает соответствие тегов и диапазонов.
    #[test]
    fn empty_cell_keeps_structure_order() {
        let md = "| A | B |\n|---|---|\n| | x |";
        let html = to_html_mapped(md);
        let tags: Vec<String> = table_ranges(&html).into_iter().map(|(t, ..)| t).collect();
        assert_eq!(tags, vec!["th", "th", "tr", "td", "td"], "html: {html}");
    }

    /// Таблица внутри цитаты тоже размечается; строка — без префикса `> `.
    #[test]
    fn table_inside_blockquote_is_mapped() {
        let md = "> | A |\n> |---|\n> | a |";
        let html = to_html_mapped(md);
        let ranges = table_ranges(&html);
        assert!(!ranges.is_empty(), "html: {html}");
        let (_, s, e) = ranges
            .iter()
            .find(|(t, ..)| t == "tr")
            .expect("нет размеченной строки");
        assert_eq!(&md[*s..*e], "| a |", "html: {html}");
    }

    /// Обычные блоки не получают вложенной разметки.
    #[test]
    fn plain_blocks_have_no_inner_ranges() {
        let md = "# H\n\ntext";
        let html = to_html_mapped(md);
        assert!(table_ranges(&html).is_empty(), "html: {html}");
        assert_eq!(mapped_ranges(md), vec![(0, 3), (5, 9)]);
    }

    /// AC-9: ячейка со смешанной inline-разметкой выделяется целиком —
    /// вместе с маркерами (`**`, бэктики), а не только по «внутреннему» тексту.
    #[test]
    fn cell_with_inline_markup_maps_full_source() {
        let md = "| **b** | `c` |\n|---|---|\n| x | y |";
        let html = to_html_mapped(md);
        let got: Vec<(String, String)> = table_ranges(&html)
            .into_iter()
            .filter(|(t, ..)| t == "th")
            .map(|(t, s, e)| (t, md[s..e].to_string()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("th".to_string(), "**b**".to_string()),
                ("th".to_string(), "`c`".to_string()),
            ],
            "html: {html}"
        );
    }

    /// AC-11: экранированный `\|` — ядро берёт диапазоны парсера, а не наивный
    /// split по `|`, поэтому ячейка мапится целиком.
    #[test]
    fn escaped_pipe_cell_maps_correctly() {
        let md = "| A\\|B | C |\n|---|---|\n| a | b |";
        let html = to_html_mapped(md);
        let got: Vec<(String, String)> = table_ranges(&html)
            .into_iter()
            .filter(|(t, ..)| t == "th")
            .map(|(t, s, e)| (t, md[s..e].to_string()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("th".to_string(), "A\\|B".to_string()),
                ("th".to_string(), "C".to_string()),
            ],
            "html: {html}"
        );
    }

    /// AC-10: CRLF — диапазоны строк и ячеек не захватывают `\r`/`\n`.
    #[test]
    fn crlf_table_ranges_trim_line_endings() {
        let md = "| A |\r\n|---|\r\n| a |\r\n";
        let html = to_html_mapped(md);
        let rows: Vec<String> = table_ranges(&html)
            .into_iter()
            .filter(|(t, ..)| t == "tr" || t == "th" || t == "td")
            .map(|(_, s, e)| md[s..e].to_string())
            .collect();
        assert_eq!(rows, vec!["A", "| a |", "a"], "html: {html}");
    }

    /// Несколько таблиц в документе размечаются независимо: курсор диапазонов
    /// не «перетекает» между блоками.
    #[test]
    fn multiple_tables_each_mapped() {
        let md = "| A |\n|---|\n| a |\n\n| B |\n|---|\n| b |";
        let html = to_html_mapped(md);
        let cells: Vec<String> = table_ranges(&html)
            .into_iter()
            .filter(|(t, ..)| t == "th" || t == "td")
            .map(|(_, s, e)| md[s..e].to_string())
            .collect();
        assert_eq!(cells, vec!["A", "a", "B", "b"], "html: {html}");
    }

    /// Выравнивание колонок (`style="text-align: …"`) не мешает инъекции:
    /// `data-md` оказывается на ячейке, диапазон корректен.
    #[test]
    fn aligned_cells_are_mapped() {
        let md = "| A | B |\n|:--|--:|\n| x | y |";
        let html = to_html_mapped(md);
        assert!(
            html.contains("text-align: right"),
            "выравнивание потеряно: {html}"
        );
        let cells: Vec<String> = table_ranges(&html)
            .into_iter()
            .filter(|(t, ..)| t == "th" || t == "td")
            .map(|(_, s, e)| md[s..e].to_string())
            .collect();
        assert_eq!(cells, vec!["A", "B", "x", "y"], "html: {html}");
    }

    /// T-4: таблица внутри списка тоже размечается; строка — без отступа списка.
    #[test]
    fn table_inside_list_is_mapped() {
        let md = "- | A |\n  |---|\n  | a |";
        let html = to_html_mapped(md);
        let ranges = table_ranges(&html);
        assert!(!ranges.is_empty(), "html: {html}");
        let (_, s, e) = ranges
            .iter()
            .find(|(t, ..)| t == "tr")
            .expect("нет размеченной строки");
        assert_eq!(&md[*s..*e], "| a |", "html: {html}");
    }

    /// AC-14: большая таблица (500×8) размечается полностью — каждая строка и
    /// ячейка. Тайминговый порог не проверяем (нестабилен), но полнота разметки
    /// ловит случайную квадратичность/пропуски на большом входе.
    #[test]
    fn large_table_maps_every_row_and_cell() {
        const COLS: usize = 8;
        const ROWS: usize = 500;

        let mut header = String::from("|");
        for c in 0..COLS {
            header.push_str(&format!(" H{c} |"));
        }
        let mut sep = String::from("|");
        for _ in 0..COLS {
            sep.push_str("---|");
        }
        let mut md = format!("{header}\n{sep}\n");
        for r in 0..ROWS {
            md.push('|');
            for c in 0..COLS {
                md.push_str(&format!(" r{r}c{c} |"));
            }
            md.push('\n');
        }

        let html = to_html_mapped(&md);
        let ranges = table_ranges(&html);
        let count = |tag: &str| ranges.iter().filter(|(t, ..)| t == tag).count();
        assert_eq!(count("th"), COLS, "шапка размечена не полностью");
        assert_eq!(count("td"), ROWS * COLS, "ячейки размечены не полностью");
        assert_eq!(count("tr"), ROWS, "строки размечены не полностью");
        for (tag, s, e) in &ranges {
            assert!(
                md.is_char_boundary(*s) && md.is_char_boundary(*e),
                "граница {tag} {s}..{e} не на символе"
            );
        }
    }
}
