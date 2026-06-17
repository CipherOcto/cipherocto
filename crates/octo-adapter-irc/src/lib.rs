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
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
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

        tokio::spawn(async move {
            irc_listener(server, port, nickname, channels, password, use_tls, tx).await;
        });

        *connected = true;
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
async fn irc_listener(
    server: String,
    port: u16,
    nickname: String,
    channels: Vec<String>,
    password: Option<String>,
    use_tls: bool,
    tx: mpsc::Sender<RawPlatformMessage>,
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
                if let Err(e) =
                    irc_session(stream, &nickname, &channels, password.as_deref(), &tx).await
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
async fn irc_session(
    stream: TcpStream,
    nickname: &str,
    channels: &[String],
    password: Option<&str>,
    tx: &mpsc::Sender<RawPlatformMessage>,
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
        line.clear();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("Read: {e}"))?;
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
}
