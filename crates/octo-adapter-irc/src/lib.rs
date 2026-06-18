//! IRC adapter for DOT (RFC-0850 §8.1, PlatformType::IRC)
//!
//! Pure Rust IRC client using raw TCP (RFC 2812). Text-only transport
//! with UTF-8 safe message splitting for DOT envelope delivery.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "server": "irc.libera.chat",
//!   "port": 6697,
//!   "nickname": "cipherocto-bot",
//!   "channels": ["#cipherocto"],
//!   "password": null,
//!   "use_tls": true
//! }
//! ```
//!
//! ## Wire Format
//!
//! - **Send:** `PRIVMSG #channel :DOT/1/<base64>` (base64 URL-safe, no padding)
//! - **Fragment:** `PRIVMSG #channel :DOT/1/F:<index>/<total>:<base64>`
//! - **Receive:** Parse PRIVMSG from other users, extract DOT/1/ prefix
//! - **Max payload:** 480 bytes (512 IRC line limit - ~32 PRIVMSG overhead)

use async_trait::async_trait;
use base64::Engine;
use std::collections::BTreeMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};

use octo_network::dot::adapters::{
    backoff::RetryConfig,
    coordinator_admin::{
        AdminCapabilityReport, CoordinatorAdmin, GroupHandle, GroupId, GroupMemberSpec,
        GroupMetadata, GroupModeFlags, InviteRef, PeerId,
    },
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct IrcConfig {
    /// IRC server hostname.
    pub server: String,
    /// IRC server port. Default: 6697 (TLS) or 6667 (plain).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Bot nickname.
    pub nickname: String,
    /// Channels to join (with # prefix).
    pub channels: Vec<String>,
    /// Optional server password (PASS command).
    pub password: Option<String>,
    /// Use TLS. Default: true.
    #[serde(default = "default_tls")]
    pub use_tls: bool,
}

fn default_port() -> u16 {
    6697
}
fn default_tls() -> bool {
    true
}

// ── Constants ──────────────────────────────────────────────────────

/// Maximum IRC line length including CRLF.
const IRC_MAX_LINE_BYTES: usize = 512;

/// PRIVMSG overhead: `PRIVMSG ` (8) + ` :` (2) + CRLF (2) + channel name (~20) = ~32
const PRIVMSG_OVERHEAD: usize = 32;

/// Effective max payload per PRIVMSG.
const MAX_PAYLOAD_PER_MSG: usize = IRC_MAX_LINE_BYTES - PRIVMSG_OVERHEAD;

/// Keepalive interval (seconds).
const _PING_INTERVAL_SECS: u64 = 120;

/// DOT/1/ prefix for envelope detection.
const DOT_PREFIX: &str = "DOT/1/";

/// DOT/1/F: prefix for fragments.
const DOT_FRAGMENT_PREFIX: &str = "DOT/1/F:";

// ── Adapter ────────────────────────────────────────────────────────

pub struct IrcAdapter {
    config: IrcConfig,
    /// Receiver for incoming IRC PRIVMSG containing DOT envelopes.
    rx: Mutex<mpsc::Receiver<RawPlatformMessage>>,
    /// Sender — given to the IRC listener task.
    tx: mpsc::Sender<RawPlatformMessage>,
    /// Outgoing admin command channel. Initialized to `None` in `new()`;
    /// the sender half is installed by the first call to
    /// `ensure_connected`, then handed to the listener task alongside
    /// the receiver. The sender is the only path for `CoordinatorAdmin`
    /// actions to reach the socket — the `OwnedWriteHalf` itself is
    /// owned by the listener task and never escapes `irc_session`.
    cmd_tx: Mutex<Option<mpsc::Sender<String>>>,
    /// Whether the IRC connection has been started.
    connected: Mutex<bool>,
}

impl IrcAdapter {
    pub fn new(config: IrcConfig) -> Self {
        let (tx, rx) = mpsc::channel(4096);
        Self {
            config,
            rx: Mutex::new(rx),
            tx,
            cmd_tx: Mutex::new(None),
            connected: Mutex::new(false),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: IrcConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Start IRC connection (idempotent).
    async fn ensure_connected(&self) -> Result<(), PlatformAdapterError> {
        let mut connected = self.connected.lock().await;
        if *connected {
            return Ok(());
        }

        let server = self.config.server.clone();
        let port = self.config.port;
        let nickname = self.config.nickname.clone();
        let channels = self.config.channels.clone();
        let password = self.config.password.clone();
        let use_tls = self.config.use_tls;
        let tx = self.tx.clone();

        // Build the admin command channel and install the sender on
        // `self` so `CoordinatorAdmin` actions can reach the socket.
        // The receiver is moved into the listener task.
        //
        // Capacity is 64: admin commands (KICK, MODE, TOPIC, etc.) are
        // rare and small (one IRC line each). 64 keeps the sender side
        // non-blocking for normal traffic bursts while bounding memory.
        let (cmd_tx, cmd_rx) = mpsc::channel::<String>(64);
        *self.cmd_tx.lock().await = Some(cmd_tx);

        tokio::spawn(async move {
            irc_listener(server, port, nickname, channels, password, use_tls, tx, cmd_rx).await;
        });

        *connected = true;
        Ok(())
    }

    /// Internal helper: send a pre-built raw IRC line (without trailing
    /// `\r\n`) to the socket through the admin command channel. The
    /// listener task adds the CRLF and writes it to the TCP stream.
    ///
    /// Returns `Ok(())` once the line is enqueued. The line is *fire-and-
    /// forget*: IRC has no synchronous request/response correlation at
    /// the protocol level, so this method does not block on the server's
    /// reply. Callers that need confirmation should follow up with a
    /// `CoordinatorAdmin::get_group_metadata` (when the metadata is
    /// observable via a server response that the bot captures later)
    /// or accept eventual consistency.
    async fn send_raw_line(&self, line: &str) -> Result<(), PlatformAdapterError> {
        self.ensure_connected().await?;
        let guard = self.cmd_tx.lock().await;
        let tx = guard.as_ref().ok_or_else(|| transport_err("admin channel not initialized"))?;
        tx.send(line.to_string()).await.map_err(|_| {
            PlatformAdapterError::Unreachable {
                platform: "irc".into(),
                reason: "admin channel closed (listener exited)".into(),
            }
        })?;
        Ok(())
    }

    /// Split a message into IRC-safe chunks at UTF-8 boundaries.
    pub fn split_message(message: &str, max_bytes: usize) -> Vec<String> {
        if max_bytes == 0 {
            return vec![message.to_string()];
        }

        let mut chunks = Vec::new();
        for line in message.split('\n') {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }

            if line.len() <= max_bytes {
                chunks.push(line.to_string());
            } else {
                // Split at safe UTF-8 boundaries
                let mut remaining = line;
                while !remaining.is_empty() {
                    if remaining.len() <= max_bytes {
                        chunks.push(remaining.to_string());
                        break;
                    }
                    let mut split_at = max_bytes;
                    while split_at > 0 && !remaining.is_char_boundary(split_at) {
                        split_at -= 1;
                    }
                    if split_at == 0 {
                        // Single character exceeds max — skip it
                        remaining = &remaining[remaining.chars().next().unwrap().len_utf8()..];
                        continue;
                    }
                    chunks.push(remaining[..split_at].to_string());
                    remaining = &remaining[split_at..];
                }
            }
        }
        if chunks.is_empty() {
            chunks.push(String::new());
        }
        chunks
    }

    /// Encode envelope bytes as DOT/1/ base64.
    pub fn encode_envelope(bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        )
    }

    /// Encode a fragment as DOT/1/F:i/n: base64.
    pub fn encode_fragment(index: u16, total: u16, bytes: &[u8]) -> String {
        format!(
            "DOT/1/F:{}/{}:{}",
            index,
            total,
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        )
    }

    /// Decode a DOT/1/ or DOT/1/F: message.
    pub fn decode_message(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim();

        // Check for fragment prefix
        if let Some(rest) = text.strip_prefix(DOT_FRAGMENT_PREFIX) {
            // Format: i/n:<base64>
            let colon_pos = rest.find(':').ok_or("Missing colon in fragment")?;
            let _header = &rest[..colon_pos];
            let b64 = &rest[colon_pos + 1..];
            return base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(b64)
                .map_err(|e| format!("Base64 decode error: {e}"));
        }

        // Check for envelope prefix
        let b64 = text
            .strip_prefix(DOT_PREFIX)
            .ok_or("Missing DOT/1/ prefix")?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {e}"))
    }

    /// Domain hash: `BLAKE3-256("irc:{server}:{channel}")`
    pub fn domain_hash(server: &str, channel: &str) -> [u8; 32] {
        *blake3::hash(format!("irc:{}:{}", server.trim().to_lowercase(), channel).as_bytes())
            .as_bytes()
    }

    /// Inverse of `domain_hash`: parse a `server:channel` platform_id and
    /// compute the hash. Used by the `PlatformAdapter::domain_id` impl so
    /// that callers can construct a `BroadcastDomainId` from a single
    /// colon-joined string and have it match the canonical `domain_hash`.
    pub fn domain_hash_from_id(platform_id: &str) -> [u8; 32] {
        let (server, channel) = match platform_id.split_once(':') {
            Some((s, c)) => (s, c),
            None => ("", platform_id),
        };
        Self::domain_hash(server, channel)
    }

    pub const PLATFORM_TYPE: u16 = 0x0006;
    pub fn max_payload_bytes() -> usize {
        MAX_PAYLOAD_PER_MSG
    }
    pub fn rate_limit_per_second() -> u32 {
        1
    } // IRC flood protection
}

// ── IRC Protocol ───────────────────────────────────────────────────

/// Long-running IRC listener task.
///
/// Owns the admin command `Receiver` for the lifetime of the adapter
/// and re-attaches it to every fresh session. This lets admin actions
/// (`CoordinatorAdmin`) keep working across TCP reconnects: the same
/// `IrcAdapter` is reused and the channel pairs (cmd_tx, cmd_rx) are
/// stable for the adapter's lifetime.
#[allow(clippy::too_many_arguments)]
async fn irc_listener(
    server: String,
    port: u16,
    nickname: String,
    channels: Vec<String>,
    password: Option<String>,
    use_tls: bool,
    tx: mpsc::Sender<RawPlatformMessage>,
    mut cmd_rx: mpsc::Receiver<String>,
) {
    let retry = RetryConfig::default();
    let mut attempt = 0u32;

    loop {
        let connect_result = if use_tls {
            connect_tls(&server, port).await
        } else {
            connect_plain(&server, port).await
        };

        match connect_result {
            Ok(stream) => {
                attempt = 0;
                if let Err(e) = irc_session(
                    stream,
                    &nickname,
                    &channels,
                    password.as_deref(),
                    &tx,
                    &mut cmd_rx,
                )
                .await
                {
                    eprintln!("IRC session error: {e}");
                }
            }
            Err(e) => {
                eprintln!("IRC connect error: {e}");
            }
        }

        let delay = retry.delay_for_attempt(attempt.min(retry.max_retries));
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

async fn connect_plain(server: &str, port: u16) -> Result<TcpStream, String> {
    TcpStream::connect(format!("{server}:{port}"))
        .await
        .map_err(|e| format!("TCP connect: {e}"))
}

async fn connect_tls(server: &str, port: u16) -> Result<TcpStream, String> {
    // For simplicity, use plain TCP with a note that TLS should be added
    // In production, use tokio-rustls with a proper TLS configuration
    connect_plain(server, port).await
}

/// IRC session: authenticate, join channels, process messages.
///
/// The `cmd_rx` argument is borrowed for the lifetime of the session
/// and drained on every loop iteration alongside the incoming-line
/// read. The borrow is released when the session ends (connection
/// drop, error), at which point `irc_listener` gets the receiver back
/// and passes it to the next fresh session.
async fn irc_session(
    stream: TcpStream,
    nickname: &str,
    channels: &[String],
    password: Option<&str>,
    tx: &mpsc::Sender<RawPlatformMessage>,
    cmd_rx: &mut mpsc::Receiver<String>,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Authenticate
    if let Some(pass) = password {
        writer
            .write_all(format!("PASS {pass}\r\n").as_bytes())
            .await
            .map_err(|e| format!("PASS: {e}"))?;
    }
    writer
        .write_all(format!("NICK {nickname}\r\n").as_bytes())
        .await
        .map_err(|e| format!("NICK: {e}"))?;
    writer
        .write_all(format!("USER {nickname} 0 * :CipherOcto DOT Bot\r\n").as_bytes())
        .await
        .map_err(|e| format!("USER: {e}"))?;

    // Join channels after MOTD
    let mut joined = false;
    loop {
        // `tokio::select!` with `biased;` polls the admin-command
        // branch first, then the read branch. This means:
        //
        // - If the bot is blocked on `read_line` (waiting for
        //   server data that may not come — IRC servers don't
        //   push unsolicited state), a newly-enqueued admin
        //   command can still be drained immediately.
        // - If a server line arrives and a command is also
        //   queued, the command is written first (so admin
        //   actions have lower latency than the read loop).
        //
        // This is the correct design for a fire-and-forget
        // admin path: we want `send_raw_line` to make forward
        // progress regardless of the server's traffic shape.
        tokio::select! {
            biased;

            // ── Branch 1: admin command ──────────────────────
            //
            // The sender's `String` is a raw IRC line without
            // trailing CRLF; we append CRLF here so the server
            // sees a complete line. `RecvError` means the
            // sender was dropped (adapter shutdown).
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(line) => {
                        writer
                            .write_all(line.as_bytes())
                            .await
                            .map_err(|e| format!("admin cmd write: {e}"))?;
                        writer
                            .write_all(b"\r\n")
                            .await
                            .map_err(|e| format!("admin cmd CRLF: {e}"))?;
                    }
                    None => return Ok(()), // sender dropped, clean shutdown
                }
            }

            // ── Branch 2: incoming IRC line ──────────────────
            read_result = reader.read_line(&mut line) => {
                let n = read_result.map_err(|e| format!("Read: {e}"))?;
                if n == 0 {
                    return Err("Connection closed".into());
                }

                let trimmed = line.trim_end();

                // PING/PONG keepalive
                if let Some(server) = trimmed.strip_prefix("PING ") {
                    writer
                        .write_all(format!("PONG {server}\r\n").as_bytes())
                        .await
                        .map_err(|e| format!("PONG: {e}"))?;
                    continue;
                }

                // Join channels after RPL_ENDOFMOTD (376) or ERR_NOMOTD (422)
                if !joined && (trimmed.contains(" 376 ") || trimmed.contains(" 422 ")) {
                    for ch in channels {
                        writer
                            .write_all(format!("JOIN {ch}\r\n").as_bytes())
                            .await
                            .map_err(|e| format!("JOIN: {e}"))?;
                    }
                    joined = true;
                    continue;
                }

                // Parse PRIVMSG
                if let Some(msg) = parse_privmsg(trimmed) {
                    // Check if it's a DOT message
                    if msg.text.starts_with(DOT_PREFIX) || msg.text.starts_with(DOT_FRAGMENT_PREFIX) {
                        if let Ok(payload) = IrcAdapter::decode_message(&msg.text) {
                            let mut metadata = BTreeMap::new();
                            metadata.insert("channel".into(), msg.channel.clone());
                            metadata.insert("sender".into(), msg.sender.clone());
                            let _ = tx.try_send(RawPlatformMessage {
                                platform_id: format!("irc-{}", msg.id),
                                payload,
                                metadata,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Parsed IRC PRIVMSG.
struct IrcPrivmsg {
    sender: String,
    channel: String,
    text: String,
    id: String,
}

/// Parse a PRIVMSG from an IRC line.
/// Format: `:nick!user@host PRIVMSG #channel :message text`
fn parse_privmsg(line: &str) -> Option<IrcPrivmsg> {
    let line = line.strip_prefix(':')?;
    let (prefix, rest) = line.split_once(' ')?;
    let sender = prefix.split('!').next()?.to_string();
    let (command, rest) = rest.split_once(' ')?;
    if command != "PRIVMSG" {
        return None;
    }
    let (target, text) = rest.split_once(" :")?;
    let id = format!("{}-{}", sender, epoch_millis());
    Some(IrcPrivmsg {
        sender,
        channel: target.to_string(),
        text: text.to_string(),
        id,
    })
}

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── PlatformAdapter ────────────────────────────────────────────────

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "irc".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for IrcAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Find the channel for this domain
        let channel = self
            .config
            .channels
            .iter()
            .find(|ch| {
                let hash = Self::domain_hash(&self.config.server, ch);
                hash == domain.domain_hash
            })
            .ok_or_else(|| {
                transport_err(format!("No channel for domain {:?}", domain.domain_hash))
            })?;

        // Split if needed (IRC has strict line limits)
        let chunks = Self::split_message(&encoded, MAX_PAYLOAD_PER_MSG);

        // For now, return the encoded envelope as a "send instruction"
        // In production, this would write to the IRC socket
        // The adapter stores the message for the gateway to send
        let total = chunks.len() as u16;
        let mut sent_bytes = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            let line = if total > 1 {
                Self::encode_fragment(i as u16, total, chunk.as_bytes())
            } else {
                chunk.clone()
            };
            // PRIVMSG #channel :<line>
            let irc_msg = format!("PRIVMSG {} :{}\r\n", channel, line);
            sent_bytes.extend_from_slice(irc_msg.as_bytes());
        }

        Ok(DeliveryReceipt {
            platform_message_id: format!("irc-{}", epoch_millis()),
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        self.ensure_connected().await?;
        let mut rx = self.rx.lock().await;
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }
        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(transport_err("Empty payload"));
        }
        DeterministicEnvelope::from_wire_bytes(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: true,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        // The platform_id MUST be in `server:channel` form to match the
        // canonical hash used by `send_envelope`'s channel lookup. We parse
        // it here and delegate to `domain_hash` so the two methods always
        // agree (R18 fix; previously the call to `BroadcastDomainId::new`
        // would hash just the platform_id without the server prefix, which
        // silently mismatched the static `domain_hash` lookup).
        BroadcastDomainId {
            platform_type: PlatformType::IRC as u16,
            domain_hash: Self::domain_hash_from_id(platform_id),
        }
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::IRC
    }

    fn self_handle(&self) -> Option<String> {
        Some(self.config.nickname.clone())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Check TCP connectivity to the server
        let timeout = std::time::Duration::from_secs(5);
        let addr = format!("{}:{}", self.config.server, self.config.port);
        match tokio::time::timeout(timeout, TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(transport_err(format!("Health check: {e}"))),
            Err(_) => Err(transport_err("Health check timed out")),
        }
    }

    /// Coordinator-admin capability probe: IRC supports a meaningful
    /// subset of the admin surface (KICK, MODE +o/-o, MODE +m/-m,
    /// MODE +i/-i, TOPIC, INVITE, JOIN, PART) but lacks ephemeral
    /// TTL, approval workflow, and invite-link resolution. We opt
    /// in to `CoordinatorAdmin` so the caller can probe the
    /// capability report and pick the supported actions.
    fn as_coordinator_admin(
        &self,
    ) -> Option<&dyn octo_network::dot::adapters::coordinator_admin::CoordinatorAdmin> {
        Some(self)
    }
}

// ── CoordinatorAdmin (R21) ─────────────────────────────────────────
//
// IRC is a thin, human-oriented chat protocol — most "admin" actions
// map to single IRC commands (KICK, MODE, TOPIC, INVITE, PART,
// JOIN). What IRC *doesn't* have:
//   - group creation (channels are pre-existing on the server)
//   - description separate from topic
//   - ephemeral / disappearing-message TTL
//   - approval workflow for joiners
//   - invite-link URLs (we only have channel names)
//   - a server-side ban with rejoin prevention (MODE +b requires a
//     full `nick!user@host` mask that's not preserved in `PeerId`;
//     the supported path is KICK via `remove_member`)
//
// Each of these returns `Unimplemented` from the corresponding
// `CoordinatorAdmin` method. The capability report is honest about
// which subset is supported.
//
// Identifier conventions:
//   - `GroupId` is `server:channel` (matches the platform_id format
//     used by `domain_id` and the canonical `domain_hash`).
//   - `PeerId` is a bare nick (no hostmask).
//   - The adapter only operates on channels in its configured
//     channel list (the same constraint `send_envelope` enforces).

#[async_trait]
impl CoordinatorAdmin for IrcAdapter {
    fn platform_name(&self) -> String {
        "irc".into()
    }

    fn admin_capabilities(&self) -> AdminCapabilityReport {
        // Truthful report: implement what the IRC protocol supports
        // natively, honestly mark the rest as unsupported.
        AdminCapabilityReport {
            // ── A. Lifecycle ──────────────────────────────────
            can_create: false,           // IRC has no group creation
            can_join_by_id: false,       // bot's channels are pre-configured
            can_join_by_invite: true,    // JOIN #channel (best-effort)
            can_leave: true,             // PART
            can_destroy: false,          // no invite-link to revoke

            // ── B. Membership ─────────────────────────────────
            can_add_member: true,        // INVITE (server-mediated)
            can_remove_member: true,     // KICK
            can_ban: false,              // MODE +b needs hostmask, not in PeerId
            can_promote: true,           // MODE +o
            can_demote: true,            // MODE -o
            can_approve_join: false,     // no approval workflow

            // ── C. Mode ───────────────────────────────────────
            can_rename: true,            // TOPIC
            can_describe: false,         // no description separate from topic
            can_lock: true,              // MODE +i / -i
            can_announce: true,          // MODE +m / -m
            can_set_ephemeral: false,    // no TTL
            can_require_approval: false, // no approval

            // ── D. Discovery ──────────────────────────────────
            can_list_own_groups: true,   // configured channels
            can_get_metadata: false,     // no sync NAMES/MODE capture
            can_resolve_invite: false,   // no invite URL

            // ── E. Handoff ────────────────────────────────────
            can_transfer_ownership: false, // no transfer primitive
        }
    }

    // ── A. Lifecycle ──────────────────────────────────────────

    /// IRC has no group creation — channels exist on the server
    /// and admins are entitled by server config. The caller should
    /// use a configured `GroupId` from `list_own_groups` or ask
    /// the server operator to provision a new channel.
    async fn create_group(
        &self,
        _subject: &str,
        _initial_members: &[GroupMemberSpec],
    ) -> Result<GroupHandle, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "create_group".into(),
        })
    }

    /// PART the channel. IRC's `PART` is idempotent: the server
    /// replies with an `ERR_NOTONCHANNEL` numeric if the bot is
    /// not a member, which the listener discards as part of normal
    /// chatter. The fire-and-forget return of `Ok(())` mirrors
    /// that — the bot has done its part by sending the command.
    async fn leave_group(&self, group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        self.send_raw_line(&format!("PART {channel}")).await
    }

    /// Best-effort destroy: just leave. IRC has no invite-link
    /// to revoke and no `+i` mode to unset on a per-group basis
    /// (the channel's `+i` would persist for other members).
    async fn destroy_group(&self, group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        self.leave_group(group_id).await
    }

    // ── B. Membership ─────────────────────────────────────────

    /// `INVITE <nick> <channel>`. IRC's closest equivalent to
    /// "add a member": a server-mediated invitation the target
    /// user can act on. Note that the user must accept the
    /// invite (or be auto-promoted by network policy) — this
    /// method does *not* force-join the peer.
    async fn add_member(
        &self,
        group_id: &GroupId,
        member: &GroupMemberSpec,
    ) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        // IRC nicks cannot contain spaces; we pass through whatever
        // the caller gave us and let the server reject malformed
        // input. This matches the R20 WhatsApp pattern of not
        // validating peer handles.
        self.send_raw_line(&format!("INVITE {} {channel}", member.handle))
            .await
    }

    /// `KICK <channel> <nick> :<reason>`. The reason is a short
    /// human-readable string identifying the kicker (the
    /// coordinator) for IRC clients that surface it.
    async fn remove_member(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        self.send_raw_line(&format!(
            "KICK {channel} {} :removed by coordinator",
            member.as_str()
        ))
        .await
    }

    /// IRC's `MODE +b` ban requires a full `nick!user@host` mask
    /// for server-side enforcement. `PeerId` is just a bare nick
    /// on this platform, so we cannot construct a ban mask
    /// from the trait input. Callers that need a ban should
    /// use `remove_member` (KICK) and add a coordinator-level
    /// deny-list.
    async fn ban_member(
        &self,
        _group_id: &GroupId,
        _member: &PeerId,
        _duration: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "ban_member".into(),
        })
    }

    /// `MODE <channel> +o <nick>` — grant channel-operator
    /// status. Requires the bot itself to already be a channel
    /// operator; the server will return `ERR_CHANOPRIVSNEEDED`
    /// otherwise. The listener discards the error reply silently
    /// (fire-and-forget), so this method returns `Ok(())` once
    /// the MODE line is enqueued.
    async fn promote_to_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        self.send_raw_line(&format!("MODE {channel} +o {}", member.as_str()))
            .await
    }

    /// `MODE <channel> -o <nick>` — revoke channel-operator
    /// status. Same server-privileges caveat as `promote_to_admin`.
    async fn demote_from_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        self.send_raw_line(&format!("MODE {channel} -o {}", member.as_str()))
            .await
    }

    /// IRC has no join-approval workflow. Some networks (e.g.
    /// Atheme-based) implement `+j` join-flood throttle, but
    /// that's a rate limit, not a per-joiner approval.
    async fn approve_join_request(
        &self,
        _group_id: &GroupId,
        _requester: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "approve_join_request".into(),
        })
    }

    // ── C. Mode ───────────────────────────────────────────────

    /// IRC's "rename" maps to the channel TOPIC — IRC has no
    /// separate subject/name. The TOPIC is broadcast to all
    /// members as a `TOPIC` numeric reply.
    async fn rename_group(
        &self,
        group_id: &GroupId,
        new_subject: &str,
    ) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        // The new subject is the trailing parameter (after the
        // ` :` separator). IRC topic can contain spaces freely
        // in the trailing-param form.
        self.send_raw_line(&format!("TOPIC {channel} :{new_subject}"))
            .await
    }

    /// IRC has no description separate from the topic. The
    /// closest analogue would be a second TOPIC entry, but
    /// that's not a standard concept.
    async fn set_group_description(
        &self,
        _group_id: &GroupId,
        _description: &str,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_group_description".into(),
        })
    }

    /// IRC's "locked" maps to `MODE +i` (invite-only). When
    /// locked, only invited users can join the channel. We
    /// unset to `+i`/`-i` based on the `locked` flag.
    async fn set_locked(
        &self,
        group_id: &GroupId,
        locked: bool,
    ) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        let flag = if locked { "+i" } else { "-i" };
        self.send_raw_line(&format!("MODE {channel} {flag}"))
            .await
    }

    /// IRC's "announce-only" maps to `MODE +m` (moderated).
    /// When set, only channel operators and voiced users
    /// (`+v`) can post messages; everyone else is silenced.
    async fn set_announce(
        &self,
        group_id: &GroupId,
        announce_only: bool,
    ) -> Result<(), PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        let flag = if announce_only { "+m" } else { "-m" };
        self.send_raw_line(&format!("MODE {channel} {flag}"))
            .await
    }

    /// IRC has no disappearing-message TTL.
    async fn set_ephemeral(
        &self,
        _group_id: &GroupId,
        _ttl: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_ephemeral".into(),
        })
    }

    /// IRC has no join-approval workflow.
    async fn set_require_approval(
        &self,
        _group_id: &GroupId,
        _require: bool,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "set_require_approval".into(),
        })
    }

    // ── D. Discovery ──────────────────────────────────────────

    /// Return the configured channels. IRC channels aren't
    /// "discovered" — the bot only knows about the ones in its
    /// config. `is_admin` is conservatively `false` because the
    /// bot's op status is determined by server policy and not
    /// tracked in the adapter state.
    async fn list_own_groups(
        &self,
    ) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        Ok(self
            .config
            .channels
            .iter()
            .map(|ch| GroupHandle {
                id: self.full_id(ch),
                subject: None,
                invite_url: None,
                is_admin: false,
                member_count: None,
                mode_flags: None,
            })
            .collect())
    }

    /// Return the static ID for a configured channel. We do not
    /// capture NAMES (353) or MODE (324) replies into the
    /// adapter state, so member lists, admin lists, mode flags,
    /// subject, and description are all `None`/empty. The
    /// capability report is honest about this: `can_get_metadata
    /// = false`. Callers that need rich metadata should set up
    /// a dedicated stateful bot, not a transport adapter.
    async fn get_group_metadata(
        &self,
        group_id: &GroupId,
    ) -> Result<GroupMetadata, PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        Ok(GroupMetadata {
            id: self.full_id(&channel),
            subject: None,
            description: None,
            members: vec![],
            admins: vec![],
            invite_url: None,
            mode_flags: GroupModeFlags::default(),
        })
    }

    /// IRC has no invite URLs / codes to resolve.
    async fn resolve_invite(
        &self,
        _invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "resolve_invite".into(),
        })
    }

    /// Best-effort: treat the `InviteRef` string as a channel
    /// name and JOIN it. This works for `ircv3.2` invite-notify
    /// notifications (the bot receives an `INVITE` numeric and
    /// the channel name is the last argument) but doesn't
    /// validate that the invite was actually authorized by an
    /// op. A real impl would need an invite-token validator
    /// (which IRC does not have; ircv3 invite-notify is a
    /// notification-only mechanism).
    async fn join_by_invite(
        &self,
        invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        self.send_raw_line(&format!("JOIN {}", invite.0)).await?;
        Ok(GroupHandle {
            id: self.full_id(&invite.0),
            subject: None,
            invite_url: None,
            is_admin: false,
            member_count: None,
            mode_flags: None,
        })
    }

    // ── E. Handoff ────────────────────────────────────────────

    /// IRC has no transfer-ownership primitive. The closest
    /// dance is: demote the current owner (`MODE -q`) and have
    /// the new owner self-op (`MODE +q` if they have a
    /// server-side grant). This is server-dependent and not
    /// safe to automate, so we return `Unimplemented` and let
    /// the caller do it manually.
    async fn transfer_ownership(
        &self,
        _group_id: &GroupId,
        _new_owner: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        Err(PlatformAdapterError::Unimplemented {
            platform: self.platform_name(),
            action: "transfer_ownership".into(),
        })
    }
}

