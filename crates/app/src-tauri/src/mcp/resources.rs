//! MCP-ресурсы: статические контракты + динамический каталог плагинов.
//!
//! Read-only (TZ-MCP §4.1): статические контракты неизменны в рантайме, динамические
//! читают локальный каталог плагинов через `plugin-catalog` (без `mlua`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mcp_core::handlers::{ResourceError, ResourceReader};
use mcp_core::registry::{Registry, ResourceDefinition};
use serde_json::{json, Value};






pub fn register_resources(registry: &mut Registry, repo: &Path) {
    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/plugin-manifest".to_string(),
        name: "plugin.json — манифест плагина".to_string(),
        description: "Схема plugin.json — манифеста плагина".to_string(),
        reader: Arc::new(StaticResource::new(manifest_schema())),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/host-api".to_string(),
        name: "Контракт host API".to_string(),
        description: "Контракт host.*/md.*/json.* функций хоста".to_string(),
        reader: Arc::new(StaticResource::new(host_api_schema())),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/events".to_string(),
        name: "Каталог событий плагинов".to_string(),
        description: "Каталог событий плагинной системы".to_string(),
        reader: Arc::new(StaticResource::new(events_schema())),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/markdown".to_string(),
        name: "Контракт Markdown".to_string(),
        description: "Контракт рендера/санитайзера Markdown".to_string(),
        reader: Arc::new(StaticResource::new(markdown_schema())),
    });
    registry.register_resource(ResourceDefinition {
        uri: "darmark://plugins/catalog".to_string(),
        name: "Каталог плагинов".to_string(),
        description: "Индекс локального каталога плагинов (index.json)".to_string(),
        reader: Arc::new(PluginsCatalogResource {
            repo: repo.to_path_buf(),
        }),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://plugins/{id}/manifest".to_string(),
        name: "Манифест плагина".to_string(),
        description: "Манифест конкретного плагина из каталога".to_string(),
        reader: Arc::new(PluginManifestResource {
            repo: repo.to_path_buf(),
        }),
    });
}


struct StaticResource {
    content: Value,
}

impl StaticResource {
    fn new(content: Value) -> Self {
        Self { content }
    }
}

impl ResourceReader for StaticResource {
    fn read(&self, _uri: &str) -> Result<Value, ResourceError> {
        Ok(self.content.clone())
    }
}


fn manifest_schema() -> Value {
    json!({
        "title": "plugin.json — манифест плагина",
        "fields": {
            "id": "string, [a-z0-9-_], ≤64",
            "name": "string",
            "version": "string",
            "api_version": format!("number, ≤ {}", plugin_core::HOST_API_VERSION),
            "entry": "string, относительный .lua-путь",
            "permissions": "string[]",
            "contributes": "object (views, commands, statusbar, toolbar, settings)"
        }
    })
}


fn host_api_schema() -> Value {
    json!({
        "host.*": "функции хоста (host.log, host.get_document, host.set_view_content и т.д.)",
        "md.*": "функции Markdown (md.get_text, md.get_selection, md.get_mappings)",
        "json.*": "функции JSON (json.get, json.set)"
    })
}


fn events_schema() -> Value {
    json!({
        "on_activate": "вызывается при активации плагина",
        "on_deactivate": "вызывается при деактивации плагина",
        "document:changed": "документ изменён (doc_id, rev)",
        "document:opened": "документ открыт (doc_id, path)",
        "document:closed": "документ закрыт (doc_id)"
    })
}


fn markdown_schema() -> Value {
    json!({
        "to_html": "markdown → HTML (без маппинга)",
        "to_html_mapped": "markdown → HTML с data-md атрибутами",
        "sanitize_fragment": "HTML → санитизированный HTML (белые списки)"
    })
}


struct PluginsCatalogResource {
    repo: PathBuf,
}

impl ResourceReader for PluginsCatalogResource {
    fn read(&self, _uri: &str) -> Result<Value, ResourceError> {
        let catalog = plugin_catalog::Catalog::load(&self.repo).map_err(ResourceError::internal)?;
        let items: Vec<Value> = catalog
            .list()
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "name": entry.name,
                    "version": entry.version,
                    "api_version": entry.api_version,
                    "permissions": entry.permissions,
                    "description": entry.description,
                })
            })
            .collect();
        Ok(json!({ "plugins": items }))
    }
}


struct PluginManifestResource {
    repo: PathBuf,
}

impl ResourceReader for PluginManifestResource {
    fn read(&self, uri: &str) -> Result<Value, ResourceError> {
        let id = uri
            .strip_prefix("darmark://plugins/")
            .and_then(|rest| rest.strip_suffix("/manifest"))
            .filter(|id| !id.is_empty())
            .ok_or_else(|| ResourceError::internal(format!("некорректный URI ресурса: {uri}")))?;

        let catalog = plugin_catalog::Catalog::load(&self.repo).map_err(ResourceError::internal)?;
        let entry = catalog
            .get(id)
            .ok_or_else(|| ResourceError::internal(format!("плагин не найден: {id}")))?;
        let manifest_bytes = std::fs::read(entry.dir.join(plugin_catalog::MANIFEST_FILE))
            .map_err(|e| ResourceError::internal(format!("plugin.json: {e}")))?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| ResourceError::internal(format!("plugin.json не парсится: {e}")))?;
        Ok(manifest)
    }
}
