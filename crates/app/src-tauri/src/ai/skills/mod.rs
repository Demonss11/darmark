//! Встроенная библиотека скиллов (готовых промтов), ADR-0032.
//!
//! Epic 3 — **5** скиллов: `explain`, `create-plugin`, `restructure`,
//! `explain-syntax` (категория `document`), `debug-plugin` (категория `plugins`).
//! Промты встроены в код (редактора скиллов нет); формат — как в ADR-0032.

use crate::ai::types::AiSkill;

/// Промт `explain`: подстановка `{{doc}}` — текущий текст документа.
///
/// Формат ответа задан явно (структура из 4 частей): от модели без рамок трудно получить
/// предсказуемый разбор — она либо пересказывает документ, либо растекается. Секции + правило
/// «не выдумывай и не перепечатывай» делают ответ стабильным и различают «о чём» и «какая разметка».
const EXPLAIN_PROMPT: &str = r#"Ты — разборщик Markdown-документов в редакторе ПЕНАЛ. Ниже документ; объясни его простыми
словами, опираясь ТОЛЬКО на этот текст (содержимое вне документа не подмешивай).

Ответь на русском ровно по этой структуре, без вступлений и извинений:
1. О чём — суть в 1–3 предложениях.
2. Структура — какие разделы есть (по заголовкам) и как они связаны.
3. Разметка — какие элементы Markdown использованы и что они дают именно здесь (заголовки, списки,
   таблицы, блоки кода, ссылки, цитаты, YAML-шапка).
4. На что обратить внимание — неочевидные или спорные места; если их нет, скажи прямо.

Правила: не выдумывай то, чего в тексте нет; не перепечатывай документ целиком и не переписывай его;
если документ пуст или почти пуст — ответь одной строкой об этом.

Документ:
{{doc}}"#;

/// Промт `create-plugin`: подстановка `{{input}}` — описание плагина из чата.
///
/// Контракт ответа (ADR-0034/ADR-0026): модель возвращает **только** один fenced-блок ```` ```json ````
/// с объектом `{ id, name, version, permissions, contributes?, source }` — его парсит фронт.
///
/// Промт намеренно задаёт контракт точки входа, перечень реальных host-функций (см. `crates/plugin-core`
/// и ADR-0023 — сам промт и есть референс host-API для генератора) и разводит три «похожих» механизма
/// (view / статусбар / команда). Без этого модель домысливает
/// модуль-таблицу (`local M ... return M`), чужой API (`host.ui`/`host.view`/`host.document`) и путает
/// `set_view_content` со статусбаром — плагин либо отвергается загрузкой, либо не работает.
/// Валидировано циклом `skill-creator` (эвалы create-plugin view/статусбар + explain).
const CREATE_PLUGIN_PROMPT: &str = r#"Ты — генератор Lua-плагинов для ПЕНАЛ (Lua 5.5 в песочнице; нет os/io/require/load/loadfile/dofile/coroutine/debug/package, нет host.document/host.ui/host.view/host.commands). По описанию: {{input}}

