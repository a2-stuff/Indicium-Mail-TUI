//! IMAP backend implementation built on `async-imap` and `async-native-tls`.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_imap::imap_proto::{MailboxDatum, Response as ImapResponse};
use async_imap::Authenticator;
use async_imap::extensions::idle::{Handle as ImapIdleHandle, IdleResponse};
use async_imap::types::{Fetch, Flag as ImapFlag, Name, NameAttribute};
use async_imap::{Client, Session};
use async_native_tls::TlsStream;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use futures::TryStreamExt;
use mail_parser::{Address as MpAddress, HeaderValue, MessageParser, MimeHeaders, PartType};
use tokio::io::{AsyncRead as TokioAsyncRead, AsyncWrite as TokioAsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{interval, timeout};

use crate::oauth::xoauth2_sasl;

use imt_core::{
    Account, Address, Attachment, AuthMethod, Flag, FolderRole, MessageBody, MessageHeaders, Tls,
};

use crate::backend::{
    EnvelopeFetch, FolderInfo, FolderState, IdleEvent, IdleHandle, IdleHandleImpl, MailBackend,
    UidRange,
};
use crate::error::{NetError, Result};

/// Provides the password for a given username on demand.
pub type PasswordProvider = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// IMAP transport stream, either implicitly TLS-wrapped or plain TCP.
enum ImapStream {
    Tls(TlsStream<TcpStream>),
    Plain(TcpStream),
}

impl TokioAsyncRead for ImapStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
            ImapStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl TokioAsyncWrite for ImapStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
            ImapStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_flush(cx),
            ImapStream::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ImapStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
            ImapStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

impl std::fmt::Debug for ImapStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImapStream")
    }
}

type ImapSession = Session<ImapStream>;

/// IMAP backend tied to a single account.
pub struct ImapBackend {
    account: Account,
    password_provider: PasswordProvider,
    session: Arc<Mutex<SessionSlot>>,
    has_idle: Arc<Mutex<Option<bool>>>,
    has_move: Arc<Mutex<Option<bool>>>,
}

/// Holds either an owned session, an active IDLE handle, or nothing.
enum SessionSlot {
    /// Not connected.
    Empty,
    /// Owned session ready for commands.
    Owned(Box<ImapSession>),
    /// Session has been moved into an IDLE handle elsewhere.
    InIdle,
}

impl SessionSlot {
    fn take_owned(&mut self) -> Result<Box<ImapSession>> {
        match std::mem::replace(self, SessionSlot::Empty) {
            SessionSlot::Owned(s) => Ok(s),
            SessionSlot::Empty => {
                *self = SessionSlot::Empty;
                Err(NetError::other("not connected"))
            }
            SessionSlot::InIdle => {
                *self = SessionSlot::InIdle;
                Err(NetError::other("session is busy in IDLE"))
            }
        }
    }

    fn as_mut_owned(&mut self) -> Result<&mut ImapSession> {
        match self {
            SessionSlot::Owned(s) => Ok(s.as_mut()),
            SessionSlot::Empty => Err(NetError::other("not connected")),
            SessionSlot::InIdle => Err(NetError::other("session is busy in IDLE")),
        }
    }
}

/// Pluggable XOAUTH2 SASL authenticator. Yields the raw (un-base64-encoded)
/// IR bytes; async-imap base64-encodes them on the wire.
struct XOAuth2Authenticator {
    raw: Vec<u8>,
    used: bool,
}

impl Authenticator for &mut XOAuth2Authenticator {
    type Response = Vec<u8>;
    fn process(&mut self, _challenge: &[u8]) -> <Self as Authenticator>::Response {
        if !self.used {
            self.used = true;
            self.raw.clone()
        } else {
            // Server sent a follow-up challenge containing an error JSON; respond empty to abort.
            Vec::new()
        }
    }
}

