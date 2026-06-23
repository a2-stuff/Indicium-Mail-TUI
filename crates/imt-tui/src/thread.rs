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

    // Fallback: if the headers didn't link anything, group by normalized subject.
    if result.len() <= 1 {
        let subj = normalize_subject(&target.headers.subject);
        if !subj.is_empty() {
            seen.clear();
            result = pool
                .iter()
                .filter(|m| normalize_subject(&m.headers.subject) == subj && seen.insert(m.id))
                .map(|m| (*m).clone())
                .collect();
        }
    }

    result.sort_by_key(|m| m.headers.date);
    result
}