Верни РОВНО один fenced-блок ```json с одним объектом (никакого текста вне блока):
{"id": "…", "name": "…", "version": "1.0.0", "permissions": [], "source": "полный код main.lua"}

Контракт `source` (иначе плагин отвергается загрузкой):
- Точка входа — ГЛОБАЛЬНАЯ функция: function on_activate(ctx) ... end.
  Запрещено: модуль-таблица (local M = {} ... return M), M.on_activate, local on_activate, trailing return.
- Опционально глобальные: on_deactivate(ctx), on_event(name, payload), on_action(view_id, action, payload).
- on_activate держи лёгким: чтение документа делай в обработчиках событий, а не на активации.
- Код должен компилироваться: без require/os/io, без вызовов несуществующих функций.
- host-функции с разрешением возвращают (value, err): используй значение только при err == nil; при отказе
  не падай — сообщи через host.show_message и продолжай.

Доступен ТОЛЬКО этот API:
- ctx.subscribe(name, handler) -> unsubscribe; handler(name, payload).
  События: document:changed {doc_id, rev}, document:opened {doc_id, path},
  document:closed {doc_id, path}, command:invoked {command_id, doc_id}.
- host.get_document_len(doc_id) -> (integer, err)
- host.get_document_range(doc_id, start, len) -> (string, err)  -- основной путь чтения; смещения байтовые (UTF-8)
- host.get_document_text(doc_id) -> (string, err)
- host.get_document_version(doc_id) -> (integer, err)
- host.apply_edit(doc_id, start, stop, text) -> (boolean, err)  -- document:write
- host.set_view_content(view_id, html) -> (boolean, err)        -- ТОЛЬКО для вкладок (view:modify)
- host.show_message(text) -> (boolean, err)                     -- текст в статусбар (ui:statusbar)
- host.log(level, message), host.get_setting(key), host.set_setting(key, value), host.export_html(html)
- md.to_html(text), md.to_html_mapped(text), json.encode(value), json.decode(s)
- Смещения start/stop/len — байты UTF-8; apply_edit(doc_id, start, stop, text) заменяет полуинтервал
  [start, stop) на text.
- get_document_range отдаёт максимум 1 МиБ за вызов: короткий документ читай целиком через
  get_document_text, большой — циклом окон (len = get_document_len(id); pos = 0; пока pos < len:
  chunk = get_document_range(id, pos, 65536), pos = pos + #chunk; в конце table.concat).
- export_html: true — файл записан, false — пользователь отменил диалог сохранения.

Не путай три механизма:
- Вкладка (view): объяви в contributes.views [{"kind": "<kind>", "title": "…", "tier": 1}] (ровно эти поля),
  permission view:create; содержимое пиши через host.set_view_content(VIEW_ID, html) (нужен view:modify), где
  VIEW_ID = "<id плагина>:<kind>": <id плагина> — это поле id из этого же JSON, <kind> — из contributes.views.
  HTML санитизируется на хосте: скрипты, javascript:, on* вырезаются.
  Клик по элементу с data-p-<plugin_id>-action="foo" (и опциональным data-p-<plugin_id>-payload="…")
  в HTML view → on_action(view_id, "foo", payload).
- Статусбар: короткий текст показывай через host.show_message(text) (permission ui:statusbar);
  host.set_view_content для статусбара НЕ используй.
- Команда: объяви в contributes.commands [{"id": "<id>.<команда>", "title": "…",
  "keybinding": "Ctrl+Alt+W"}] (отдельный permission не нужен) и обрабатывай command:invoked {command_id}.

permissions — только из набора: document:read, document:write, view:create, view:modify, ui:statusbar.
- contributes.statusbar / toolbar / settings не добавляй: статусбар — только host.show_message,
  toolbar и settings GUI не рендерит.
id — только [a-z0-9_-]. Если вкладка/команда/статусбар не нужны — ключ contributes не добавляй.

Пример каркаса (подстрой под задачу, не копируй дословно):
```lua
local VIEW_ID = "my-plugin:stats"   -- "<id плагина>:<kind>"

function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local text, err = host.get_document_text(payload.doc_id)
    if err == nil and text then host.set_view_content(VIEW_ID, "<p>длина " .. #text .. "</p>") end
  end)
end
```"#;

/// Промт `restructure`: подстановка `{{doc}}` — текущий текст документа.
///
/// Ответ — план новой структуры + переписанный документ целиком. Markdown **без** fenced-блоков:
/// фронт ищет карточку плагина по ```json, и случайный блок кода в ответе мог бы её спровоцировать.
const RESTRUCTURE_PROMPT: &str = r#"Ты — редактор Markdown-документов в ПЕНАЛ. Ниже документ; переструктурируй его:
предложи новую структуру и перепиши документ целиком, сохранив всё содержимое и смысл.

Ответь на русском ровно по этой структуре:
1. Новая структура — план разделов (заголовки по уровням, коротко: что где стоит).
2. Переписанный документ — полный текст документа в новой структуре.

