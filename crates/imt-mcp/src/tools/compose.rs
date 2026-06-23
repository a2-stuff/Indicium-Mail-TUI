//! Tools: send, reply - Compose and send email messages.

use anyhow::Context;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use imt_core::draft::DraftAttachment;
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

/// Guess a MIME type from a file extension. Covers the common document, image,
/// and archive types; falls back to application/octet-stream.
fn guess_mime(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let m = match ext.as_str() {
        "txt" | "text" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "tar" => "application/x-tar",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    };
    m.to_string()
}

/// Build draft attachments from a JSON `attachments` param. Accepts either an
/// array of file-path strings, or an array of objects `{path, filename?}`. Each
/// path must point to a readable file on disk. Returns an error naming any path
/// that does not exist so the agent can correct it.
fn build_attachments(params: &Value) -> anyhow::Result<Vec<DraftAttachment>> {
    let Some(items) = params["attachments"].as_array() else {
        return Ok(vec![]);
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let (path_str, name_override) = match item {
            Value::String(s) => (s.as_str(), None),
            Value::Object(_) => (
                item["path"].as_str().context("attachment object needs a 'path'")?,
                item["filename"].as_str().map(|s| s.to_string()),
            ),
            _ => anyhow::bail!("each attachment must be a file path string or an object with 'path'"),
        };
        let path = std::path::PathBuf::from(path_str);
        let meta = std::fs::metadata(&path)
            .with_context(|| format!("attachment not found or unreadable: {path_str}"))?;
        if !meta.is_file() {
            anyhow::bail!("attachment is not a file: {path_str}");
        }
        let filename = name_override.unwrap_or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string()
        });
        out.push(DraftAttachment {
            filename,
            mime_type: guess_mime(&path),
            path,
            size: meta.len(),
        });
    }
    Ok(out)
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

    let attachments = build_attachments(&params)?;
    let n_att = attachments.len();

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
        attachments,
        created_at: now,
        updated_at: now,
    };

    ctx.engine.send(&draft).await?;

    Ok(tool_ok(if n_att > 0 {
        format!("Message sent successfully with {n_att} attachment(s)")
    } else {
        "Message sent successfully".to_string()
    }))
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

    let attachments = build_attachments(&params)?;
    let n_att = attachments.len();

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
        attachments,
        created_at: now,
        updated_at: now,
    };

    ctx.engine.send(&draft).await?;

    Ok(tool_ok(if n_att > 0 {
        format!("Reply sent successfully with {n_att} attachment(s)")
    } else {
        "Reply sent successfully".to_string()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mime_by_extension() {
        assert_eq!(guess_mime(std::path::Path::new("a.csv")), "text/csv");
        assert_eq!(guess_mime(std::path::Path::new("a.pdf")), "application/pdf");
        assert_eq!(guess_mime(std::path::Path::new("a.png")), "image/png");
        assert_eq!(
            guess_mime(std::path::Path::new("a.xlsx")),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert_eq!(guess_mime(std::path::Path::new("a.zip")), "application/zip");
        assert_eq!(
            guess_mime(std::path::Path::new("mystery")),
            "application/octet-stream"
        );
    }

    #[test]
    fn build_from_paths_and_objects() {
        let dir = std::env::temp_dir().join(format!("imt-mcp-test-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let f1 = dir.join("report.csv");
        std::fs::write(&f1, b"a,b,c\n1,2,3\n").unwrap();
        let f2 = dir.join("data.bin");
        std::fs::write(&f2, b"\x00\x01\x02").unwrap();

        // Array of path strings.
        let params = json!({ "attachments": [f1.to_str().unwrap()] });
        let atts = build_attachments(&params).unwrap();
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].filename, "report.csv");
        assert_eq!(atts[0].mime_type, "text/csv");
        assert_eq!(atts[0].size, 12);

        // Array of objects with a filename override.
        let params = json!({ "attachments": [{ "path": f2.to_str().unwrap(), "filename": "renamed.bin" }] });
        let atts = build_attachments(&params).unwrap();
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].filename, "renamed.bin");
        assert_eq!(atts[0].mime_type, "application/octet-stream");

        // Missing param -> no attachments.
        assert!(build_attachments(&json!({})).unwrap().is_empty());

        // Nonexistent file -> error naming the path.
        let bad = json!({ "attachments": ["/no/such/file.txt"] });
        let err = build_attachments(&bad).unwrap_err().to_string();
        assert!(err.contains("/no/such/file.txt"), "got: {err}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
