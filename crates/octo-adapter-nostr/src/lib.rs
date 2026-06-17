//! Nostr relay adapter for DOT (RFC-0850 §8.1, PlatformType::Nostr)
//!
//! Minimal Nostr client implementation using raw WebSockets.
//! No `nostr-sdk` dependency — implements only NIP-01 event format
//! needed for DOT envelope transport.
//!
//! DOT envelopes are already encrypted (RFC-0853), so Nostr DMs
//! (NIP-04/NIP-17) are unnecessary. Public events with a
//! `cipherocto-dot` tag enable relay-level filtering without
//! exposing payload contents.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "relays": ["wss://relay.damus.io", "wss://nos.lol"],
//!   "private_key": "<hex-encoded ed25519 secret key>",
//!   "channel_tag": "cipherocto-dot"
//! }
//! ```

use async_trait::async_trait;
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeMap;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use octo_network::dot::adapters::{
    backoff::RetryConfig, CapabilityReport, DeliveryReceipt, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

// ── NIP-01 Event ──────────────────────────────────────────────────

/// Custom event kind for CipherOcto DOT envelopes.
/// Kind 30078 = parameterized replaceable event (NIP-33 range).
const DOT_EVENT_KIND: u64 = 30078;

/// A minimal NIP-01 event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct NostrEvent {
    id: String,
    pubkey: String,
    created_at: u64,
    kind: u64,
    tags: Vec<Vec<String>>,
    content: String,
    sig: String,
}

impl NostrEvent {
    /// Create and sign a new Nostr event.
    fn create(
        signing_key: &SigningKey,
        content: String,
        kind: u64,
        tags: Vec<Vec<String>>,
        created_at: u64,
    ) -> Self {
        let pubkey = hex_encode(&signing_key.verifying_key().to_bytes());

        // NIP-01 event ID: SHA256(JSON([0, pubkey, created_at, kind, tags, content]))
        let id_json = serde_json::json!([0, pubkey, created_at, kind, tags, content]);
        let id_bytes = sha256(id_json.to_string().as_bytes());
        let id = hex_encode(&id_bytes);

        // Sign the event ID
        let sig = signing_key.sign(&id_bytes);
        let sig_hex = hex_encode(&sig.to_bytes());

        Self {
            id,
            pubkey,
            created_at,
            kind,
            tags,
            content,
            sig: sig_hex,
        }
    }
}

// ── Configuration ──────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct NostrConfig {
    /// List of relay WebSocket URLs.
    pub relays: Vec<String>,
    /// Ed25519 private key (hex-encoded, 32 bytes).
    pub private_key: String,
    /// Tag used to filter CipherOcto DOT events. Default: "cipherocto-dot".
    ///
    /// This is the **default** channel tag used when a domain has no override
    /// in `domain_tags`. To run multiple "groups" on a single Nostr identity,
    /// add per-domain overrides in `domain_tags`; each entry is a
    /// `domain_hash -> channel_tag` mapping computed by `domain_hash_from_id`.
    #[serde(default = "default_channel_tag")]
    pub channel_tag: String,
    /// Per-domain channel tag overrides. Keyed by hex-encoded `domain_hash`
    /// (32-byte BLAKE3 output of `domain_hash_from_id`). R18: lets a single
    /// `NostrAdapter` serve multiple "groups" by using different `#t` tags
    /// per domain, instead of forcing the user to instantiate one adapter
    /// per group.
    #[serde(default)]
    pub domain_tags: std::collections::BTreeMap<String, String>,
}

fn default_channel_tag() -> String {
    "cipherocto-dot".to_string()
}

// ── Adapter ────────────────────────────────────────────────────────

pub struct NostrAdapter {
    config: NostrConfig,
    signing_key: SigningKey,
    /// Receiver for incoming events from relay subscriptions.
    rx: Mutex<mpsc::Receiver<RawPlatformMessage>>,
    /// Sender — cloned into relay listener tasks.
    tx: mpsc::Sender<RawPlatformMessage>,
    /// Whether relay connections have been started.
    relays_started: Mutex<bool>,
}

