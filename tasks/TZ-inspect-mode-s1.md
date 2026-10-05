## Разбиение задачи на этапы

| Этап | Область | Что входит | Результат |
|------|---------|------------|-----------|
| **Этап 1 — Ядро** | `crates/md-core/src/lib.rs` | `to_html_mapped()`, разбиение на топ-блоки, 8+ тестов | `cargo test -p md-core` зелёный |
| **Этап 2 — IPC + модуль inspector** | `src-tauri/src/lib.rs`, `tauri.ts`, новый `inspector.ts` | параметр `mapped`, `createInspector()`, hover/selection-маппинг, byte↔UTF-16 | модуль готов, API `createInspector` стабилен |
| **Этап 3 — UI + интеграция** | `index.html`, `style.css`, `main.ts` | кнопка тулбара, хоткеи, `.inspect-active`, подключение `inspector.ts` | `npm run build` зелёный, AC-1..AC-9 пройдены вручную |

---

## Этап 1 — Детальный план работ

### Цель
Реализовать публичную функцию `to_html_mapped(md: &str) -> String` в `crates/md-core/src/lib.rs` и покрыть её тестами.

### Что меняется
- Один файл: `crates/md-core/src/lib.rs`
- Ни один другой файл не трогается
- Никаких новых зависимостей

### Алгоритм `to_html_mapped`

```
1. Собрать все OffsetItem из Parser::new_ext(md, DEFAULT_OPTIONS).into_offset_iter()
2. Пройти по событиям, отслеживая depth (начинаем с 0):
   a. Event::Start — depth++, если depth == 1 → это начало нового топ-блока (запомнить start)
   b. Event::End — если depth == 1 → конец блока (запомнить end), depth--
   c. Все остальные события при depth == 1 → относятся к текущему блоку (игнорируем для границ)
3. Если depth > 0 в конце (незакрытый блок) → добираем до md.len()
4. Для каждого блока (start, end):
   a. Вырезать slice = md[start..end]
   b. trimmed = slice.trim_end() — убрать хвостовые пустые строки
   c. Если trimmed пуст — пропустить блок
   d. end_trimmed = start + trimmed.len()
   e. html_out = html::push_html(Parser::new_ext(trimmed, DEFAULT_OPTIONS))
   f. result += `<div class="md-block" data-md="{start},{end_trimmed}">` + sanitize_html(html_out) + `</div>`
5. Вернуть result
```

### Инварианты (зафиксировать в коде как комментарии)

| Инвариант | Проверка |
|-----------|----------|
| Детерминированность: одинаковый вход → одинаковый выход | regression-тест |
| Вывод без обёрток совпадает с `to_html()` | сравнение строк |
| `data-md` содержит только `[0-9,]` | безопасный HTML |
| Границы — UTF-8 char boundaries | гарантия OffsetIter pulldown-cmark |
| Пустые блоки не создаются | проверка `trimmed.is_empty()` |

### Тесты (минимум 8 из T-4 + регрессия)

| # | Название теста | Markdown-вход | Что проверяем |
|---|----------------|---------------|---------------|
| 1 | `heading_and_paragraph_two_blocks` | `# H\n\ntext` | 2 блока с правильными границами |
| 2 | `gfm_table_one_block` | `| A |\n|---|\n| 1 |` | 1 блок на всю таблицу |
| 3 | `nested_list_one_block` | `- item\n  - sub\n  - sub2` | 1 блок на весь вложенный список |
| 4 | `blockquote_with_nested` | `> quote\n> - item` | 1 блок на `<blockquote>` с вложенным списком |
| 5 | `fenced_code_block` | ````\n``\n# not heading\n```` | 1 блок, внутренние `#` не режут |
| 6 | `footnote_definition` | `Text[^1]\n\n[^1]: note` | Footnote-def — отдельный блок |
| 7 | `setext_heading` | `Title\n=====` | range включает обе строки |
| 8 | `empty_input` | `""` | пустая строка → пустой результат |
| 9 | `matches_to_html_regression` | Любой | `to_html_mapped(md).replace(".md-block\" data-md=\"...\">", "").replace("</div>", "")` ≈ `to_html(md)` |
| 10 | `trimmed_no_trailing_blank_lines` | `# H\n\n\n\n` | trim_end не захватывает пустые строки |
| 11 | `utf8_boundary` | `# Привет 🌍` | границы на char boundary |
| 12 | `crlf_bytes` | `# H\r\n\r\ntext\r\n` | байтовые смещения корректны |

### Проверка этапа
- `cargo test -p md-core` — зелёный
- `cargo check -p md-core` — без warnings (или warnings те же, что и до)

### Что НЕ входит в этап 1
- Никаких изменений в `src-tauri/`
- Никаких изменений во фронтенде
- Никаких IPC-изменений
- Никаких UI-кнопок
