//! Tool: search - Full-text search across messages.

use anyhow::Context;
use serde_json::Value;
use uuid::Uuid;

use imt_core::AccountId;
use imt_store::{MessageRepo, SearchRepo};

use crate::protocol::tool_ok;
use crate::McpContext;

pub async fn search(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let query = params["query"]
        .as_str()
        .context("query required")?;

    let account_id_opt = match params["account_id"].as_str() {
        Some(s) => {
            let uuid = Uuid::parse_str(s).context("invalid account_id UUID")?;
            Some(AccountId(uuid))
        }
        None => None,
    };

    let limit = params["limit"].as_u64().unwrap_or(25) as u32;

    let pool = ctx.db.pool();
    let message_ids = SearchRepo::new(pool)
        .query(account_id_opt, query, limit)
        .await?;

    let msg_repo = MessageRepo::new(pool);
    let mut results = Vec::new();

    for id in message_ids {
        let msg = match msg_repo.get(id).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let from_str = msg
            .headers
            .from
            .first()
            .map(|a| a.format())
            .unwrap_or_default();

        let date_str = msg.headers.date.to_rfc3339();

        results.push(serde_json::json!({
            "id": msg.id.0.to_string(),
            "subject": msg.headers.subject,
            "from": from_str,
            "date": date_str,
            "snippet": msg.snippet,
            "folder_id": msg.folder_id.0.to_string(),
        }));
    }

    let text = serde_json::to_string_pretty(&results)?;
    Ok(tool_ok(text))
}
