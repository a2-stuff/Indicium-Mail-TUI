//! Tools: move_message, delete_message - Move and delete messages.

use anyhow::Context;
use serde_json::Value;
use uuid::Uuid;

use imt_core::{FolderId, FolderRole, MessageId};
use imt_store::{FolderRepo, MessageRepo};

use crate::protocol::tool_ok;
use crate::McpContext;

/// Move a message to a different folder.
pub async fn move_message(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let msg_id_str = params["message_id"]
        .as_str()
        .context("message_id required")?;
    let msg_uuid = Uuid::parse_str(msg_id_str).context("invalid message_id UUID")?;
    let message_id = MessageId(msg_uuid);

    let folder_id_str = params["folder_id"]
        .as_str()
        .context("folder_id required")?;
    let folder_uuid = Uuid::parse_str(folder_id_str).context("invalid folder_id UUID")?;
    let folder_id = FolderId(folder_uuid);

    ctx.engine.move_message(message_id, folder_id).await?;

    Ok(tool_ok("Message moved"))
}

/// Delete a message by moving it to the Trash folder.
pub async fn delete_message(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let id_str = params["message_id"]
        .as_str()
        .context("message_id required")?;
    let uuid = Uuid::parse_str(id_str).context("invalid UUID")?;
    let message_id = MessageId(uuid);

    let pool = ctx.db.pool();
    let msg = MessageRepo::new(pool).get(message_id).await?;
    let folders = FolderRepo::new(pool).list_by_account(msg.account_id).await?;

    let trash = folders.iter().find(|f| f.role == FolderRole::Trash);

    match trash {
        Some(trash_folder) => {
            ctx.engine
                .move_message(message_id, trash_folder.id)
                .await?;
            Ok(tool_ok("Message moved to Trash"))
        }
        None => Ok(tool_ok(
            "No Trash folder found - message kept on server",
        )),
    }
}
