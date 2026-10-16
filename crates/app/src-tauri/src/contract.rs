//! Контрактные типы IPC с фронтендом для плагинной подсистемы.
//!
//! Лежат здесь, а не в [`crate::plugins`], потому что это контракт IPC, а не
//! внутреннее состояние плагинов: сериализация этих типов — контракт с TS,
//! и её не должны двигать правки рантайма плагинов.




#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginViewInfo {
    pub view_id: String,
    pub plugin_id: String,
    pub kind: String,
    pub title: String,
    pub html: String,
}




#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PluginStatus {
    
    Stopped,
    
    Active,
    
    Failed { message: String },
    
    Quarantined,
}


#[derive(Clone, Debug, serde::Serialize)]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    
    pub keybinding: Option<String>,
}




#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginInfo {
    pub id: String,
    
    pub name: String,
    
    pub version: String,
    pub status: PluginStatus,
    
    pub permissions: Vec<String>,
    
    
    pub granted_permissions: Vec<String>,
    pub enabled: bool,
    pub commands: Vec<CommandInfo>,
    
    pub notices: Vec<String>,
    
    
    pub settings_schema: Option<serde_json::Value>,
    
    
    pub settings_values: serde_json::Value,
}
