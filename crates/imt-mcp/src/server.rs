use std::sync::Arc;

use serde_json::Value;
use tracing::warn;

use crate::protocol::{JsonRpcResponse, RawMessage, tool_error};
use crate::transport::Transport;
use crate::{tools, McpContext};

pub async fn run(ctx: Arc<McpContext>) -> anyhow::Result<()> {
    let mut transport = Transport::new();

    loop {
        let line = match transport.read_line().await? {
            Some(l) => l,
            None => return Ok(()), // EOF
        };

        if line.is_empty() {
            continue;
        }

        let msg: RawMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to parse JSON-RPC message: {e}");
                continue;
            }
        };

        // Notifications (no id) - handle and skip any response
        if msg.is_notification() {
            // "initialized" and other notifications - nothing to respond to
            continue;
        }

        let id = msg.id.clone();

        let response = match msg.method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "imt-mcp", "version": "0.0.16" }
                });
                JsonRpcResponse::ok(id, result)
            }

            "ping" => JsonRpcResponse::ok(id, serde_json::json!({})),

            "tools/list" => {
                let result = serde_json::json!({ "tools": tools::all_tools() });
                JsonRpcResponse::ok(id, result)
            }

            "tools/call" => {
                let params = msg.params;
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(Value::Object(Default::default()));

                let tool_result = dispatch_tool(&ctx, name, args).await;

                match tool_result {
                    Ok(val) => JsonRpcResponse::ok(id, val),
                    Err(e) => {
                        JsonRpcResponse::ok(id, tool_error(format!("{e:#}")))
                    }
                }
            }

            other => {
                warn!("Unknown method: {other}");
                JsonRpcResponse::err(id, -32601, "Method not found")
            }
        };

        if let Err(e) = transport.write_response(&response) {
            warn!("Failed to write response: {e}");
        }
    }
}

async fn dispatch_tool(
    ctx: &McpContext,
    name: &str,
    args: Value,
) -> anyhow::Result<Value> {
    match name {
        "list_accounts" => tools::accounts::list_accounts(ctx, args).await,
        "list_folders" => tools::folders::list_folders(ctx, args).await,
        "list_messages" => tools::messages::list_messages(ctx, args).await,
        "read_message" => tools::messages::read_message(ctx, args).await,
        "search" => tools::search::search(ctx, args).await,
        "send" => tools::compose::send(ctx, args).await,
        "reply" => tools::compose::reply(ctx, args).await,
        "mark_read" => tools::flags::mark_read(ctx, args).await,
        "toggle_flag" => tools::flags::toggle_flag(ctx, args).await,
        "move_message" => tools::move_::move_message(ctx, args).await,
        "delete_message" => tools::move_::delete_message(ctx, args).await,
        _ => Err(anyhow::anyhow!("unknown tool: {name}")),
    }
}
