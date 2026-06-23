//! Background AI reply generation.
//!
//! Drives a locally-installed CLI (Claude / Gemini / Codex) as a one-shot
//! subprocess to draft an email reply, then hands the text back to the compose
//! modal. Runs off the UI thread via `tokio::spawn`; the result is delivered
//! through a `std::sync::mpsc` channel the app polls each tick.

use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::Duration;

use imt_core::MessageBody;

use crate::settings::AiProvider;

/// Result of a background generation: the drafted reply, or an error message.
pub type AiResult = Result<String, String>;

/// Everything the model needs to draft a reply.
pub struct ReplyContext {
    pub my_name: String,
    pub my_email: String,
    pub from: String,
    pub subject: String,
    pub date: String,
    /// Plain text of the message/thread being replied to.
    pub original: String,
    /// Anything the user already typed in the body (key points / partial draft).
    pub user_notes: String,
}

/// Build the prompt fed to the provider CLI.
pub fn build_prompt(ctx: &ReplyContext) -> String {
    let mut p = String::new();
    p.push_str(
        "You are drafting a reply to an email on behalf of the user. \
Output ONLY the reply body text - no subject line, no To/From headers, no quoted \
original text, no markdown code fences, and no commentary before or after. \
Use a natural, professional email tone and a sensible greeting and sign-off. \
Do not invent facts that are not supported by the email thread or the user's notes.\n\n",
    );
    if !ctx.my_name.trim().is_empty() || !ctx.my_email.trim().is_empty() {
        p.push_str(&format!(
            "The reply is sent by: {} <{}>\n\n",
            ctx.my_name, ctx.my_email
        ));
    }
    p.push_str("--- EMAIL BEING REPLIED TO ---\n");
    if !ctx.from.trim().is_empty() {
        p.push_str(&format!("From: {}\n", ctx.from));
    }
    if !ctx.subject.trim().is_empty() {
        p.push_str(&format!("Subject: {}\n", ctx.subject));
    }
    if !ctx.date.trim().is_empty() {
        p.push_str(&format!("Date: {}\n", ctx.date));
    }
    p.push('\n');
    p.push_str(ctx.original.trim());
    p.push_str("\n--- END EMAIL ---\n\n");

    if ctx.user_notes.trim().is_empty() {
        p.push_str(
            "Write a complete, ready-to-send reply that appropriately responds to the email above.",
        );
    } else {
        p.push_str(
            "The user jotted the key points / a partial draft for the reply below. Expand and \
polish them into a complete, ready-to-send reply: weave in the concrete details they gave \
(times, locations, names, answers) together with the relevant context from the email above. \
Keep every concrete detail the user provided; do not drop or contradict any of them.\n\n",
        );
        p.push_str("--- USER'S NOTES / DRAFT ---\n");
        p.push_str(ctx.user_notes.trim());
        p.push_str("\n--- END NOTES ---");
    }
    p
}

/// Spawn a background task that runs the provider CLI and sends the result on `tx`.
/// Must be called from within a Tokio runtime (the TUI event loop is async).
pub fn spawn_generate(provider: AiProvider, model: String, prompt: String, tx: Sender<AiResult>) {
    tokio::spawn(async move {
        let res = run_cli(provider, &model, &prompt).await;
        let _ = tx.send(res);
    });
}

