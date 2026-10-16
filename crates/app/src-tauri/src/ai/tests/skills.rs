//! Тесты встроенной библиотеки скиллов [`super::super::skills`].

use crate::ai::skills::{builtin_skills, skills};
use crate::ai::types::AiSkill;

/// Скилл по `id`: позиционные обращения (`skills()[1]`) не говорят, какой скилл проверяется,
/// и молча уезжают на другой при перестановке состава.
fn skill_by_id(id: &str) -> AiSkill {
    skills()
        .into_iter()
        .find(|skill| skill.id == id)
        .unwrap_or_else(|| panic!("скилл {id} отсутствует"))
}

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
            "explain-syntax",
            "debug-plugin"
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
    assert_eq!(skills.len(), 5, "встроенных скиллов должно быть 5");
    let expected: &[(&str, &str, &str)] = &[
        ("explain", "document", "{{doc}}"),
        ("create-plugin", "plugins", "{{input}}"),
        ("restructure", "document", "{{doc}}"),
        ("explain-syntax", "document", "{{doc}}"),
        ("debug-plugin", "plugins", "{{input}}"),
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
    let create = skill_by_id("create-plugin");
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
    let prompt = &skill_by_id("create-plugin").prompt_template;
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
    assert!(
        prompt.contains("data-p-<plugin_id>-action") && prompt.contains("on_action"),
        "промт описывает роутинг клика view → on_action"
    );
    assert!(
        prompt.contains("HTML санитизируется на хосте")
            && prompt.contains("javascript:")
            && prompt.contains("on* вырезаются"),
        "промт описывает санитизацию HTML вкладки"
    );
}

#[test]
fn create_plugin_prompt_covers_host_semantics_and_manifest_limits() {
    let prompt = &skill_by_id("create-plugin").prompt_template;

    assert!(
        prompt.contains("os/io/require/load/loadfile/dofile/coroutine/debug/package"),
        "промт перечисляет полный список запрещённых библиотек Lua 5.5"
    );
    assert!(
        prompt.contains("[start, stop)") && prompt.contains("байты UTF-8"),
        "промт задаёт полуинтервал apply_edit и байтовые смещения"
    );
    assert!(
        prompt.contains("1 МиБ") && prompt.contains("циклом окон"),
        "промт называет лимит get_document_range и цикл окон"
    );
    assert!(
        prompt.contains("get_document_text"),
        "промт оставляет чтение короткого документа допустимым путём"
    );
    assert!(
        prompt.contains("export_html: true"),
        "промт описывает семантику export_html"
    );

    assert!(
        prompt.contains("contributes.commands")
            && prompt.contains("\"keybinding\"")
            && prompt.contains("command:invoked {command_id}"),
        "промт задаёт поля команды и событие command:invoked"
    );
    assert!(
        prompt.contains("contributes.statusbar / toolbar / settings не добавляй"),
        "промт запрещает contributes.statusbar/toolbar/settings, которых не рендерит GUI"
    );
}

#[test]
fn explain_prompt_has_output_structure() {
    let explain = skill_by_id("explain");
    let prompt = &explain.prompt_template;
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

/// Промт отладки обязан звать `plugin_read` первым ходом и иметь fallback (TASK-003 §3),
/// а его чек-лист — покрывать рантайм-классы отказов, а не только загрузку (TASK-003 §4).
/// Усечение исходника транспортом обязано быть **объявлено** промтом: модель не судит о коде,
/// которого не видела.
#[test]
fn debug_plugin_prompt_reads_source_and_covers_runtime_failures() {
    let prompt = &skill_by_id("debug-plugin").prompt_template;

    assert!(
        prompt.contains("Первым ходом") && prompt.contains("`plugin_read`"),
        "промт зовёт инструмент чтения исходника первым ходом: {prompt}"
    );
    assert!(
        prompt.contains("Fallback")
            && prompt.contains("main.lua")
            && prompt.contains("plugin.json"),
        "промт просит вставить main.lua и plugin.json, если исходник не получен: {prompt}"
    );
    assert!(
        prompt.contains("truncated: true") && prompt.contains("НЕПОЛНЫМ"),
        "промт объявляет усечение исходника (truncated) и запрещает выводы о невидимой части: {prompt}"
    );
    for needle in [
        "ничего не происходит",
        "400 мс",
        "1500 мс",
        "карантин",
        "permission_denied",
    ] {
        assert!(
            prompt.contains(needle),
            "чек-лист промта покрывает «{needle}»: {prompt}"
        );
    }
}