impl ImapBackend {
    /// Construct a new backend; no network activity happens until `connect`.
    pub fn new(account: Account, password_provider: PasswordProvider) -> Self {
        Self {
            account,
            password_provider,
            session: Arc::new(Mutex::new(SessionSlot::Empty)),
            has_idle: Arc::new(Mutex::new(None)),
            has_move: Arc::new(Mutex::new(None)),
        }
    }

    async fn open_session(&self) -> Result<ImapSession> {
        let host = self.account.imap.host.clone();
        let port = self.account.imap.port;
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|e| NetError::Connect(format!("tcp connect {}:{}: {}", host, port, e)))?;

        let stream = match self.account.imap.tls {
            Tls::Implicit => {
                let tls = async_native_tls::connect(host.as_str(), tcp)
                    .await
                    .map_err(|e| NetError::Tls(e.to_string()))?;
                ImapStream::Tls(tls)
            }
            Tls::None => ImapStream::Plain(tcp),
            Tls::StartTls => {
                let plain = ImapStream::Plain(tcp);
                let mut client = Client::new(plain);
                let _ = client
                    .read_response()
                    .await
                    .ok_or_else(|| NetError::Protocol("missing greeting".into()))?
                    .map_err(|e| NetError::Protocol(e.to_string()))?;
                client
                    .run_command_and_check_ok("STARTTLS", None)
                    .await
                    .map_err(|e| NetError::Protocol(format!("STARTTLS: {}", e)))?;
                let inner = client.into_inner();
                let raw_tcp = match inner {
                    ImapStream::Plain(t) => t,
                    ImapStream::Tls(_) => unreachable!("starttls began on plain stream"),
                };
                let tls = async_native_tls::connect(host.as_str(), raw_tcp)
                    .await
                    .map_err(|e| NetError::Tls(e.to_string()))?;
                let stream = ImapStream::Tls(tls);
                let client = Client::new(stream);
                return self.authenticate(&mut Some(client)).await;
            }
        };

        let mut client = Client::new(stream);
        let _ = client
            .read_response()
            .await
            .ok_or_else(|| NetError::Protocol("missing greeting".into()))?
            .map_err(|e| NetError::Protocol(e.to_string()))?;

        let mut slot = Some(client);
        self.authenticate(&mut slot).await
    }

    async fn authenticate(&self, slot: &mut Option<Client<ImapStream>>) -> Result<ImapSession> {
        let client = slot.take().ok_or_else(|| NetError::other("no client"))?;
        match &self.account.imap.auth {
            AuthMethod::Password { username } => {
                let password = (self.password_provider)(username).ok_or_else(|| {
                    NetError::Auth(format!("no password available for {}", username))
                })?;
                client
                    .login(username, password)
                    .await
                    .map_err(|(e, _)| NetError::Auth(e.to_string()))
            }
            AuthMethod::OAuth2 { username, .. } => {
                let access_token = (self.password_provider)(username).ok_or_else(|| {
                    NetError::Auth(format!("no oauth access token available for {}", username))
                })?;
                // Build the raw SASL initial response. async-imap will base64-encode
                // whatever we return from `process`, so we pass the unencoded bytes
                // and ignore the public `xoauth2_sasl` (which produces the wire
                // base64 form for callers that need it directly).
                let raw = format!(
                    "user={}\x01auth=Bearer {}\x01\x01",
                    username, access_token
                )
                .into_bytes();
                let _ = xoauth2_sasl; // silence unused import in this scope
                let mut auth = XOAuth2Authenticator { raw, used: false };
                client
                    .authenticate("XOAUTH2", &mut auth)
                    .await
                    .map_err(|(e, _)| NetError::Auth(e.to_string()))
            }
        }
    }

}

