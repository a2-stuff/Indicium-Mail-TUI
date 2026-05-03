//! Build reply / forward drafts from an existing message.

use chrono::Utc;
use imt_core::{
    draft::DraftKind, Address, Draft, DraftId, Message, MessageBody,
};

fn ensure_prefix(prefix: &str, subject: &str) -> String {
    if subject.to_lowercase().starts_with(&prefix.to_lowercase()) {
        subject.to_string()
    } else {
        format!("{prefix}{subject}")
    }
}

fn quote_body(body: &Option<MessageBody>) -> String {
    let plain = body
        .as_ref()
        .and_then(|b| b.text_plain.clone())
        .or_else(|| {
            body.as_ref().and_then(|b| {
                b.text_html.as_ref().map(|h| {
                    html2text::from_read(h.as_bytes(), 80).unwrap_or_default()
                })
            })
        })
        .unwrap_or_default();
    plain.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")
}

/// Build a reply draft. If `all` is true, include the original `cc` recipients.
pub fn build_reply(orig: &Message, all: bool, me: &Address) -> Draft {
    let now = Utc::now();
    let mut to: Vec<Address> = if !orig.headers.reply_to.is_empty() {
        orig.headers.reply_to.clone()
    } else {
        orig.headers.from.clone()
    };
    let mut cc: Vec<Address> = Vec::new();
    if all {
        for a in &orig.headers.to {
            if a.email != me.email && !to.iter().any(|x| x.email == a.email) {
                to.push(a.clone());
            }
        }
        for a in &orig.headers.cc {
            if a.email != me.email && !to.iter().any(|x| x.email == a.email) {
                cc.push(a.clone());
            }
        }
    }
    let intro = format!(
        "On {} {} wrote:",
        orig.headers.date.format("%Y-%m-%d %H:%M"),
        orig.headers.from.first().map(|a| a.format()).unwrap_or_default()
    );
    let body_text = format!("\n\n{}\n{}\n", intro, quote_body(&orig.body));
    let mut references = orig.headers.references.clone();
    if let Some(mid) = &orig.headers.rfc_message_id {
        references.push(mid.clone());
    }
    Draft {
        id: DraftId::new(),
        account_id: orig.account_id,
        kind: if all { DraftKind::ReplyAll } else { DraftKind::Reply },
        in_reply_to: Some(orig.id),
        from: me.clone(),
        to,
        cc,
        bcc: Vec::new(),
        subject: ensure_prefix("Re: ", &orig.headers.subject),
        body_text,
        attachments: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}

/// Build a forward draft.
pub fn build_forward(orig: &Message, me: &Address) -> Draft {
    let now = Utc::now();
    let header_block = format!(
        "---------- Forwarded message ----------\nFrom: {}\nDate: {}\nSubject: {}\nTo: {}\n",
        orig.headers.from.first().map(|a| a.format()).unwrap_or_default(),
        orig.headers.date.format("%Y-%m-%d %H:%M"),
        orig.headers.subject,
        orig.headers.to.iter().map(|a| a.format()).collect::<Vec<_>>().join(", "),
    );
    let body_plain = orig
        .body
        .as_ref()
        .and_then(|b| b.text_plain.clone())
        .or_else(|| {
            orig.body.as_ref().and_then(|b| {
                b.text_html.as_ref().map(|h| {
                    html2text::from_read(h.as_bytes(), 80).unwrap_or_default()
                })
            })
        })
        .unwrap_or_default();
    let body_text = format!("\n\n{}\n{}\n", header_block, body_plain);
    Draft {
        id: DraftId::new(),
        account_id: orig.account_id,
        kind: DraftKind::Forward,
        in_reply_to: Some(orig.id),
        from: me.clone(),
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: ensure_prefix("Fwd: ", &orig.headers.subject),
        body_text,
        attachments: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}
