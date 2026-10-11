//! Встроенная библиотека скиллов (готовых промтов), TZ §7.
//!
//! В MVP — **ровно 2** скилла: `explain` (документ) и `create-plugin` (плагины).
//! Промты встроены в код (редактора скиллов в MVP нет); формат — как в ТЗ §7.2.

use crate::ai::types::AiSkill;

/// Промт `explain`: подстановка `{{doc}}` — текущий текст документа.
const EXPLAIN_PROMPT: &str = "Объясни этот документ простыми словами: о чём он, какова структура, \
что делает разметка Markdown. Отвечай без правок самого документа.\n\nДокумент:\n{{doc}}";

/// Промт `create-plugin`: подстановка `{{input}}` — описание плагина из чата.
///
/// Контракт ответа (TZ §6): модель возвращает **только** один fenced-блок ```` ```json ````
/// с объектом `{ id, name, version, permissions, source }` — его парсит фронт.
const CREATE_PLUGIN_PROMPT: &str = r#"Ты — генератор Lua-плагинов для darmark. По описанию: {{input}}
Верни РОВНО один fenced-блок ```json с одним объектом:
{"id": "…", "name": "…", "version": "1.0.0", "permissions": [], "source": "полный код main.lua"}
Правила: id — только строчные латинские буквы, цифры, '-' и '_'; permissions — только из набора
darmark (document:read, document:write, view:create, view:modify, ui:statusbar); source — полный
код main.lua с функцией on_activate и host-API песочницы darmark. Никакого текста вне блока."#;

/// Список встроенных скиллов MVP (ровно 2, TZ §7.1).
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
        assert_eq!(skills.len(), 2, "в MVP ровно 2 скилла (TZ §7.1)");
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
        // Контракт парсинга фронта (TZ §6): только fenced ```json с полями объекта.
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
            prompt.contains("Никакого текста вне"),
            "промт запрещает текст вне блока"
        );
    }
}