fn map_role(path: &str, attrs: &[NameAttribute<'_>]) -> FolderRole {
    for a in attrs {
        match a {
            NameAttribute::Sent => return FolderRole::Sent,
            NameAttribute::Drafts => return FolderRole::Drafts,
            NameAttribute::Trash => return FolderRole::Trash,
            NameAttribute::Junk => return FolderRole::Junk,
            NameAttribute::Archive => return FolderRole::Archive,
            _ => {}
        }
    }
    if path.eq_ignore_ascii_case("INBOX") {
        FolderRole::Inbox
    } else {
        FolderRole::Other
    }
}

fn folder_display_name(path: &str, delimiter: Option<&str>) -> String {
    if let Some(d) = delimiter {
        if !d.is_empty() {
            if let Some(idx) = path.rfind(d) {
                return path[idx + d.len()..].to_string();
            }
        }
    }
    path.to_string()
}

fn cow_to_string(c: std::borrow::Cow<'_, str>) -> String {
    c.into_owned()
}

fn convert_address_list(addr: Option<&MpAddress<'_>>) -> Vec<Address> {
    let Some(addr) = addr else {
        return Vec::new();
    };
    match addr {
        MpAddress::List(addrs) => addrs
            .iter()
            .filter_map(|a| {
                a.address.as_ref().map(|email| Address {
                    name: a.name.as_ref().map(|n| n.to_string()),
                    email: email.to_string(),
                })
            })
            .collect(),
        MpAddress::Group(groups) => groups
            .iter()
            .flat_map(|g| g.addresses.iter())
            .filter_map(|a| {
                a.address.as_ref().map(|email| Address {
                    name: a.name.as_ref().map(|n| n.to_string()),
                    email: email.to_string(),
                })
            })
            .collect(),
    }
}

fn extract_message_id_list(value: &HeaderValue<'_>) -> Vec<String> {
    match value {
        HeaderValue::Text(t) => vec![t.to_string()],
        HeaderValue::TextList(list) => list.iter().map(|s| s.to_string()).collect(),
        _ => Vec::new(),
    }
}

fn extract_in_reply_to(value: &HeaderValue<'_>) -> Option<String> {
    match value {
        HeaderValue::Text(t) => Some(t.to_string()),
        HeaderValue::TextList(list) => list.first().map(|s| s.to_string()),
        _ => None,
    }
}

fn parse_headers_from_bytes(bytes: &[u8]) -> Result<MessageHeaders> {
    let parsed = MessageParser::default()
        .parse(bytes)
        .ok_or_else(|| NetError::Parse("could not parse RFC 822 headers".into()))?;

    let date = parsed
        .date()
        .and_then(|dt| Utc.timestamp_opt(dt.to_timestamp(), 0).single())
        .unwrap_or_else(Utc::now);

    Ok(MessageHeaders {
        rfc_message_id: parsed.message_id().map(|s| s.to_string()),
        in_reply_to: extract_in_reply_to(parsed.in_reply_to()),
        references: extract_message_id_list(parsed.references()),
        from: convert_address_list(parsed.from()),
        to: convert_address_list(parsed.to()),
        cc: convert_address_list(parsed.cc()),
        bcc: convert_address_list(parsed.bcc()),
        reply_to: convert_address_list(parsed.reply_to()),
        subject: parsed.subject().unwrap_or("").to_string(),
        date,
    })
}

fn convert_flag(f: &ImapFlag<'_>) -> Option<Flag> {
    match f {
        ImapFlag::Seen => Some(Flag::Seen),
        ImapFlag::Answered => Some(Flag::Answered),
        ImapFlag::Flagged => Some(Flag::Flagged),
        ImapFlag::Deleted => Some(Flag::Deleted),
        ImapFlag::Draft => Some(Flag::Draft),
        ImapFlag::Recent => Some(Flag::Recent),
        ImapFlag::MayCreate => None,
        ImapFlag::Custom(s) => Some(Flag::Custom(s.to_string())),
    }
}

fn flags_to_imap_list(flags: &[Flag]) -> String {
    let inner: Vec<String> = flags.iter().map(|f| f.as_imap_str().to_string()).collect();
    format!("({})", inner.join(" "))
}

fn fetch_to_envelope(f: &Fetch) -> Result<EnvelopeFetch> {
    let uid = f
        .uid
        .ok_or_else(|| NetError::Protocol("fetch missing UID".into()))?;
    let header_bytes = f
        .header()
        .ok_or_else(|| NetError::Protocol("fetch missing header".into()))?;
    let headers = parse_headers_from_bytes(header_bytes)?;

    let flags: Vec<Flag> = f.flags().filter_map(|fl| convert_flag(&fl)).collect();
    let size = f.size.unwrap_or(0) as u64;
    let internal_date: DateTime<Utc> = f
        .internal_date()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    Ok(EnvelopeFetch {
        uid,
        headers,
        flags,
        size,
        internal_date,
        snippet: String::new(),
    })
}

fn save_attachment_temp(filename: &str, data: &[u8]) -> Option<std::path::PathBuf> {
    let tmp = std::env::temp_dir().join("imt-attachments");
    std::fs::create_dir_all(&tmp).ok()?;
    let safe_name = filename
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    let name = if safe_name.is_empty() { "attachment".to_string() } else { safe_name };
    // Use a short random suffix to avoid collisions.
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    let path = tmp.join(format!("{}_{}", id, name));
    std::fs::write(&path, data).ok()?;
    Some(path)
}

fn parse_full_body(bytes: &[u8]) -> Result<MessageBody> {
    let parsed = MessageParser::default()
        .parse(bytes)
        .ok_or_else(|| NetError::Parse("could not parse RFC 822 body".into()))?;

    let text_plain = parsed.body_text(0).map(|c| cow_to_string(c));
    let text_html = parsed.body_html(0).map(|c| cow_to_string(c));

    let mut attachments = Vec::new();
    for (idx, part) in parsed.parts.iter().enumerate() {
        let is_attachment = matches!(
            part.body,
            PartType::Binary(_) | PartType::InlineBinary(_)
        );
        if !is_attachment {
            continue;
        }
        let mime = part
            .content_type()
            .map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.ctype(), sub),
                None => ct.ctype().to_string(),
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let filename = part
            .attachment_name()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let raw_bytes: &[u8] = match &part.body {
            PartType::Binary(b) => b.as_ref(),
            PartType::InlineBinary(b) => b.as_ref(),
            _ => &[],
        };
        let size = raw_bytes.len() as u64;
        let inline = matches!(part.body, PartType::InlineBinary(_));
        // Save all attachments to temp so they can be viewed or saved later.
        let temp_path = save_attachment_temp(&filename, raw_bytes);
        attachments.push(Attachment {
            filename,
            mime_type: mime,
            size,
            part_id: idx.to_string(),
            content_id: part.content_id().map(|s| s.to_string()),
            inline,
            temp_path,
        });
    }

    Ok(MessageBody {
        text_plain,
        text_html,
        attachments,
    })
}

