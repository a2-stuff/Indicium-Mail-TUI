use anyhow::Context;
use imt_core::{AccountId, FolderRole};
use imt_store::FolderRepo;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::protocol::tool_ok;
use crate::McpContext;

pub async fn list_folders(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let account_id_str = params
        .get("account_id")
        .and_then(|v| v.as_str())
        .context("missing required parameter: account_id")?;

    let uuid = Uuid::parse_str(account_id_str)
        .with_context(|| format!("invalid account_id UUID: {account_id_str}"))?;
    let account_id = AccountId(uuid);

    let folders = FolderRepo::new(ctx.db.pool())
        .list_by_account(account_id)
        .await
        .context("failed to list folders")?;

    let items: Vec<Value> = folders
        .iter()
        .map(|f| {
            let role = role_to_str(f.role);
            json!({
                "id": f.id.0.to_string(),
                "name": f.name,
                "path": f.path,
                "role": role,
                "message_count": f.message_count,
                "unread_count": f.unread_count,
            })
        })
        .collect();

    let text = serde_json::to_string_pretty(&items).context("failed to serialize folders")?;
    Ok(tool_ok(text))
}

fn role_to_str(role: FolderRole) -> &'static str {
    match role {
        FolderRole::Inbox => "inbox",
        FolderRole::Sent => "sent",
        FolderRole::Drafts => "drafts",
        FolderRole::Trash => "trash",
        FolderRole::Junk => "junk",
        FolderRole::Archive => "archive",
        FolderRole::Other => "other",
    }
}
