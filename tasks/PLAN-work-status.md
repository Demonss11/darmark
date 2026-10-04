# План работ — статус по TZ-fixes (после ревью v1.2)

Обновлено: 2026-10-04. Основа: `tasks/TZ-fixes.md` v1.2, `reviews/TZ-fixes-review-v1.2.md`.
HEAD: `c4eb434` («fixes»). Все перечисленные ниже правки — **незакоммичены**.

## 1. Сделано

### 1.1 Замечания ревью

| # | Замечание | Статус |
|---|-----------|--------|
| 3.1 | Противоречие P2.3 (`union()` в `DEFAULT_OPTIONS`) | ✅ Устранено документационно: `union()` оставлен осознанно — в bitflags 2.13 `BitOr` не `const fn`. Формулировки в статус-листе и в тексте P2.3 унифицированы |
| 3.2 | Статусы P0.1/P0.2 завышены (GUI не проверялся) | ✅ В статус-лист добавлены строки «P0.1 GUI-чек 🔧 частично», «P0.2 GUI-чек 🔧 частично» |

### 1.2 Код: реальный дефект санитайзера (найден при проверке 3.1)

`remove_event_handlers` и `sanitize_urls_in_tag` смешивали **байтовые** индексы
(`find`, `as_bytes`, `tag.len()`) и **символьный** вывод `tag.chars().nth(i).unwrap()`.
На валидном кириллическом атрибуте (`[текст](http://x.com "заголовок")`) это давало панику
`start byte index 31 is not a char boundary; it is inside 'з'`.

- Обе функции заменены одной char-безопасной `rewrite_tag(tag: &str) -> String` на `Vec<char>`;
  добавлен хелпер `is_name_char`. `sanitize_tag` вызывает `rewrite_tag(&tag)`.
- `crates/app/src-tauri/src/lib.rs`: `base_roots` — `manual_flatten` → `.into_iter().flatten()`.
- Убран `unused_assignments` в `rewrite_tag` (лишнее `i += 1` перед `break`).

### 1.3 Регрессионные тесты (все 5 — новые, в `crates/md-core`)

`cyrillic_link_title_survives`, `cyrillic_img_alt_survives`, `emoji_in_attribute_survives`,
`event_handler_after_cyrillic_is_still_stripped`,
`javascript_href_with_cyrillic_context_neutralized`.

### 1.4 Прогнанные проверки

| Проверка | Результат |
|----------|-----------|
| `cargo test --workspace` | ✅ 26 тестов (19 md-core + 7 mdedit_lib) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ чисто |
| `cargo fmt --all` | ✅ применён |
| `npm run build` (`tsc && vite build`) | ✅ успешно |
| GUI-чек-лист (`npx tauri dev`) | ⚠️ **не прогонялся** — headless-среда |

> `tasks/TZ-fixes.md` по-прежнему указывает «21 тест (14+7)» — цифру надо обновить на 26 (19+7).

### 1.5 Незакоммиченное (`git status`)

- `M crates/md-core/src/lib.rs` — правки санитайзера + регресс-тесты
- `M crates/app/src-tauri/src/lib.rs` — clippy-фикс
- `M tasks/TZ-fixes.md` — унификация P2.3, честные статусы GUI-чека
- `M .kodacli/settings.json`
- `A reviews/TZ-fixes-review-v1.2.md`, `A .koda/skills/qa/*`, `A .koda/skills/review/SKILL.md`
- `?? .koda/skills/frontend-*/`

## 2. Надо сделать

### 2.1 Обязательное (блокирует закрытие TZ-fixes)

1. **Прогнать GUI-чек-лист через `npx tauri dev`** (требует WebView2):
   - [ ] P0.1: кириллический заголовок/`alt` в предпросмотре не рвётся и не паникует
   - [ ] P0.1: `onerror`/`onclick` в HTML-блоке не исполняются
   - [ ] P0.2: ресайз колонок сохраняется после перерендера
   - [ ] P0.2: sticky-первый столбец и сортировка работают вместе
   - [ ] P1.4: мини-поиск по таблице — пошагово, см. п. 4
   - Только после этого P0.1/P0.2 переводятся в ✅.
2. **Коммит и push** — по явной команде пользователя (сейчас не делать).
3. **`tauri-plugin-opener` вместо `window.open`** для внешних ссылок:
   `window.open` в Tauri 2 не открывает системный браузер и обходит CSP.
   Требует явного согласия на новую зависимость (см. `.kodarules`).
4. **README: пошаговый сценарий проверки P1.4** (мини-поиск), чтобы AC был воспроизводимым,
   а не «проверить, что работает».

### 2.2 Технический долг (не блокирует, но фиксировать)

5. **Санитайзер остаётся строковым и слабым.** `rewrite_tag` — эвристика по тексту тега;
   она не покрывает вложенные/незакрытые теги, CDATA, контексты `<svg>`/`<math>`.
   Кандидаты на замену: `ammonia` (Rust, в ядре) или DOMPurify (но он ломает CSP-бюджет
   и требует front-зависимость). Решение — отдельное ТЗ.
6. **`O(n²)` в `rewrite_tag`:** `chars().nth(i)` внутри цикла. Работает на реальных теггах,
   но при удлинении HTML вырастет линейно-квадратично. Рефакторинг — единый проход
   по `char_indices()` либо перепись на `html::write_event`-подобный писатель.
7. **Нет полного покрытия `preserves_*`:** тесты проверяют отдельные case'ы,
   нет свойственного/табличного прогона «никакой `on*=` не проходит ни в одном контексте».
   Полезно завести harness-прогон поверх `md-core` (уже в дорожной карте README).

## 3. Краткий чек-лист следующего шага

- [ ] `npx tauri dev` → GUI-чек-лист п. 2.1 (5 пунктов)
- [ ] Обновить счётчик тестов в `tasks/TZ-fixes.md`: 21 → 26
- [ ] Решить по `tauri-plugin-opener` (зависимость → нужно согласие)
- [ ] Дописать README-сценарий P1.4
- [ ] Коммит (по запросу)