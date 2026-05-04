//! Tools: mark_read, toggle_flag - Manage message flags.

use anyhow::Context;
use serde_json::Value;
use uuid::Uuid;

use imt_core::{Flag, MessageId};
use imt_store::MessageRepo;

use crate::protocol::tool_ok;
use crate::McpContext;

/// Mark a message as read or unread.
pub async fn mark_read(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let id_str = params["message_id"]
        .as_str()
        .context("message_id required")?;
    let uuid = Uuid::parse_str(id_str).context("invalid UUID")?;
    let message_id = MessageId(uuid);

    let read = params["read"].as_bool().unwrap_or(true);

    ctx.engine.set_flag(message_id, Flag::Seen, read).await?;

    let text = if read { "Marked as read" } else { "Marked as unread" };
    Ok(tool_ok(text))
}

/// Toggle the starred/flagged status of a message.
pub async fn toggle_flag(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let id_str = params["message_id"]
        .as_str()
        .context("message_id required")?;
    let uuid = Uuid::parse_str(id_str).context("invalid UUID")?;
    let message_id = MessageId(uuid);

    let msg = MessageRepo::new(ctx.db.pool()).get(message_id).await?;
    let is_flagged = msg.flags.contains(&Flag::Flagged);

    ctx.engine
        .set_flag(message_id, Flag::Flagged, !is_flagged)
        .await?;

    let text = if is_flagged { "Flag removed" } else { "Message flagged" };
    Ok(tool_ok(text))
}
