use anyhow::Context;
use imt_core::AuthMethod;
use imt_store::AccountRepo;
use serde_json::{json, Value};

use crate::protocol::tool_ok;
use crate::McpContext;

pub async fn list_accounts(ctx: &McpContext, _params: Value) -> anyhow::Result<Value> {
    let accounts = AccountRepo::new(ctx.db.pool())
        .list()
        .await
        .context("failed to list accounts")?;

    let items: Vec<Value> = accounts
        .iter()
        .map(|a| {
            let auth_type = match &a.imap.auth {
                AuthMethod::Password { .. } => "password",
                AuthMethod::OAuth2 { .. } => "oauth2",
            };
            json!({
                "id": a.id.0.to_string(),
                "display_name": a.display_name,
                "email": a.address.email,
                "imap_host": a.imap.host,
                "imap_port": a.imap.port,
                "smtp_host": a.smtp.host,
                "smtp_port": a.smtp.port,
                "auth_type": auth_type,
            })
        })
        .collect();

    let text = serde_json::to_string_pretty(&items).context("failed to serialize accounts")?;
    Ok(tool_ok(text))
}
