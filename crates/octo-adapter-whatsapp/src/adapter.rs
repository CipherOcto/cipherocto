//! WhatsApp Web adapter for DOT (RFC-0850 §8.1)
//!
//! Uses whatsapp-rust (native WhatsApp Web protocol) to transport DOT envelopes.
//! No Meta Business verification required — authentication via QR code or pair code.

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

use super::store::StoolapStore;

// ── Configuration ──────────────────────────────────────────────────

/// Configuration for the WhatsApp Web adapter
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct WhatsAppConfig {
    /// Path to the session database (stoolap)
    pub session_path: String,
    /// Phone number for pair code linking (optional)
    pub pair_phone: Option<String>,
    /// Custom pair code (optional)
    pub pair_code: Option<String>,
    /// Override WebSocket URL (test/proxy setups)
    pub ws_url: Option<String>,
    /// Group IDs to monitor for DOT envelopes
    pub groups: Vec<String>,
    /// Per-group sender allowlist (defense in depth for RFC-0850p-a v1.15 D-WA-10).
    ///
    /// Key: a group identifier (must match an entry in `groups`, either with
    ///      the explicit `@g.us` suffix or as the bare digits form).
    /// Value: list of E.164 phone numbers that are allowed to inject `DOT/1/...`
    ///        envelopes into this group. Normalized to digits-only at runtime,
    ///        so formatting (`+1 555 123 4567`, `15551234567`, etc.) is flexible.
    ///
    /// Semantics: if a group has no entry in this map, or its entry is an empty
    /// `Vec`, the legacy behavior applies: any current member of the WhatsApp
    /// group can inject a `DOT/1/...` envelope. This is backwards-compatible
    /// for existing configs that don't set the field.
    #[serde(default)]
    pub sender_allowlist: BTreeMap<String, Vec<String>>,
}

impl std::fmt::Debug for WhatsAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhatsAppConfig")
            .field("session_path", &self.session_path)
            .field(
                "pair_phone",
                &self.pair_phone.as_ref().map(|_| "<redacted>"),
            )
            .field("pair_code", &self.pair_code.as_ref().map(|_| "<redacted>"))
            .field("ws_url", &self.ws_url)
            .field("groups", &self.groups)
            .field(
                "sender_allowlist",
                &format!(
                    "<{} groups, {} total senders>",
                    self.sender_allowlist.len(),
                    self.sender_allowlist
                        .values()
                        .map(|v| v.len())
                        .sum::<usize>()
                ),
            )
            .finish()
    }
}

impl WhatsAppConfig {
    /// Validate the config in-memory.
    ///
    /// R1-H1: pure field-shape check (no filesystem I/O). Filesystem
    /// writability of `session_path` is a CLI pre-flight concern in
    /// `pair_link::run` / `qr_link::run`, not part of `validate()`.
    /// Modeled after `TelegramConfig::validate()` at
    /// `octo-adapter-telegram/src/config.rs:94-110`.
    ///
    /// Checks:
    /// - `pair_phone` is E.164 if set: `+` followed by 7-15 digits
    /// - `ws_url` starts with `ws://` or `wss://` if set
    /// - `groups` entries are non-empty strings (empty groups Vec is OK
    ///   — the operator may have no chats to monitor yet)
    pub fn validate(&self) -> std::result::Result<(), String> {
        if let Some(ref phone) = self.pair_phone {
            if !is_e164(phone) {
                return Err(format!(
                    "pair_phone {:?} is not a valid E.164 number (expected + followed by 7-15 digits)",
                    phone
                ));
            }
        }
        if let Some(ref ws_url) = self.ws_url {
            if !(ws_url.starts_with("ws://") || ws_url.starts_with("wss://")) {
                return Err(format!(
                    "ws_url {:?} must start with ws:// or wss://",
                    ws_url
                ));
            }
        }
        for group in &self.groups {
            if group.is_empty() {
                return Err("groups contains an empty string".to_string());
            }
        }
        Ok(())
    }
}

