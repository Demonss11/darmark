//! Тир-1 view плагинов: снимок реестра представлений, обратная маршрутизация `on_action`
//! и состояние view для гейта регистрации (`view:create`, F44/S4).

use plugin_core::envelope::PluginError;
use plugin_core::manifest::ViewContrib;
use serde_json::{json, Value};

use crate::PluginViewInfo;

use super::{lock, PluginHost};

impl PluginHost {
    /// `plugin_views`: снимок реестра представлений для фронтенда.
    pub fn plugin_views(&self) -> Vec<PluginViewInfo> {
        lock(&self.views).snapshot()
    }

    /// `plugin_view_action`: обратная маршрутизация из тир-1 view (§9.4).
    ///
    /// `plugin_id` берётся из `view_id` до `:`; вызывается `on_action` плагина.
    /// Возвращённый плагином HTML уходит через `host.set_view_content` и
    /// санитизируется на хосте.
    pub fn plugin_view_action(
        &self,
        view_id: &str,
        action: &str,
        payload: Option<Value>,
    ) -> Result<Value, PluginError> {
        let plugin_id = view_id
            .split_once(':')
            .map(|(plugin_id, _)| plugin_id)
            .filter(|plugin_id| !plugin_id.is_empty())
            .ok_or_else(|| {
                PluginError::new("bad_view_id", format!("view_id без plugin_id: {view_id}"))
            })?;
        if !lock(&self.views).contains(view_id) {
            return Err(PluginError::new(
                "unknown_view",
                format!("представление не зарегистрировано: {view_id}"),
            ));
        }

        let result = {
            let mut manager = lock(&self.manager);
            let runtime = manager.get_mut(plugin_id).ok_or_else(|| {
                PluginError::new("unknown_plugin", format!("нет плагина {plugin_id}"))
            })?;
            runtime.invoke(
                "action",
                json!({
                    "view_id": view_id,
                    "action": action,
                    "payload": payload.unwrap_or(Value::Null),
                }),
            )
        };
        let failed = result.is_err();
        self.pump();
        if failed {
            self.notify_plugins();
        }
        result
    }

    /// Состояние плагина: `(enabled, contributes.views, effective permissions)`, если он
    /// зарегистрирован. Эффективный набор нужен для гейта регистрации view (`view:create`).
    pub(super) fn runtime_view_state(
        &self,
        id: &str,
    ) -> Option<(bool, Vec<ViewContrib>, Vec<String>)> {
        lock(&self.manager).get(id).map(|runtime| {
            (
                runtime.is_enabled(),
                runtime.views.clone(),
                runtime.effective(),
            )
        })
    }
}
