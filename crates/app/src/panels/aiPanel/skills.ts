// Встроенные скиллы AI-ассистента (фолбэк, если хост не вернул список).

import type { AiSkill } from "../../core/tauri";

export const FALLBACK_SKILLS: readonly AiSkill[] = [
  {
    id: "explain",
    title: "Объяснить файл",
    description: "Разобрать и объяснить текущий документ",
    prompt_template: `Ты — разборщик Markdown-документов в редакторе ПЕНАЛ. Ниже документ; объясни его простыми
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
{{doc}}`,
    category: "document",
  },
  {
    id: "create-plugin",
    title: "Создать плагин",
    description: "Сгенерировать новый Lua-плагин по описанию",
    prompt_template: `Ты — генератор Lua-плагинов для ПЕНАЛ (Lua 5.5 в песочнице; нет os/io/require/load, нет host.document/host.ui/host.view/host.commands). По описанию: {{input}}

Верни РОВНО один fenced-блок \`\`\`json с одним объектом (никакого текста вне блока):
{"id": "…", "name": "…", "version": "1.0.0", "permissions": [], "source": "полный код main.lua"}

Контракт \`source\` (иначе плагин отвергается загрузкой):
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

Не путай три механизма:
- Вкладка (view): объяви в contributes.views [{"kind": "<kind>", "title": "…", "tier": 1}] (ровно эти поля),
  permission view:create; содержимое пиши через host.set_view_content(VIEW_ID, html) (нужен view:modify), где
  VIEW_ID = "<id плагина>:<kind>": <id плагина> — это поле id из этого же JSON, <kind> — из contributes.views.
  HTML санитизируется на хосте.
- Статусбар: короткий текст показывай через host.show_message(text) (permission ui:statusbar);
  host.set_view_content для статусбара НЕ используй.
- Команда: объяви в contributes.commands и обрабатывай событие command:invoked {command_id}.

permissions — только из набора: document:read, document:write, view:create, view:modify, ui:statusbar.
id — только [a-z0-9_-]. Если вкладка/команда/статусбар не нужны — ключ contributes не добавляй.

Пример каркаса (подстрой под задачу, не копируй дословно):
\`\`\`lua
local VIEW_ID = "my-plugin:stats"   -- "<id плагина>:<kind>"

function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local text, err = host.get_document_text(payload.doc_id)
    if err == nil and text then host.set_view_content(VIEW_ID, "<p>длина " .. #text .. "</p>") end
  end)
end
\`\`\``,
    category: "plugins",
  },
];