/// E.164 validation: `+` followed by 7-15 ASCII digits, no leading 0 after `+`.
fn is_e164(phone: &str) -> bool {
    if !phone.starts_with('+') {
        return false;
    }
    let digits = &phone[1..];
    if digits.is_empty() || digits.len() < 7 || digits.len() > 15 {
        return false;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if digits.starts_with('0') {
        return false;
    }
    true
}

// ── Reconnect constants ────────────────────────────────────────────

const MAX_RETRIES: u32 = 10;
const BASE_DELAY_SECS: u64 = 3;
const MAX_DELAY_SECS: u64 = 300;

fn compute_retry_delay(attempt: u32) -> u64 {
    std::cmp::min(
        BASE_DELAY_SECS.saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1))),
        MAX_DELAY_SECS,
    )
}

// ── Helper functions ───────────────────────────────────────────────

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "whatsapp".into(),
        reason: msg.into(),
    }
}

// ── WhatsAppWebAdapter ─────────────────────────────────────────────

/// Per-message accept decision for the inbound on_event handler.
///
/// Returned by [`WhatsAppWebAdapter::accept_message`]. Distinguishes the
/// security-relevant rejection (sender not in the per-group allowlist)
/// from the routine filtering rejections (empty text, unconfigured
/// group, not a DOT envelope). Only the first is logged via
/// `tracing::warn!` at the call site; the others are silent to preserve
/// the existing behavior.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum AcceptDecision {
    Accept,
    Reject { reason: &'static str },
}

/// WhatsApp Web adapter implementing DOT PlatformAdapter
pub struct WhatsAppWebAdapter {
    config: WhatsAppConfig,
    /// Bot handle for shutdown
    bot_handle: Arc<Mutex<Option<whatsapp_rust::bot::BotHandle>>>,
    /// Client for sending messages
    client: Arc<Mutex<Option<Arc<whatsapp_rust::Client>>>>,
    /// Internal message buffer: on_event() pushes, receive_messages() drains
    inbound_rx: Arc<Mutex<tokio::sync::mpsc::Receiver<RawPlatformMessage>>>,
    inbound_tx: tokio::sync::mpsc::Sender<RawPlatformMessage>,
    /// Bot's own phone number (resolved on connect)
    self_phone: Arc<Mutex<Option<String>>>,
}

impl WhatsAppWebAdapter {
    pub fn new(config: WhatsAppConfig) -> Self {
        let (inbound_tx, inbound_rx) = tokio::sync::mpsc::channel(1024);
        Self {
            config,
            bot_handle: Arc::new(Mutex::new(None)),
            client: Arc::new(Mutex::new(None)),
            inbound_rx: Arc::new(Mutex::new(inbound_rx)),
            inbound_tx,
            self_phone: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: WhatsAppConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        Ok(Self::new(config))
    }

    /// Domain hash: `BLAKE3-256("whatsapp:{group_id}")`
    pub fn domain_hash(group_id: &str) -> [u8; 32] {
        let normalized = group_id.trim().to_lowercase();
        *blake3::hash(format!("whatsapp:{}", normalized).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0008;
    pub fn max_payload_bytes() -> usize {
        65_536
    }
    pub fn rate_limit_per_second() -> u32 {
        20
    }

    /// Encode an envelope as base64 with DOT/1/ prefix.
    pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope_bytes)
        )
    }