#[async_trait]
impl MailBackend for ImapBackend {
    async fn connect(&mut self) -> Result<()> {
        let mut session = self.open_session().await?;
        // Probe IDLE capability once at connect time so later `idle()` calls can
        // pick the push or polling implementation without an extra round-trip.
        let (has_idle, has_move) = match session.capabilities().await {
            Ok(caps) => (caps.has_str("IDLE"), caps.has_str("MOVE")),
            Err(e) => {
                tracing::debug!(error = %e, "CAPABILITY probe failed; assuming no IDLE/MOVE");
                (false, false)
            }
        };
        *self.has_idle.lock().await = Some(has_idle);
        *self.has_move.lock().await = Some(has_move);
        *self.session.lock().await = SessionSlot::Owned(Box::new(session));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        let mut guard = self.session.lock().await;
        if let SessionSlot::Owned(mut s) = std::mem::replace(&mut *guard, SessionSlot::Empty) {
            let _ = s.logout().await;
        }
        Ok(())
    }

    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut_owned()?;

        let stream = session
            .list(Some(""), Some("*"))
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        let names: Vec<Name> = stream
            .try_collect()
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;

        let mut out = Vec::with_capacity(names.len());
        for n in &names {
            let path = n.name().to_string();
            let role = map_role(&path, n.attributes());
            let display = folder_display_name(&path, n.delimiter());

            let status_res = session
                .status(&path, "(MESSAGES UNSEEN UIDVALIDITY UIDNEXT)")
                .await;
            let (uid_validity, uid_next, message_count, unread_count) = match status_res {
                Ok(mb) => (
                    mb.uid_validity.unwrap_or(0),
                    mb.uid_next.unwrap_or(0),
                    mb.exists,
                    mb.unseen.unwrap_or(0),
                ),
                Err(_) => (0, 0, 0, 0),
            };

            out.push(FolderInfo {
                path,
                name: display,
                role,
                uid_validity,
                uid_next,
                message_count,
                unread_count,
            });
        }
        Ok(out)
    }

    async fn select_folder(&mut self, path: &str) -> Result<FolderState> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut_owned()?;
        let mb = session
            .select(path)
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        Ok(FolderState {
            uid_validity: mb.uid_validity.unwrap_or(0),
            uid_next: mb.uid_next.unwrap_or(0),
            exists: mb.exists,
            unseen: mb.unseen.unwrap_or(0),
        })
    }

    async fn fetch_envelopes(
        &mut self,
        folder: &str,
        uid_range: UidRange,
    ) -> Result<Vec<EnvelopeFetch>> {
        let set = uid_range.to_imap_set();
        if set.is_empty() {
            return Ok(Vec::new());
        }

        let mut guard = self.session.lock().await;
        let session = guard.as_mut_owned()?;

        session
            .select(folder)
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;

        let stream = session
            .uid_fetch(
                set,
                "(UID FLAGS RFC822.SIZE INTERNALDATE BODY.PEEK[HEADER])",
            )
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        let fetches: Vec<Fetch> = stream
            .try_collect()
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;

        let mut out = Vec::with_capacity(fetches.len());
        for f in &fetches {
            out.push(fetch_to_envelope(f)?);
        }
        Ok(out)
    }

    async fn fetch_body(&mut self, folder: &str, uid: u32) -> Result<MessageBody> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut_owned()?;

        session
            .select(folder)
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;

        let stream = session
            .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        let fetches: Vec<Fetch> = stream
            .try_collect()
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;

        let f = fetches
            .first()
            .ok_or_else(|| NetError::Protocol(format!("uid {} not found", uid)))?;
        let body_bytes = f
            .body()
            .ok_or_else(|| NetError::Protocol("fetch missing body".into()))?;
        parse_full_body(body_bytes)
    }

    async fn set_flags(
        &mut self,
        folder: &str,
        uid: u32,
        add: &[Flag],
        remove: &[Flag],
    ) -> Result<()> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut_owned()?;
        session
            .select(folder)
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;

        if !add.is_empty() {
            let q = format!("+FLAGS.SILENT {}", flags_to_imap_list(add));
            let stream = session
                .uid_store(uid.to_string(), q)
                .await
                .map_err(|e| NetError::Protocol(e.to_string()))?;
            let _: Vec<_> = stream
                .try_collect()
                .await
                .map_err(|e| NetError::Protocol(e.to_string()))?;
        }
        if !remove.is_empty() {
            let q = format!("-FLAGS.SILENT {}", flags_to_imap_list(remove));
            let stream = session
                .uid_store(uid.to_string(), q)
                .await
                .map_err(|e| NetError::Protocol(e.to_string()))?;
            let _: Vec<_> = stream
                .try_collect()
                .await
                .map_err(|e| NetError::Protocol(e.to_string()))?;
        }
        Ok(())
    }

    async fn append(
        &mut self,
        folder: &str,
        raw_rfc822: &[u8],
        flags: &[Flag],
    ) -> Result<u32> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut_owned()?;
        let flag_str = if flags.is_empty() {
            None
        } else {
            Some(flags_to_imap_list(flags))
        };
        session
            .append(folder, flag_str.as_deref(), None, raw_rfc822)
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        // async-imap 0.10's `append` does not surface the OK response, so we
        // cannot extract APPENDUID without reimplementing the command. Return 0
        // and let the caller treat that as "unknown" and resync via UIDNEXT.
        tracing::debug!(folder, "APPEND ok; APPENDUID not extracted in this build");
        Ok(0)
    }

    async fn move_uid(
        &mut self,
        folder: &str,
        uid: u32,
        dest_folder: &str,
    ) -> Result<()> {
        let supports_move = self.has_move.lock().await.unwrap_or(false);
        let mut guard = self.session.lock().await;
        let session = guard.as_mut_owned()?;
        session
            .select(folder)
            .await
            .map_err(|e| NetError::Protocol(format!("select {}: {}", folder, e)))?;

        if supports_move {
            session
                .uid_mv(uid.to_string(), dest_folder)
                .await
                .map_err(|e| NetError::Protocol(format!("uid move: {}", e)))?;
        } else {
            session
                .uid_copy(uid.to_string(), dest_folder)
                .await
                .map_err(|e| NetError::Protocol(format!("uid copy: {}", e)))?;
            let stream = session
                .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)")
                .await
                .map_err(|e| NetError::Protocol(format!("store deleted: {}", e)))?;
            let _: Vec<_> = stream
                .try_collect()
                .await
                .map_err(|e| NetError::Protocol(format!("store drain: {}", e)))?;
            let _stream = session
                .expunge()
                .await
                .map_err(|e| NetError::Protocol(format!("expunge: {}", e)))?;
        }
        Ok(())
    }

    async fn idle(&mut self, folder: &str) -> Result<IdleHandle> {
        // SELECT first so the IDLE applies to the right mailbox.
        {
            let mut guard = self.session.lock().await;
            let session = guard.as_mut_owned()?;
            session
                .select(folder)
                .await
                .map_err(|e| NetError::Protocol(e.to_string()))?;
        }

        let supports_idle = self.has_idle.lock().await.unwrap_or(false);

        if supports_idle {
            // Move the session into an IDLE handle. Mark the slot as InIdle so
            // any concurrent commands return a clear error.
            let session_box = {
                let mut guard = self.session.lock().await;
                let owned = guard.take_owned()?;
                *guard = SessionSlot::InIdle;
                owned
            };
            while session_box.unsolicited_responses.try_recv().is_ok() {}
            let session = *session_box;
            let mut handle = session.idle();
            if let Err(e) = handle.init().await {
                let mut guard = self.session.lock().await;
                *guard = SessionSlot::Empty;
                drop(handle);
                return Err(NetError::Protocol(format!("IDLE init: {}", e)));
            }
            let push = PushIdle {
                handle: Some(handle),
                session_slot: Arc::clone(&self.session),
                last_idle_started: tokio::time::Instant::now(),
            };
            tracing::debug!(folder, "IDLE push session started");
            Ok(IdleHandle {
                inner: Box::new(push),
            })
        } else {
            tracing::debug!(folder, "IDLE not advertised; using STATUS polling");
            let inner = PollingIdle {
                session: Arc::clone(&self.session),
                folder: folder.to_string(),
                ticker: interval(Duration::from_secs(30)),
                last_exists: 0,
                primed: false,
            };
            Ok(IdleHandle {
                inner: Box::new(inner),
            })
        }
    }
}

