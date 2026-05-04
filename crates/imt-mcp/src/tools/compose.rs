//! Tools: send, reply - Compose and send email messages.

use anyhow::Context;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use imt_core::{AccountId, Address, Draft, DraftId, DraftKind, MessageId};
use imt_store::{AccountRepo, MessageRepo};

use crate::protocol::tool_ok;
use crate::McpContext;

/// Parse a comma-separated string of email addresses into a Vec<Address>.
/// Each token is trimmed and wrapped with no display name.
fn parse_addresses(s: &str) -> Vec<Address> {
    s.split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|email| Address::new(email))
        .collect()
}

/// Compose and send a new email message via SMTP.
pub async fn send(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let account_id_str = params["account_id"]
        .as_str()
        .context("account_id required")?;
    let account_uuid = Uuid::parse_str(account_id_str).context("invalid account_id UUID")?;
    let account_id = AccountId(account_uuid);

    let to_str = params["to"].as_str().context("to required")?;
    let subject = params["subject"].as_str().context("subject required")?.to_string();
    let body = params["body"].as_str().context("body required")?.to_string();

    let pool = ctx.db.pool();
    let account = AccountRepo::new(pool).get(account_id).await?;

    let to = parse_addresses(to_str);
    let cc = params["cc"]
        .as_str()
        .map(parse_addresses)
        .unwrap_or_default();
    let bcc = params["bcc"]
        .as_str()
        .map(parse_addresses)
        .unwrap_or_default();

    let now = Utc::now();
    let draft = Draft {
        id: DraftId::new(),
        account_id,
        kind: DraftKind::New,
        in_reply_to: None,
        from: account.address.clone(),
        to,
        cc,
        bcc,
        subject,
        body_text: body,
        attachments: vec![],
        created_at: now,
        updated_at: now,
    };

    ctx.engine.send(&draft).await?;

    Ok(tool_ok("Message sent successfully"))
}

/// Reply to an existing message.
pub async fn reply(ctx: &McpContext, params: Value) -> anyhow::Result<Value> {
    let message_id_str = params["message_id"]
        .as_str()
        .context("message_id required")?;
    let message_uuid = Uuid::parse_str(message_id_str).context("invalid message_id UUID")?;
    let message_id = MessageId(message_uuid);

    let body = params["body"].as_str().context("body required")?.to_string();
    let reply_all = params["reply_all"].as_bool().unwrap_or(false);

    let pool = ctx.db.pool();
    let msg = MessageRepo::new(pool).get(message_id).await?;
    let account = AccountRepo::new(pool).get(msg.account_id).await?;

    let subject = if msg.headers.subject.starts_with("Re: ") {
        msg.headers.subject.clone()
    } else {
        format!("Re: {}", msg.headers.subject)
    };

    // Reply goes to the original sender(s)
    let to = msg.headers.from.clone();

    // For reply-all, include original To (minus self) and original Cc in cc field
    let cc = if reply_all {
        let self_email = account.address.email.to_lowercase();
        let mut cc_addrs: Vec<Address> = msg
            .headers
            .to
            .iter()
            .filter(|a| a.email.to_lowercase() != self_email)
            .cloned()
            .collect();
        cc_addrs.extend(msg.headers.cc.clone());
        cc_addrs
    } else {
        vec![]
    };

    let kind = if reply_all { DraftKind::ReplyAll } else { DraftKind::Reply };
    let now = Utc::now();
    let draft = Draft {
        id: DraftId::new(),
        account_id: msg.account_id,
        kind,
        in_reply_to: Some(message_id),
        from: account.address.clone(),
        to,
        cc,
        bcc: vec![],
        subject,
        body_text: body,
        attachments: vec![],
        created_at: now,
        updated_at: now,
    };

    ctx.engine.send(&draft).await?;

    Ok(tool_ok("Reply sent successfully"))
}