Правила:
- Не выдумывай и не удаляй содержимое: каждый факт и фрагмент исходника должен сохраниться.
- Не перепечатывай исходник без изменений — единственное допустимое изменение это новая структура.
- Не используй fenced-блоки (```) и не вставляй чужую разметку — ответ должен быть чистым Markdown.
- Не добавляй вступлений, пояснений вне двух секций, извинений.
- Если документ пуст — ответь одной строкой об этом.

Документ:
{{doc}}"#;

/// Промт `explain-syntax`: подстановка `{{doc}}` — текущий текст документа.
///
/// Проверка корректности **разметки** по фрагментам: чем каждый элемент Markdown отличается от
/// остального текста и что он даёт. В отличие от `explain` (о чём документ), здесь фокус на синтаксисе.
const EXPLAIN_SYNTAX_PROMPT: &str = r#"Ты — анализатор разметки Markdown в ПЕНАЛ. Ниже документ; разбери его разметку
по фрагментам: какие элементы использованы и что они дают именно здесь.

Ответь на русском ровно по этой структуре (для каждого типа элементов — отдельный пункт):
1. Заголовки — сколько уровней, какая иерархия разделов, корректна ли вложенность.
2. Списки — маркированные/нумерованные/вложенные, где и зачем.
3. Таблицы — где стоят, сколько колонок, выровнены ли.
4. Блоки кода — с указанием языка или без, что внутри, не сломано ли открытие/закрытие.
5. Ссылки и изображения — куда ведут, корректен ли синтаксис.
6. Цитаты — где стоят, вложенность.
7. YAML-шапка — есть ли, какие поля, корректна ли.
8. Прочее — жирный/курсив/зачёркнутый/инлайн-код, горизонтальные линии, HTML-вставки.

Правила:
- Не выдумывай: разбирай только то, что есть в документе.
- Не перепечатывай документ целиком — только указание фрагмента и его роль.
- Если тип элементов отсутствует — пропускай пункт, не пиши про него.

Документ:
{{doc}}"#;

/// Промт `debug-plugin`: подстановка `{{input}}` — ввод из чата.
///
/// Источник кода — MCP-инструмент `plugin_read`, который модель зовёт **первым ходом** по
/// `id` плагина (решение C8, TASK-003 §3): промт даёт инструкцию и fallback-чеклист, но не
/// принуждает вызов. Формат ответа «симптом → причина → исправление» текстом: карточка
/// плагина (`pluginProposal.ts`) рисуется только при **полном** `source` в ```json-блоке,
/// которого отладка не просит.
const DEBUG_PLUGIN_PROMPT: &str = r#"Ты — отладчик Lua-плагинов для ПЕНАЛ (Lua 5.5 в песочнице).
Найди причины, по которым плагин не загружается, падает или ничего не делает, и предложи исправления.

Первым ходом вызови инструмент `plugin_read` с `id` плагина — он вернёт `plugin.json` и
`main.lua`. Дальше диагностируй строго по полученному полю `source`: не рассуждай о коде,
которого не видел.

Fallback-чеклист (исходник не получен):
- Инструмент недоступен, `id` неизвестен или пользователь не назвал плагин → попроси вставить
  в чат код `main.lua` и содержимое `plugin.json`.
- Пока кода нет — не выдумывай причины: перечисли, каких данных не хватает, и задай вопрос
  (id плагина, текст ошибки из журнала, статус плагина в менеджере).
- Если пользователь описал только симптом — сначала назови, какую информацию нужно собрать.

Ответь на русском ровно по этой структуре для каждой найденной проблемы:
- Симптом: что видит пользователь (статус «ошибка»/«карантин», плагина нет в списке,
  вкладка/статусбар не появляются, «ничего не происходит», отказ host-вызова).
- Причина: конкретное место в коде, которое это вызывает.
- Исправление: что именно изменить (короткий фрагмент кода или точная инструкция).

Типичные причины отказа загрузкой (проверяй в первую очередь):
- Точка входа не глобальная: local on_activate, модуль-таблица (local M = {} ... return M), M.on_activate.
  Нужна ГЛОБАЛЬНАЯ функция: function on_activate(ctx) ... end.
- Запрещённые библиотеки: require, os, io, load, loadfile, dofile, coroutine, debug, package.
- Несуществующие host-функции: host.document, host.ui, host.view, host.commands, host.fs, host.net.
  Доступны только: ctx.subscribe, host.get_document_len, host.get_document_range, host.get_document_text,
  host.get_document_version, host.apply_edit, host.set_view_content, host.show_message, host.log,
  host.get_setting, host.set_setting, host.export_html, md.to_html, md.to_html_mapped, json.encode, json.decode.
- Запрещённые permissions: ui:menu, ui:sidebar, ui:toolbar, filesystem:*, network.
  Допустимы только: document:read, document:write, view:create, view:modify, ui:statusbar.
- view_id не соответствует контракту: должен быть "<id плагина>:<kind>", где kind из contributes.views.
- Ошибки в манифесте plugin.json: id не [a-z0-9_-], entry не *.lua, api_version > 1, пустые name/version.

«Ничего не происходит» (загрузился, но молчит):
- Работу делает не обработчик, а `on_activate`: чтение документа и `set_view_content` должны жить
  в `ctx.subscribe(name, handler)`, а не на активации.
- Нет события, на которое подписан плагин: он ждёт `document:changed`/`command:invoked`, но
  пользователь его не порождает — сверь имена событий с реальным списком выше.
- Подписка снята или handler вернул ошибку молча: при отказе host-вызова плагин обязан не падать,
  а сообщать через host.show_message.

Watchdog и карантин (плагин есть, но остановлен или выключен):
- Прогресс-таймаут invocation — 400 мс без любого события от плагина: тяжёлый синхронный цикл
  или бесконечный while роняет invocation. Долгую работу дроби и возвращай управление.
- Абсолютный дедлайн invocation — 1500 мс, событиями не сбрасывается: «chatty»-плагин,
  бесконечно дёргающий быстрые host-вызовы, снимается по дедлайну.
- 3 падения подряд → карантин: плагин не запускается автоматически, включается вручную
  в менеджере плагинов (перезагрузка карантин не снимает).

Разрешения в рантайме:
- Отказ permission приходит ЗНАЧЕНИЕМ, а не исключением: host-функция возвращает
  (nil, { code = "permission_denied", message = "…", permission = "…" }). Код обязан проверить
  err и не использовать первое значение как успех, например host.get_document_text → (text, err).
- Набор permissions в манифесте не совпадает с реально вызываемыми host-функциями:
  set_view_content требует view:modify, apply_edit — document:write, show_message — ui:statusbar.

Правила:
- Не выдумывай проблемы: каждая причина опирается на конкретный фрагмент кода из исходника.
- Если код корректен — скажи прямо, что ошибок не найдено.
- Не предлагай исправления, которые не связаны с симптомом.
- Не оформляй ответ как ```json-блок плагина: карточка плагина рисуется только для полного
  `source` и здесь не нужна — отвечай текстом по структуре выше.

Код плагина:
{{input}}"#;

/// Список встроенных скиллов (5, ADR-0032).
pub fn builtin_skills() -> Vec<AiSkill> {
    vec![
        AiSkill {
            id: "explain".to_string(),
            title: "Объяснить файл".to_string(),
            description:
                "Разобрать и объяснить текущий документ (структура, смысл, синтаксис Markdown)"
                    .to_string(),
            prompt_template: EXPLAIN_PROMPT.to_string(),
            category: "document".to_string(),
        },
        AiSkill {
            id: "create-plugin".to_string(),
            title: "Создать плагин".to_string(),
            description: "Сгенерировать новый Lua-плагин по описанию".to_string(),
            prompt_template: CREATE_PLUGIN_PROMPT.to_string(),
            category: "plugins".to_string(),
        },
        AiSkill {
            id: "restructure".to_string(),
            title: "Переструктурировать".to_string(),
            description: "Предложить новую структуру и переписать документ целиком".to_string(),
            prompt_template: RESTRUCTURE_PROMPT.to_string(),
            category: "document".to_string(),
        },
        AiSkill {
            id: "explain-syntax".to_string(),
            title: "Проверить разметку".to_string(),
            description:
                "Проверить, правильно ли использован Markdown: вложенность заголовков, блоки кода, ссылки"
                    .to_string(),
            prompt_template: EXPLAIN_SYNTAX_PROMPT.to_string(),
            category: "document".to_string(),
        },
        AiSkill {
            id: "debug-plugin".to_string(),
            title: "Отладить плагин".to_string(),
            description: "Найти причины, по которым Lua-плагин не загружается или не работает"
                .to_string(),
            prompt_template: DEBUG_PLUGIN_PROMPT.to_string(),
            category: "plugins".to_string(),
        },
    ]
}

/// Все скиллы: встроенные + пользовательские (появятся в Epic 5). Сейчас — только встроенные.
pub fn skills() -> Vec<AiSkill> {
    builtin_skills()
}
