//! Типы чата AI-ассистента и harness'а (ADR-0032/0034): сообщения, ответ, скиллы,
//! история, диффы и валидация плагинов.

use serde::{Deserialize, Serialize};

use crate::ai::tool_calls::ToolCall;






#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
    
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
    
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}


#[derive(Clone, Debug, Serialize)]
pub struct AiResponse {
    pub message: AiMessage,
    pub usage: Option<AiUsage>,
    
    pub finish_reason: Option<String>,
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AiSkill {
    pub id: String,
    pub title: String,
    pub description: String,
    
    pub prompt_template: String,
    
    pub category: String,
}





#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HistoryView {
    
    pub session_id: String,
    
    pub updated_at: u64,
    
    pub messages: Vec<AiMessage>,
    
    pub warning: Option<String>,
}


#[derive(Clone, Debug, Serialize)]
pub struct AiTestResult {
    pub ok: bool,
    
    pub model: Option<String>,
    
    pub error: Option<String>,
}




#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiEditOp {
    pub start: usize,
    pub end: usize,
    pub text: String,
}


#[derive(Clone, Debug, Serialize)]
pub struct AiDiff {
    pub doc_id: String,
    
    pub base_rev: u64,
    pub original: String,
    pub modified: String,
    pub ops: Vec<AiEditOp>,
}


#[derive(Clone, Debug, Serialize)]
pub struct AiValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    
    
    pub report: Option<serde_json::Value>,
}
