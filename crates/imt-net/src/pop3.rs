//! POP3 backend implementing [`MailBackend`].
//!
//! POP3 (RFC 1939) is a download-oriented protocol: it exposes a single
//! maildrop (modelled here as the `INBOX` folder), has no server-side flags,
//! no folders, and no IDLE. Message identity is the per-session sequence
//! number, which is stable for an append-only maildrop (the common case when
//! `keep_on_server` is true). When `keep_on_server` is false the sync engine
//! issues [`delete_uid`](MailBackend::delete_uid) (POP3 `DELE`) after a body is
//! downloaded; the deletion is committed by the `QUIT` issued on disconnect.
//!
//! Reading parsers (`parse_headers_from_bytes`, `parse_full_body`,
//! `header_has_attachments`) are shared with the IMAP backend.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_native_tls::TlsStream;
use async_trait::async_trait;
use base64::Engine as _;
use tokio::io::{
    AsyncBufReadExt, AsyncRead as TokioAsyncRead, AsyncWrite as TokioAsyncWrite, AsyncWriteExt,
    BufReader,
};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::sleep;

use imt_core::{Account, AuthMethod, MessageBody, FolderRole, Tls};

use crate::backend::{
    EnvelopeFetch, FolderInfo, FolderState, IdleEvent, IdleHandle, IdleHandleImpl, MailBackend,
    UidRange,
};
use crate::imap::{header_has_attachments, parse_full_body, parse_headers_from_bytes, PasswordProvider};
use crate::error::{NetError, Result};

/// How often the POP3 worker re-checks the maildrop for new mail (POP3 has no
/// IDLE). Short enough to feel live, long enough to keep the connection's
/// autologout timer (RFC 1939 minimum 10 minutes) from firing.
const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// POP3 transport stream, either implicitly TLS-wrapped or plain TCP.
enum Pop3Stream {
    Tls(Box<TlsStream<TcpStream>>),
    Plain(TcpStream),
}

impl TokioAsyncRead for Pop3Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Pop3Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
            Pop3Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl TokioAsyncWrite for Pop3Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Pop3Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
            Pop3Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Pop3Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
            Pop3Stream::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Pop3Stream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
            Pop3Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// A live, authenticated POP3 connection.
struct Pop3Conn {
    reader: BufReader<Pop3Stream>,
}

/// Strip a single trailing CRLF (or lone LF) from a raw line.
fn trim_eol(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    if end > 0 && line[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && line[end - 1] == b'\r' {
        end -= 1;
    }
    &line[..end]
}

impl Pop3Conn {
    fn new(stream: Pop3Stream) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    /// Read one CRLF-terminated line as raw bytes (includes the terminator).
    async fn read_line(&mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        let n = self
            .reader
            .read_until(b'\n', &mut buf)
            .await
            .map_err(|e| NetError::Connect(e.to_string()))?;
        if n == 0 {
            return Err(NetError::Connect("POP3 connection closed".into()));
        }
        Ok(buf)
    }

    /// Read a status line, returning the text after `+OK ` on success or an
    /// error carrying the server's `-ERR` text.
    async fn read_status(&mut self) -> Result<String> {
        let line = self.read_line().await?;
        let text = String::from_utf8_lossy(trim_eol(&line)).to_string();
        if let Some(rest) = text.strip_prefix("+OK") {
            Ok(rest.trim_start().to_string())
        } else if let Some(rest) = text.strip_prefix("-ERR") {
            Err(NetError::Protocol(format!("POP3 -ERR{}", rest)))
        } else {
            Err(NetError::Protocol(format!("unexpected POP3 response: {text}")))
        }
    }

    /// Write a command line and flush it.
    async fn write_cmd(&mut self, cmd: &str) -> Result<()> {
        self.reader
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| NetError::Connect(e.to_string()))?;
        self.reader
            .write_all(b"\r\n")
            .await
            .map_err(|e| NetError::Connect(e.to_string()))?;
        self.reader
            .flush()
            .await
            .map_err(|e| NetError::Connect(e.to_string()))?;
        Ok(())
    }

    /// Send a single-line command and return the `+OK` status text.
    async fn command(&mut self, cmd: &str) -> Result<String> {
        self.write_cmd(cmd).await?;
        self.read_status().await
    }

    /// Read a multi-line response body (after a `+OK`), terminated by a line
    /// containing only `.`. Un-stuffs leading dots and preserves CRLFs.
    async fn read_multiline(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let line = self.read_line().await?;
            if trim_eol(&line) == b"." {
                break;
            }
            // Byte-stuffing: a leading '.' added by the server is doubled.
            if line.first() == Some(&b'.') {
                out.extend_from_slice(&line[1..]);
            } else {
                out.extend_from_slice(&line);
            }
        }
        Ok(out)
    }

    /// Send a command expected to produce a multi-line response.
    async fn command_multiline(&mut self, cmd: &str) -> Result<Vec<u8>> {
        self.write_cmd(cmd).await?;
        self.read_status().await?;
        self.read_multiline().await
    }

    /// `STAT` -> (message count, total size in octets).
    async fn stat(&mut self) -> Result<(u32, u64)> {
        let status = self.command("STAT").await?;
        let mut parts = status.split_whitespace();
        let count = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let size = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        Ok((count, size))
    }

    /// Extract the underlying TCP stream (used for the STARTTLS upgrade).
    fn into_tcp(self) -> Result<TcpStream> {
        match self.reader.into_inner() {
            Pop3Stream::Plain(t) => Ok(t),
            Pop3Stream::Tls(_) => Err(NetError::other("STLS began on a TLS stream")),
        }
    }
}

