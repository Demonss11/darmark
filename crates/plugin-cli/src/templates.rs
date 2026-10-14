//! Встроенные в бинарник шаблоны плагина и имя файла манифеста.

pub const MANIFEST_FILE: &str = "plugin.json";

pub const TEMPLATE_PLUGIN_JSON: &str = include_str!("../templates/plugin/plugin.json");
pub const TEMPLATE_MAIN_LUA: &str = include_str!("../templates/plugin/main.lua");
pub const TEMPLATE_README: &str = include_str!("../templates/plugin/README.md");