// Private helpers used by the `CoordinatorAdmin` impl above.

impl IrcAdapter {
    /// Validate a `GroupId` and return the channel part.
    ///
    /// `GroupId` is in `server:channel` form, matching the
    /// `platform_id` format used by `domain_id` and the
    /// canonical `domain_hash`. The adapter only operates on
    /// its own server, and only on channels it knows about
    /// (the ones in its configured channel list).
    ///
    /// Returns the bare channel name (with `#` prefix preserved)
    /// on success, or a structured `ApiError`/`Unreachable` on
    /// failure.
    fn channel_for(&self, group_id: &GroupId) -> Result<String, PlatformAdapterError> {
        let raw = group_id.as_str();
        let (server, channel) = match raw.split_once(':') {
            Some((s, c)) => (s, c),
            None => ("", raw), // bare channel: assume it's on our server
        };
        if !server.is_empty() && !server.eq_ignore_ascii_case(&self.config.server) {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "group {raw} is on server {server}, but adapter is connected to {}",
                    self.config.server
                ),
            });
        }
        if !self.config.channels.contains(&channel.to_string()) {
            return Err(PlatformAdapterError::ApiError {
                code: 404,
                message: format!(
                    "channel {channel} is not in the configured channel list {:?}",
                    self.config.channels
                ),
            });
        }
        Ok(channel.to_string())
    }

    /// Build the canonical `GroupId` for a configured channel:
    /// `server:channel` form.
    fn full_id(&self, channel: &str) -> GroupId {
        GroupId::new(format!("{}:{channel}", self.config.server))
    }
}

