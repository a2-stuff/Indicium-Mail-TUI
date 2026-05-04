use anyhow::Context;
use imt_core::{Flag, FolderId, MessageId};
use imt_store::MessageRepo;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::protocol::tool_ok;
use crate::McpContext;

pub async fn list_messages(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let folder_id_str = params
        .get("folder_id")
        .and_then(|v| v.as_str())
        .context("missing required parameter: folder_id")?;

    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as u32;

    let offset = params
        .get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let folder_uuid = Uuid::parse_str(folder_id_str)
        .with_context(|| format!("invalid folder_id UUID: {folder_id_str}"))?;
    let folder_id = FolderId(folder_uuid);

    let messages = MessageRepo::new(ctx.db.pool())
        .list_by_folder(folder_id, limit, offset)
        .await
        .context("failed to list messages")?;

    let items: Vec<Value> = messages
        .iter()
        .map(|m| {
            let from = format_addresses(&m.headers.from);
            let flags = format_flags(&m.flags);
            json!({
                "id": m.id.0.to_string(),
                "subject": m.headers.subject,
                "from": from,
                "date": m.internal_date.to_rfc3339(),
                "snippet": m.snippet,
                "flags": flags,
                "size": m.size,
                "has_body": m.body.is_some(),
            })
        })
        .collect();

    let text = serde_json::to_string_pretty(&items).context("failed to serialize messages")?;
    Ok(tool_ok(text))
}

pub async fn read_message(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let message_id_str = params
        .get("message_id")
        .and_then(|v| v.as_str())
        .context("missing required parameter: message_id")?;

    let message_uuid = Uuid::parse_str(message_id_str)
        .with_context(|| format!("invalid message_id UUID: {message_id_str}"))?;
    let message_id = MessageId(message_uuid);

    // Initial fetch to check if body is present
    let msg = MessageRepo::new(ctx.db.pool())
        .get(message_id)
        .await
        .context("failed to fetch message")?;

    // If no body cached, fetch from IMAP and re-read
    let msg = if msg.body.is_none() {
        ctx.engine
            .fetch_body(message_id)
            .await
            .context("failed to fetch body from IMAP")?;

        MessageRepo::new(ctx.db.pool())
            .get(message_id)
            .await
            .context("failed to re-fetch message after body fetch")?
    } else {
        msg
    };

    let body_text = msg
        .body
        .as_ref()
        .and_then(|b| b.text_plain.as_deref())
        .unwrap_or("");

    let body_html = msg
        .body
        .as_ref()
        .and_then(|b| b.text_html.as_deref())
        .unwrap_or("");

    let result = json!({
        "id": msg.id.0.to_string(),
        "subject": msg.headers.subject,
        "from": format_addresses(&msg.headers.from),
        "to": format_addresses(&msg.headers.to),
        "cc": format_addresses(&msg.headers.cc),
        "date": msg.internal_date.to_rfc3339(),
        "flags": format_flags(&msg.flags),
        "body_text": body_text,
        "body_html": body_html,
        "snippet": msg.snippet,
    });

    let text = serde_json::to_string_pretty(&result).context("failed to serialize message")?;
    Ok(tool_ok(text))
}

fn format_addresses(addrs: &[imt_core::Address]) -> String {
    addrs
        .iter()
        .map(|a| a.format())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_flags(flags: &[Flag]) -> Vec<String> {
    flags
        .iter()
        .map(|f| match f {
            Flag::Seen => "Seen".to_string(),
            Flag::Answered => "Answered".to_string(),
            Flag::Flagged => "Flagged".to_string(),
            Flag::Deleted => "Deleted".to_string(),
            Flag::Draft => "Draft".to_string(),
            Flag::Recent => "Recent".to_string(),
            Flag::Custom(s) => s.clone(),
        })
        .collect()
}
