//! Реестр плагинных тир-1 представлений (Фаза 4 TZ-H2, §5.3/§9.4, ADR-0022).
//!
//! Плагин с `contributes.views[].tier == 1` даёт `HtmlViewProvider`: хост хранит
//! его HTML (приходит из child через `host.set_view_content`). HTML проходит ту же
//! санитизацию `md-core`, что и предпросмотр (§11.1): плагин не может протолкнуть
//! сырой HTML в DOM.
//!
//! Тип [`PluginViewInfo`] объявлен платформенно-нейтрально в `lib.rs`: его использует
//! Tauri-команда `plugin_views`, которая должна компилироваться и на не-Windows.

use std::collections::BTreeMap;

use plugin_proto::envelope::PluginError;
use plugin_proto::manifest::ViewContrib;

use crate::PluginViewInfo;

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
                    // Повторная регистрация (reload/повторный скан): заголовок и kind из
                    // свежего манифеста, HTML сохраняем — его перезапишет set_view_content.
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

        // XSS-вектор срезается тем же санитайзером, что и предпросмотр.
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
}