/// Push-based IDLE handle that owns the session for the duration of the IDLE.
struct PushIdle {
    handle: Option<ImapIdleHandle<ImapStream>>,
    session_slot: Arc<Mutex<SessionSlot>>,
    last_idle_started: tokio::time::Instant,
}

impl PushIdle {
    /// Renew IDLE every 28 minutes per RFC 2177 to avoid being logged off.
    async fn maybe_renew(&mut self) -> Result<()> {
        const RENEW_AFTER: Duration = Duration::from_secs(28 * 60);
        if self.last_idle_started.elapsed() < RENEW_AFTER {
            return Ok(());
        }
        let h = self
            .handle
            .take()
            .ok_or_else(|| NetError::other("idle handle gone"))?;
        let session = h
            .done()
            .await
            .map_err(|e| NetError::Protocol(format!("IDLE done for renew: {}", e)))?;
        while session.unsolicited_responses.try_recv().is_ok() {}
        let mut new_handle = session.idle();
        new_handle
            .init()
            .await
            .map_err(|e| NetError::Protocol(format!("IDLE re-init: {}", e)))?;
        self.handle = Some(new_handle);
        self.last_idle_started = tokio::time::Instant::now();
        Ok(())
    }
}

#[async_trait]
impl IdleHandleImpl for PushIdle {
    async fn next_event(&mut self) -> Result<IdleEvent> {
        const KEEPALIVE: Duration = Duration::from_secs(60);
        loop {
            self.maybe_renew().await?;
            let handle = self
                .handle
                .as_mut()
                .ok_or_else(|| NetError::other("idle handle gone"))?;

            let (fut, _stop) = handle.wait_with_timeout(KEEPALIVE);
            match fut.await {
                Ok(IdleResponse::NewData(data)) => {
                    let ev = match data.parsed() {
                        ImapResponse::MailboxData(MailboxDatum::Exists(n)) => {
                            Some(IdleEvent::Exists(*n))
                        }
                        ImapResponse::Expunge(n) => Some(IdleEvent::Expunge(*n)),
                        ImapResponse::Fetch(seq, _) => Some(IdleEvent::Flags(*seq)),
                        _ => None,
                    };
                    if let Some(ev) = ev {
                        return Ok(ev);
                    }
                    continue;
                }
                Ok(IdleResponse::ManualInterrupt) => continue,
                Ok(IdleResponse::Timeout) => continue,
                Err(e) => {
                    return Err(NetError::Protocol(format!("IDLE wait: {}", e)));
                }
            }
        }
    }