impl NostrAdapter {
    pub async fn new(config: NostrConfig) -> Result<Self, String> {
        let key_bytes =
            hex_decode(&config.private_key).map_err(|_| "Invalid hex in private_key")?;
        if key_bytes.len() != 32 {
            return Err("private_key must be 32 bytes (64 hex chars)".into());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&key_bytes);
        let signing_key = SigningKey::from_bytes(&arr);

        let (tx, rx) = mpsc::channel(4096);

        Ok(Self {
            config,
            signing_key,
            rx: Mutex::new(rx),
            tx,
            relays_started: Mutex::new(false),
        })
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: NostrConfig =
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {}", e))?;
        // Block on async new() — acceptable for plugin ABI
        tokio::runtime::Handle::try_current()
            .map_err(|_| "No tokio runtime")?
            .block_on(Self::new(config))
    }

    /// Start relay connections (idempotent).
    async fn ensure_relays_started(&self) -> Result<(), PlatformAdapterError> {
        let mut started = self.relays_started.lock().await;
        if *started {
            return Ok(());
        }

        for relay_url in &self.config.relays {
            let url = relay_url.clone();
            let tx = self.tx.clone();
            let tag = self.config.channel_tag.clone();

            tokio::spawn(async move {
                relay_listener(url, tx, tag).await;
            });
        }

        *started = true;
        Ok(())
    }

    /// Publish an event to all configured relays.
    async fn publish_event(&self, event: &NostrEvent) -> Result<(), PlatformAdapterError> {
        let msg = serde_json::json!(["EVENT", event]);
        let msg_str =
            serde_json::to_string(&msg).map_err(|e| transport_err(format!("JSON error: {e}")))?;

        let retry = RetryConfig::default();
        let mut published = 0;

        for relay_url in &self.config.relays {
            for attempt in 0..=retry.max_retries {
                match publish_to_relay(relay_url, &msg_str).await {
                    Ok(()) => {
                        published += 1;
                        break;
                    }
                    Err(e) => {
                        if retry.should_retry(attempt) {
                            tokio::time::sleep(retry.delay_for_attempt(attempt)).await;
                            continue;
                        }
                        // Log but don't fail — other relays may succeed
                        eprintln!("Nostr publish to {relay_url} failed: {e}");
                        break;
                    }
                }
            }
        }

        if published == 0 && !self.config.relays.is_empty() {
            return Err(transport_err("Failed to publish to any relay"));
        }
        Ok(())
    }

    /// Domain hash: `BLAKE3-256("nostr:{relay_url}:{channel_tag}")`
    pub fn domain_hash(relay_url: &str, channel_tag: &str) -> [u8; 32] {
        let normalized = relay_url.trim().to_lowercase();
        *blake3::hash(format!("nostr:{normalized}:{channel_tag}").as_bytes()).as_bytes()
    }

    /// Inverse of `domain_hash`: parse a `relay_url:channel_tag` platform_id
    /// and compute the hash. Used by `PlatformAdapter::domain_id` so that
    /// callers can construct a `BroadcastDomainId` from a single colon-joined
    /// string and have it match the canonical `domain_hash`. The relay URL
    /// is case- and whitespace-normalized; the channel tag is preserved as-is.
    ///
    /// We split on the **last** colon (via `rsplit_once`) so that relay URLs
    /// containing `://` (e.g. `wss://relay.damus.io:tag`) parse correctly:
    /// the first colon is part of the URL scheme, the last colon is the
    /// separator between URL and channel tag.
    pub fn domain_hash_from_id(platform_id: &str) -> [u8; 32] {
        let (relay, tag) = match platform_id.rsplit_once(':') {
            Some((r, t)) => (r, t),
            None => (platform_id, ""),
        };
        Self::domain_hash(relay, tag)
    }

    /// Look up the channel tag for a domain. Falls back to the default
    /// `channel_tag` in config if the domain has no override.
    fn channel_tag_for_domain(&self, domain: &BroadcastDomainId) -> String {
        let key = hex_encode(&domain.domain_hash);
        self.config
            .domain_tags
            .get(&key)
            .cloned()
            .unwrap_or_else(|| self.config.channel_tag.clone())
    }

    pub fn public_key_hex(&self) -> String {
        hex_encode(&self.signing_key.verifying_key().to_bytes())
    }

    pub const PLATFORM_TYPE: u16 = 0x0004;
    pub fn max_payload_bytes() -> usize {
        65_536
    }
    pub fn rate_limit_per_second() -> u32 {
        10
    }
}