    /// Decode a DOT/1/-prefixed base64 envelope.
    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let text = text.trim();
        let b64 = text
            .strip_prefix("DOT/1/")
            .ok_or_else(|| "Missing DOT/1/ prefix".to_string())?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64 decode error: {}", e))
    }

    /// Normalize phone number to digits-only
    fn normalize_phone(phone: &str) -> String {
        phone.chars().filter(|c| c.is_ascii_digit()).collect()
    }

    /// Convert a group ID to a WhatsApp group JID
    fn group_to_jid(group_id: &str) -> String {
        if group_id.contains('@') {
            group_id.to_string()
        } else {
            format!("{}@g.us", group_id)
        }
    }

    /// Pure accept-reject decision for an inbound message.
    ///
    /// Encapsulates the filtering logic that was previously inlined at
    /// `adapter.rs:261-280` (the legacy "anyone in a configured group can
    /// inject a DOT/1/... envelope" check) and adds the optional per-group
    /// sender allowlist (defense in depth for the D-WA-10 gap documented
    /// in RFC-0850p-a v1.15 §Adversary Analysis).
    ///
    /// Backwards-compatible: when `sender_allowlist` has no entry for the
    /// configured group, or the entry is an empty `Vec`, the legacy
    /// "anyone in the group can inject" behavior applies.
    ///
    /// Pure: no I/O, no logging, no side effects. Safe to call from tests.
    pub(crate) fn accept_message(
        chat_jid: &str,
        sender: &str,
        text: &str,
        groups: &[String],
        sender_allowlist: &BTreeMap<String, Vec<String>>,
    ) -> AcceptDecision {
        let text_trimmed = text.trim();
        if text_trimmed.is_empty() {
            return AcceptDecision::Reject {
                reason: "empty text",
            };
        }

        // Match against configured groups (preserves the `<digits>@g.us` JID
        // hack from the original code: a chat JID like `1234567890:0@s.whatsapp.net`
        // matches a configured group `1234567890@g.us` because the latter's
        // `@g.us` suffix is 4 chars and `1234567890` is 10 chars, so the
        // `starts_with` check on the 10-char prefix succeeds).
        let configured_group = groups.iter().find(|g| {
            let jid = Self::group_to_jid(g);
            chat_jid == jid || (jid.len() >= 4 && chat_jid.starts_with(&jid[..jid.len() - 4]))
        });
        let group_id = match configured_group {
            Some(g) => g,
            None => {
                return AcceptDecision::Reject {
                    reason: "unconfigured group",
                }
            }
        };

        if !text_trimmed.starts_with("DOT/1/") {
            return AcceptDecision::Reject {
                reason: "not a DOT envelope",
            };
        }

        // Per-group sender allowlist. Empty allowlist = legacy behavior.
        if let Some(allowed) = sender_allowlist.get(group_id) {
            if !allowed.is_empty() {
                let sender_normalized = Self::normalize_phone(sender);
                let is_allowed = allowed
                    .iter()
                    .any(|p| Self::normalize_phone(p) == sender_normalized);
                if !is_allowed {
                    return AcceptDecision::Reject {
                        reason: "sender not in allowlist",
                    };
                }
            }
        }

        AcceptDecision::Accept
    }

    /// Start the WhatsApp Web bot in a background task.
    pub async fn start_bot(&self) -> Result<()> {
        let expanded_path = shellexpand::tilde(&self.config.session_path).to_string();
        let storage = StoolapStore::new(&expanded_path)
            .map_err(|e| anyhow::anyhow!("stoolap store init at {expanded_path:?}: {e:#}"))?;
        let backend = Arc::new(storage);

        // Create transport factory
        let mut transport_factory =
            whatsapp_rust_tokio_transport::TokioWebSocketTransportFactory::new();
        if let Some(ref ws_url) = self.config.ws_url {
            transport_factory = transport_factory.with_url(ws_url.clone());
        }

        let http_client = whatsapp_rust_ureq_http_client::UreqHttpClient::new();

        // Clone values for the event handler
        let inbound_tx = self.inbound_tx.clone();
        let self_phone = self.self_phone.clone();
        let groups = self.config.groups.clone();
        let sender_allowlist = self.config.sender_allowlist.clone();

        // Build the bot
        let mut builder = whatsapp_rust::bot::Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .with_runtime(whatsapp_rust::TokioRuntime)
            .with_device_props(
                wacore::store::DevicePropsOverride::new()
                    .with_os("CipherOcto")
                    .with_platform_type(waproto::whatsapp::device_props::PlatformType::Desktop),
            )
            .on_event(move |event, client| {
                let inbound_tx = inbound_tx.clone();
                let self_phone = self_phone.clone();
                let groups = groups.clone();
                let sender_allowlist = sender_allowlist.clone();

                async move {
                    use wacore::proto_helpers::MessageExt;
                    use wacore::types::events::Event;

                    match &*event {
                        Event::Message(msg, info) => {
                            let text = msg.text_content().unwrap_or("").to_string();
                            let chat = info.source.chat.to_string();
                            let sender = info.source.sender.to_string();

                            let decision = Self::accept_message(
                                &chat,
                                &sender,
                                &text,
                                &groups,
                                &sender_allowlist,
                            );

                            // Emit a single warn! for the security-relevant
                            // rejection (D-WA-10 mitigation). Routine filtering
                            // rejections remain silent to preserve the existing
                            // log volume behavior.
                            if let AcceptDecision::Reject {
                                reason: "sender not in allowlist",
                            } = &decision
                            {
                                tracing::warn!(
                                    "rejecting DOT envelope: non-allowlisted sender {} in {}",
                                    Self::normalize_phone(&sender),
                                    chat,
                                );
                            }
                            if !matches!(decision, AcceptDecision::Accept) {
                                return;
                            }

                            let raw = RawPlatformMessage {
                                platform_id: format!("{}:{}", chat, uuid::Uuid::new_v4()),
                                payload: text.into_bytes(),
                                metadata: [
                                    ("chat".to_string(), chat),
                                    ("sender".to_string(), sender),
                                ]
                                .into_iter()
                                .collect(),
                            };
                            if let Err(e) = inbound_tx.try_send(raw) {
                                tracing::warn!("inbound channel full or closed: {e}");
                            }
                        }
                        Event::Connected(_) => {
                            let device = client.persistence_manager().get_device_snapshot().await;
                            if let Some(ref pn) = device.pn {
                                let pn_str = pn.to_string();
                                let user_part = pn_str.split_once('@').map(|(u, _)| u).unwrap_or(&pn_str);
                                let digits = Self::normalize_phone(user_part);
                                if !digits.is_empty() {
                                    *self_phone.lock() = Some(digits);
                                    tracing::info!("resolved bot identity: +{user_part}");
                                }
                            }
                        }
                        Event::LoggedOut(_) => { tracing::warn!("WhatsApp Web logged out"); }
                        Event::PairingQrCode { code, .. } => {
                            match qrcode::QrCode::new(code.as_bytes()) {
                                Ok(qr) => {
                                    let rendered = qr.render::<qrcode::render::unicode::Dense1x2>().quiet_zone(true).build();
                                    eprintln!("\nWhatsApp Web QR code (scan in WhatsApp > Linked Devices):\n{rendered}\n");
                                }
                                Err(e) => { eprintln!("\nWhatsApp QR payload: {code}\n(failed to render: {e})\n"); }
                            }
                        }
                        Event::PairingCode { code, .. } => {
                            eprintln!("\nWhatsApp pair code: {code}");
                            eprintln!("Enter this in WhatsApp > Linked Devices\n");
                        }
                        Event::StreamError(err) => { tracing::error!("WhatsApp stream error: {err:?}"); }
                        _ => {}
                    }
                }
            });

        if let Some(ref phone) = self.config.pair_phone {
            builder = builder.with_pair_code(whatsapp_rust::pair_code::PairCodeOptions {
                phone_number: phone.clone(),
                custom_code: self.config.pair_code.clone(),
                ..Default::default()
            });
        }

        let mut bot = builder.build().await?;
        *self.client.lock() = Some(bot.client());

        // Run the bot in a background task so start_bot() returns immediately
        let bot_handle = bot.run().await?;
        *self.bot_handle.lock() = Some(bot_handle);

        tracing::info!("WhatsApp Web bot started");
        Ok(())
    }

    /// Run the reconnect loop (blocking). Call this after start_bot().
    pub async fn run_reconnect_loop(&self) {
        let mut retry_count: u32 = 0;

        loop {
            // Wait for the bot to stop (logout or error)
            // The bot runs in start_bot(), this just handles reconnection
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            // Check if bot is still alive
            let bot_alive = self.bot_handle.lock().is_some();
            if bot_alive {
                continue;
            }

            // Bot stopped — attempt reconnect
            retry_count += 1;
            if retry_count > MAX_RETRIES {
                tracing::error!("exceeded {MAX_RETRIES} reconnect attempts, giving up");
                break;
            }

            let delay = compute_retry_delay(retry_count);
            tracing::info!("reconnecting in {delay}s (attempt {retry_count}/{MAX_RETRIES})");
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;

            // Clear state
            *self.client.lock() = None;
            *self.bot_handle.lock() = None;

            // Attempt restart
            match self.start_bot().await {
                Ok(()) => {
                    retry_count = 0;
                    tracing::info!("reconnected successfully");
                }
                Err(e) => {
                    tracing::error!("reconnect failed: {e}");
                }
            }
        }
    }
}

