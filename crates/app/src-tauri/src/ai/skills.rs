//! Встроенная библиотека скиллов (готовых промтов), ADR-0032.
//!
//! В MVP — **ровно 2** скилла: `explain` (документ) и `create-plugin` (плагины).
//! Промты встроены в код (редактора скиллов в MVP нет); формат — как в ADR-0032.

use crate::ai::types::AiSkill;

/// Промт `explain`: подстановка `{{doc}}` — текущий текст документа.
///
/// Формат ответа задан явно (структура из 4 частей): от модели без рамок трудно получить
/// предсказуемый разбор — она либо пересказывает документ, либо растекается. Секции + правило
/// «не выдумывай и не перепечатывай» делают ответ стабильным и различают «о чём» и «какая разметка».
const EXPLAIN_PROMPT: &str = r#"Ты — разборщик Markdown-документов в редакторе darmark. Ниже документ; объясни его простыми
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
/// Промт намеренно задаёт контракт точки входа, перечень реальных host-функций (см. `docs/PLUGIN_API.md`)
/// и разводит три «похожих» механизма (view / статусбар / команда). Без этого модель домысливает
/// модуль-таблицу (`local M ... return M`), чужой API (`host.ui`/`host.view`/`host.document`) и путает
/// `set_view_content` со статусбаром — плагин либо отвергается загрузкой, либо не работает.
/// Валидировано циклом `skill-creator` (эвалы create-plugin view/статусбар + explain).
const CREATE_PLUGIN_PROMPT: &str = r#"Ты — генератор Lua-плагинов для darmark (Lua 5.5 в песочнице; нет os/io/require/load, нет host.document/host.ui/host.view/host.commands). По описанию: {{input}}

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
```lua
local VIEW_ID = "my-plugin:stats"   -- "<id плагина>:<kind>"

function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local text, err = host.get_document_text(payload.doc_id)
    if err == nil and text then host.set_view_content(VIEW_ID, "<p>длина " .. #text .. "</p>") end
  end)
end
```"#;

/// Список встроенных скиллов MVP (ровно 2, ADR-0032).
pub fn skills() -> Vec<AiSkill> {
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvp_has_exactly_two_skills() {
        let skills = skills();
        assert_eq!(skills.len(), 2, "в MVP ровно 2 скилла (ADR-0032)");
        let ids: Vec<&str> = skills.iter().map(|skill| skill.id.as_str()).collect();
        assert_eq!(ids, vec!["explain", "create-plugin"]);
    }

    #[test]
    fn categories_and_placeholders_match_contract() {
        let skills = skills();
        let explain = &skills[0];
        assert_eq!(explain.category, "document");
        assert!(explain.prompt_template.contains("{{doc}}"));

        let create = &skills[1];
        assert_eq!(create.category, "plugins");
        assert!(create.prompt_template.contains("{{input}}"));
    }

    #[test]
    fn create_plugin_prompt_requires_json_contract() {
        // Контракт парсинга фронта (ADR-0034/ADR-0026): только fenced ```json с полями объекта.
        let create = &skills()[1];
        let prompt = &create.prompt_template;
        assert!(prompt.contains("```json"), "промт просит fenced json-блок");
        for field in [
            "\"id\"",
            "\"name\"",
            "\"version\"",
            "\"permissions\"",
            "\"source\"",
        ] {
            assert!(
                prompt.contains(field),
                "промт задаёт поле {field}: {prompt}"
            );
        }
        assert!(
            prompt.contains("текста вне блока"),
            "промт запрещает текст вне блока"
        );
    }

    #[test]
    fn create_plugin_prompt_pins_entry_point_and_host_api() {
        // Без этого модель генерирует `local M ... return M` и чужой API, и плагин отвергается
        // проверкой `on_activate` (см. отчёт пользователя про `yaml-table-view`).
        let prompt = &skills()[1].prompt_template;
        assert!(
            prompt.contains("function on_activate(ctx)"),
            "промт требует глобальную точку входа"
        );
        for forbidden in ["return M", "M.on_activate", "local on_activate"] {
            assert!(
                prompt.contains(forbidden),
                "промт явно запрещает {forbidden}"
            );
        }
        for api in [
            "ctx.subscribe",
            "host.get_document_text",
            "host.get_document_range",
            "host.set_view_content",
            "host.show_message",
        ] {
            assert!(
                prompt.contains(api),
                "промт перечисляет реальный API: {api}"
            );
        }
        // View-плагин без contributes не зарегистрирует вкладку (ADR-0026): промт обязан
        // просить contributes.views и называть права view:create/view:modify.
        assert!(
            prompt.contains("contributes") && prompt.contains("views"),
            "промт просит contributes.views для view-плагинов"
        );
        assert!(
            prompt.contains("view:create") && prompt.contains("view:modify"),
            "промт называет права view:create/view:modify"
        );
        // Разведение view/статусбар/команда (эвал показал: baseline путал статусбар с set_view_content)
        // и явная сборка view_id из id плагина (иначе Runtime unknown_view).
        assert!(
            prompt.contains("Не путай") && prompt.contains("статусбар"),
            "промт разводит view, статусбар и команду"
        );
        assert!(
            prompt.contains("<id плагина>:<kind>"),
            "промт задаёт состав view_id = <id плагина>:<kind>"
        );
    }

    #[test]
    fn explain_prompt_has_output_structure() {
        // Формат из 4 секций и запрет перепечатки — иначе ответ непредсказуем (см. цикл skill-creator).
        let prompt = &skills()[0].prompt_template;
        for section in ["О чём", "Структура", "Разметка", "На что обратить внимание"]
        {
            assert!(
                prompt.contains(section),
                "промт `explain` задаёт секцию «{section}»"
            );
        }
        assert!(
            prompt.contains("не выдумывай") && prompt.contains("не перепечатывай"),
            "промт `explain` запрещает выдумывать и перепечатывать документ"
        );
    }
}