/// Publish a single event to a relay via WebSocket.
async fn publish_to_relay(relay_url: &str, event_msg: &str) -> Result<(), String> {
    let (mut ws, _) = connect_async(relay_url)
        .await
        .map_err(|e| format!("WS connect: {e}"))?;

    ws.send(Message::Text(event_msg.to_string()))
        .await
        .map_err(|e| format!("WS send: {e}"))?;

    // Wait for OK or NOTICE response (with timeout)
    let timeout = std::time::Duration::from_secs(5);
    match tokio::time::timeout(timeout, ws.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => {
            if text.contains("\"OK\"") && text.contains("true") {
                Ok(())
            } else if text.contains("\"NOTICE\"") {
                Err(format!("Relay notice: {text}"))
            } else {
                Ok(())
            }
        }
        Ok(Some(Err(e))) => Err(format!("WS error: {e}")),
        Ok(None) => Err("WS closed".into()),
        Ok(Some(Ok(_))) => Ok(()), // Binary/Ping/Pong/Frame — ignore
        Err(_) => Err("WS timeout".into()),
    }
}

/// Long-running relay listener task.
/// Subscribes to DOT events and forwards them to the adapter's channel.
async fn relay_listener(
    relay_url: String,
    tx: mpsc::Sender<RawPlatformMessage>,
    channel_tag: String,
) {
    let retry = RetryConfig::default();
    let mut attempt = 0u32;

    loop {
        match connect_async(&relay_url).await {
            Ok((mut ws, _)) => {
                attempt = 0; // Reset on successful connection

                // Subscribe to DOT events
                let sub_id = format!("dot-{}", epoch_millis());
                let filter = serde_json::json!({
                    "kinds": [DOT_EVENT_KIND],
                    "#t": [channel_tag],
                    "limit": 100
                });
                let req = serde_json::json!(["REQ", sub_id, filter]);
                if ws.send(Message::Text(req.to_string())).await.is_err() {
                    continue;
                }

                // Listen for events
                while let Some(msg) = ws.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(arr) = serde_json::from_str::<serde_json::Value>(&text) {
                                if arr[0] == "EVENT" {
                                    if let Some(event) = parse_nostr_event(&arr[2]) {
                                        let mut metadata = BTreeMap::new();
                                        metadata.insert("relay".into(), relay_url.clone());
                                        metadata.insert("event_id".into(), event.id.clone());
                                        metadata.insert("pubkey".into(), event.pubkey.clone());

                                        if let Ok(payload) = base64_decode(&event.content) {
                                            let _ = tx.try_send(RawPlatformMessage {
                                                platform_id: format!("nostr-{}", event.id),
                                                payload,
                                                metadata,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Err(_) => break,
                        _ => {}
                    }
                }
            }
            Err(_) => {
                attempt += 1;
            }
        }

        // Reconnect with backoff
        let delay = retry.delay_for_attempt(attempt.min(retry.max_retries));
        tokio::time::sleep(delay).await;
    }
}

/// Parse a Nostr event from JSON value.
fn parse_nostr_event(val: &serde_json::Value) -> Option<NostrEvent> {
    serde_json::from_value(val.clone()).ok()
}

// ── PlatformAdapter ────────────────────────────────────────────────

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "nostr".into(),
        reason: msg.into(),
    }
}

#[async_trait]
impl PlatformAdapter for NostrAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let wire_bytes = envelope.to_wire_bytes();

        if wire_bytes.len() > Self::max_payload_bytes() {
            return Err(transport_err(format!(
                "Envelope too large: {} > {}",
                wire_bytes.len(),
                Self::max_payload_bytes()
            )));
        }

        // R18 fix: look up the per-domain channel tag. If the caller has
        // registered an override in `config.domain_tags`, use it; otherwise
        // fall back to the default `config.channel_tag`. Previously this
        // method ignored the domain entirely.
        let channel_tag = self.channel_tag_for_domain(domain);

        // Encode as base64 for Nostr event content
        let content = base64_encode(&wire_bytes);

        // Build tags
        let tags = vec![
            vec!["t".into(), channel_tag],
            vec!["network".into(), "cipherocto".into()],
        ];

        let event = NostrEvent::create(
            &self.signing_key,
            content,
            DOT_EVENT_KIND,
            tags,
            epoch_millis() / 1000, // Nostr uses seconds
        );

        let event_id = event.id.clone();
        self.publish_event(&event).await?;

        Ok(DeliveryReceipt {
            platform_message_id: event_id,
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        self.ensure_relays_started().await?;
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
            supports_fragmentation: false,
            supports_encryption: false, // DOT has its own encryption
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: None,
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        // The platform_id MUST be in `relay_url:channel_tag` form to match
        // the canonical hash used by `send_envelope`'s per-domain lookup.
        // We parse it here and delegate to `domain_hash` so the two methods
        // always agree (R18 fix; previously the call to
        // `BroadcastDomainId::new` would hash just the platform_id, which
        // mismatched the static `domain_hash` format).
        BroadcastDomainId {
            platform_type: PlatformType::Nostr as u16,
            domain_hash: Self::domain_hash_from_id(platform_id),
        }
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Nostr
    }

    fn self_handle(&self) -> Option<String> {
        Some(self.public_key_hex())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        // Check if at least one relay is reachable
        let timeout = std::time::Duration::from_secs(5);
        for relay_url in &self.config.relays {
            match tokio::time::timeout(timeout, connect_async(relay_url)).await {
                Ok(Ok(_)) => return Ok(()),
                _ => continue,
            }
        }
        Err(transport_err("No relays reachable"))
    }
}

// ── Plugin ABI ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0004
}

#[no_mangle]
/// # Safety
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut () {
    if config.is_null() || config_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = std::slice::from_raw_parts(config, config_len);
    match NostrAdapter::from_config_bytes(bytes) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
/// # Safety
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(adapter: *mut ()) {
    if !adapter.is_null() {
        let _ = Box::from_raw(adapter as *mut NostrAdapter);
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, ()> {
    if !hex.len().is_multiple_of(2) {
        return Err(());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(s: &str) -> Result<Vec<u8>, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| ())
}

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Verifier, VerifyingKey};

    #[test]
    fn test_hex_roundtrip() {
        let data = vec![0u8, 1, 127, 128, 255];
        assert_eq!(hex_decode(&hex_encode(&data)).unwrap(), data);
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"test envelope data for DOT";
        let encoded = base64_encode(data);
        assert_eq!(base64_decode(&encoded).unwrap(), data);
    }

    #[test]
    fn test_nostr_event_create() {
        let key = SigningKey::from_bytes(&[42u8; 32]);
        let event = NostrEvent::create(
            &key,
            "hello".into(),
            DOT_EVENT_KIND,
            vec![vec!["t".into(), "test".into()]],
            1000,
        );
        // ID should be deterministic
        let event2 = NostrEvent::create(
            &key,
            "hello".into(),
            DOT_EVENT_KIND,
            vec![vec!["t".into(), "test".into()]],
            1000,
        );
        assert_eq!(event.id, event2.id);
        assert_eq!(event.pubkey, event2.pubkey);
    }

    #[test]
    fn test_nostr_event_signature_valid() {
        let key = SigningKey::from_bytes(&[42u8; 32]);
        let event = NostrEvent::create(&key, "test".into(), DOT_EVENT_KIND, vec![], 1000);
        // Verify signature
        let id_bytes = hex_decode(&event.id).unwrap();
        let sig_bytes = hex_decode(&event.sig).unwrap();
        let pubkey_bytes = hex_decode(&event.pubkey).unwrap();
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
        let mut pk_arr = [0u8; 32];
        pk_arr.copy_from_slice(&pubkey_bytes);
        let verifying_key = VerifyingKey::from_bytes(&pk_arr).unwrap();
        assert!(verifying_key.verify(&id_bytes, &sig).is_ok());
    }

    #[test]
    fn test_nostr_event_different_content_different_id() {
        let key = SigningKey::from_bytes(&[42u8; 32]);
        let e1 = NostrEvent::create(&key, "a".into(), DOT_EVENT_KIND, vec![], 1000);
        let e2 = NostrEvent::create(&key, "b".into(), DOT_EVENT_KIND, vec![], 1000);
        assert_ne!(e1.id, e2.id);
    }

    #[test]
    fn test_domain_hash_deterministic() {
        let h1 = NostrAdapter::domain_hash("wss://relay.damus.io", "cipherocto-dot");
        let h2 = NostrAdapter::domain_hash("wss://relay.damus.io", "cipherocto-dot");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_hash_normalized() {
        assert_eq!(
            NostrAdapter::domain_hash("wss://Relay.DAMUS.io", "cipherocto-dot"),
            NostrAdapter::domain_hash("  wss://relay.damus.io  ", "cipherocto-dot")
        );
    }

    #[test]
    fn test_domain_hash_different_relays() {
        let h1 = NostrAdapter::domain_hash("wss://relay.damus.io", "cipherocto-dot");
        let h2 = NostrAdapter::domain_hash("wss://nos.lol", "cipherocto-dot");
        assert_ne!(h1, h2);
    }

    // R18 fix: the trait-method `domain_id(platform_id)` must produce the
    // same hash as the static `domain_hash(relay_url, channel_tag)` so
    // that `send_envelope` can look up the per-domain channel tag. The
    // platform_id is the colon-joined form `relay_url:channel_tag`.
    #[test]
    fn test_domain_id_matches_domain_hash() {
        let from_id = NostrAdapter::domain_hash_from_id("wss://relay.damus.io:cipherocto-dot");
        let from_args = NostrAdapter::domain_hash("wss://relay.damus.io", "cipherocto-dot");
        assert_eq!(from_id, from_args);
    }

    #[test]
    fn test_domain_id_normalizes_relay_case_and_whitespace() {
        let h1 = NostrAdapter::domain_hash_from_id("  WSS://Relay.Damus.IO  :tag");
        let h2 = NostrAdapter::domain_hash("wss://relay.damus.io", "tag");
        assert_eq!(h1, h2);
    }

    // R18: relay URLs contain `://` which would confuse a `split_once(':')`
    // parser; we use `rsplit_once(':')` so the URL is preserved intact.
    #[test]
    fn test_domain_id_handles_relay_url_with_scheme_colon() {
        let h1 = NostrAdapter::domain_hash_from_id("wss://relay.damus.io:cipherocto-dot");
        let h2 = NostrAdapter::domain_hash("wss://relay.damus.io", "cipherocto-dot");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_platform_type() {
        assert_eq!(NostrAdapter::PLATFORM_TYPE, 0x0004);
    }

    #[test]
    fn test_abi_exports() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0004);
    }

    #[test]
    fn test_config_from_json() {
        let json = serde_json::json!({
            "relays": ["wss://relay.damus.io", "wss://nos.lol"],
            "private_key": "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a",
            "channel_tag": "my-tag"
        });
        let config: NostrConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.relays.len(), 2);
        assert_eq!(config.channel_tag, "my-tag");
    }

    #[test]
    fn test_config_default_channel_tag() {
        let json = serde_json::json!({
            "relays": ["wss://relay.damus.io"],
            "private_key": "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
        });
        let config: NostrConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.channel_tag, "cipherocto-dot");
    }

    #[test]
    fn test_config_default_domain_tags_is_empty() {
        // R18: domain_tags defaults to an empty map.
        let json = serde_json::json!({
            "relays": ["wss://relay.damus.io"],
            "private_key": "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
        });
        let config: NostrConfig = serde_json::from_value(json).unwrap();
        assert!(config.domain_tags.is_empty());
    }

    #[test]
    fn test_config_parses_domain_tags() {
        // R18: domain_tags is deserialized from JSON.
        let json = serde_json::json!({
            "relays": ["wss://relay.damus.io"],
            "private_key": "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a",
            "channel_tag": "default-tag",
            "domain_tags": {
                "deadbeef": "override-tag"
            }
        });
        let config: NostrConfig = serde_json::from_value(json).unwrap();
        assert_eq!(
            config.domain_tags.get("deadbeef"),
            Some(&"override-tag".to_string())
        );
    }

    #[test]
    fn test_nostr_event_json_roundtrip() {
        let key = SigningKey::from_bytes(&[42u8; 32]);
        let event = NostrEvent::create(
            &key,
            "test content".into(),
            DOT_EVENT_KIND,
            vec![vec!["t".into(), "cipherocto-dot".into()]],
            1234567890,
        );
        let json = serde_json::to_string(&event).unwrap();
        let parsed: NostrEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, event.id);
        assert_eq!(parsed.pubkey, event.pubkey);
        assert_eq!(parsed.content, event.content);
        assert_eq!(parsed.kind, event.kind);
    }
}
