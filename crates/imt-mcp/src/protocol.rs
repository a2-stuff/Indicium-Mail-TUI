use serde::{Deserialize, Serialize};
use serde_json::Value;

// Incoming: either a request (has id) or a notification (no id)
#[derive(Deserialize, Debug)]
pub struct RawMessage {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl RawMessage {
    pub fn is_notification(&self) -> bool {
        self.id.is_null()
    }
}

#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }
    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: None, error: Some(RpcError { code, message: message.into() }) }
    }
}

// MCP tool call result structure
pub fn tool_result(text: impl Into<String>, is_error: bool) -> Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": is_error
    })
}

pub fn tool_ok(text: impl Into<String>) -> Value {
    tool_result(text, false)
}

pub fn tool_error(text: impl Into<String>) -> Value {
    tool_result(text, true)
}
