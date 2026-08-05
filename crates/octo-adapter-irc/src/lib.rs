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
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf as TokioReadHalf,
    WriteHalf as TokioWriteHalf,
};
use tokio::net::{
    tcp::OwnedReadHalf as TcpReadHalf, tcp::OwnedWriteHalf as TcpWriteHalf, TcpStream,
};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_rustls::{client::TlsStream, TlsConnector};

use octo_network::dot::adapters::{
    backoff::RetryConfig,
    coordinator_admin::{
        AddMemberOutput, AdminCapabilityReport, CoordinatorAdmin, GroupHandle, GroupId,
        GroupMemberSpec, GroupMetadata, GroupModeFlags, InviteRef, PeerId,
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

impl IrcConfig {
    /// Pure field-shape validation (no I/O). Modeled on
    /// `WhatsAppConfig::validate` (see `octo-adapter-whatsapp/src/adapter.rs`).
    ///
    /// Checks:
    /// - `server` is non-empty and contains no whitespace or `/`
    /// - `port` is non-zero
    /// - `nickname` is non-empty and contains no whitespace
    /// - `channels` entries are non-empty
    /// - `channels` entries start with `#`, `&&`, `+`, or `!` (IRC channel
    ///   name prefixes)
    /// - `channels` entries contain no spaces, commas, colons, or NUL
    /// - `channels` entries are not the IRC "JOIN 0" special token
    pub fn validate(&self) -> std::result::Result<(), String> {
        validate_server(&self.server)?;
        if self.port == 0 {
            return Err("port must be non-zero".into());
        }
        if self.nickname.trim().is_empty() {
            return Err("nickname must not be empty".into());
        }
        if self.nickname.contains(char::is_whitespace) {
            return Err(format!(
                "nickname {:?} must not contain whitespace",
                self.nickname
            ));
        }
        for ch in &self.channels {
            validate_channel_name(ch)?;
        }
        Ok(())
    }
}

/// Validate an IRC server name (used by `IrcConfig::validate`).
///
/// Rules:
/// - Non-empty after trim
/// - No whitespace, no `/` (path separator would corrupt DNS),
///   no control characters
/// - No `..` (RFC-952 forbids empty labels; an IRC hostname made
///   of only dots is also a clear sign of a config typo like
///   `"irc.example.com.."` or just `".."`)
fn validate_server(server: &str) -> std::result::Result<(), String> {
    if server.trim().is_empty() {
        return Err("server must not be empty".into());
    }
    if server.contains(|c: char| c.is_whitespace() || c == '/' || c.is_control()) {
        return Err(format!(
            "server {server:?} must not contain whitespace, '/', or control characters"
        ));
    }
    if server.contains("..") {
        return Err(format!(
            "server {server:?} must not contain empty labels ('..')"
        ));
    }
    Ok(())
}

/// Validate an IRC channel name (used by both `IrcConfig::validate`
/// and `CoordinatorAdmin::join_by_invite`).
///
/// Rules:
/// - Non-empty
/// - Must start with `#`, `&&`, `+`, or `!` (IRC channel-name prefixes)
/// - No whitespace, commas, colons, or NUL
/// - Must not be the IRC "JOIN 0" special token (`#0`, `&&0`, `+0`, `!0`)
///   which makes the client PART all channels.
pub(crate) fn validate_channel_name(ch: &str) -> std::result::Result<(), String> {
    if ch.is_empty() {
        return Err("channel must not be empty".into());
    }
    let first = ch.chars().next().expect("non-empty checked above");
    if !matches!(first, '#' | '&' | '+' | '!') {
        return Err(format!("channel {ch:?} must start with one of #, &, +, !"));
    }
    if ch.contains(|c: char| c.is_whitespace() || c == ',' || c == ':' || c == '\0') {
        return Err(format!(
            "channel {ch:?} must not contain whitespace, commas, colons, or NUL"
        ));
    }
    // Reject IRC's "JOIN 0" special token (which leaves all channels).
    if ch == "#0" || ch == "&0" || ch == "+0" || ch == "!0" {
        return Err(format!(
            "channel {ch:?} is the IRC \"leave all\" special token"
        ));
    }
    Ok(())
}

// ── M7 pending-reply correlation types ─────────────────────────────

/// RFC-0861 §4 M7: per-command nonce for correlating an
/// outbound IRC command (e.g. `INVITE`) with its server reply
/// (e.g. `341 RPL_INVITING` or `482 ERR_CHANOPRIVSNEEDED`).
/// Monotonically increasing, allocated from
/// `IrcAdapter::next_command_id`. The numeric is the *internal*
/// correlation key; the IRC protocol has no built-in
/// per-command tag, so matching is FIFO at the listener
/// (see `pending_invites` in `irc_session`).
pub type CommandId = u64;

/// RFC-0861 §4 M7: the resolved result of a server reply
/// correlated with an outbound command. `Ok` covers `RPL_*`
/// success numerics; `Err` carries the server's error code
/// (the numeric itself) and the trailing message text, which
/// `add_member` maps to a `PlatformAdapterError` shape.
#[derive(Debug)]
pub enum NumericResult {
    /// Server returned a success numeric (e.g. 341 RPL_INVITING).
    Ok { code: u16 },
    /// Server returned an error numeric (e.g. 482
    /// ERR_CHANOPRIVSNEEDED). `code` is the numeric itself;
    /// `message` is the trailing text, often empty.
    Err { code: u16, message: String },
    /// The server did not reply within the configured timeout.
    /// Surfaced as a separate variant so callers can distinguish
    /// "server said no" from "server is silent".
    Timeout,
}

// ── Constants ──────────────────────────────────────────────────────

/// Maximum IRC line length including CRLF.
const IRC_MAX_LINE_BYTES: usize = 512;

/// Effective max payload per PRIVMSG: computed per-call rather than
/// as a global constant, because the PRIVMSG overhead includes the
/// channel name. See [`max_payload_for_channel`] for the exact
/// formula.
fn max_payload_for_channel(channel: &str) -> usize {
    // "PRIVMSG " (8) + channel + " :" (2) + CRLF (2)
    let overhead = 12 + channel.len();
    IRC_MAX_LINE_BYTES.saturating_sub(overhead)
}

/// Compatibility constant retained for the `CapabilityReport`. The
/// value assumes a typical 20-char channel name; for longer names
/// the per-call [`max_payload_for_channel`] returns a smaller
/// payload. Use the constant only for advertising the *typical*
/// limit to callers, not for splitting.
const TYPICAL_CHANNEL_LEN: usize = 20;
const PRIVMSG_OVERHEAD_TYPICAL: usize = 12 + TYPICAL_CHANNEL_LEN;
const MAX_PAYLOAD_PER_MSG: usize = IRC_MAX_LINE_BYTES - PRIVMSG_OVERHEAD_TYPICAL;

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
    /// Outgoing channel carrying pre-built IRC lines (without trailing
    /// CRLF) for both `CoordinatorAdmin` actions and regular `send_envelope`
    /// traffic. Initialized to `None` in `new()`; the sender half is
    /// installed by the first call to `ensure_connected`, then handed to
    /// the listener task alongside the receiver. The `OwnedWriteHalf`
    /// itself is owned by the listener task and never escapes
    /// `irc_session`; this channel is the only path from the public API
    /// to the socket.
    out_tx: Mutex<Option<mpsc::Sender<String>>>,
    /// Whether the IRC connection has been started. The watchdog pattern
    /// is: any send/recv failure on `out_tx` / `rx` indicates the
    /// listener task has died; the failing public method resets this
    /// flag to `false` so the next `ensure_connected` call respawns.
    connected: Mutex<bool>,
    /// RFC-0861 §4 M8: whether the IRC session has completed the
    /// NICK/USER handshake. Set to `true` in the listener's
    /// 376 (RPL_ENDOFMOTD) / 422 (ERR_NOMOTD) branch — those
    /// numerics are only sent *after* authentication completes, so
    /// they are the canonical "we are authenticated and the
    /// session is usable" signal. Cleared in BOTH `mark_disconnected`
    /// (transient drop, at `lib.rs:377`) AND `shutdown` (full
    /// teardown, at `lib.rs:1086`) so `health_check` never lies
    /// about a half-up session. Wrapped in `Arc` so the spawned
    /// listener task can mutate it without a `Mutex`; `health_check`
    /// and the listener share the same atomic via cheap refcount
    /// clone, no lock contention.
    is_authenticated: Arc<AtomicBool>,
    /// Stop-signal channel. `shutdown()` takes the sender (or replaces
    /// it with `None`) and notifies the listener, which exits its
    /// select loop. The receiver is moved into the listener spawn on
    /// the first `ensure_connected` call.
    shutdown_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    /// JoinHandle for the spawned listener task. `shutdown()` aborts
    /// it as a backstop in case the stop signal is racing with a
    /// blocked `read_line()`.
    listener_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Set to `true` once `shutdown()` has been called. After this,
    /// `ensure_connected` refuses to spawn a new listener — the
    /// adapter is terminal and callers must construct a fresh
    /// `IrcAdapter` to resume. This is the *hard-shutdown* contract:
    /// soft recovery (respawn) is intentionally not supported, so a
    /// caller that misuses a shut-down adapter gets a clear error
    /// instead of a silently-working adapter with zombie state.
    shutting_down: AtomicBool,
    /// Channels the bot has joined at runtime (via `join_by_invite`).
    /// Merged with `config.channels` by `list_own_groups` and
    /// `channel_for` so the bot can see and administer groups it joined
    /// outside the static config. Backwards-compatible: when empty,
    /// behavior is identical to the static-config-only path. Uses
    /// `std::sync::Mutex` (not `tokio::sync::Mutex`) because the
    /// critical sections are short string-vec operations with no
    /// `.await` inside — the lock is safe to hold in async context
    /// as long as the body doesn't await.
    runtime_channels: StdMutex<Vec<String>>,
    /// RFC-0861 §4 M7: monotonically increasing per-command
    /// nonce, allocated by `add_member` (and any future
    /// request/response pair) so the listener can correlate a
    /// server reply with the originating outbound command. The
    /// IRC protocol has no built-in per-command tag, so the
    /// correlation is FIFO via `pending_invites` below.
    next_command_id: AtomicU64,
    /// RFC-0861 §4 M7: pending INVITE requests awaiting a
    /// `341 RPL_INVITING` / `482 ERR_CHANOPRIVSNEEDED` reply.
    /// Keyed by the `CommandId` allocated at send time; the
    /// listener pops the entry with the smallest key on a
    /// matching reply and resolves the oneshot. `BTreeMap`
    /// (vs. `HashMap` in the spec text) because the spec's
    /// matching rule is "the next reply resolves the first
    /// sent command" — `BTreeMap::pop_first` gives O(log n)
    /// FIFO; `HashMap` would be O(n) and order-undefined.
    /// Wrapped in `Arc` so the listener task (which lives in
    /// `irc_session`) can resolve entries without holding the
    /// adapter's full state.
    pending_invites: Arc<Mutex<BTreeMap<CommandId, oneshot::Sender<NumericResult>>>>,
}

impl IrcAdapter {
    pub fn new(config: IrcConfig) -> Self {
        let (tx, rx) = mpsc::channel(4096);
        Self {
            config,
            rx: Mutex::new(rx),
            tx,
            out_tx: Mutex::new(None),
            connected: Mutex::new(false),
            is_authenticated: Arc::new(AtomicBool::new(false)),
            shutdown_tx: Mutex::new(None),
            listener_handle: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
            runtime_channels: StdMutex::new(Vec::new()),
            next_command_id: AtomicU64::new(1),
            pending_invites: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: IrcConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Start IRC connection (idempotent). Spawns the listener task on
    /// the first call; subsequent calls are no-ops as long as the
    /// listener is alive. The watchdog pattern: any public method that
    /// fails to send to / recv from the listener's channel resets
    /// `connected = false` (see `send_raw_line`, `receive_messages`,
    /// `send_envelope`), so the next `ensure_connected` respawns.
    ///
    /// **Hard shutdown (R23e N14):** once `shutdown()` has been called,
    /// `ensure_connected` returns `Err(transport_err("..."))` instead
    /// of respawning. The caller must construct a fresh `IrcAdapter`
    /// to recover. This makes post-shutdown misuse fail loudly rather
    /// than silently spawning a new listener over a half-torn-down one.
    async fn ensure_connected(&self) -> Result<(), PlatformAdapterError> {
        // Pre-flight: validate the config so a malformed `IrcConfig`
        // (empty channel, empty server, etc.) is caught here with a
        // clear error before we try to TCP-connect. Note this runs
        // on every call to ensure_connected, but it's a tiny pure
        // function — the cost is negligible compared to TCP connect.
        self.config.validate().map_err(transport_err)?;

        // The `connected` lock is held throughout the spawn sequence
        // (build channels, install senders, spawn listener). Shutdown
        // also acquires this lock as its FIRST step (before touching
        // shutdown_tx / out_tx / listener_handle), so a concurrent
        // shutdown blocks here until our spawn is fully visible — or
        // we block until shutdown is done. The shutting_down check
        // is inside the lock as well: even if shutdown ran while we
        // were queued on the lock, when we acquire it we re-check
        // the flag and bail without spawning.
        let mut connected = self.connected.lock().await;

        // R23e N14 + R23f N21: refuse to respawn after shutdown.
        // The check is INSIDE the `connected` lock for two reasons:
        //   1. So shutdown can't sneak in between the check and the
        //      spawn — its `connected.lock().await` blocks until our
        //      entire spawn sequence completes.
        //   2. So that if we acquire the lock AFTER shutdown ran
        //      (shutdown has set shutting_down=true and released
        //      connected), we still refuse to spawn.
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(transport_err(
                "IrcAdapter has been shut down; construct a fresh adapter to reconnect",
            ));
        }

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
        // RFC-0861 §4 M8: clone the `Arc<AtomicBool>` so the spawned
        // listener can flip the flag on 376/422 while the field on
        // `self` stays readable by `health_check` and clearable by
        // `mark_disconnected` / `shutdown`.
        let is_authenticated = self.is_authenticated.clone();
        // RFC-0861 §4 M7: clone the pending_invites Arc so the
        // listener can resolve entries on 341/482 without
        // holding the adapter's full state.
        let pending_invites = self.pending_invites.clone();

        // Build the unified outbound channel (admin + send_envelope).
        // The receiver is moved into the listener task; the sender is
        // installed on `self` for the public API.
        //
        // Capacity is 128: admin commands (KICK, MODE, TOPIC, etc.) are
        // rare, but `send_envelope` can produce many PRIVMSG fragments
        // for large envelopes. 128 lets a 100-fragment envelope plus
        // 28 admin commands queue without backpressure. The listener
        // drains in `biased;` select so outbound makes forward progress
        // even when the server isn't pushing data.
        let (out_tx, out_rx) = mpsc::channel::<String>(128);
        *self.out_tx.lock().await = Some(out_tx);

        // Build a stop-signal watch channel. The receiver is moved
        // into the listener; the sender is kept on `self` so
        // `shutdown()` can wake the listener. (R23c N3: previously
        // the listener ran forever and the FFI shutdown path
        // leaked the spawned task.)
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        *self.shutdown_tx.lock().await = Some(stop_tx);

        let handle = tokio::spawn(async move {
            irc_listener(
                server,
                port,
                nickname,
                channels,
                password,
                use_tls,
                tx,
                out_rx,
                stop_rx,
                is_authenticated,
                pending_invites,
            )
            .await;
        });
        *self.listener_handle.lock().await = Some(handle);

        *connected = true;
        Ok(())
    }

    /// Mark the adapter as disconnected. Called by any public method
    /// that detects the listener task has died (send/recv failure on
    /// the listener's channels). The next `ensure_connected` will
    /// respawn. Note: we don't abort the listener here — if the
    /// outbound channel send fails, the listener is in its
    /// `out_rx.recv()` arm and will return `Ok(())` shortly. If it's
    /// truly stuck, the `shutdown()`-based abort is the right
    /// recovery path.
    async fn mark_disconnected(&self) {
        *self.connected.lock().await = false;
        *self.out_tx.lock().await = None;
        // RFC-0861 §4 M8: clear the authentication flag on every
        // transient drop so `health_check` can't report Ok(()) for
        // a half-up session that just lost the socket. The flag
        // will be re-set on the next 376/422 once the listener
        // reconnects and re-handshakes.
        self.is_authenticated
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Internal helper: send a pre-built raw IRC line (without trailing
    /// `\r\n`) to the socket through the outbound channel. The listener
    /// task adds the CRLF and writes it to the TCP stream.
    ///
    /// Returns `Ok(())` once the line is enqueued. The line is *fire-and-
    /// forget*: IRC has no synchronous request/response correlation at
    /// the protocol level, so this method does not block on the server's
    /// reply. Callers that need confirmation should follow up with a
    /// `CoordinatorAdmin::get_group_metadata` (when the metadata is
    /// observable via a server response that the bot captures later)
    /// or accept eventual consistency.
    ///
    /// **Validation:** rejects lines containing CR, LF, or NUL to defend
    /// against command-injection (a future caller passing user-supplied
    /// text into `format!` could otherwise emit `\r\nNICK pwned\r\n`).
    /// The check is the belt to the listener's suspenders (see
    /// `irc_session`).
    async fn send_raw_line(&self, line: &str) -> Result<(), PlatformAdapterError> {
        if line.contains('\r') || line.contains('\n') || line.contains('\0') {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!("admin line contains illegal byte (CR/LF/NUL): {line:?}"),
            });
        }
        self.ensure_connected().await?;
        let tx = {
            let guard = self.out_tx.lock().await;
            guard
                .as_ref()
                .ok_or_else(|| transport_err("outbound channel not initialized"))?
                .clone()
        };
        if let Err(_e) = tx.send(line.to_string()).await {
            // Listener task is dead (the receiver was dropped). Mark
            // disconnected so the next call respawns.
            self.mark_disconnected().await;
            return Err(PlatformAdapterError::Unreachable {
                platform: "irc".into(),
                reason: "outbound channel closed (listener exited)".into(),
            });
        }
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

/// The "IrcWriter" enum is the union of the two writer types
/// `irc_session` supports: a plain TCP `OwnedWriteHalf` and the write
/// half of a `tokio_rustls::client::TlsStream`. We box the writes
/// through a small `write_line` async helper that hides the variant
/// behind a single method call.
///
/// Why an enum and not a trait object? `AsyncWrite` is a real trait,
/// but splitting the read/write halves and using `select!` requires
/// that the writer's `&&mut self` borrow be held for the duration of
/// the session. The enum is statically dispatched, the borrow is
/// checked at compile time, and there's no boxing overhead.
enum IrcWriter {
    Plain(TcpWriteHalf),
    Tls(TokioWriteHalf<TlsStream<TcpStream>>),
}

impl IrcWriter {
    async fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        match self {
            IrcWriter::Plain(w) => {
                w.write_all(line.as_bytes()).await?;
                w.write_all(b"\r\n").await?;
            }
            IrcWriter::Tls(w) => {
                w.write_all(line.as_bytes()).await?;
                w.write_all(b"\r\n").await?;
            }
        }
        Ok(())
    }
}

/// The "IrcReader" enum is the union of the two reader types
/// `irc_session` supports: a plain TCP `OwnedReadHalf` wrapped in
/// `BufReader`, and the read half of a `tokio_rustls::client::TlsStream`
/// wrapped in `BufReader`. We expose `read_line` as a single method
/// so the rest of the session loop doesn't have to branch.
enum IrcReader {
    Plain(BufReader<TcpReadHalf>),
    Tls(BufReader<TokioReadHalf<TlsStream<TcpStream>>>),
}

impl IrcReader {
    async fn read_line(&mut self, buf: &mut String) -> std::io::Result<usize> {
        match self {
            IrcReader::Plain(r) => r.read_line(buf).await,
            IrcReader::Tls(r) => r.read_line(buf).await,
        }
    }
}

/// Build a rustls `ClientConfig` with the Mozilla CA bundle and no
/// client authentication. This is the standard "trust the public
/// WebPKI" config for IRC servers (irc.libera.chat, irc.oftc.net,
/// etc.). SNI is set from the server name on each connect.
///
/// The `ClientConfig` is built once per process and cached at the
/// module level via `OnceLock`, so the CA bundle is parsed only on
/// first use.
fn tls_client_config() -> Arc<ClientConfig> {
    use std::sync::OnceLock;
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            Arc::new(
                ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )
        })
        .clone()
}

/// The fully-connected (TCP, optionally with TLS) IRC stream, just
/// before we split it into read/write halves.
#[allow(clippy::large_enum_variant)] // TLS handshake is in-memory; size gap is intentional.
enum IrcStream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

/// Long-running IRC listener task.
///
/// Owns the outbound `Receiver` for the lifetime of the adapter and
/// re-attaches it to every fresh session. This lets admin actions
/// (`CoordinatorAdmin`) and regular `send_envelope` traffic keep
/// working across TCP reconnects: the same `IrcAdapter` is reused
/// and the channel pair (`out_tx`, `out_rx`) is stable for the
/// adapter's lifetime.
///
/// The `stop_rx` watch is the cooperative-shutdown signal set by
/// `IrcAdapter::shutdown`. The listener selects on `stop_rx.changed()`
/// alongside `out_rx.recv()` and `reader.read_line()`, so the listener
/// can wake up from any of those arms when shutdown is requested.
/// (R23c N3: previously the listener could only be exited by dropping
/// `out_rx`, which didn't help if the listener was parked in a
/// non-yielding read.)
#[allow(clippy::too_many_arguments)]
async fn irc_listener(
    server: String,
    port: u16,
    nickname: String,
    channels: Vec<String>,
    password: Option<String>,
    use_tls: bool,
    tx: mpsc::Sender<RawPlatformMessage>,
    mut out_rx: mpsc::Receiver<String>,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
    is_authenticated: Arc<AtomicBool>,
    pending_invites: Arc<Mutex<BTreeMap<CommandId, oneshot::Sender<NumericResult>>>>,
) {
    let retry = RetryConfig::default();
    let mut attempt = 0u32;

    loop {
        // Check the stop signal before each connect attempt. If
        // shutdown was requested while we were in a previous
        // session's `select!`, we'll see the change here.
        if *stop_rx.borrow() {
            return;
        }
        let connect_result = if use_tls {
            connect_tls(&server, port, &server).await
        } else {
            connect_plain(&server, port).await.map(IrcStream::Plain)
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
                    &mut out_rx,
                    &mut stop_rx,
                    &is_authenticated,
                    &pending_invites,
                )
                .await
                {
                    tracing::warn!(target: "octo.adapter.irc", error = %e, "IRC session error");
                }
            }
            Err(e) => {
                tracing::warn!(target: "octo.adapter.irc", error = %e, "IRC connect error");
            }
        }

        // Check stop signal before sleeping on backoff. Otherwise
        // a fast shutdown could be delayed by up to
        // `retry.max_delay_secs` seconds.
        if *stop_rx.borrow() {
            return;
        }
        let delay = retry.delay_for_attempt(attempt.min(retry.max_retries));
        tokio::select! {
            biased;
            _ = stop_rx.changed() => return,
            _ = tokio::time::sleep(delay) => {}
        }
        attempt += 1;
    }
}