/// POP3 backend tied to a single account.
pub struct Pop3Backend {
    account: Account,
    password_provider: PasswordProvider,
    conn: Arc<Mutex<Option<Pop3Conn>>>,
}

impl Pop3Backend {
    /// Construct a new backend; no network activity happens until `connect`.
    pub fn new(account: Account, password_provider: PasswordProvider) -> Self {
        Self {
            account,
            password_provider,
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Dial the server, consume the greeting, and (for STARTTLS) upgrade.
    /// Returns an un-authenticated connection.
    async fn dial(&self) -> Result<Pop3Conn> {
        let host = self.account.imap.host.clone();
        let port = self.account.imap.port;
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|e| NetError::Connect(format!("tcp connect {}:{}: {}", host, port, e)))?;

        match self.account.imap.tls {
            Tls::Implicit => {
                let tls = async_native_tls::connect(host.as_str(), tcp)
                    .await
                    .map_err(|e| NetError::Tls(e.to_string()))?;
                let mut conn = Pop3Conn::new(Pop3Stream::Tls(Box::new(tls)));
                conn.read_status().await?; // greeting
                Ok(conn)
            }
            Tls::None => {
                let mut conn = Pop3Conn::new(Pop3Stream::Plain(tcp));
                conn.read_status().await?; // greeting
                Ok(conn)
            }
            Tls::StartTls => {
                let mut conn = Pop3Conn::new(Pop3Stream::Plain(tcp));
                conn.read_status().await?; // greeting
                conn.command("STLS").await?;
                let raw = conn.into_tcp()?;
                let tls = async_native_tls::connect(host.as_str(), raw)
                    .await
                    .map_err(|e| NetError::Tls(e.to_string()))?;
                // No greeting is sent after a successful STLS upgrade.
                Ok(Pop3Conn::new(Pop3Stream::Tls(Box::new(tls))))
            }
        }
    }

    /// Authenticate the freshly-dialed connection (USER/PASS or AUTH XOAUTH2).
    async fn authenticate(&self, conn: &mut Pop3Conn) -> Result<()> {
        match &self.account.imap.auth {
            AuthMethod::Password { username } => {
                let password = (self.password_provider)(username).ok_or_else(|| {
                    NetError::Auth(format!("no password available for {}", username))
                })?;
                conn.command(&format!("USER {username}"))
                    .await
                    .map_err(|e| NetError::Auth(format!("USER: {e}")))?;
                conn.command(&format!("PASS {password}"))
                    .await
                    .map_err(|e| NetError::Auth(format!("PASS rejected: {e}")))?;
                Ok(())
            }
            AuthMethod::OAuth2 { username, .. } => {
                let access_token = (self.password_provider)(username).ok_or_else(|| {
                    NetError::Auth(format!("no oauth access token available for {}", username))
                })?;
                let raw = format!("user={}\x01auth=Bearer {}\x01\x01", username, access_token);
                let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
                conn.command(&format!("AUTH XOAUTH2 {encoded}"))
                    .await
                    .map_err(|e| NetError::Auth(format!("XOAUTH2 rejected: {e}")))?;
                Ok(())
            }
        }
    }

    /// Resolve a [`UidRange`] into concrete 1-based POP3 sequence numbers,
    /// clamped to the current message count.
    fn seqs_for(range: &UidRange, count: u32) -> Vec<u32> {
        match range {
            UidRange::All => (1..=count).collect(),
            UidRange::Range(start, end) => {
                let lo = (*start).max(1);
                let hi = (*end).min(count);
                if lo > hi {
                    Vec::new()
                } else {
                    (lo..=hi).collect()
                }
            }
            UidRange::Set(uids) => uids.iter().copied().filter(|u| *u >= 1 && *u <= count).collect(),
        }
    }
}

#[async_trait]
impl MailBackend for Pop3Backend {
    async fn connect(&mut self) -> Result<()> {
        if matches!(self.account.imap.tls, Tls::None) {
            tracing::warn!(
                target: "imt-net::pop3",
                "POP3 connection to {} is using PLAINTEXT (Tls::None) - credentials transmitted unencrypted",
                self.account.imap.host
            );
        }
        let mut conn = self.dial().await?;
        self.authenticate(&mut conn).await?;
        *self.conn.lock().await = Some(conn);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        let mut guard = self.conn.lock().await;
        if let Some(mut conn) = guard.take() {
            // QUIT enters the UPDATE state and commits any pending DELE.
            let _ = conn.command("QUIT").await;
        }
        Ok(())
    }

    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
        let (count, _size) = conn.stat().await?;
        Ok(vec![FolderInfo {
            path: "INBOX".to_string(),
            name: "Inbox".to_string(),
            role: FolderRole::Inbox,
            uid_validity: 1,
            uid_next: count.saturating_add(1),
            message_count: count,
            // POP3 has no server-side read state; treat the whole maildrop as
            // unread. Local read marking persists in the store between syncs.
            unread_count: count,
        }])
    }

