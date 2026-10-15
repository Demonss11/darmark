//! Реестр плагинных тир-1 представлений (Фаза 4 плагинной системы, §5.3/§9.4, ADR-0022).
//!
//! Плагин с `contributes.views[].tier == 1` даёт `HtmlViewProvider`: хост хранит
//! его HTML (приходит из child через `host.set_view_content`). HTML проходит ту же
//! санитизацию `md-core`, что и предпросмотр (§11.1): плагин не может протолкнуть
//! сырой HTML в DOM.
//!
//! Тип [`PluginViewInfo`] живёт в `crate::contract`, а не здесь: это контракт IPC, а не
//! внутреннее состояние плагинов.

use std::collections::BTreeMap;

use plugin_core::envelope::PluginError;
use plugin_core::manifest::ViewContrib;

use crate::PluginViewInfo;

/// Разрешение, гейтящее регистрацию тир-1 view (F44/S4, §8): без него `contributes.views`
/// не превращается во вкладку. Запись HTML всё равно требует отдельного `view:modify`.
pub const VIEW_CREATE_PERMISSION: &str = "view:create";

/// Разрешение, гейтящее **запись** HTML в тир-1 view (`host.set_view_content`, §8).
/// Отзыв `view:modify` не убирает вкладку, но очищает её содержимое: владение
/// записью возвращается хосту, пока плагин снова не получит право.
pub const VIEW_MODIFY_PERMISSION: &str = "view:modify";

/// Реестр представлений: `view_id = "{plugin_id}:{kind}"` → HTML и метаданные.
///
/// `BTreeMap` даёт детерминированный (отсортированный по `view_id`) порядок
/// [`PluginViews::snapshot`] — фронтенд получает стабильный список.
#[derive(Default)]
pub struct PluginViews {
    views: BTreeMap<String, PluginViewInfo>,
}

impl PluginViews {
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует тир-1 view плагина (идемпотентно). HTML изначально пуст —
    /// плагин заполнит его через `set_view_content` при активации.
    ///
    /// Тиры выше 1 игнорируются: тир-2 — built-in only (ADR-0022), плагинные
    /// представления всегда тир-1.
    pub fn register(&mut self, plugin_id: &str, contributes: &[ViewContrib]) {
        for view in contributes.iter().filter(|v| v.tier == 1) {
            let view_id = format!("{plugin_id}:{}", view.kind);
            self.views
                .entry(view_id.clone())
                .and_modify(|existing| {
                    existing.title = view.title.clone();
                    existing.kind = view.kind.clone();
                })
                .or_insert_with(|| PluginViewInfo {
                    view_id,
                    plugin_id: plugin_id.to_string(),
                    kind: view.kind.clone(),
                    title: view.title.clone(),
                    html: String::new(),
                });
        }
    }

    /// Обновляет HTML представления, санитизируя его `md-core`. Неизвестный `view_id` —
    /// ошибка (значением, не паникой): плагин адресует только свои view.
    ///
    /// Возвращает `true`, если HTML действительно изменился: вызывающий не должен
    /// эмитить событие фронтенду на `set_view_content` с тем же содержимым (иначе
    /// «болтливый» плагин вызывает шторм IPC/перерисовок).
    pub fn set_content(&mut self, view_id: &str, html: &str) -> Result<bool, PluginError> {
        let view = self.views.get_mut(view_id).ok_or_else(|| {
            PluginError::new(
                "unknown_view",
                format!("представление не найдено: {view_id}"),
            )
        })?;
        let sanitized = md_core::sanitize_fragment(html);
        let changed = view.html != sanitized;
        view.html = sanitized;
        Ok(changed)
    }

    /// Снимок для фронтенда (отсортирован по `view_id`).
    pub fn snapshot(&self) -> Vec<PluginViewInfo> {
        self.views.values().cloned().collect()
    }

    /// Зарегистрировано ли представление с таким `view_id` (валидация маршрутизации).
    pub fn contains(&self, view_id: &str) -> bool {
        self.views.contains_key(view_id)
    }

    /// Убирает все представления плагина (выключение/перезагрузка/удаление).
    pub fn remove_plugin(&mut self, plugin_id: &str) {
        self.views.retain(|_, view| view.plugin_id != plugin_id);
    }

