//! Реестр инструментов и ресурсов.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
use crate::protocol;


pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: Arc<dyn ToolHandler>,
}





pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub reader: Arc<dyn ResourceReader>,
}











#[derive(Clone, Debug)]
pub enum CallToolError {
    
    UnknownTool(String),
    
    Execution(ToolError),
}


#[derive(Default)]
pub struct Registry {
    tools: Vec<ToolDefinition>,
    resources: Vec<ResourceDefinition>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_tool(&mut self, def: ToolDefinition) {
        self.tools.push(def);
    }

    pub fn register_resource(&mut self, def: ResourceDefinition) {
        self.resources.push(def);
    }

    
    pub fn tools_list(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            })
            .collect();
        json!({ "tools": tools })
    }

    
    pub fn resources_list(&self) -> Value {
        let resources: Vec<Value> = self
            .resources
            .iter()
            .map(|r| {
                json!({
                    "uri": r.uri,
                    "name": r.name,
                    "description": r.description,
                })
            })
            .collect();
        json!({ "resources": resources })
    }

    
    
    
    
    pub fn call_tool(&self, name: &str, params: Value) -> Result<Value, CallToolError> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| CallToolError::UnknownTool(name.to_string()))?;
        tool.handler.call(params).map_err(CallToolError::Execution)
    }

    
    
    
    
    
    
    pub fn read_resource(&self, uri: &str) -> Result<Value, ResourceError> {
        if let Some(resource) = self.resources.iter().find(|r| r.uri == uri) {
            return resource.reader.read(uri);
        }
        if let Some(resource) = self
            .resources
            .iter()
            .find(|r| uri_matches_template(&r.uri, uri))
        {
            return resource.reader.read(uri);
        }
        Err(ResourceError::new(
            protocol::RESOURCE_NOT_FOUND,
            format!("ресурс не найден: {uri}"),
        ))
    }
}







fn uri_matches_template(template: &str, actual: &str) -> bool {
    if !template.contains('{') {
        return template == actual;
    }
    let template_segments: Vec<&str> = template.split('/').collect();
    let actual_segments: Vec<&str> = actual.split('/').collect();
    if template_segments.len() != actual_segments.len() {
        return false;
    }
    template_segments
        .iter()
        .zip(actual_segments.iter())
        .all(|(expected, got)| match placeholder_name(expected) {
            Some(name) => !name.is_empty() && !got.is_empty(),
            None => expected == got,
        })
}


fn placeholder_name(segment: &str) -> Option<&str> {
    segment.strip_prefix('{')?.strip_suffix('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_uri_matches_only_itself() {
        assert!(uri_matches_template("darmark://test", "darmark://test"));
        assert!(!uri_matches_template("darmark://test", "darmark://other"));
        
        assert!(!uri_matches_template(
            "darmark://test",
            "darmark://test/extra"
        ));
    }

    #[test]
    fn placeholder_matches_any_non_empty_segment() {
        let template = "darmark://plugins/{id}/manifest";
        assert!(uri_matches_template(
            template,
            "darmark://plugins/word-count/manifest"
        ));
        
        assert!(!uri_matches_template(
            template,
            "darmark://plugins//manifest"
        ));
        
        assert!(!uri_matches_template(
            template,
            "darmark://plugins/word-count/manifest/extra"
        ));
        assert!(!uri_matches_template(
            template,
            "darmark://plugins/word-count"
        ));
        
        assert!(!uri_matches_template(
            template,
            "darmark://plugins/word-count/meta"
        ));
    }

    #[test]
    fn placeholder_name_extracts_inner_text() {
        assert_eq!(placeholder_name("{id}"), Some("id"));
        assert_eq!(placeholder_name("plugins"), None);
        assert_eq!(placeholder_name("{}"), Some(""));
    }
}