    async fn select_folder(&mut self, _path: &str) -> Result<FolderState> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
        let (count, _size) = conn.stat().await?;
        Ok(FolderState {
            // Constant uid_validity so the sync engine never forces a full
            // re-fetch (POP3 has no UIDVALIDITY concept).
            uid_validity: 1,
            uid_next: count.saturating_add(1),
            exists: count,
            unseen: count,
        })
    }

    async fn fetch_envelopes(
        &mut self,
        _folder: &str,
        uid_range: UidRange,
    ) -> Result<Vec<EnvelopeFetch>> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
        let (count, _size) = conn.stat().await?;
        let seqs = Self::seqs_for(&uid_range, count);

        let mut out = Vec::with_capacity(seqs.len());
        for seq in seqs {
            let size = match conn.command(&format!("LIST {seq}")).await {
                Ok(status) => status
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0),
                Err(_) => 0,
            };

            // `TOP n 0` returns the headers only. Fall back to a full RETR if
            // the server does not implement the (optional) TOP command.
            let header_bytes = match conn.command_multiline(&format!("TOP {seq} 0")).await {
                Ok(b) => b,
                Err(_) => conn.command_multiline(&format!("RETR {seq}")).await?,
            };

            let headers = parse_headers_from_bytes(&header_bytes)?;
            let has_attachments = header_has_attachments(&header_bytes);
            let internal_date = headers.date;

            out.push(EnvelopeFetch {
                uid: seq,
                headers,
                flags: Vec::new(),
                size,
                internal_date,
                snippet: String::new(),
                has_attachments,
            });
        }
        Ok(out)
    }

    async fn fetch_body(&mut self, _folder: &str, uid: u32) -> Result<MessageBody> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
        let bytes = conn.command_multiline(&format!("RETR {uid}")).await?;
        parse_full_body(&bytes)
    }

    async fn set_flags(
        &mut self,
        _folder: &str,
        _uid: u32,
        _add: &[imt_core::Flag],
        _remove: &[imt_core::Flag],
    ) -> Result<()> {
        // POP3 has no server-side flags; read/flag state is kept locally only.
        Ok(())
    }

    async fn append(
        &mut self,
        _folder: &str,
        _raw_rfc822: &[u8],
        _flags: &[imt_core::Flag],
    ) -> Result<u32> {
        Err(NetError::other("POP3 does not support APPEND"))
    }

    async fn move_uid(
        &mut self,
        _folder: &str,
        _uid: u32,
        _dest_folder: &str,
    ) -> Result<()> {
        Err(NetError::other("POP3 has no folders; move is unsupported"))
    }

    async fn expunge_folder(&mut self, _folder: &str) -> Result<()> {
        {
            let mut guard = self.conn.lock().await;
            let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
            let (count, _size) = conn.stat().await?;
            for seq in 1..=count {
                let _ = conn.command(&format!("DELE {seq}")).await;
            }
        }
        // DELE only commits on QUIT; cycle the connection to apply and resume.
        self.disconnect().await?;
        self.connect().await?;
        Ok(())
    }

    async fn delete_uid(&mut self, _folder: &str, uid: u32) -> Result<()> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
        // Marked for deletion; committed by the QUIT issued on disconnect.
        conn.command(&format!("DELE {uid}")).await?;
        Ok(())
    }

    async fn idle(&mut self, _folder: &str) -> Result<IdleHandle> {
        // POP3 has no IDLE: poll STAT periodically and surface an Exists event
        // when the maildrop count changes. The same shared connection is used,
        // so this keeps it warm between explicit syncs.
        let last_count = {
            let mut guard = self.conn.lock().await;
            let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
            conn.stat().await?.0
        };
        Ok(IdleHandle {
            inner: Box::new(Pop3Idle {
                conn: Arc::clone(&self.conn),
                last_count,
            }),
        })
    }
}