async fn connect_plain(server: &str, port: u16) -> Result<TcpStream, String> {
    TcpStream::connect(format!("{server}:{port}"))
        .await
        .map_err(|e| format!("TCP connect: {e}"))
}

/// TLS connect: TCP first, then rustls handshake. The server name is
/// used both as the SNI value (most IRC servers require SNI on port
/// 6697) and for certificate hostname verification.
async fn connect_tls(server: &str, port: u16, sni: &str) -> Result<IrcStream, String> {
    let tcp = connect_plain(server, port).await?;
    let connector = TlsConnector::from(tls_client_config());
    let name = ServerName::try_from(sni.to_string())
        .map_err(|e| format!("invalid server name for SNI {sni:?}: {e}"))?;
    connector
        .connect(name, tcp)
        .await
        .map(IrcStream::Tls)
        .map_err(|e| format!("TLS handshake: {e}"))
}

/// IRC session: authenticate, join channels, process messages.
///
/// The `out_rx` argument is borrowed for the lifetime of the session
/// and drained on every loop iteration alongside the incoming-line
/// read. The borrow is released when the session ends (connection
/// drop, error), at which point `irc_listener` gets the receiver back
/// and passes it to the next fresh session.
///
/// The `stop_rx` watch is the cooperative-shutdown signal. The
/// session loop selects on `stop_rx.changed()` alongside the other
/// arms so a shutdown request is observed promptly. (R23c N3.)
#[allow(clippy::too_many_arguments)] // Session loop requires all of these handles.
async fn irc_session(
    stream: IrcStream,
    nickname: &str,
    channels: &[String],
    password: Option<&str>,
    tx: &mpsc::Sender<RawPlatformMessage>,
    out_rx: &mut mpsc::Receiver<String>,
    stop_rx: &mut tokio::sync::watch::Receiver<bool>,
    is_authenticated: &Arc<AtomicBool>,
    pending_invites: &Arc<Mutex<BTreeMap<CommandId, oneshot::Sender<NumericResult>>>>,
) -> Result<(), String> {
    // Split the (possibly TLS) stream into read and write halves.
    let (mut reader, mut writer): (IrcReader, IrcWriter) = match stream {
        IrcStream::Plain(s) => {
            let (r, w) = s.into_split();
            (IrcReader::Plain(BufReader::new(r)), IrcWriter::Plain(w))
        }
        IrcStream::Tls(s) => {
            let (r, w) = tokio::io::split(s);
            (IrcReader::Tls(BufReader::new(r)), IrcWriter::Tls(w))
        }
    };
    let mut line = String::new();

    // Authenticate
    if let Some(pass) = password {
        writer
            .write_line(&format!("PASS {pass}"))
            .await
            .map_err(|e| format!("PASS: {e}"))?;
    }
    writer
        .write_line(&format!("NICK {nickname}"))
        .await
        .map_err(|e| format!("NICK: {e}"))?;
    writer
        .write_line(&format!("USER {nickname} 0 * :CipherOcto DOT Bot"))
        .await
        .map_err(|e| format!("USER: {e}"))?;

    // Join channels after MOTD
    let mut joined = false;
    loop {
        // `tokio::select!` with `biased;` polls the stop-signal,
        // outbound, and read branches in that order. The stop-signal
        // is highest priority so a shutdown request preempts all I/O
        // immediately. See the long comment in `irc_listener` for
        // the rationale; the key point is that outbound
        // (`send_raw_line`, `send_envelope`) makes forward progress
        // regardless of the server's traffic shape. (R23c N3.)
        tokio::select! {
            biased;

            // ── Branch 0: stop signal ────────────────────────
            //
            // Cooperative shutdown from `IrcAdapter::shutdown`.
            // Fires when `shutdown_tx` sends `true`. Returns
            // Ok so the listener exits cleanly. We don't write
            // QUIT to the server: the gateway may have a tight
            // shutdown deadline, and the server-side
            // disconnect happens naturally when the socket
            // closes during process exit.
            _ = stop_rx.changed() => {
                tracing::info!(target: "octo.adapter.irc", "IRC session received shutdown signal");
                return Ok(());
            }

            // ── Branch 1: outbound line ──────────────────────
            //
            // The sender's `String` is a raw IRC line without
            // trailing CRLF; we append CRLF here so the server
            // sees a complete line. `None` means all senders
            // were dropped (adapter shutdown).
            out = out_rx.recv() => {
                match out {
                    Some(out_line) => {
                        writer
                            .write_line(&out_line)
                            .await
                            .map_err(|e| format!("outbound write: {e}"))?;
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
                        .write_line(&format!("PONG {server}"))
                        .await
                        .map_err(|e| format!("PONG: {e}"))?;
                    line.clear();
                    continue;
                }

                // Join channels after RPL_ENDOFMOTD (376) or ERR_NOMOTD (422)
                if !joined && (trimmed.contains(" 376 ") || trimmed.contains(" 422 ")) {
                    // RFC-0861 §4 M8: receiving 376/422 is the
                    // canonical "we are authenticated and the
                    // session is usable" signal — these numerics
                    // are only sent *after* the NICK/USER
                    // handshake completes. Flip the shared
                    // atomic so `health_check` (which reads it
                    // on the public API path) can return Ok(()) .
                    is_authenticated.store(true, std::sync::atomic::Ordering::SeqCst);
                    for ch in channels {
                        writer
                            .write_line(&format!("JOIN {ch}"))
                            .await
                            .map_err(|e| format!("JOIN: {e}"))?;
                    }
                    joined = true;
                    line.clear();
                    continue;
                }

                // RFC-0861 §4 M7: correlate INVITE replies
                // (341 RPL_INVITING / 482 ERR_CHANOPRIVSNEEDED)
                // with the pending sender in `pending_invites`.
                // We pop the oldest entry (FIFO) — IRC numerics
                // are FIFO at the protocol level so the next
                // reply after an INVITE is the reply for that
                // INVITE. Other error numerics (e.g. 401
                // ERR_NOSUCHNICK, 442 ERR_NOTONCHANNEL) flow
                // through the same path; the listener doesn't
                // filter by code at this point — the caller
                // (`add_member`) maps the code to a
                // PlatformAdapterError.
                if let Some(numeric) = parse_numeric_reply(trimmed) {
                    if numeric.command == "INVITE" || matches!(numeric.code, 341 | 482 | 401 | 442 | 443) {
                        let mut pending = pending_invites.lock().await;
                        if let Some((_id, sender)) = pending.pop_first() {
                            let result = if (200..400).contains(&numeric.code) {
                                NumericResult::Ok { code: numeric.code }
                            } else {
                                NumericResult::Err {
                                    code: numeric.code,
                                    message: numeric.message,
                                }
                            };
                            // Ignore send errors: the receiver
                            // may have been dropped (timeout
                            // path, shutdown). The drop is
                            // benign — the entry is already
                            // removed from the map.
                            let _ = sender.send(result);
                        }
                        // Continue parsing more of the line
                        // (a numeric reply line is normally
                        // a single record; nothing else to do).
                        line.clear();
                        continue;
                    }
                }

                // Parse PRIVMSG
                if let Some(msg) = parse_privmsg(trimmed) {
                    // Check if it's a DOT message
                    if msg.text.starts_with(DOT_PREFIX) || msg.text.starts_with(DOT_FRAGMENT_PREFIX) {
                        if let Ok(payload) = IrcAdapter::decode_message(&msg.text) {
                            let mut metadata = BTreeMap::new();
                            metadata.insert("channel".into(), msg.channel.clone());
                            metadata.insert("sender".into(), msg.sender.clone());
                            // R23c N4 fix: use `try_send` instead of
                            // `send().await`. The previous version
                            // parked the entire select! body in
                            // `tx.send().await` when the inbound
                            // channel was full, which meant we
                            // couldn't process PINGs and the
                            // server timed us out. With `try_send`,
                            // the worst case is a logged drop on
                            // overload, which is visible (vs the
                            // R1 silent-drop on `try_send`).
                            //
                            // The trade-off: under sustained
                            // overload we drop envelopes rather than
                            // disconnecting. The consumer
                            // (`receive_messages`) is expected to
                            // drain at ≥ IRC rate (1 msg/s default);
                            // if it can't, the gateway has a
                            // throughput problem.
                            match tx.try_send(RawPlatformMessage {
                                platform_id: format!("irc-{}", msg.id),
                                payload,
                                metadata,
                            }) {
                                Ok(()) => {}
                                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                    tracing::warn!(
                                        target: "octo.adapter.irc",
                                        channel = %msg.channel,
                                        "IRC inbound channel full; envelope dropped"
                                    );
                                }
                                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                    tracing::warn!(
                                        target: "octo.adapter.irc",
                                        "IRC inbound channel closed; listener exiting"
                                    );
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                line.clear();
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

/// Parsed IRC numeric reply (e.g. `:server 341 nickname #chan :invited`).
///
/// `command` is the command that produced this reply (e.g.
/// `INVITE` for `:... 341 ... INVITE ...`) or empty if the
/// server didn't echo the command verb. The IRC protocol puts
/// the command in the third positional parameter for RPL_*
/// replies, but servers vary — we extract it best-effort.
///
/// `code` is the numeric (e.g. 341 for RPL_INVITING, 482 for
/// ERR_CHANOPRIVSNEEDED). `message` is the trailing text
/// after the final ` :`.
struct NumericReply {
    code: u16,
    command: String,
    message: String,
}

/// RFC-0861 §4 M7: parse a numeric reply from the server. The
/// format is `:prefix <code> <me> [args...] [:trailing]`. We
/// don't strictly need the prefix (the server's hostname); we
/// just need the code and the trailing message for `add_member`'s
/// error mapping. The "command" field is best-effort — many
/// servers echo the originating command verb as a positional
/// arg (e.g. `:s 341 me #chan nick :already invited`), some
/// don't.
fn parse_numeric_reply(line: &str) -> Option<NumericReply> {
    // Numeric replies always start with `:` (a server prefix).
    let line = line.strip_prefix(':')?;
    let (prefix, rest) = line.split_once(' ')?;
    // `prefix` is unused for our purposes; the server
    // hostname is logged elsewhere.
    let _ = prefix;
    // The next token is the numeric code.
    let (code_str, rest) = rest.split_once(' ')?;
    let code: u16 = code_str.parse().ok()?;
    // The remaining args are positional. Split on spaces, then
    // handle the trailing ` :message` form. The optional
    // command verb (e.g. `INVITE`) is the LAST positional arg
    // before the trailing message in standard `INVITE` echo
    // numerics.
    let mut parts = rest.splitn(2, " :");
    let positional = parts.next().unwrap_or("");
    let trailing = parts.next().unwrap_or("").to_string();
    // Heuristic: the command verb (if echoed) is the last
    // token of the positional string. Most servers don't
    // echo it for arbitrary error numerics.
    let command = positional
        .split_whitespace()
        .next_back()
        .unwrap_or("")
        .to_string();
    Some(NumericReply {
        code,
        command,
        message: trailing,
    })
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
    async fn send_message(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
        _payload: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Spawn the listener if it isn't already running. Without
        // this, a `send_envelope` call before any `receive_messages`
        // would never establish the IRC connection (R23b C3).
        self.ensure_connected().await?;

        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Find the channel for this domain. The lookup is in the
        // merged set of statically-configured and runtime-joined
        // channels (R23b C4). The runtime set is guarded by a
        // `std::sync::Mutex` (see `IrcAdapter::runtime_channels`),
        // so a brief blocking lock is safe here.
        let channel = {
            let in_static = self.config.channels.iter().find(|ch| {
                let hash = Self::domain_hash(&self.config.server, ch);
                hash == domain.domain_hash
            });
            if let Some(ch) = in_static {
                ch.clone()
            } else {
                let runtime = self
                    .runtime_channels
                    .lock()
                    .map_err(|e| transport_err(format!("runtime_channels poisoned: {e}")))?;
                let in_runtime = runtime
                    .iter()
                    .find(|ch| {
                        let hash = Self::domain_hash(&self.config.server, ch);
                        hash == domain.domain_hash
                    })
                    .cloned();
                in_runtime.ok_or_else(|| {
                    transport_err(format!("No channel for domain {:?}", domain.domain_hash))
                })?
            }
        };

        // Split if needed (IRC has strict line limits). The
        // per-channel overhead matters: a 24-char channel name
        // overflows the 512-byte IRC line if we use the typical
        // 20-char-headroom `MAX_PAYLOAD_PER_MSG` (R23c N5).
        let max_bytes = max_payload_for_channel(&channel);
        let chunks = Self::split_message(&encoded, max_bytes);
        let total = chunks.len() as u16;

        // Build the PRIVMSG lines and enqueue each one on the
        // outbound channel so the listener writes them to the wire
        // (R23b C2: previously this was a no-op that returned a fake
        // DeliveryReceipt).
        let now = epoch_millis();
        for (i, chunk) in chunks.iter().enumerate() {
            let line = if total > 1 {
                Self::encode_fragment(i as u16, total, chunk.as_bytes())
            } else {
                chunk.clone()
            };
            // PRIVMSG #channel :<line> (the listener appends CRLF)
            let irc_msg = format!("PRIVMSG {channel} :{line}");
            self.send_raw_line(&irc_msg).await?;
        }

        Ok(DeliveryReceipt {
            platform_message_id: format!("irc-{now}"),
            delivered_at: now,
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
            ..Default::default()
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
        // R23c N3 + R23e N14 + R23f N21: actually shut down with
        // proper serialization against `ensure_connected`. The
        // sequence is:
        //
        //   1. Set the `shutting_down` flag. This is the gate that
        //      any racing `ensure_connected` checks; once it's
        //      set, future ensure_connected calls refuse to spawn.
        //   2. Acquire the `connected` lock *before* touching any
        //      of the related state (shutdown_tx, out_tx,
        //      listener_handle). This is the critical R23f N21
        //      fix: without it, a racing ensure_connected could
        //      install shutdown_tx / listener_handle *after*
        //      shutdown had already taken None for them, leaving
        //      a zombie listener that no one would ever signal
        //      or abort. Holding `connected` for the entire
        //      teardown ensures at most one of {spawn, teardown}
        //      is in flight at a time.
        //   3. Take shutdown_tx (signal stop), drop out_tx (None),
        //      take listener_handle (abort), set connected=false.
        //
        // Lock ordering: shutdown → ensure_connected both hold
        // `connected` for the duration of their state changes.
        // No deadlock because neither holds any other lock while
        // waiting for `connected`.
        self.shutting_down.store(true, Ordering::SeqCst);
        let mut connected = self.connected.lock().await;
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(true);
        }
        *self.out_tx.lock().await = None;
        if let Some(handle) = self.listener_handle.lock().await.take() {
            handle.abort();
            // Best-effort: don't block shutdown on the abort.
            // (We could `.await` here for cleanliness, but the
            // gateway's shutdown deadline matters more than
            // waiting for a stuck task.)
        }
        *connected = false;
        // RFC-0861 §4 M8: clear the authentication flag on full
        // teardown too. After `shutdown()` returns, the adapter
        // is terminal; a subsequent `health_check` (e.g. on a
        // revived adapter reference) MUST see `is_authenticated
        // = false` until a fresh handshake completes.
        self.is_authenticated
            .store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // RFC-0861 §4 M8: the TCP path can be up while the IRC
        // session is still in the NICK/USER handshake (or has
        // silently half-dropped and not yet seen 376/422). A bare
        // `TcpStream::connect` would lie in that window. Check
        // `is_authenticated` first: if the listener hasn't
        // confirmed the 376/422 numerics, the session is not
        // usable yet — return 503 so callers can distinguish
        // "TCP up, auth pending" from "TCP down".
        if !self
            .is_authenticated
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(PlatformAdapterError::ApiError {
                code: 503,
                message: "IRC session not authenticated".into(),
            });
        }
        // RFC-0861 §7 M3: when `use_tls = true`, `health_check`
        // must attempt the same TLS handshake the listener does,
        // not just a plain `TcpStream::connect`. Otherwise the
        // check would report Ok(()) on a session whose TCP path
        // is up but whose TLS layer is broken (e.g. expired cert,
        // MITM strip, cipher mismatch). On TLS handshake failure,
        // return 525 (a custom code distinct from 503 auth and
        // from generic transport errors) so callers can
        // distinguish "TCP up, TLS broken" from "TCP down".
        let timeout = std::time::Duration::from_secs(5);
        let addr = format!("{}:{}", self.config.server, self.config.port);
        if self.config.use_tls {
            let sni = self.config.server.clone();
            match tokio::time::timeout(
                timeout,
                connect_tls(&self.config.server, self.config.port, &sni),
            )
            .await
            {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(reason)) => {
                    if reason.starts_with("TCP connect") {
                        Err(transport_err(format!("Health check: {reason}")))
                    } else {
                        Err(PlatformAdapterError::ApiError {
                            code: 525,
                            message: format!("TLS handshake failed: {reason}"),
                        })
                    }
                }
                Err(_) => Err(transport_err("Health check timed out")),
            }
        } else {
            // Check TCP connectivity to the server
            match tokio::time::timeout(timeout, TcpStream::connect(&addr)).await {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(e)) => Err(transport_err(format!("Health check: {e}"))),
                Err(_) => Err(transport_err("Health check timed out")),
            }
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
            can_create: false,        // IRC has no group creation
            can_join_by_id: true,     // RFC-0861 §1 M10: JOIN #channel IS join-by-id
            can_join_by_invite: true, // JOIN #channel (best-effort)
            can_leave: true,          // PART
            can_destroy: false,       // no invite-link to revoke

            // ── B. Membership ─────────────────────────────────
            can_add_member: true,    // INVITE (server-mediated)
            can_remove_member: true, // KICK
            can_ban: false,          // MODE +b needs hostmask, not in PeerId
            can_promote: true,       // MODE +o
            can_demote: true,        // MODE -o
            can_approve_join: false, // no approval workflow

            // ── C. Mode ───────────────────────────────────────
            can_rename: true,            // TOPIC
            can_describe: false,         // no description separate from topic
            can_lock: true,              // MODE +i / -i
            can_announce: true,          // MODE +m / -m
            can_set_ephemeral: false,    // no TTL
            can_require_approval: false, // no approval

            // ── D. Discovery ──────────────────────────────────
            can_list_own_groups: true, // configured channels
            can_get_metadata: false,   // no sync NAMES/MODE capture
            can_resolve_invite: false, // no invite URL

            // ── E. Handoff ────────────────────────────────────
            can_transfer_ownership: false, // no transfer primitive

            // ── F. Misc admin (Session 7.H) ────────────────────
            can_get_invite_link: false, // IRC invites are per-channel, not server-stored + revocable
            can_update_member_label: false, // no per-member admin title on IRC
            can_get_profile_pictures: false, // IRC has no group avatars
            can_set_profile_picture: false, // IRC has no group avatars
            can_remove_profile_picture: false, // IRC has no group avatars
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
    ///
    /// **RFC-0861 §4 M7.** The send is correlated with the
    /// reply via `pending_invites`. The reply is one of:
    ///
    /// - `341 RPL_INVITING` → `Ok(AddMemberOutput { added: true, promoted: None })`
    /// - `482 ERR_CHANOPRIVSNEEDED` → `Err(ApiError { code: 403, message: "not a channel operator" })`
    /// - no reply within the timeout → `Err(ApiError { code: 504, message: "no reply from server" })`
    ///
    /// The match is FIFO across concurrent `add_member` calls:
    /// the next reply resolves the oldest pending send. IRC
    /// numerics are FIFO at the protocol level, so this
    /// matches the natural order of the conversation.
    async fn add_member(
        &self,
        group_id: &GroupId,
        member: &GroupMemberSpec,
    ) -> Result<AddMemberOutput, PlatformAdapterError> {
        let channel = self.channel_for(group_id)?;
        // Allocate a per-call nonce. The reply is correlated
        // by FIFO, so the nonce is just a unique key.
        let cmd_id = self.next_command_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel::<NumericResult>();
        {
            let mut pending = self.pending_invites.lock().await;
            pending.insert(cmd_id, tx);
        }
        // Fire the INVITE. If the send fails, remove our
        // pending entry so the listener doesn't later resolve
        // a sender that no one is awaiting (the sender is
        // dropped, the `await rx` will return `RecvError`).
        if let Err(e) = self
            .send_raw_line(&format!("INVITE {} {channel}", member.handle))
            .await
        {
            // Best-effort cleanup. If the listener is already
            // mid-resolve, this `remove` is a no-op (the entry
            // is gone) — that's fine, the rx just hangs in
            // a tokio task that we never created (no future
            // exists; the `await rx` happens below only if
            // the send succeeded).
            let mut pending = self.pending_invites.lock().await;
            pending.remove(&cmd_id);
            return Err(e);
        }
        // Await the reply with a timeout. The timeout is
        // generous (5s) because some networks rate-limit
        // responses; if no reply comes, the caller can retry.
        let result = match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_recv_err)) => {
                // Listener dropped the sender without
                // resolving — this happens on session
                // shutdown mid-flight.
                return Err(PlatformAdapterError::ApiError {
                    code: 504,
                    message: "add_member: pending invite was dropped (session closed?)".into(),
                });
            }
            Err(_elapsed) => {
                // Timeout: clean up our entry so a stale
                // pending doesn't accumulate.
                let mut pending = self.pending_invites.lock().await;
                pending.remove(&cmd_id);
                return Err(PlatformAdapterError::ApiError {
                    code: 504,
                    message: "add_member: no reply from server within 5s".into(),
                });
            }
        };
        match result {
            NumericResult::Ok { code: _ } => Ok(AddMemberOutput {
                added: true,
                promoted: None,
            }),
            NumericResult::Err { code, message } => {
                // M7: 482 ERR_CHANOPRIVSNEEDED is the canonical
                // "you're not a channel operator" reply. Map
                // it to ApiError 403 per the spec. Other
                // error numerics (e.g. 401 ERR_NOSUCHNICK,
                // 442 ERR_NOTONCHANNEL) flow through with
                // their own codes so the caller can
                // distinguish.
                let mapped_code = if code == 482 { 403 } else { code };
                let mapped_msg = if code == 482 {
                    "not a channel operator".to_string()
                } else {
                    if message.is_empty() {
                        format!("add_member rejected with numeric {code}")
                    } else {
                        format!("add_member: {message} (numeric {code})")
                    }
                };
                Err(PlatformAdapterError::ApiError {
                    code: mapped_code,
                    message: mapped_msg,
                })
            }
            NumericResult::Timeout => Err(PlatformAdapterError::ApiError {
                code: 504,
                message: "add_member: no reply from server (timed out)".into(),
            }),
        }
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
        self.send_raw_line(&format!("MODE {channel} {flag}")).await
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
        self.send_raw_line(&format!("MODE {channel} {flag}")).await
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

    /// Return the configured channels *and* any channels the bot has
    /// joined at runtime via `join_by_invite`. IRC channels aren't
    /// "discovered" — the bot only knows about the ones in its config
    /// plus any it has successfully JOINed. The merge is
    /// deduplicating: if a runtime channel is already in the static
    /// config, it appears once. `is_admin` is conservatively `false`
    /// because the bot's op status is determined by server policy and
    /// not tracked in the adapter state. (R23c N1 fix.)
    async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        // Lock briefly to snapshot the runtime channels, then drop
        // the lock before iterating. `config.channels` is read-only
        // after construction so it can be iterated directly.
        let runtime_snapshot: Vec<String> = {
            let runtime =
                self.runtime_channels
                    .lock()
                    .map_err(|e| PlatformAdapterError::Unreachable {
                        platform: "irc".into(),
                        reason: format!("runtime_channels poisoned: {e}"),
                    })?;
            runtime.clone()
        };
        let mut names: Vec<String> = self.config.channels.clone();
        for ch in &runtime_snapshot {
            if !names.iter().any(|c| c == ch) {
                names.push(ch.clone());
            }
        }
        Ok(names
            .into_iter()
            .map(|ch| GroupHandle {
                id: self.full_id(&ch),
                subject: None,
                invite_url: None,
                is_admin: false,
                member_count: None,
                mode_flags: None,
                initial_admins_promoted: false,
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
            phone_for_peer: std::collections::HashMap::new(),
            is_parent_group: false,
            parent_group_jid: None,
            is_default_sub_group: false,
            is_general_chat: false,
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

    /// RFC-0861 §1 M10: IRC's `JOIN #channel` is exactly
    /// join-by-id. Wrap `join_by_invite` with a `GroupId` →
    /// `InviteRef` adapter and forward. The capability report
    /// has `can_join_by_id: true` (was `false` pre-M10; the bit
    /// was conservative-but-wrong: the bot has always been able
    /// to JOIN by channel name, we just didn't expose the
    /// method). The body is identical to `join_by_invite`'s
    /// because the IRC protocol is the same — both go through
    /// `send_raw_line("JOIN ...")` — and the validation in
    /// `validate_channel_name` is the same for both.
    async fn join_by_id(&self, group_id: &GroupId) -> Result<GroupHandle, PlatformAdapterError> {
        let invite = InviteRef::new(group_id.as_str().to_string());
        self.join_by_invite(&invite).await
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
        // R23c N2: validate the channel name before sending JOIN
        // so that IRC special tokens (`JOIN 0`), no-prefix names,
        // and bad characters are caught client-side with a
        // structured error rather than producing an opaque
        // server-side rejection (or worse, parting all channels
        // via `JOIN 0`).
        validate_channel_name(&invite.0).map_err(|e| PlatformAdapterError::ApiError {
            code: 400,
            message: format!("join_by_invite: {e}"),
        })?;
        self.send_raw_line(&format!("JOIN {}", invite.0)).await?;
        // R23c N1: actually record the channel so subsequent
        // `list_own_groups` / `channel_for` / `send_envelope`
        // calls see it. Without this, the C4 fix is non-functional:
        // the server joins but the adapter state doesn't reflect it.
        {
            // R23e N20: if the mutex is poisoned, the JOIN has
            // already been sent to the server (line above), so the
            // bot *is* in the channel but our state disagrees.
            // Log loudly so an operator can reconcile manually
            // (e.g. shutdown + recreate the adapter). Returning
            // an error alone would leave the operator in the dark.
            let mut runtime = self.runtime_channels.lock().map_err(|e| {
                tracing::warn!(
                    target: "octo.adapter.irc",
                    channel = %invite.0,
                    error = %e,
                    "runtime_channels mutex poisoned AFTER successful JOIN; \
                     server-side join succeeded but adapter state will not record it"
                );
                PlatformAdapterError::Unreachable {
                    platform: "irc".into(),
                    reason: format!("runtime_channels poisoned: {e}"),
                }
            })?;
            if !runtime.iter().any(|c| c == &invite.0) {
                runtime.push(invite.0.clone());
            }
        }
        Ok(GroupHandle {
            id: self.full_id(&invite.0),
            subject: None,
            invite_url: None,
            is_admin: false,
            member_count: None,
            mode_flags: None,
            initial_admins_promoted: false,
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
    /// — the configured channel list *plus* any channel the
    /// bot has joined at runtime via `join_by_invite` (R23b C4
    /// fix: previously runtime-joined channels were
    /// invisible to admin actions).
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
        if !server.is_empty() && !server.eq_ignore_ascii_case(&&self.config.server) {
            return Err(PlatformAdapterError::ApiError {
                code: 400,
                message: format!(
                    "group {raw} is on server {server}, but adapter is connected to {}",
                    self.config.server
                ),
            });
        }
        if !self.config.channels.contains(&channel.to_string()) {
            // Fall back to the runtime-joined channel set
            // (populated by `join_by_invite`). Locking is brief
            // (vec.contains on a small list) so a blocking mutex
            // is fine here.
            let runtime =
                self.runtime_channels
                    .lock()
                    .map_err(|e| PlatformAdapterError::Unreachable {
                        platform: "irc".into(),
                        reason: format!("runtime_channels poisoned: {e}"),
                    })?;
            if !runtime.iter().any(|c| c == channel) {
                return Err(PlatformAdapterError::ApiError {
                    code: 404,
                    message: format!(
                        "channel {channel} is not in the configured channel list {:?} nor the runtime-joined set",
                        self.config.channels
                    ),
                });
            }
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
        let chunks = IrcAdapter::split_message(msg, 5);
        // Should split at UTF-8 boundaries
        for chunk in &chunks {
            assert!(chunk.len() <= 5);
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
        }
    }

    #[test]
    fn test_split_chinese_chars() {
        let msg = "中中中中中"; // 5 * 3 = 15 bytes
        let chunks = IrcAdapter::split_message(msg, 5);
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
        // RFC-0861 §1 M10: `can_join_by_id = true` because
        // `JOIN #channel` is exactly join-by-id (was
        // conservative-but-wrong `false` pre-M10).
        assert!(
            caps.can_join_by_id,
            "IRC's JOIN #channel is join-by-id (RFC-0861 §1 M10)"
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
        assert!(!caps.can_ban, "MODE +b needs hostmask, not in PeerId");
        assert!(caps.can_promote, "MODE +o is supported");
        assert!(caps.can_demote, "MODE -o is supported");
        assert!(!caps.can_approve_join, "no approval workflow");

        // Mode
        assert!(caps.can_rename, "TOPIC is supported");
        assert!(!caps.can_describe, "no description separate from topic");
        assert!(caps.can_lock, "MODE +i/-i is supported");
        assert!(caps.can_announce, "MODE +m/-m is supported");
        assert!(!caps.can_set_ephemeral, "no TTL");
        assert!(!caps.can_require_approval, "no approval");

        // Discovery
        assert!(
            caps.can_list_own_groups,
            "configured channels are enumerable"
        );
        assert!(!caps.can_get_metadata, "no sync NAMES/MODE capture");
        assert!(!caps.can_resolve_invite, "no invite URL");

        // Handoff
        assert!(!caps.can_transfer_ownership, "no transfer primitive");
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
        let err = adapter.channel_for(&GroupId::new("#unknown")).unwrap_err();
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
        assert!(matches!(
            err,
            PlatformAdapterError::ApiError { code: 404, .. }
        ));
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
            Err(PlatformAdapterError::Unimplemented {
                platform,
                action: a,
            }) => {
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
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), async {
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            received_clone.lock().await.extend_from_slice(&buf[..n]);
                        }
                        Err(_) => break,
                    }
                }
            })
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
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), async {
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            received_clone.lock().await.extend_from_slice(&buf[..n]);
                        }
                        Err(_) => break,
                    }
                }
            })
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
            .remove_member(&GroupId::new("127.0.0.1:#alpha"), &PeerId::new("alice"))
            .await
            .unwrap();

        // Also drive `promote_to_admin` and check the MODE line.
        adapter
            .promote_to_admin(&GroupId::new("127.0.0.1:#alpha"), &PeerId::new("bob"))
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

    // ── R23c regression tests ──────────────────────────────────
    //
    // These tests cover the issues found in the second-round
    // adversarial review. Each test maps to one or more findings:
    //
    // - N1 (CRITICAL): runtime_channels was never populated
    // - N2 (HIGH): join_by_invite did not validate channel name
    // - N3 (HIGH): shutdown was a no-op
    // - N4 (HIGH): tx.send().await blocked PING handling
    // - N5 (MEDIUM): PRIVMSG_OVERHEAD assumed 20-char channel name
    // - N9 (LOW): server name was never shape-validated
    //
    // The tests are pure-unit where possible (no socket, no
    // runtime) and only spin up tokio where the behavior under
    // test is async.

    /// N1 (regression): `join_by_invite` must record the channel in
    /// `runtime_channels` so `list_own_groups`, `channel_for`, and
    /// `send_envelope` can find it. Without the R23c fix, the
    /// `runtime_channels` field was added but never populated.
    ///
    /// Flow under test:
    ///   1. `join_by_invite` validates the channel name first
    ///      (N2 fix) — no socket I/O attempted for a malformed name.
    ///   2. `send_raw_line("JOIN #beta")` is called. This *does*
    ///      spawn the listener via `ensure_connected`; the listener
    ///      then loops on connect-refused (port 1) but holds the
    ///      `out_rx` so the mpsc buffer accepts the line (capacity
    ///      128). `send_raw_line` returns `Ok(())`.
    ///   3. Only after `send_raw_line` succeeds does the adapter
    ///      push `#beta` to `runtime_channels`. The ordering means
    ///      a failed send would *not* pollute the runtime list.
    ///
    /// This test exercises that ordering: the listener is alive (so
    /// send succeeds), the push happens, and `list_own_groups` sees
    /// both the configured `#alpha` and the runtime-joined `#beta`.
    #[tokio::test]
    async fn test_join_by_invite_records_runtime_channel() {
        let adapter = IrcAdapter::new(IrcConfig {
            server: "127.0.0.1".into(),
            port: 1, // refused: listener stays alive but never connects
            nickname: "testbot".into(),
            channels: vec!["#alpha".into()],
            password: None,
            use_tls: false,
        });
        // Pre-condition: list_own_groups only sees the configured channel.
        let pre = adapter.list_own_groups().await.unwrap();
        assert_eq!(pre.len(), 1, "expected one configured channel");
        assert_eq!(pre[0].id.as_str(), "127.0.0.1:#alpha");

        // Join a new channel. `send_raw_line` enqueues `JOIN #beta`
        // in the mpsc buffer (the listener holds `out_rx` and is
        // busy in the connect-refused retry loop), returns Ok, and
        // then we push `#beta` into `runtime_channels`.
        let _ = adapter.join_by_invite(&InviteRef::new("#beta")).await;

        // Now `list_own_groups` should see both #alpha (configured)
        // and #beta (runtime-joined).
        let post = adapter.list_own_groups().await.unwrap();
        let ids: Vec<String> = post.iter().map(|g| g.id.to_string()).collect();
        assert!(
            ids.iter().any(|s| s == "127.0.0.1:#alpha"),
            "configured channel must remain visible: {ids:?}"
        );
        assert!(
            ids.iter().any(|s| s == "127.0.0.1:#beta"),
            "runtime-joined channel must be visible after join_by_invite: {ids:?}"
        );
        assert_eq!(post.len(), 2, "expected exactly 2 channels, got: {ids:?}");

        // `channel_for` should accept the runtime channel.
        let resolved = adapter
            .channel_for(&GroupId::new("127.0.0.1:#beta"))
            .unwrap();
        assert_eq!(resolved, "#beta");

        // Tidy up so the listener doesn't outlive the test runtime.
        adapter.shutdown().await.unwrap();
    }

    /// N1: `list_own_groups` deduplicates when a runtime-joined
    /// channel is already in the static config.
    #[tokio::test]
    async fn test_list_own_groups_dedupes_static_and_runtime() {
        let adapter = IrcAdapter::new(IrcConfig {
            server: "127.0.0.1".into(),
            port: 1,
            nickname: "testbot".into(),
            channels: vec!["#alpha".into(), "#beta".into()],
            password: None,
            use_tls: false,
        });
        // Re-join a channel that's already in the config. The
        // runtime_channels vec should NOT grow.
        let _ = adapter.join_by_invite(&InviteRef::new("#alpha")).await;
        let post = adapter.list_own_groups().await.unwrap();
        assert_eq!(post.len(), 2, "expected dedup; got {} channels", post.len());

        // Tidy up the spawned listener.
        adapter.shutdown().await.unwrap();
    }

    /// N2: `join_by_invite` must reject the IRC "JOIN 0" special
    /// token before sending it to the server. Without the
    /// validation, `JOIN 0` would PART the bot from every channel
    /// it's in.
    #[tokio::test]
    async fn test_join_by_invite_rejects_join_zero() {
        let adapter = IrcAdapter::new(IrcConfig {
            server: "127.0.0.1".into(),
            port: 1,
            nickname: "testbot".into(),
            channels: vec![],
            password: None,
            use_tls: false,
        });
        for bad in ["0", "#0", "&0", "+0", "!0"] {
            let err = adapter
                .join_by_invite(&InviteRef::new(bad.to_string()))
                .await
                .unwrap_err();
            assert!(
                matches!(err, PlatformAdapterError::ApiError { code: 400, .. }),
                "expected 400 ApiError for {bad:?}, got: {err:?}"
            );
            // Sanity: the channel must NOT have been recorded.
            let groups = adapter.list_own_groups().await.unwrap();
            assert!(
                !groups.iter().any(|g| g.id.as_str().ends_with(bad)),
                "rejected channel {bad:?} must not appear in list_own_groups: {:?}",
                groups.iter().map(|g| g.id.to_string()).collect::<Vec<_>>()
            );
        }
    }

    /// N2: `join_by_invite` must reject empty channel names and
    /// names that don't start with `#`, `&&`, `+`, or `!`.
    #[tokio::test]
    async fn test_join_by_invite_rejects_malformed_channel_names() {
        let adapter = IrcAdapter::new(IrcConfig {
            server: "127.0.0.1".into(),
            port: 1,
            nickname: "testbot".into(),
            channels: vec![],
            password: None,
            use_tls: false,
        });
        // Empty InviteRef is now rejected at the constructor
        // (RFC-0861 M2 debug_assert). Skip it here — the IRC
        // adapter's `channel_for` rejection still covers the
        // remaining malformed cases. Use `try_new` so the empty
        // path can still be exercised without tripping the
        // constructor's debug_assert.
        for bad in [
            "no-prefix",
            "#chan with space",
            "#chan,multi",
            "#chan\0bad",
            "#chan:colon",
        ] {
            let err = adapter
                .join_by_invite(&InviteRef::new(bad.to_string()))
                .await
                .unwrap_err();
            assert!(
                matches!(err, PlatformAdapterError::ApiError { code: 400, .. }),
                "expected 400 ApiError for {bad:?}, got: {err:?}"
            );
        }
        // Empty-input constructor path: verify `try_new` returns None.
        assert!(InviteRef::try_new("").is_none());
        assert!(InviteRef::try_new("non-empty").is_some());
    }

    /// N2 (free function): `validate_channel_name` returns Ok for
    /// well-formed names and Err for malformed ones.
    #[test]
    fn test_validate_channel_name_free_function() {
        // OK cases
        assert!(validate_channel_name("#cipherocto").is_ok());
        assert!(validate_channel_name("&local").is_ok());
        assert!(validate_channel_name("+modeless").is_ok());
        assert!(validate_channel_name("!safe").is_ok());

        // Err cases
        assert!(validate_channel_name("").is_err());
        assert!(validate_channel_name("0").is_err());
        assert!(validate_channel_name("#0").is_err());
        assert!(validate_channel_name("no-prefix").is_err());
        assert!(validate_channel_name("#chan space").is_err());
        assert!(validate_channel_name("#chan,multi").is_err());
        assert!(validate_channel_name("#chan:colon").is_err());
        assert!(validate_channel_name("#chan\0bad").is_err());
    }

    /// N9: `IrcConfig::validate` rejects server names containing
    /// whitespace, `/`, control characters, or empty labels.
    #[test]
    fn test_irc_config_validate_rejects_bad_server_names() {
        let mut config = IrcConfig {
            server: "irc.example.org".into(),
            port: 6697,
            nickname: "testbot".into(),
            channels: vec!["#test".into()],
            password: None,
            use_tls: true,
        };
        assert!(config.validate().is_ok(), "baseline must validate");

        config.server = "".into();
        assert!(config.validate().is_err(), "empty server must fail");
        config.server = "  ".into();
        assert!(config.validate().is_err(), "whitespace server must fail");
        config.server = "host with spaces".into();
        assert!(config.validate().is_err(), "spaces in server must fail");
        config.server = "path/to/nowhere".into();
        assert!(config.validate().is_err(), "/ in server must fail");
        config.server = "host\0bad".into();
        assert!(config.validate().is_err(), "NUL in server must fail");
        config.server = "irc\twith\ttab".into();
        assert!(config.validate().is_err(), "tab in server must fail");
        // R23e N19: empty labels (..) are forbidden by RFC-952.
        config.server = "..".into();
        assert!(config.validate().is_err(), "empty-label server must fail");
        config.server = "irc.example.com..".into();
        assert!(
            config.validate().is_err(),
            "trailing empty-label server must fail"
        );
    }

    /// N5: `max_payload_for_channel` returns smaller values for
    /// longer channel names, so the assembled PRIVMSG line stays
    /// within the 512-byte IRC limit.
    #[test]
    fn test_max_payload_for_channel_shrinks_with_longer_names() {
        // "PRIVMSG " (8) + channel + " :" (2) + CRLF (2) = 12 + channel.len()
        let baseline = max_payload_for_channel("#alpha");
        assert!(baseline <= IRC_MAX_LINE_BYTES - 12 - "#alpha".len());
        assert!(baseline > 0, "even short channels must allow >0 bytes");

        let long = max_payload_for_channel("#a-very-long-channel-name-for-a-specific-purpose");
        assert!(
            long < baseline,
            "longer channel ({}) should give smaller payload: {long} vs {baseline}",
            long
        );

        // The exact formula: any assembled line must fit in 512.
        let ch = "#a-very-long-channel-name-for-a-specific-purpose";
        let payload = max_payload_for_channel(ch);
        let line = format!("PRIVMSG {ch} :{}{}", "x".repeat(payload), "\r\n");
        assert!(
            line.len() <= IRC_MAX_LINE_BYTES,
            "assembled line len {} > IRC_MAX_LINE_BYTES {}",
            line.len(),
            IRC_MAX_LINE_BYTES
        );
    }

    /// N3 + N14: `shutdown` must drop the outbound sender, signal the
    /// stop channel, and (after R23e) make `ensure_connected` a
    /// *hard* no-op. We verify:
    ///
    ///   - the post-shutdown state is clean (`connected` is false,
    ///     `out_tx` is `None`, `shutdown_tx` is `None`,
    ///     `listener_handle` is `None`),
    ///   - a subsequent `ensure_connected` returns `Err` (the
    ///     `shutting_down` flag is set, so respawn is refused —
    ///     callers must construct a fresh `IrcAdapter`),
    ///   - `send_raw_line` after shutdown surfaces the error
    ///     (not a silent respawn, not a hang).
    #[tokio::test(flavor = "current_thread")]
    async fn test_shutdown_prevents_respawn() {
        // Point at a non-existent server so the listener
        // task is spawned but immediately loops on connect
        // failure. The shutdown should still kill it.
        let adapter = IrcAdapter::new(IrcConfig {
            server: "127.0.0.1".into(),
            port: 1, // refused
            nickname: "testbot".into(),
            channels: vec!["#alpha".into()],
            password: None,
            use_tls: false,
        });

        // First call spawns the listener.
        adapter.ensure_connected().await.unwrap();
        // Give the spawn a tick.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        adapter.shutdown().await.unwrap();

        // After shutdown: connected should be false.
        let connected = *adapter.connected.lock().await;
        assert!(!connected, "shutdown must reset connected=false");

        // The outbound sender should be cleared.
        let out_tx_empty = adapter.out_tx.lock().await.is_none();
        assert!(out_tx_empty, "shutdown must clear out_tx");

        // The shutdown sender should be cleared (taken).
        let shutdown_tx_empty = adapter.shutdown_tx.lock().await.is_none();
        assert!(shutdown_tx_empty, "shutdown must take shutdown_tx");

        // The JoinHandle should be cleared (taken).
        let handle_empty = adapter.listener_handle.lock().await.is_none();
        assert!(handle_empty, "shutdown must take listener_handle");

        // R23e N14: the shutting_down flag must be set, so a
        // subsequent ensure_connected refuses to respawn.
        assert!(
            adapter
                .shutting_down
                .load(std::sync::atomic::Ordering::SeqCst),
            "shutdown must set shutting_down flag"
        );

        // N14: ensure_connected after shutdown returns Err
        // (hard shutdown — no soft recovery).
        let err = adapter.ensure_connected().await.unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("shut down"),
            "post-shutdown ensure_connected must return 'shut down' error, got: {msg}"
        );

        // N14: send_raw_line surfaces the failure (via
        // ensure_connected's Err) rather than hanging or respawning.
        let err = adapter
            .send_raw_line("KICK #alpha alice :removed by coordinator")
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("shut down"),
            "post-shutdown send_raw_line must surface 'shut down' error, got: {msg}"
        );
    }

    /// R23f N21: `ensure_connected` and `shutdown` can race when
    /// called from two concurrent tasks. The previous (R23e)
    /// implementation had the `shutting_down` check *outside* the
    /// `connected` lock, which left a window where shutdown could
    /// set the flag and take None from `shutdown_tx` /
    /// `listener_handle` (because ensure_connected hadn't installed
    /// them yet), letting ensure_connected install them *after*
    /// shutdown had already done its work — leaving a zombie
    /// listener that shutdown couldn't abort and that nothing
    /// would ever signal (because shutdown_tx was re-installed).
    ///
    /// After the fix (check moved inside the lock), at most one of
    /// {spawn, shutdown} actually completes its work; the other
    /// observes a coherent state. We exercise this with
    /// `tokio::join!` so the two futures interleave at every
    /// `.await` point — exactly the scheduling that maximizes the
    /// race window. We repeat it many times so any unrecovered
    /// interleaving would surface as a failure.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_ensure_connected_shutdown_race_no_zombie() {
        for _ in 0..100 {
            let adapter = std::sync::Arc::new(IrcAdapter::new(IrcConfig {
                server: "127.0.0.1".into(),
                port: 1, // refused: the listener never reaches the wire
                nickname: "testbot".into(),
                channels: vec!["#alpha".into()],
                password: None,
                use_tls: false,
            }));

            // Fire ensure_connected and shutdown on separate
            // spawned tasks so they can run truly in parallel
            // across multiple worker threads. Without the R23f
            // N21 fix (shutdown acquires connected first AND
            // ensure_connected re-checks shutting_down inside
            // the lock), the interleaving can leave a zombie
            // listener: shutdown takes None for shutdown_tx /
            // listener_handle because ensure_connected hasn't
            // installed them yet, then ensure_connected
            // installs them *after* shutdown has finished.
            let a1 = adapter.clone();
            let a2 = adapter.clone();
            let h1 = tokio::spawn(async move { a1.ensure_connected().await });
            let h2 = tokio::spawn(async move { a2.shutdown().await });
            let _ = h1.await.unwrap();
            let _ = h2.await.unwrap();

            // Final state must be: shutting_down=true, and
            // listener_handle is None. Critically, the
            // listener_handle must NEVER be Some after
            // shutdown — that would mean ensure_connected
            // installed it *after* shutdown had already done
            // its work, i.e. the zombie case.
            assert!(
                adapter
                    .shutting_down
                    .load(std::sync::atomic::Ordering::SeqCst),
                "shutting_down must be true after shutdown"
            );
            let handle_still_present = adapter.listener_handle.lock().await.is_some();
            assert!(
                !handle_still_present,
                "shutdown must have taken the listener_handle (no zombie listener allowed); \
                 this means ensure_connected installed a handle AFTER shutdown took None"
            );

            // Tidy up: this is a no-op since the adapter is
            // already shut down, but it exercises the
            // idempotent-shutdown path.
            adapter.shutdown().await.unwrap();
        }
    }

    /// RFC-0861 §4 M8: a freshly-constructed adapter has
    /// `is_authenticated = false`, and `health_check` returns 503
    /// until the listener has confirmed 376/422. The contract
    /// is independent of the TCP path: even if a TCP probe
    /// would succeed, the adapter is not "healthy" from the
    /// caller's POV until the IRC handshake is observable.
    #[tokio::test]
    async fn health_check_returns_503_when_not_authenticated() {
        let cfg = IrcConfig {
            server: "irc.example.org".into(),
            port: 6697,
            nickname: "test".into(),
            channels: vec!["#test".into()],
            password: None,
            use_tls: true,
        };
        let adapter = IrcAdapter::new(cfg);
        // No listener has been spawned; is_authenticated is the
        // initial `false`.
        let result = adapter.health_check().await;
        match result {
            Ok(()) => {
                panic!("health_check on a fresh adapter must return Err(ApiError 503), not Ok(())")
            }
            Err(PlatformAdapterError::ApiError { code, message }) => {
                assert_eq!(code, 503, "code should be 503, got {code}");
                assert!(
                    message.contains("not authenticated"),
                    "message should mention not authenticated, got: {message}"
                );
            }
            Err(other) => panic!("expected ApiError 503, got {other:?}"),
        }
    }

    /// RFC-0861 §4 M8: a direct `is_authenticated.store(true, ...)`
    /// (simulating the listener seeing 376/422) is observable by
    /// `health_check`, which still must also verify TCP — but in
    /// this test we only flip the flag and rely on the TCP check
    /// failing (no IRC server listening at 127.0.0.1:1). The 503
    /// must NOT fire, since the flag is true; we expect the TCP
    /// check to fail with a transport error instead.
    #[tokio::test]
    async fn health_check_passes_auth_gate_when_is_authenticated_true() {
        let cfg = IrcConfig {
            server: "127.0.0.1".into(),
            port: 1, // nothing listens here
            nickname: "test".into(),
            channels: vec!["#test".into()],
            password: None,
            use_tls: true,
        };
        let adapter = IrcAdapter::new(cfg);
        adapter
            .is_authenticated
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let result = adapter.health_check().await;
        match result {
            Ok(()) => panic!(
                "health_check with port=1 (no listener) must fail, but the failure MUST NOT be the 503 'not authenticated' error"
            ),
            Err(PlatformAdapterError::ApiError { code, .. }) => {
                assert_ne!(
                    code, 503,
                    "the 503 'not authenticated' path must NOT fire when is_authenticated = true"
                );
            }
            Err(PlatformAdapterError::Unreachable { .. }) => {
                // Expected: TCP connect to 127.0.0.1:1 fails.
            }
            Err(_) => {
                // Other error types are fine too.
            }
        }
    }

    // ── M7 parse_numeric_reply tests ─────────────────────────────

    #[test]
    fn parse_numeric_reply_extracts_code_and_trailing() {
        // Standard RPL_INVITING (341) with trailing message.
        let nr = parse_numeric_reply(":irc.example.org 341 mynick #chan alice :has been invited")
            .expect("must parse 341");
        assert_eq!(nr.code, 341);
        assert_eq!(nr.command, "alice", "last positional before trailing");
        assert_eq!(nr.message, "has been invited");
    }

    #[test]
    fn parse_numeric_reply_handles_no_trailing() {
        // Numeric reply with no trailing ` :message` (some
        // servers do this for 482 ERR_CHANOPRIVSNEEDED).
        let nr = parse_numeric_reply(":irc.example.org 482 mynick #chan").expect("must parse 482");
        assert_eq!(nr.code, 482);
        assert_eq!(nr.command, "#chan", "last positional token");
        assert_eq!(nr.message, "");
    }

    #[test]
    fn parse_numeric_reply_rejects_non_numeric_lines() {
        // PRIVMSG-style lines must not parse as numerics.
        assert!(parse_numeric_reply("PING :irc.example.org").is_none());
        assert!(parse_numeric_reply(":nick!u@h PRIVMSG #chan :hi").is_none());
        // Missing leading colon.
        assert!(parse_numeric_reply("341 mynick #chan alice").is_none());
        // Non-numeric code.
        assert!(parse_numeric_reply(":server NOTACODE me :x").is_none());
    }

    /// RFC-0861 §4 M7: a fresh adapter has an empty
    /// `pending_invites` map, and `next_command_id` starts at 1
    /// (so the first allocated nonce is 1, not 0 — a small
    /// smell-check that the field is wired).
    #[tokio::test]
    async fn pending_invites_and_next_command_id_start_clean() {
        let cfg = IrcConfig {
            server: "irc.example.org".into(),
            port: 6697,
            nickname: "test".into(),
            channels: vec!["#test".into()],
            password: None,
            use_tls: true,
        };
        let adapter = IrcAdapter::new(cfg);
        let pending = adapter.pending_invites.lock().await;
        assert!(pending.is_empty(), "fresh adapter has no pending invites");
        drop(pending);
        let id = adapter
            .next_command_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(id, 1, "next_command_id should start at 1, got {id}");
    }

    /// RFC-0861 §4 M7: the FIFO correlation is correct.
    /// Insert two entries under different IDs, then call
    /// `pop_first()` and verify the smaller ID is removed
    /// first. This is the algorithm the listener uses to
    /// match a reply with the oldest pending send.
    #[tokio::test]
    async fn pending_invites_pop_first_is_fifo() {
        let cfg = IrcConfig {
            server: "irc.example.org".into(),
            port: 6697,
            nickname: "test".into(),
            channels: vec!["#test".into()],
            password: None,
            use_tls: true,
        };
        let adapter = IrcAdapter::new(cfg);
        let (tx_a, _rx_a) = oneshot::channel::<NumericResult>();
        let (tx_b, _rx_b) = oneshot::channel::<NumericResult>();
        {
            let mut pending = adapter.pending_invites.lock().await;
            // Insert in reverse-numerical order to confirm
            // BTreeMap orders by key, not insertion.
            pending.insert(20, tx_b);
            pending.insert(10, tx_a);
        }
        // Pop the first (smallest key = 10). The sender for
        // key 20 must still be present.
        let popped = {
            let mut pending = adapter.pending_invites.lock().await;
            pending.pop_first()
        };
        assert!(popped.is_some(), "expected first pop to succeed");
        let (popped_id, _popped_sender) = popped.unwrap();
        assert_eq!(
            popped_id, 10,
            "BTreeMap::pop_first must return the smallest key"
        );
        // The remaining entry (key 20) is still there.
        let remaining = adapter.pending_invites.lock().await;
        assert_eq!(remaining.len(), 1, "one entry should remain");
        assert!(
            remaining.contains_key(&20),
            "key 20 should be the remaining entry"
        );
    }
}