async fn run_cli(provider: AiProvider, model: &str, prompt: &str) -> AiResult {
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;

    let bin = provider.cli_bin();
    let model = model.trim();
    let mut cmd = Command::new(bin);

    // Non-interactive, single-shot invocation per provider. The prompt is passed
    // as one argv entry (no shell, so newlines/quotes are safe).
    match provider {
        AiProvider::Claude => {
            // `--strict-mcp-config` with no `--mcp-config` loads zero MCP
            // servers, so a one-shot reply doesn't pay the cost of connecting
            // to every MCP server in the user's global config.
            cmd.arg("-p").arg("--strict-mcp-config");
            if !model.is_empty() {
                cmd.arg("--model").arg(model);
            }
            cmd.arg(prompt);
        }
        AiProvider::Gemini => {
            if !model.is_empty() {
                cmd.arg("-m").arg(model);
            }
            cmd.arg("-p").arg(prompt);
        }
        AiProvider::Codex => {
            cmd.arg("exec");
            if !model.is_empty() {
                cmd.arg("-m").arg(model);
            }
            cmd.arg(prompt);
        }
    }

    // Run in a neutral directory so the CLI does not pick up a project-local
    // context file (e.g. CLAUDE.md) from wherever `imt` was launched.
    cmd.current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err(format!(
                    "'{bin}' CLI not found on PATH - install it or pick another provider in Settings"
                ));
            }
            return Err(format!("failed to run '{bin}': {e}"));
        }
    };

    let mut stdout = match child.stdout.take() {
        Some(s) => s,
        None => return Err(format!("could not capture {bin} output")),
    };
    let mut stderr = child.stderr.take();

    // Read the reply WITHOUT waiting for full process exit. Some environments
    // run slow exit hooks (e.g. a SessionEnd memory-sync) that keep the process
    // alive for a minute after the reply text is already printed. We read until
    // a brief idle gap follows the output, then stop the process.
    let first_byte = Duration::from_secs(180);
    let idle = Duration::from_secs(3);
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 8192];
    let mut got_any = false;
    loop {
        let to = if got_any { idle } else { first_byte };
        match tokio::time::timeout(to, stdout.read(&mut tmp)).await {
            Ok(Ok(0)) => break,                                   // clean EOF
            Ok(Ok(n)) => {
                got_any = true;
                buf.extend_from_slice(&tmp[..n]);
            }
            Ok(Err(e)) => return Err(format!("error reading {bin} output: {e}")),
            Err(_) => break, // startup timeout (no output) or idle gap (done)
        }
    }

    // Stop the process so a slow exit hook doesn't linger.
    let _ = child.start_kill();
    let _ = child.wait().await;

    let text = String::from_utf8_lossy(&buf).trim().to_string();
    if text.is_empty() {
        let mut errbuf = String::new();
        if let Some(mut e) = stderr.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), e.read_to_string(&mut errbuf)).await;
        }
        let short = errbuf
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        return Err(if short.is_empty() {
            format!("{bin} returned no output")
        } else {
            format!("{bin}: {short}")
        });
    }
    Ok(strip_fences(&text))
}

/// Strip a wrapping ``` code fence the model may add around the reply.
fn strip_fences(s: &str) -> String {
    let t = s.trim();
    if t.starts_with("```") {
        let mut lines: Vec<&str> = t.lines().collect();
        if !lines.is_empty() {
            lines.remove(0);
        }
        if lines
            .last()
            .map(|l| l.trim_start().starts_with("```"))
            .unwrap_or(false)
        {
            lines.pop();
        }
        return lines.join("\n").trim().to_string();
    }
    t.to_string()
}

/// Extract the plain text of a message body (preferring text, falling back to
/// rendered HTML).
pub fn body_to_text(b: &MessageBody) -> String {
    b.text_plain
        .clone()
        .or_else(|| {
            b.text_html
                .as_ref()
                .map(|h| html2text::from_read(h.as_bytes(), 80).unwrap_or_default())
        })
        .unwrap_or_default()
}

/// Split a reply-draft body into (user-typed notes, quoted original block).
/// The quoted block starts at the first `> ...` line or the `On ... wrote:` intro.
pub fn split_notes_and_quote(body: &str) -> (String, String) {
    let lines: Vec<&str> = body.lines().collect();
    let split_at = lines.iter().position(|line| {
        let t = line.trim_start();
        t.starts_with('>') || (t.starts_with("On ") && t.trim_end().ends_with("wrote:"))
    });
    match split_at {
        Some(i) => (
            lines[..i].join("\n").trim().to_string(),
            lines[i..].join("\n"),
        ),
        None => (body.trim().to_string(), String::new()),
    }
}

/// Strip leading `>` quote markers from a quoted block.
pub fn unquote(quoted: &str) -> String {
    quoted
        .lines()
        .map(|l| l.trim_start().trim_start_matches('>').trim_start())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