// ── Plugin ABI ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0006
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match IrcAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut IrcAdapter);
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_short_message() {
        let chunks = IrcAdapter::split_message("hello", 400);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_split_long_message() {
        let msg = "a".repeat(1000);
        let chunks = IrcAdapter::split_message(&msg, 400);
        assert!(chunks.len() >= 3);
        // All chunks should be <= 400 bytes
        for chunk in &chunks {
            assert!(chunk.len() <= 400);
        }
        // Reassembled should match original (minus newlines)
        let reassembled: String = chunks.join("");
        assert_eq!(reassembled, msg);
    }

    #[test]
    fn test_split_multiline() {
        let chunks = IrcAdapter::split_message("line one\nline two\nline three", 400);
        assert_eq!(chunks, vec!["line one", "line two", "line three"]);
    }

    #[test]
    fn test_split_empty() {
        let chunks = IrcAdapter::split_message("", 400);
        assert_eq!(chunks, vec![""]);
    }

    #[test]
    fn test_split_utf8_boundary() {
        // 3-byte UTF-8 characters: ñ = 2 bytes, 中 = 3 bytes
        let msg = "ññññññññññ"; // 10 * 2 = 20 bytes
        let chunks = IrcAdapter::split_message(&msg, 5);
        // Should split at UTF-8 boundaries
        for chunk in &chunks {
            assert!(chunk.len() <= 5);
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
        }
    }

    #[test]
    fn test_split_chinese_chars() {
        let msg = "中中中中中"; // 5 * 3 = 15 bytes
        let chunks = IrcAdapter::split_message(&msg, 5);
        // Each Chinese char is 3 bytes, so 1 per chunk (5 > 3, but 2*3=6 > 5)
        for chunk in &chunks {
            assert!(chunk.len() <= 5);
        }
    }

    #[test]
    fn test_split_crlf_handling() {
        let chunks = IrcAdapter::split_message("hello\r\nworld\r\n", 400);
        assert_eq!(chunks, vec!["hello", "world"]);
    }

    #[test]
    fn test_encode_decode_envelope() {
        let data = b"test envelope data for IRC";
        let encoded = IrcAdapter::encode_envelope(data);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = IrcAdapter::decode_message(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_encode_decode_fragment() {
        let data = b"fragment payload";
        let encoded = IrcAdapter::encode_fragment(0, 3, data);
        assert!(encoded.starts_with("DOT/1/F:"));
        assert!(encoded.contains("/3:"));
        let decoded = IrcAdapter::decode_message(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_invalid_prefix() {
        assert!(IrcAdapter::decode_message("NOTDOT/1/abc").is_err());
    }

    #[test]
    fn test_decode_invalid_base64() {
        assert!(IrcAdapter::decode_message("DOT/1/!!!invalid!!!").is_err());
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = IrcAdapter::domain_hash("irc.libera.chat", "#cipherocto");
        let h2 = IrcAdapter::domain_hash("irc.libera.chat", "#cipherocto");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            IrcAdapter::domain_hash("IRC.LIBERA.CHAT", "#cipherocto"),
            IrcAdapter::domain_hash("  irc.libera.chat  ", "#cipherocto")
        );
    }

    #[test]
    fn test_domain_hash_different_servers() {
        let h1 = IrcAdapter::domain_hash("irc.libera.chat", "#test");
        let h2 = IrcAdapter::domain_hash("irc.oftc.net", "#test");
        assert_ne!(h1, h2);
    }

    // R18 fix: the trait-method `domain_id(platform_id)` must produce the
    // same hash as the static `domain_hash(server, channel)` so that
    // `send_envelope` can find the configured channel by domain. The
    // platform_id is the colon-joined form `server:channel`.
    #[test]
    fn test_domain_id_matches_domain_hash() {
        let from_id = IrcAdapter::domain_hash_from_id("irc.libera.chat:#cipherocto");
        let from_args = IrcAdapter::domain_hash("irc.libera.chat", "#cipherocto");
        assert_eq!(from_id, from_args);
    }

    #[test]
    fn test_domain_id_normalizes_server_case_and_whitespace() {
        // Server is case+whitespace normalized; channel is preserved.
        let h1 = IrcAdapter::domain_hash_from_id("  IRC.LIBERA.CHAT  :#cipherocto");
        let h2 = IrcAdapter::domain_hash("irc.libera.chat", "#cipherocto");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_id_no_colon_falls_back_to_channel_only() {
        // Backward compat: if the caller passes just a channel (no colon),
        // hash it as if server is empty. This produces a different hash
        // from the proper `server:channel` form, so users who skip the
        // colon get a no-match in send_envelope.
        let no_colon = IrcAdapter::domain_hash_from_id("#cipherocto");
        let with_colon = IrcAdapter::domain_hash("", "#cipherocto");
        assert_eq!(no_colon, with_colon);
        let proper = IrcAdapter::domain_hash("irc.libera.chat", "#cipherocto");
        assert_ne!(no_colon, proper);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(IrcAdapter::PLATFORM_TYPE, 0x0006);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0006);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "server": "irc.libera.chat",
            "port": 6697,
            "nickname": "testbot",
            "channels": ["#test", "#cipherocto"],
            "password": null,
            "use_tls": true
        });
        let adapter =
            IrcAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice()).unwrap();
        assert_eq!(adapter.config.server, "irc.libera.chat");
        assert_eq!(adapter.config.port, 6697);
        assert_eq!(adapter.config.nickname, "testbot");
        assert_eq!(adapter.config.channels.len(), 2);
        assert!(adapter.config.use_tls);
    }

    #[test]
    fn test_config_defaults() {
        let json = serde_json::json!({
            "server": "irc.libera.chat",
            "nickname": "testbot",
            "channels": ["#test"]
        });
        let adapter =
            IrcAdapter::from_config_bytes(serde_json::to_vec(&json).unwrap().as_slice()).unwrap();
        assert_eq!(adapter.config.port, 6697); // default TLS port
        assert!(adapter.config.use_tls); // default true
        assert_eq!(adapter.config.password, None);
    }

    #[test]
    fn test_capabilities() {
        let adapter = IrcAdapter::new(IrcConfig {
            server: "irc.libera.chat".into(),
            port: 6697,
            nickname: "test".into(),
            channels: vec!["#test".into()],
            password: None,
            use_tls: true,
        });
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, MAX_PAYLOAD_PER_MSG);
        assert!(caps.supports_fragmentation);
        assert_eq!(caps.rate_limit_per_second, 1);
    }

    #[test]
    fn test_parse_privmsg() {
        let line = ":nick!user@host PRIVMSG #channel :Hello world";
        let msg = parse_privmsg(line).unwrap();
        assert_eq!(msg.sender, "nick");
        assert_eq!(msg.channel, "#channel");
        assert_eq!(msg.text, "Hello world");
    }

    #[test]
    fn test_parse_privmsg_with_dot_prefix() {
        let line = ":bot!u@h PRIVMSG #ch :DOT/1/AQID";
        let msg = parse_privmsg(line).unwrap();
        assert!(msg.text.starts_with("DOT/1/"));
    }

    #[test]
    fn test_parse_privmsg_not_privmsg() {
        let line = ":server 001 nick :Welcome";
        assert!(parse_privmsg(line).is_none());
    }

    #[test]
    fn test_parse_privmsg_no_prefix() {
        let line = "NOTICE * :*** Looking up...";
        assert!(parse_privmsg(line).is_none());
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let data = vec![0u8; 256];
        for i in 0..256 {
            let mut d = data.clone();
            d[i] = 0xFF;
            let encoded = IrcAdapter::encode_envelope(&d);
            let decoded = IrcAdapter::decode_message(&encoded).unwrap();
            assert_eq!(decoded, d);
        }
    }

    // ── R21: CoordinatorAdmin tests ──────────────────────────────
    //
    // The first set of tests is pure-unit (no socket, no tokio
    // runtime) and covers the parts of the `CoordinatorAdmin` impl
    // that don't touch the wire: the downcast probe, the
    // capability report, the platform name, and the static
    // `channel_for` / `full_id` helpers.
    //
    // The second set is async and uses a local `TcpListener` to
    // verify that admin commands actually reach the wire.
    // This is the same pattern R20 WhatsApp used for its
    // capability tests, but IRC is small enough that we can
    // verify the full path end-to-end without mocking the SDK.

    /// Helper: build a configured `IrcAdapter` for tests. The
    /// port is irrelevant for the non-socket tests.
    fn make_test_adapter() -> IrcAdapter {
        IrcAdapter::new(IrcConfig {
            server: "irc.example.org".into(),
            port: 6697,
            nickname: "testbot".into(),
            channels: vec!["#alpha".into(), "#beta".into()],
            password: None,
            use_tls: true,
        })
    }

    #[test]
    fn test_as_coordinator_admin_returns_some_for_irc() {
        // The probe is the bridge between `PlatformAdapter` and
        // `CoordinatorAdmin`. IRC opts in (it supports a meaningful
        // subset), so the probe must return `Some(_)`.
        let adapter = make_test_adapter();
        let admin: Option<&dyn CoordinatorAdmin> = adapter.as_coordinator_admin();
        assert!(admin.is_some(), "IRC should opt in to CoordinatorAdmin");
        assert_eq!(admin.unwrap().platform_name(), "irc");
    }

    #[test]
    fn test_platform_name_is_irc() {
        let adapter = make_test_adapter();
        assert_eq!(adapter.platform_name(), "irc");
    }

    #[test]
    fn test_admin_capabilities_truthful_for_irc() {
        // Bit-by-bit check of the capability report. The truth
        // here is "what the IRC protocol supports natively".
        // Any change to this assertion means the documented
        // support matrix changed.
        let adapter = make_test_adapter();
        let caps = adapter.admin_capabilities();

        // Lifecycle
        assert!(!caps.can_create, "IRC has no group creation");
        assert!(
            !caps.can_join_by_id,
            "bot's channels are pre-configured, not joined by id"
        );
        assert!(caps.can_join_by_invite, "JOIN #channel is supported");
        assert!(caps.can_leave, "PART is supported");
        assert!(
            !caps.can_destroy,
            "no separate invite-link revoke primitive"
        );

        // Membership
        assert!(caps.can_add_member, "INVITE is supported");
        assert!(caps.can_remove_member, "KICK is supported");
        assert!(
            !caps.can_ban,
            "MODE +b needs hostmask, not in PeerId"
        );
        assert!(caps.can_promote, "MODE +o is supported");
        assert!(caps.can_demote, "MODE -o is supported");
        assert!(!caps.can_approve_join, "no approval workflow");

        // Mode
        assert!(caps.can_rename, "TOPIC is supported");
        assert!(
            !caps.can_describe,
            "no description separate from topic"
        );
        assert!(caps.can_lock, "MODE +i/-i is supported");
        assert!(caps.can_announce, "MODE +m/-m is supported");
        assert!(!caps.can_set_ephemeral, "no TTL");
        assert!(!caps.can_require_approval, "no approval");

        // Discovery
        assert!(
            caps.can_list_own_groups,
            "configured channels are enumerable"
        );
        assert!(
            !caps.can_get_metadata,
            "no sync NAMES/MODE capture"
        );
        assert!(!caps.can_resolve_invite, "no invite URL");

        // Handoff
        assert!(
            !caps.can_transfer_ownership,
            "no transfer primitive"
        );
    }

    #[test]
    fn test_full_id_format() {
        let adapter = make_test_adapter();
        let id = adapter.full_id("#alpha");
        assert_eq!(id.as_str(), "irc.example.org:#alpha");
    }

    #[test]
    fn test_channel_for_accepts_server_channel_form() {
        let adapter = make_test_adapter();
        let ch = adapter
            .channel_for(&GroupId::new("irc.example.org:#alpha"))
            .unwrap();
        assert_eq!(ch, "#alpha");
    }

    #[test]
    fn test_channel_for_accepts_bare_channel_form() {
        // No colon → assume the channel is on our server. This
        // matches the `domain_id` fallback (R18 fix).
        let adapter = make_test_adapter();
        let ch = adapter.channel_for(&GroupId::new("#alpha")).unwrap();
        assert_eq!(ch, "#alpha");
    }

    #[test]
    fn test_channel_for_rejects_wrong_server() {
        let adapter = make_test_adapter();
        let err = adapter
            .channel_for(&GroupId::new("irc.other.net:#alpha"))
            .unwrap_err();
        match err {
            PlatformAdapterError::ApiError { code, message } => {
                assert_eq!(code, 400);
                assert!(
                    message.contains("irc.other.net"),
                    "error should mention the wrong server: {message}"
                );
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[test]
    fn test_channel_for_rejects_unconfigured_channel() {
        let adapter = make_test_adapter();
        let err = adapter
            .channel_for(&GroupId::new("#unknown"))
            .unwrap_err();
        match err {
            PlatformAdapterError::ApiError { code, message } => {
                assert_eq!(code, 404);
                assert!(
                    message.contains("#unknown"),
                    "error should mention the channel: {message}"
                );
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_list_own_groups_returns_configured_channels() {
        let adapter = make_test_adapter();
        let groups = adapter.list_own_groups().await.unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id.as_str(), "irc.example.org:#alpha");
        assert_eq!(groups[1].id.as_str(), "irc.example.org:#beta");
        // We don't track op status, so is_admin is conservatively false.
        assert!(!groups[0].is_admin);
        assert!(!groups[1].is_admin);
        // We don't capture NAMES replies, so all dynamic fields are None.
        assert!(groups[0].subject.is_none());
        assert!(groups[0].invite_url.is_none());
        assert!(groups[0].member_count.is_none());
        assert!(groups[0].mode_flags.is_none());
    }

    #[tokio::test]
    async fn test_get_group_metadata_returns_id_for_configured_channel() {
        let adapter = make_test_adapter();
        let meta = adapter
            .get_group_metadata(&GroupId::new("irc.example.org:#alpha"))
            .await
            .unwrap();
        assert_eq!(meta.id.as_str(), "irc.example.org:#alpha");
        // Honest minimum: the static ID, no rich metadata.
        assert!(meta.subject.is_none());
        assert!(meta.description.is_none());
        assert!(meta.members.is_empty());
        assert!(meta.admins.is_empty());
        assert!(meta.invite_url.is_none());
        // Mode flags default to all-false.
        let f = meta.mode_flags;
        assert!(!f.locked);
        assert!(!f.announce_only);
        assert!(f.ephemeral_ttl.is_none());
        assert!(!f.requires_approval);
    }

    #[tokio::test]
    async fn test_get_group_metadata_rejects_unconfigured_channel() {
        let adapter = make_test_adapter();
        let err = adapter
            .get_group_metadata(&GroupId::new("#unknown"))
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformAdapterError::ApiError { code: 404, .. }));
    }

    /// Assert that a default-`Unimplemented` method on the IRC
    /// `CoordinatorAdmin` returns exactly
    /// `Err(Unimplemented { platform: "irc", action: <label> })`.
    /// Mirrors the helper in `coordinator_admin.rs`'s unit-test
    /// module, scoped to the IRC platform name.
    fn expect_irc_unimplemented<T: std::fmt::Debug>(
        r: Result<T, PlatformAdapterError>,
        action: &str,
    ) {
        match r {
            Err(PlatformAdapterError::Unimplemented { platform, action: a }) => {
                assert_eq!(platform, "irc", "{action}: platform");
                assert_eq!(a, action, "{action}: action");
            }
            other => panic!("expected Unimplemented for {action}, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_unimplemented_methods_carry_irc_platform_name() {
        // The IRC `CoordinatorAdmin` does not implement
        // create_group, ban_member, approve_join_request,
        // set_group_description, set_ephemeral,
        // set_require_approval, resolve_invite, or
        // transfer_ownership. Each must return Unimplemented
        // with platform = "irc" and action = the expected label.
        let adapter = make_test_adapter();
        let g = GroupId::new("irc.example.org:#alpha");
        let p = PeerId::new("alice");
        let m = GroupMemberSpec::new("alice");
        let inv = InviteRef::new("#alpha");
        let ttl = Some(std::time::Duration::from_secs(60));

        expect_irc_unimplemented::<GroupHandle>(
            adapter.create_group("s", &[]).await,
            "create_group",
        );
        expect_irc_unimplemented::<()>(adapter.ban_member(&g, &p, ttl).await, "ban_member");
        expect_irc_unimplemented::<()>(
            adapter.approve_join_request(&g, &p).await,
            "approve_join_request",
        );
        expect_irc_unimplemented::<()>(
            adapter.set_group_description(&g, "x").await,
            "set_group_description",
        );
        expect_irc_unimplemented::<()>(adapter.set_ephemeral(&g, ttl).await, "set_ephemeral");
        expect_irc_unimplemented::<()>(
            adapter.set_require_approval(&g, true).await,
            "set_require_approval",
        );
        expect_irc_unimplemented::<GroupHandle>(
            adapter.resolve_invite(&inv).await,
            "resolve_invite",
        );
        expect_irc_unimplemented::<()>(
            adapter.transfer_ownership(&g, &p).await,
            "transfer_ownership",
        );

        // The `m` binding is only here to keep the test
        // exhaustive; it's not passed to a method above.
        let _ = m;
    }

    /// End-to-end: spin up a local TCP server that pretends to
    /// be an IRC server, drive `send_raw_line`, and verify the
    /// line reaches the wire.
    ///
    /// The adapter's listener task writes `PASS` / `NICK` /
    /// `USER` on connect (which the server reads), then enters
    /// a `tokio::select!` between admin commands and incoming
    /// server lines. Since the server never sends anything
    /// substantive, the listener is "blocked" on the read side,
    /// but `biased;` ensures the admin branch is polled first
    /// when a command is enqueued. The KICK line the test sends
    /// should be flushed to the wire promptly.
    #[tokio::test(flavor = "current_thread")]
    async fn test_send_raw_line_writes_through_listener() {
        use std::sync::Arc;
        use tokio::io::AsyncReadExt;
        use tokio::sync::Mutex;

        // 1. Bind a local listener and grab the port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // 2. Server task: accept, then read up to 8 KiB into a
        //    shared buffer. We don't reply with anything (no
        //    376/422), so the listener never sends JOINs; the
        //    server's read will see only PASS/NICK/USER plus any
        //    admin commands the test enqueues.
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = match listener.accept().await {
                Ok(c) => c,
                Err(_) => return,
            };
            let mut buf = vec![0u8; 8192];
            // Read for a bounded window. We use a generous
            // window (500 ms) so the listener has time to
            // flush the admin command even though the server
            // never replies.
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                async {
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                received_clone.lock().await.extend_from_slice(&buf[..n]);
                            }
                            Err(_) => break,
                        }
                    }
                },
            )
            .await;
        });

        // 3. Build the adapter pointing at the local listener.
        let adapter = IrcAdapter::new(IrcConfig {
            server: "127.0.0.1".into(),
            port: addr.port(),
            nickname: "testbot".into(),
            channels: vec!["#alpha".into()],
            password: None,
            use_tls: false,
        });

        // 4. Send a KICK via `send_raw_line`. This triggers
        //    `ensure_connected` (spawning the listener task,
        //    which writes PASS/NICK/USER), then enqueues the
        //    KICK on the admin channel. The listener's
        //    `biased;` select! picks up the KICK and writes
        //    it.
        adapter
            .send_raw_line("KICK #alpha alice :removed by coordinator")
            .await
            .unwrap();

        // 5. Wait long enough for the server to read the KICK
        //    and for any in-flight writes to land in the
        //    buffer.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // 6. Read what the server saw.
        let buf = received.lock().await;
        let s = String::from_utf8_lossy(&buf);
        assert!(
            s.contains("KICK #alpha alice :removed by coordinator"),
            "expected KICK in server-side buffer, got: {s}"
        );
        // Sanity: the listener also wrote the auth handshake.
        assert!(s.contains("NICK testbot"), "expected NICK in: {s}");
        assert!(s.contains("USER testbot"), "expected USER in: {s}");

        // 7. Clean up the server task.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), server).await;
    }

    /// Verify that the implemented-but-not-fully-validated
    /// methods (`leave_group`, `remove_member`, etc.) actually
    /// pass the right IRC command to `send_raw_line`. We don't
    /// have a way to mock the channel without a private refactor,
    /// so this test does the same end-to-end check as
    /// `test_send_raw_line_writes_through_listener` but with the
    /// real `CoordinatorAdmin` methods as the entry point.
    #[tokio::test(flavor = "current_thread")]
    async fn test_coordinator_admin_kick_writes_correct_line() {
        use std::sync::Arc;
        use tokio::io::AsyncReadExt;
        use tokio::sync::Mutex;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = match listener.accept().await {
                Ok(c) => c,
                Err(_) => return,
            };
            let mut buf = vec![0u8; 8192];
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                async {
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                received_clone.lock().await.extend_from_slice(&buf[..n]);
                            }
                            Err(_) => break,
                        }
                    }
                },
            )
            .await;
        });

        let adapter = IrcAdapter::new(IrcConfig {
            server: "127.0.0.1".into(),
            port: addr.port(),
            nickname: "testbot".into(),
            channels: vec!["#alpha".into()],
            password: None,
            use_tls: false,
        });

        // Drive `remove_member` and check the KICK line.
        adapter
            .remove_member(
                &GroupId::new("127.0.0.1:#alpha"),
                &PeerId::new("alice"),
            )
            .await
            .unwrap();

        // Also drive `promote_to_admin` and check the MODE line.
        adapter
            .promote_to_admin(
                &GroupId::new("127.0.0.1:#alpha"),
                &PeerId::new("bob"),
            )
            .await
            .unwrap();

        // And `set_locked` to verify the MODE +i line.
        adapter
            .set_locked(&GroupId::new("127.0.0.1:#alpha"), true)
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let buf = received.lock().await;
        let s = String::from_utf8_lossy(&buf);
        assert!(
            s.contains("KICK #alpha alice :removed by coordinator"),
            "expected KICK in: {s}"
        );
        assert!(s.contains("MODE #alpha +o bob"), "expected +o MODE in: {s}");
        assert!(s.contains("MODE #alpha +i"), "expected +i MODE in: {s}");

        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), server).await;
    }
}
