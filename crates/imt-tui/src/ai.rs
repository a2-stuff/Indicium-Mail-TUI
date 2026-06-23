//! Background AI reply generation.
//!
//! Drives a locally-installed CLI (Claude / Gemini / Codex) as a one-shot
//! subprocess to draft an email reply, then hands the text back to the compose
//! modal. Runs off the UI thread via `tokio::spawn`; the result is delivered
//! through a `std::sync::mpsc` channel the app polls each tick.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::Duration;

use imt_core::MessageBody;

use crate::settings::AiProvider;

/// A generated reply: the drafted body text plus any files the model created in
/// its working `attachments/` directory (to be attached to the email).
pub struct AiReply {
    pub text: String,
    pub attachments: Vec<PathBuf>,
}

/// Result of a background generation: the drafted reply, or an error message.
pub type AiResult = Result<AiReply, String>;

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
    /// Extra instruction / context the user gave for this reply (optional).
    pub instruction: String,
}

/// Build the prompt fed to the provider CLI.
pub fn build_prompt(ctx: &ReplyContext) -> String {
    let mut p = String::new();
    p.push_str(
        "You are drafting a reply to an email on behalf of the user. \
Output ONLY the reply body text - no subject line, no To/From headers, no quoted \
original text, no markdown code fences, and no commentary before or after.\n\n\
Write like a real person, not a template. Use natural paragraphs of one to a few \
sentences each. Separate paragraphs with a SINGLE blank line. Do NOT put a blank \
line between every line or after every sentence. Start with a brief greeting and \
end with a short sign-off. Keep it concise. \
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

    if !ctx.instruction.trim().is_empty() {
        p.push_str(
            "\n\nAdditional instruction / context from the user for this reply - follow it \
while still respecting the email thread above:\n--- INSTRUCTION ---\n",
        );
        p.push_str(ctx.instruction.trim());
        p.push_str("\n--- END INSTRUCTION ---");
    }

    // File attachments: any final file dropped in ./attachments/ is attached to
    // the email automatically. Only relevant if the request asks for a file.
    p.push_str(
        "\n\nATTACHMENTS: If the instruction or notes ask you to create, generate, or attach \
a file (text, CSV, Excel .xlsx, PDF, image .png/.jpg, ZIP, etc.), write each FINAL file you \
want attached into the ./attachments/ directory of your current working directory (it already \
exists). Use the rest of the working directory for any scratch/intermediate work - only files \
in ./attachments/ are attached. You may write and run scripts to produce real binary files \
(e.g. a Python script to render a PNG or build an .xlsx). Do NOT mention file paths in the \
reply body; still output ONLY the reply body text on stdout. If no file was requested, create \
nothing and just write the reply.",
    );
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

    // Fresh, isolated working directory for this run. The model writes any files
    // to attach into <workdir>/attachments/; we scan that afterwards. Using a
    // dedicated temp dir also means the CLI never picks up a project-local
    // context file (e.g. CLAUDE.md) from wherever `imt` was launched.
    let workdir = std::env::temp_dir().join(format!(
        "imt-ai-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    let attach_dir = workdir.join("attachments");
    let _ = std::fs::create_dir_all(&attach_dir);

    // Non-interactive, single-shot invocation per provider. The prompt is passed
    // as one argv entry (no shell, so newlines/quotes are safe).
    match provider {
        AiProvider::Claude => {
            // `--strict-mcp-config` with no `--mcp-config` loads zero MCP
            // servers, so a one-shot reply doesn't pay the cost of connecting
            // to every MCP server in the user's global config.
            cmd.arg("-p").arg("--strict-mcp-config");
            // Auto-approve the tools needed to create files (write a script,
            // run it, save the output) so file generation works unattended in
            // this isolated working directory.
            cmd.arg("--permission-mode").arg("acceptEdits");
            cmd.arg("--allowedTools")
                .arg("Read Write Edit Bash Glob Grep");
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

    cmd.current_dir(&workdir)
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

    let text = String::from_utf8_lossy(&buf).to_string();
    let text = text.trim().to_string();
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
    Ok(AiReply {
        text: normalize_reply(&strip_fences(&text)),
        attachments: collect_attachments(&attach_dir),
    })
}

/// Collect the files the model left in its `attachments/` directory, to attach
/// to the email. Top-level files only (skips subdirectories and dotfiles).
fn collect_attachments(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
        let hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(true);
        if is_file && !hidden {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Tidy model output into human spacing: trim trailing whitespace on each line
/// and collapse runs of 3+ blank lines down to a single blank line between
/// paragraphs (kills the robotic "every line double-spaced" look).
pub fn normalize_reply(s: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut blanks = 0;
    for raw in s.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            blanks += 1;
            continue;
        }
        if !out.is_empty() && blanks > 0 {
            out.push(String::new()); // exactly one blank line between paragraphs
        }
        blanks = 0;
        out.push(line.to_string());
    }
    out.join("\n").trim().to_string()
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

/// Hard-wrap a compose body to `width` columns. Normal paragraphs (runs of
/// non-blank, non-quoted lines) are merged then re-wrapped at word boundaries;
/// blank lines and quoted/reply-intro lines (`>` / "On ... wrote:") are kept
/// verbatim so the quoted thread is never reflowed.
pub fn wrap_body(text: &str, width: usize) -> String {
    if width < 8 {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let is_special = |l: &str| {
        let t = l.trim_start();
        t.starts_with('>') || (t.starts_with("On ") && t.trim_end().ends_with("wrote:"))
    };
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            out.push(String::new());
            i += 1;
            continue;
        }
        if is_special(line) {
            out.push(line.to_string());
            i += 1;
            continue;
        }
        // Gather a paragraph of consecutive normal lines.
        let mut para = String::new();
        while i < lines.len() && !lines[i].trim().is_empty() && !is_special(lines[i]) {
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(lines[i].trim());
            i += 1;
        }
        out.extend(wrap_paragraph(&para, width));
    }
    out.join("\n")
}

/// Break lines longer than `width` at word boundaries WITHOUT merging existing
/// line breaks (so signatures, addresses, and lists keep their lines). Only ever
/// swaps a space for a newline, so total length and character indices are
/// preserved (lets callers keep the caret position). Quoted `>` lines are left
/// untouched. Words longer than `width` with no space are left to overflow.
pub fn break_long_lines(text: &str, width: usize) -> String {
    if width < 8 {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut col = 0usize;
    let mut last_space: Option<usize> = None;
    let mut quote_line = n > 0 && chars[0] == '>';
    while i < n {
        let ch = chars[i];
        if ch == '\n' {
            col = 0;
            last_space = None;
            quote_line = i + 1 < n && chars[i + 1] == '>';
            i += 1;
            continue;
        }
        if ch == ' ' {
            last_space = Some(i);
        }
        col += 1;
        if !quote_line && col > width {
            if let Some(s) = last_space {
                chars[s] = '\n';
                col = i - s; // chars now on the new line, up to and including i
                last_space = None;
            }
        }
        i += 1;
    }
    chars.into_iter().collect()
}

fn wrap_paragraph(p: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in p.split_whitespace() {
        let wlen = word.chars().count();
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.chars().count() + 1 + wlen <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
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
