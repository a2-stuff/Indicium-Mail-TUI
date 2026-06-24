//! On-demand conversation threading.
//!
//! `thread_id` is not populated by the sync engine, so we reconstruct a thread
//! at view time from the RFC 822 headers (Message-ID / In-Reply-To / References)
//! of the messages already loaded across folders. This naturally groups an
//! original message with replies that live in other folders (e.g. Sent).

use std::collections::HashSet;

use imt_core::{Message, MessageId};

/// The Message-ID-ish keys that tie a message into a conversation.
fn keys(m: &Message) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(id) = &m.headers.rfc_message_id {
        if !id.is_empty() {
            v.push(id.clone());
        }
    }
    if let Some(ir) = &m.headers.in_reply_to {
        if !ir.is_empty() {
            v.push(ir.clone());
        }
    }
    for r in &m.headers.references {
        if !r.is_empty() {
            v.push(r.clone());
        }
    }
    v
}

/// Normalize a subject for fallback grouping: strip leading Re:/Fwd:/Fw:
/// prefixes and lowercase.
pub fn normalize_subject(s: &str) -> String {
    let mut t = s.trim();
    loop {
        let lower = t.to_ascii_lowercase();
        let stripped = lower
            .strip_prefix("re:")
            .or_else(|| lower.strip_prefix("fwd:"))
            .or_else(|| lower.strip_prefix("fw:"));
        match stripped {
            Some(_) => {
                // advance past the prefix in the original string
                let idx = t.find(':').map(|i| i + 1).unwrap_or(0);
                t = t[idx..].trim_start();
            }
            None => break,
        }
    }
    t.trim().to_ascii_lowercase()
}

/// Collect the conversation that `target_id` belongs to from `all` messages,
/// sorted oldest-first. Returns at least the target (if present).
pub fn collect_thread(all: &[Message], target_id: MessageId) -> Vec<Message> {
    let target = match all.iter().find(|m| m.id == target_id) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let account = target.account_id;
    let pool: Vec<&Message> = all.iter().filter(|m| m.account_id == account).collect();

    // Grow the set of messages reachable through shared header keys.
    let mut in_thread: HashSet<MessageId> = HashSet::new();
    in_thread.insert(target.id);
    let mut keyset: HashSet<String> = keys(target).into_iter().collect();
    loop {
        let mut added = false;
        for m in &pool {
            if in_thread.contains(&m.id) {
                continue;
            }
            let mk = keys(m);
            if mk.iter().any(|k| keyset.contains(k)) {
                in_thread.insert(m.id);
                keyset.extend(mk);
                added = true;
            }
        }
        if !added {
            break;
        }
    }

    let mut seen: HashSet<MessageId> = HashSet::new();
    let mut result: Vec<Message> = pool
        .iter()
        .filter(|m| in_thread.contains(&m.id) && seen.insert(m.id))
        .map(|m| (*m).clone())
        .collect();

    // NOTE: grouping is by RFC 822 references only (Message-ID / In-Reply-To /
    // References). We deliberately do NOT fall back to matching on the subject
    // line - unrelated messages that merely share a subject (even the same
    // subject sent from a different account) must not be lumped into one thread.

    result.sort_by_key(|m| m.headers.date);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use imt_core::{AccountId, FolderId, MessageHeaders, Uid};

    fn headers(subject: &str, msg_id: Option<&str>, in_reply_to: Option<&str>) -> MessageHeaders {
        MessageHeaders {
            rfc_message_id: msg_id.map(str::to_string),
            in_reply_to: in_reply_to.map(str::to_string),
            references: Vec::new(),
            from: Vec::new(),
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            subject: subject.to_string(),
            date: Utc::now(),
        }
    }

    fn msg(account: AccountId, headers: MessageHeaders) -> Message {
        Message {
            id: MessageId::new(),
            account_id: account,
            folder_id: FolderId::new(),
            thread_id: None,
            uid: Uid(1),
            headers,
            flags: Vec::new(),
            size: 0,
            body: None,
            has_attachments: false,
            snippet: String::new(),
            internal_date: Utc::now(),
        }
    }

    #[test]
    fn same_subject_unrelated_messages_do_not_group() {
        let acc = AccountId::new();
        // Two messages with the identical subject but NO shared references.
        let a = msg(acc, headers("Tester", Some("<a@x>"), None));
        let b = msg(acc, headers("Tester", Some("<b@y>"), None));
        let id = a.id;
        let all = vec![a, b];
        let thread = collect_thread(&all, id);
        assert_eq!(thread.len(), 1, "subject must not group unrelated messages");
    }

    #[test]
    fn genuine_reply_groups_via_references() {
        let acc = AccountId::new();
        let original = msg(acc, headers("Tester", Some("<orig@x>"), None));
        // Reply points at the original via In-Reply-To.
        let reply = msg(acc, headers("Re: Tester", Some("<reply@x>"), Some("<orig@x>")));
        let id = original.id;
        let all = vec![original, reply];
        let thread = collect_thread(&all, id);
        assert_eq!(thread.len(), 2, "genuine reply should join the thread");
    }
}
