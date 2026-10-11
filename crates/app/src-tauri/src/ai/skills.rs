//! Встроенная библиотека скиллов (готовых промтов), TZ §7.
//!
//! В MVP — **ровно 2** скилла: `explain` (документ) и `create-plugin` (плагины).
//! Промты встроены в код (редактора скиллов в MVP нет); формат — как в ТЗ §7.2.

use crate::ai::types::AiSkill;

/// Промт `explain`: подстановка `{{doc}}` — текущий текст документа.
const EXPLAIN_PROMPT: &str = "Объясни этот документ простыми словами: о чём он, какова структура, \
что делает разметка Markdown. Отвечай без правок самого документа.\n\nДокумент:\n{{doc}}";

/// Промт `create-plugin`: подстановка `{{input}}` — описание плагина из чата.
const CREATE_PLUGIN_PROMPT: &str = "Сгенерируй Lua-плагин для darmark по описанию: {{input}}. \
Верни main.lua и plugin.json (host-API и permissions — по песочнице darmark).";

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
}