    /// Очищает HTML всех представлений плагина, оставляя сами вкладки в реестре.
    ///
    /// Нужно при отзыве `view:modify` (deny-by-default): вкладка остаётся видимой,
    /// но её содержимое, записанное плагином, сбрасывается — иначе старый HTML
    /// продолжал бы отображаться, хотя право на запись уже отозвано.
    ///
    /// Возвращает `true`, если хотя бы одно представление реально изменилось
    /// (было непустым): вызывающий не должен эмитить событие на no-op.
    pub fn clear_content(&mut self, plugin_id: &str) -> bool {
        let mut changed = false;
        for view in self
            .views
            .values_mut()
            .filter(|view| view.plugin_id == plugin_id)
        {
            if !view.html.is_empty() {
                view.html.clear();
                changed = true;
            }
        }
        changed
    }

    /// Синхронизирует представления плагина с его **эффективными** правами (F44/S4).
    ///
    /// Тир-1 view регистрируется только если выдан `view:create` (deny-by-default, как
    /// `document:*`); иначе все view плагина снимаются. Единая точка для загрузки, вкл/выкл,
    /// `reload` и смены прав: вкладка появляется/исчезает вслед за согласием пользователя.
    /// Вызов идемпотентен и безопасен при пустом `contributes`.
    ///
    /// Если `view:create` есть, но `view:modify` отозван — вкладку **не снимаем**, а
    /// очищаем её содержимое: запись HTML — отдельное право, и при его отсутствии
    /// показанный ранее контент больше не принадлежит плагину (deny-by-default). При
    /// возврате `view:modify` плагин переиздаёт HTML через `set_view_content`.
    pub fn sync(&mut self, plugin_id: &str, contributes: &[ViewContrib], effective: &[String]) {
        let can_create = effective
            .iter()
            .any(|permission| permission == VIEW_CREATE_PERMISSION);
        if can_create && contributes.iter().any(|view| view.tier == 1) {
            self.register(plugin_id, contributes);
            let can_modify = effective
                .iter()
                .any(|permission| permission == VIEW_MODIFY_PERMISSION);
            if !can_modify {
                self.clear_content(plugin_id);
            }
        } else {
            self.remove_plugin(plugin_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(kind: &str, title: &str, tier: u8) -> ViewContrib {
        ViewContrib {
            kind: kind.to_string(),
            title: title.to_string(),
            tier,
        }
    }

    #[test]
    fn registers_tier1_and_is_idempotent() {
        let mut views = PluginViews::new();
        let contributes = vec![view("stats", "Статистика", 1)];
        views.register("word-count", &contributes);
        views.register("word-count", &contributes);

        let snapshot = views.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].view_id, "word-count:stats");
        assert_eq!(snapshot[0].plugin_id, "word-count");
        assert_eq!(snapshot[0].kind, "stats");
        assert_eq!(snapshot[0].title, "Статистика");
        assert_eq!(snapshot[0].html, "");
        assert!(views.contains("word-count:stats"));
        assert!(!views.contains("nope:view"));
    }

    #[test]
    fn tier2_is_not_registered() {
        let mut views = PluginViews::new();
        views.register("p", &[view("builtin", "Встроенный", 2)]);
        assert!(views.snapshot().is_empty(), "тир-2 не плагинный");
    }

    #[test]
    fn set_content_sanitizes_html() {
        let mut views = PluginViews::new();
        views.register("p", &[view("main", "Main", 1)]);

        views
            .set_content("p:main", r#"<span data-p-p-action="inc">x</span>"#)
            .unwrap();
        let snapshot = views.snapshot();
        assert!(
            snapshot[0].html.contains(r#"data-p-p-action="inc""#),
            "html: {}",
            snapshot[0].html
        );
        views
            .set_content("p:main", r#"<img src="x" onerror="alert(1)">"#)
            .unwrap();
        let html = &views.snapshot()[0].html;
        assert!(!html.contains("onerror"), "html: {html}");
    }

    #[test]
    fn set_content_reports_change() {
        let mut views = PluginViews::new();
        views.register("p", &[view("main", "Main", 1)]);

        assert!(
            views.set_content("p:main", "<b>x</b>").unwrap(),
            "первый раз — смена"
        );
        assert!(
            !views.set_content("p:main", "<b>x</b>").unwrap(),
            "тот же HTML — не смена"
        );
        assert!(
            views.set_content("p:main", "<b>y</b>").unwrap(),
            "новый HTML — смена"
        );
    }

    #[test]
    fn unknown_view_is_error() {
        let mut views = PluginViews::new();
        let err = views.set_content("nope:main", "<b>x</b>").unwrap_err();
        assert_eq!(err.code, "unknown_view");
    }

    #[test]
    fn remove_plugin_drops_only_its_views() {
        let mut views = PluginViews::new();
        views.register("a", &[view("one", "1", 1)]);
        views.register("b", &[view("two", "2", 1)]);
        views.remove_plugin("a");

        let snapshot = views.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].view_id, "b:two");
    }

    #[test]
    fn sync_registers_only_with_view_create() {
        let mut views = PluginViews::new();
        let contributes = vec![view("main", "Main", 1)];
        views.sync("p", &contributes, &["view:modify".to_string()]);
        assert!(views.snapshot().is_empty(), "без view:create вкладки нет");
        views.sync(
            "p",
            &contributes,
            &["view:create".to_string(), "view:modify".to_string()],
        );
        assert_eq!(views.snapshot().len(), 1);
        views.sync("p", &contributes, &[]);
        assert!(
            views.snapshot().is_empty(),
            "отзыв view:create снимает view"
        );
    }

    #[test]
    fn clear_content_empties_only_target_plugin() {
        let mut views = PluginViews::new();
        views.register("a", &[view("main", "A", 1)]);
        views.register("b", &[view("main", "B", 1)]);
        views.set_content("a:main", "<b>a</b>").unwrap();
        views.set_content("b:main", "<b>b</b>").unwrap();

        assert!(views.clear_content("a"), "у a был контент — смена");
        assert!(!views.clear_content("a"), "повторная очистка — не смена");

        let snapshot = views.snapshot();
        let a = snapshot.iter().find(|v| v.plugin_id == "a").unwrap();
        let b = snapshot.iter().find(|v| v.plugin_id == "b").unwrap();
        assert!(a.html.is_empty(), "контент a очищен");
        assert_eq!(b.html, "<b>b</b>", "чужой плагин не затронут");
    }

    /// Отзыв `view:modify` при живом `view:create`: вкладка остаётся, но HTML пуст
    /// (deny-by-default на запись), и очистка идемпотентна.
    #[test]
    fn sync_without_view_modify_clears_content_but_keeps_view() {
        let mut views = PluginViews::new();
        let contributes = vec![view("main", "Main", 1)];

        views.sync(
            "p",
            &contributes,
            &["view:create".to_string(), "view:modify".to_string()],
        );
        views.set_content("p:main", "<b>живой</b>").unwrap();
        assert!(!views.snapshot()[0].html.is_empty());
        views.sync("p", &contributes, &["view:create".to_string()]);
        let snapshot = views.snapshot();
        assert_eq!(snapshot.len(), 1, "вкладка остаётся при отзыве view:modify");
        assert!(
            snapshot[0].html.is_empty(),
            "без view:modify контент очищен: {:?}",
            snapshot[0].html
        );
    }

    /// `view:create`+`view:modify`: контент сохраняется при повторном `sync`.
    #[test]
    fn sync_with_view_modify_preserves_content() {
        let mut views = PluginViews::new();
        let contributes = vec![view("main", "Main", 1)];
        let effective = ["view:create".to_string(), "view:modify".to_string()];

        views.sync("p", &contributes, &effective);
        views.set_content("p:main", "<b>живой</b>").unwrap();
        views.sync("p", &contributes, &effective);
        assert_eq!(
            views.snapshot()[0].html,
            "<b>живой</b>",
            "view:modify сохраняет контент"
        );
    }

    /// Полный сценарий E2E: наполнили → сняли `view:modify` (контент пуст) → вернули
    /// (вкладка пуста до нового `set_content`) → новое `set_content` снова наполняет.
    #[test]
    fn revoking_and_regranting_view_modify_round_trip() {
        let mut views = PluginViews::new();
        let contributes = vec![view("main", "Main", 1)];
        let both = ["view:create".to_string(), "view:modify".to_string()];

        views.sync("p", &contributes, &both);
        views.set_content("p:main", "<b>start</b>").unwrap();
        views.sync("p", &contributes, &["view:create".to_string()]);
        assert!(views.snapshot()[0].html.is_empty());
        views.sync("p", &contributes, &both);
        assert!(
            views.snapshot()[0].html.is_empty(),
            "хост не выдумывает контент"
        );
        views.set_content("p:main", "<b>start</b>").unwrap();
        assert_eq!(views.snapshot()[0].html, "<b>start</b>");
    }

    #[test]
    fn sync_with_empty_contributes_removes_stale_view() {
        let mut views = PluginViews::new();
        views.sync(
            "p",
            &[view("main", "Main", 1)],
            &["view:create".to_string()],
        );
        assert_eq!(views.snapshot().len(), 1);
        views.sync("p", &[], &["view:create".to_string()]);
        assert!(views.snapshot().is_empty());
    }
}
