//! Контрактные типы IPC с фронтендом для плагинной подсистемы.
//!
//! Лежат здесь, а не в [`crate::plugins`], потому что это контракт IPC, а не
//! внутреннее состояние плагинов: сериализация этих типов — контракт с TS,
//! и её не должны двигать правки рантайма плагинов.

/// Информация о плагинном тир-1 представлении — контракт с фронтендом.
///
/// `serde` — snake_case по умолчанию.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginViewInfo {
    pub view_id: String,
    pub plugin_id: String,
    pub kind: String,
    pub title: String,
    pub html: String,
}

/// Статус плагина для менеджера UI (§4.7 плагинной системы).
///
/// Сериализуется как `{"state":"active"}` и т.п.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PluginStatus {
    /// Остановлен (выключен пользователем или ещё не запускался).
    Stopped,
    /// Загружен и активирован.
    Active,
    /// Отказ загрузки/активации; сообщение для UI.
    Failed { message: String },
    /// Автоотключён после серии падений (карантин).
    Quarantined,
}

/// Команда, объявленная плагином (`contributes.commands`), — контракт с фронтендом.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    /// Клавиатурная привязка; сериализуется как `null`, если не задана.
    pub keybinding: Option<String>,
}

/// Проекция плагина для менеджера UI (IPC-команда `list_plugins`).
///
/// Пустые `commands`/`notices` допустимы: плагин может ничего не объявлять.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginInfo {
    pub id: String,
    /// Человекочитаемое имя плагина из манифеста (S12a); фолбэк на `id` — на стороне фронта.
    pub name: String,
    /// Версия плагина из манифеста (S12a).
    pub version: String,
    pub status: PluginStatus,
    /// Запрашиваемые плагином права из манифеста (декларация).
    pub permissions: Vec<String>,
    /// Согласованные пользователем права фактически (подмножество `permissions`, F5).
    /// Пусто — согласие не выдано (deny-by-default).
    pub granted_permissions: Vec<String>,
    pub enabled: bool,
    pub commands: Vec<CommandInfo>,
    /// Формулировки границы изоляции/доступа к документу (F38).
    pub notices: Vec<String>,
    /// Схема настроек из манифеста (`contributes.settings`) для UI-формы (S5b);
    /// `null`, если плагин настроек не объявил.
    pub settings_schema: Option<serde_json::Value>,
    /// Текущие сохранённые значения настроек плагина (JSON-объект) для UI-формы (S5b);
    /// пустой объект, если настроек нет. Обогащается хостом из `PluginSettingsStore`.
    pub settings_values: serde_json::Value,
}