/// Polling stand-in for IDLE on POP3.
struct Pop3Idle {
    conn: Arc<Mutex<Option<Pop3Conn>>>,
    last_count: u32,
}

#[async_trait]
impl IdleHandleImpl for Pop3Idle {
    async fn next_event(&mut self) -> Result<IdleEvent> {
        loop {
            sleep(POLL_INTERVAL).await;
            let count = {
                let mut guard = self.conn.lock().await;
                let conn = guard.as_mut().ok_or_else(|| NetError::other("not connected"))?;
                conn.stat().await?.0
            };
            if count != self.last_count {
                self.last_count = count;
                return Ok(IdleEvent::Exists(count));
            }
        }
    }

    async fn terminate(self: Box<Self>) -> Result<()> {
        // The connection lives in the backend; nothing to reclaim here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_eol_strips_crlf_and_lf() {
        assert_eq!(trim_eol(b"hi\r\n"), b"hi");
        assert_eq!(trim_eol(b"hi\n"), b"hi");
        assert_eq!(trim_eol(b"hi"), b"hi");
        assert_eq!(trim_eol(b""), b"");
    }

    #[test]
    fn seqs_for_resolves_and_clamps_ranges() {
        assert_eq!(Pop3Backend::seqs_for(&UidRange::All, 3), vec![1, 2, 3]);
        assert_eq!(Pop3Backend::seqs_for(&UidRange::All, 0), Vec::<u32>::new());
        // Range clamps the upper bound to the message count.
        assert_eq!(Pop3Backend::seqs_for(&UidRange::Range(2, 10), 4), vec![2, 3, 4]);
        // A start past the end yields nothing.
        assert_eq!(Pop3Backend::seqs_for(&UidRange::Range(9, 12), 4), Vec::<u32>::new());
        // Set drops out-of-range sequence numbers.
        assert_eq!(Pop3Backend::seqs_for(&UidRange::Set(vec![1, 5, 3]), 3), vec![1, 3]);
    }
}