// ── PlatformAdapter ────────────────────────────────────────────────

#[async_trait]
impl PlatformAdapter for WhatsAppWebAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Clone client Arc to avoid holding mutex guard across await
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| transport_err("WhatsApp Web client not connected"))?
        };

        let wire_bytes = envelope.to_wire_bytes();
        let encoded = Self::encode_envelope(&wire_bytes);

        // Find the group JID for this domain
        let group_id = self
            .config
            .groups
            .iter()
            .find(|g| Self::domain_hash(g) == domain.domain_hash)
            .ok_or_else(|| {
                transport_err(format!(
                    "No group found for domain {:?}",
                    domain.domain_hash
                ))
            })?;

        let jid = Self::group_to_jid(group_id);
        let to: wacore_binary::jid::Jid = jid
            .parse()
            .map_err(|e| transport_err(format!("Invalid JID {jid}: {e}")))?;

        let outgoing = waproto::whatsapp::Message {
            conversation: Some(encoded),
            ..Default::default()
        };

        let send_result = Box::pin(client.send_message(to, outgoing))
            .await
            .map_err(|e| transport_err(format!("send_message failed: {e}")))?;

        Ok(DeliveryReceipt {
            platform_message_id: send_result.message_id,
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let mut messages = Vec::new();

        // Drain all immediately available messages (non-blocking)
        let mut rx = self.inbound_rx.lock();
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

        // Extract text from payload bytes
        let text = String::from_utf8_lossy(&raw.payload);

        // Decode DOT/1/ envelope
        let wire_bytes =
            Self::decode_envelope(&text).map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            })?;

        DeterministicEnvelope::from_wire_bytes(&wire_bytes).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: false,
            supports_encryption: true, // Signal Protocol via whatsapp-rust
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::WhatsApp, platform_id)
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::WhatsApp
    }

    fn self_handle(&self) -> Option<String> {
        self.self_phone.lock().clone()
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        let handle = self.bot_handle.lock();
        if handle.is_some() {
            Ok(())
        } else {
            Err(transport_err("WhatsApp Web bot not running"))
        }
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        // Abort the bot handle
        let handle = self.bot_handle.lock().take();
        if let Some(h) = handle {
            h.abort();
            let _ = h.await;
        }

        // Clear client
        *self.client.lock() = None;
        *self.self_phone.lock() = None;

        tracing::info!("WhatsApp Web adapter shut down");
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = WhatsAppWebAdapter::domain_hash("group-id-1");
        let h2 = WhatsAppWebAdapter::domain_hash("group-id-1");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            WhatsAppWebAdapter::domain_hash("GROUP-ID-1"),
            WhatsAppWebAdapter::domain_hash("  group-id-1  ")
        );
    }

    #[test]
    fn test_encode_decode_envelope() {
        let original = b"test whatsapp envelope";
        let encoded = WhatsAppWebAdapter::encode_envelope(original);
        assert!(encoded.starts_with("DOT/1/"));
        let decoded = WhatsAppWebAdapter::decode_envelope(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(WhatsAppWebAdapter::PLATFORM_TYPE, 0x0008);
    }

    #[test]
    fn test_group_to_jid() {
        assert_eq!(
            WhatsAppWebAdapter::group_to_jid("120363012345678901"),
            "120363012345678901@g.us"
        );
        assert_eq!(
            WhatsAppWebAdapter::group_to_jid("120363012345678901@g.us"),
            "120363012345678901@g.us"
        );
    }

    #[test]
    fn test_normalize_phone() {
        assert_eq!(
            WhatsAppWebAdapter::normalize_phone("+1 (555) 123-4567"),
            "15551234567"
        );
        assert_eq!(
            WhatsAppWebAdapter::normalize_phone("15551234567@s.whatsapp.net"),
            "15551234567"
        );
    }

    #[test]
    fn test_compute_retry_delay() {
        let expected = [3, 6, 12, 24, 48, 96, 192, 300, 300, 300];
        for (i, &want) in expected.iter().enumerate() {
            let attempt = (i + 1) as u32;
            assert_eq!(compute_retry_delay(attempt), want, "attempt {attempt}");
        }
    }

    #[test]
    fn test_compute_retry_delay_zero() {
        assert_eq!(compute_retry_delay(0), BASE_DELAY_SECS);
    }

    #[test]
    fn test_capabilities() {
        let config = WhatsAppConfig {
            session_path: "/tmp/test.db".into(),
            pair_phone: None,
            pair_code: None,
            ws_url: None,
            groups: vec![],
            sender_allowlist: BTreeMap::new(),
        };
        let adapter = WhatsAppWebAdapter::new(config);
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 65_536);
        assert!(!caps.supports_fragmentation);
        assert!(caps.supports_encryption);
        assert_eq!(caps.rate_limit_per_second, 20);
    }

    #[tokio::test]
    async fn test_health_check_not_running() {
        let config = WhatsAppConfig {
            session_path: "/tmp/test.db".into(),
            pair_phone: None,
            pair_code: None,
            ws_url: None,
            groups: vec![],
            sender_allowlist: BTreeMap::new(),
        };
        let adapter = WhatsAppWebAdapter::new(config);
        assert!(adapter.health_check().await.is_err());
    }

    #[test]
    fn test_self_handle_none_when_not_connected() {
        let config = WhatsAppConfig {
            session_path: "/tmp/test.db".into(),
            pair_phone: None,
            pair_code: None,
            ws_url: None,
            groups: vec![],
            sender_allowlist: BTreeMap::new(),
        };
        let adapter = WhatsAppWebAdapter::new(config);
        assert!(adapter.self_handle().is_none());
    }

    #[test]
    fn test_decode_envelope_missing_prefix() {
        assert!(WhatsAppWebAdapter::decode_envelope("hello world").is_err());
    }

    #[test]
    fn test_decode_envelope_invalid_base64() {
        assert!(WhatsAppWebAdapter::decode_envelope("DOT/1/!!!invalid!!!").is_err());
    }

    // ── WhatsAppConfig::validate() tests (R1 mission AC) ────────

    fn cfg_with(
        session_path: &str,
        pair_phone: Option<&str>,
        pair_code: Option<&str>,
        ws_url: Option<&str>,
        groups: Vec<&str>,
    ) -> WhatsAppConfig {
        WhatsAppConfig {
            session_path: session_path.to_string(),
            pair_phone: pair_phone.map(str::to_string),
            pair_code: pair_code.map(str::to_string),
            ws_url: ws_url.map(str::to_string),
            groups: groups.into_iter().map(str::to_string).collect(),
            sender_allowlist: BTreeMap::new(),
        }
    }

    fn cfg_with_allowlist(
        session_path: &str,
        groups: Vec<&str>,
        allowlist: &[(&str, &[&str])],
    ) -> WhatsAppConfig {
        let mut cfg = cfg_with(session_path, None, None, None, groups);
        for (group, senders) in allowlist {
            cfg.sender_allowlist.insert(
                group.to_string(),
                senders.iter().map(|s| s.to_string()).collect(),
            );
        }
        cfg
    }

    #[test]
    fn validate_accepts_valid_config_with_all_fields() {
        let cfg = cfg_with(
            "/tmp/test.db",
            Some("+15551234567"),
            None,
            Some("wss://example.com"),
            vec!["120363012345678901@g.us"],
        );
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_accepts_minimal_config() {
        let cfg = cfg_with("/tmp/test.db", None, None, None, vec![]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_malformed_phone() {
        for bad in [
            "5551234",        // no leading +
            "+0123456789",    // leading 0 after +
            "+1-555-1234567", // non-digit
            "+",              // no digits
            "+abcdefg",       // non-digit
        ] {
            let cfg = cfg_with("/tmp/test.db", Some(bad), None, None, vec![]);
            assert!(cfg.validate().is_err(), "phone {bad:?} should be rejected");
        }
    }

    #[test]
    fn validate_rejects_malformed_ws_url() {
        for bad in ["http://example.com", "ftp://example.com", "example.com"] {
            let cfg = cfg_with("/tmp/test.db", None, None, Some(bad), vec![]);
            assert!(cfg.validate().is_err(), "ws_url {bad:?} should be rejected");
        }
    }

    #[test]
    fn validate_accepts_ws_and_wss() {
        for good in ["ws://localhost:8080", "wss://example.com"] {
            let cfg = cfg_with("/tmp/test.db", None, None, Some(good), vec![]);
            assert!(cfg.validate().is_ok(), "ws_url {good:?} should be accepted");
        }
    }

    #[test]
    fn validate_rejects_empty_group() {
        let cfg = cfg_with("/tmp/test.db", None, None, None, vec!["valid", ""]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_accepts_empty_groups_vec() {
        // R1-L1: empty groups Vec is OK; the operator may have no chats
        // to monitor yet; groups can be added later by editing the config.
        let cfg = cfg_with("/tmp/test.db", None, None, None, vec![]);
        assert!(cfg.validate().is_ok());
    }

    // ── Sender allowlist tests (D-WA-10 mitigation) ─────────────

    #[test]
    fn accept_message_accepts_configured_group_without_allowlist() {
        // Legacy behavior: no allowlist entry means anyone in the group can inject.
        let cfg = cfg_with(
            "/tmp/test.db",
            None,
            None,
            None,
            vec!["120363012345678901@g.us"],
        );
        let groups = cfg.groups.clone();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "+15551234567@s.whatsapp.net",
            "DOT/1/abc",
            &groups,
            &cfg.sender_allowlist,
        );
        assert_eq!(decision, AcceptDecision::Accept);
    }

    #[test]
    fn accept_message_rejects_unconfigured_group() {
        let cfg = cfg_with(
            "/tmp/test.db",
            None,
            None,
            None,
            vec!["120363012345678901@g.us"],
        );
        let groups = cfg.groups.clone();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363099999999999@g.us", // different group
            "+15551234567@s.whatsapp.net",
            "DOT/1/abc",
            &groups,
            &cfg.sender_allowlist,
        );
        assert_eq!(
            decision,
            AcceptDecision::Reject {
                reason: "unconfigured group"
            }
        );
    }

    #[test]
    fn accept_message_rejects_non_dot_envelope() {
        let cfg = cfg_with(
            "/tmp/test.db",
            None,
            None,
            None,
            vec!["120363012345678901@g.us"],
        );
        let groups = cfg.groups.clone();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "+15551234567@s.whatsapp.net",
            "not a dot envelope",
            &groups,
            &cfg.sender_allowlist,
        );
        assert_eq!(
            decision,
            AcceptDecision::Reject {
                reason: "not a DOT envelope"
            }
        );
    }

    #[test]
    fn accept_message_accepts_allowlisted_sender() {
        let cfg = cfg_with_allowlist(
            "/tmp/test.db",
            vec!["120363012345678901@g.us"],
            &[("120363012345678901@g.us", &["+15551234567"])],
        );
        let groups = cfg.groups.clone();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "+15551234567@s.whatsapp.net",
            "DOT/1/abc",
            &groups,
            &cfg.sender_allowlist,
        );
        assert_eq!(decision, AcceptDecision::Accept);
    }

    #[test]
    fn accept_message_rejects_non_allowlisted_sender() {
        // D-WA-10 mitigation: when an allowlist is configured, only listed
        // senders can inject envelopes into the corresponding broadcast domain.
        let cfg = cfg_with_allowlist(
            "/tmp/test.db",
            vec!["120363012345678901@g.us"],
            &[("120363012345678901@g.us", &["+15551234567"])],
        );
        let groups = cfg.groups.clone();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "+15559998888@s.whatsapp.net", // not in the allowlist
            "DOT/1/abc",
            &groups,
            &cfg.sender_allowlist,
        );
        assert_eq!(
            decision,
            AcceptDecision::Reject {
                reason: "sender not in allowlist",
            }
        );
    }

    #[test]
    fn accept_message_empty_allowlist_vec_means_legacy_open_group() {
        // An empty allowlist `Vec` (operator explicitly set the entry to empty)
        // is equivalent to no entry: legacy "anyone in the group can inject".
        let cfg = cfg_with_allowlist(
            "/tmp/test.db",
            vec!["120363012345678901@g.us"],
            &[("120363012345678901@g.us", &[])],
        );
        let groups = cfg.groups.clone();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "+15559998888@s.whatsapp.net", // arbitrary sender
            "DOT/1/abc",
            &groups,
            &cfg.sender_allowlist,
        );
        assert_eq!(decision, AcceptDecision::Accept);
    }

    #[test]
    fn accept_message_allowlist_phone_numbers_normalized() {
        // The allowlist comparison normalizes both sides to digits-only, so
        // formatting differences (`+1 555 123 4567`, `+15551234567`,
        // `15551234567@s.whatsapp.net`) all match the same logical sender.
        let cfg = cfg_with_allowlist(
            "/tmp/test.db",
            vec!["120363012345678901@g.us"],
            &[("120363012345678901@g.us", &["+1 555 123 4567"])],
        );
        let groups = cfg.groups.clone();
        for sender_form in [
            "+15551234567@s.whatsapp.net",
            "15551234567",
            "+1 (555) 123-4567",
        ] {
            let decision = WhatsAppWebAdapter::accept_message(
                "120363012345678901@g.us",
                sender_form,
                "DOT/1/abc",
                &groups,
                &cfg.sender_allowlist,
            );
            assert_eq!(
                decision,
                AcceptDecision::Accept,
                "sender {sender_form:?} should be accepted"
            );
        }
    }
}