    async fn terminate(mut self: Box<Self>) -> Result<()> {
        if let Some(h) = self.handle.take() {
            match timeout(Duration::from_secs(10), h.done()).await {
                Ok(Ok(session)) => {
                    let mut guard = self.session_slot.lock().await;
                    *guard = SessionSlot::Owned(Box::new(session));
                }
                Ok(Err(e)) => {
                    let mut guard = self.session_slot.lock().await;
                    *guard = SessionSlot::Empty;
                    return Err(NetError::Protocol(format!("IDLE done: {}", e)));
                }
                Err(_) => {
                    let mut guard = self.session_slot.lock().await;
                    *guard = SessionSlot::Empty;
                    return Err(NetError::Protocol("IDLE done timed out".into()));
                }
            }
        }
        Ok(())
    }
}

/// Polling fallback used when the server does not advertise IDLE.
struct PollingIdle {
    session: Arc<Mutex<SessionSlot>>,
    folder: String,
    ticker: tokio::time::Interval,
    last_exists: u32,
    primed: bool,
}

#[async_trait]
impl IdleHandleImpl for PollingIdle {
    async fn next_event(&mut self) -> Result<IdleEvent> {
        loop {
            self.ticker.tick().await;
            let mut guard = self.session.lock().await;
            let session = guard.as_mut_owned()?;
            let mb = session
                .status(&self.folder, "(MESSAGES)")
                .await
                .map_err(|e| NetError::Protocol(e.to_string()))?;
            if !self.primed {
                self.last_exists = mb.exists;
                self.primed = true;
                continue;
            }
            if mb.exists != self.last_exists {
                self.last_exists = mb.exists;
                return Ok(IdleEvent::Exists(mb.exists));
            }
        }
    }

    async fn terminate(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}

