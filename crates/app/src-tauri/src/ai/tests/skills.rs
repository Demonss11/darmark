//! Тесты встроенной библиотеки скиллов [`super::super::skills`].

use crate::ai::skills::{builtin_skills, skills};

#[test]
fn built_in_skills_have_expected_ids() {
    let skills = builtin_skills();
    let ids: Vec<&str> = skills.iter().map(|skill| skill.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "explain",
            "create-plugin",
            "restructure",
            "summarize",
            "explain-syntax",
            "debug-plugin",
            "add-view"
        ]
    );
    for skill in &skills {
        assert!(
            !skill.title.is_empty(),
            "у скилла {} пустой title",
            skill.id
        );
        assert!(
            !skill.description.is_empty(),
            "у скилла {} пустой description",
            skill.id
        );
        assert!(
            !skill.prompt_template.is_empty(),
            "у скилла {} пустой prompt_template",
            skill.id
        );
        assert!(
            skill.category == "document" || skill.category == "plugins",
            "у скилла {} неизвестная категория {}",
            skill.id,
            skill.category
        );
    }
}

#[test]
fn categories_and_placeholders_match_contract() {
    let skills = builtin_skills();
    assert_eq!(skills.len(), 7, "встроенных скиллов должно быть 7");
    let expected: &[(&str, &str, &str)] = &[
        ("explain", "document", "{{doc}}"),
        ("create-plugin", "plugins", "{{input}}"),
        ("restructure", "document", "{{doc}}"),
        ("summarize", "document", "{{doc}}"),
        ("explain-syntax", "document", "{{doc}}"),
        ("debug-plugin", "plugins", "{{input}}"),
        ("add-view", "plugins", "{{input}}"),
    ];

    for (id, category, placeholder) in expected {
        let skill = skills
            .iter()
            .find(|s| s.id == *id)
            .unwrap_or_else(|| panic!("скилл {id} отсутствует"));
        assert_eq!(skill.category, *category, "категория скилла {id}");
        assert!(
            skill.prompt_template.contains(placeholder),
            "промт скилла {id} должен содержать {placeholder}"
        );
    }
}

#[test]
fn create_plugin_prompt_requires_json_contract() {
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
    assert!(
        prompt.contains("contributes") && prompt.contains("views"),
        "промт просит contributes.views для view-плагинов"
    );
    assert!(
        prompt.contains("view:create") && prompt.contains("view:modify"),
        "промт называет права view:create/view:modify"
    );
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
