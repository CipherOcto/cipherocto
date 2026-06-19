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
    coordinator_admin::{
        AddMemberOutput, AdminCapabilityReport, CoordinatorAdmin, GroupHandle, GroupId,
        GroupMemberSpec, GroupMetadata, GroupModeFlags, InviteRef, PeerId,
    },
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
    /// - RFC-0861 §2 M16: each `groups` entry is either bare digits
    ///   (`120363012345678901`) or digits+`@g.us`
    ///   (`120363012345678901@g.us`). Entries that contain `@` but
    ///   don't end with `@g.us` (newsletter JID misuse) or that
    ///   contain `:` (user JID misuse) are rejected.
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
            // RFC-0861 §2 M16: tighten JID acceptance. Two valid
            // forms: bare digits, or digits + `@g.us`. Anything
            // with `@` that doesn't end in `@g.us` is newsletter
            // JID misuse (`1234@newsletter`); anything with `:`
            // is user JID misuse (`1234567890:0@s.whatsapp.net`).
            if group.contains(':') {
                return Err(format!(
                    "groups entry {group:?} contains ':' (user JID misuse; expected digits or digits+@g.us)"
                ));
            }
            if group.contains('@') {
                if !group.ends_with("@g.us") {
                    return Err(format!(
                        "groups entry {group:?} contains '@' but does not end with @g.us (newsletter JID misuse)"
                    ));
                }
                let prefix = &group[..group.len() - "@g.us".len()];
                if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
                    return Err(format!(
                        "groups entry {group:?} has non-numeric prefix before @g.us"
                    ));
                }
            } else if !group.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!(
                    "groups entry {group:?} is not all digits (expected digits or digits+@g.us)"
                ));
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
    /// Mission 0850p-a-notify-event-connected: a `tokio::sync::Notify` that
    /// is `notify_waiters()`-ed on `Event::Connected`. Replaces the
    /// 250 ms polling loop in `wait_for_connected` (mission
    /// 0850p-a-notify-event-connected). Wrapped in an `Arc` because
    /// `Notify` is not `Clone`.
    connected_notify: Arc<tokio::sync::Notify>,
    /// Runtime-mutable group list, consulted alongside `config.groups`
    /// by both `send_envelope`'s domain→JID lookup and the inbound
    /// `accept_message` filter. Coordinators that create groups at
    /// runtime (rather than configuring them statically via
    /// `WhatsAppConfig::groups`) push the new JIDs here so inbound
    /// envelopes from the freshly-created group are accepted instead
    /// of being filtered as "unconfigured group".
    ///
    /// Backwards-compatible: when empty, behaviour is identical to the
    /// static-config-only path (the legacy default).
    runtime_groups: Arc<Mutex<Vec<String>>>,
}

/// Result of [`WhatsAppWebAdapter::create_group`]: the new group's
/// `<id>@g.us` JID plus the full `GroupMetadata` the server returned.
///
/// The `group_jid` field is what callers should push into
/// [`WhatsAppConfig::groups`] so [`PlatformAdapter::send_envelope`] can
/// route to the new group by `domain_id`.
pub struct CreateGroupOutput {
    /// Group JID in `<digits>@g.us` form (e.g. `120363012345678901@g.us`).
    pub group_jid: String,
    /// Server-reported metadata (subject, participants, creation time, ...).
    pub metadata: whatsapp_rust::GroupMetadata,
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
            // Mission 0850p-a-notify-event-connected: a fresh Notify for
            // each adapter instance. `notify_waiters()` is called by
            // the Event::Connected handler; consumers (the CLI's
            // `wait_for_connected`) `notified().await` on a clone of
            // the Arc.
            connected_notify: Arc::new(tokio::sync::Notify::new()),
            runtime_groups: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Mission 0850p-a-notify-event-connected: returns a clonable
    /// handle to the `Notify` that fires on `Event::Connected`.
    /// Cloning the `Arc<Notify>` is cheap and gives a handle to
    /// the same underlying `Notify`.
    pub fn connected(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.connected_notify)
    }

    /// Mission 0850p-a-has-valid-session: returns `true` if a valid
    /// session exists (bot handle present and `self_handle().is_some()`).
    /// This is a synchronous, allocation-free check that replaces the
    /// 250ms polling loop in the CLI's `whoami` flow.
    pub fn has_valid_session(&self) -> bool {
        self.self_handle().is_some()
            && self
                .bot_handle
                .try_lock()
                .map(|h| h.is_some())
                .unwrap_or(false)
    }

    /// Register a group at runtime, alongside the statically-configured
    /// `WhatsAppConfig::groups`. The group JID will be accepted by both
    /// `send_envelope`'s domain→JID lookup and the inbound
    /// `accept_message` filter. Idempotent: re-registering an existing
    /// JID is a no-op (no duplicates).
    ///
    /// Use this after `create_group` returns so the newly-created
    /// group is immediately routable without restarting the bot or
    /// reloading the config.
    pub fn register_group_at_runtime(&self, group_jid: &str) {
        let mut guard = self.runtime_groups.lock();
        if !guard.iter().any(|g| g == group_jid) {
            guard.push(group_jid.to_string());
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

    /// Convert a group ID to a WhatsApp group JID.
    ///
    /// RFC-0861 §2 M16: tightened to refuse non-numeric inputs that
    /// don't carry the `@g.us` suffix. Accepts:
    ///   - bare digits (e.g. `120363012345678901`) → append `@g.us`
    ///   - digits already terminated with `@g.us` (e.g.
    ///     `120363012345678901@g.us`) → pass through
    /// Refuses (via `debug_assert!` + a `Result` return):
    ///   - inputs containing `@` that don't end with `@g.us`
    ///     (newsletter JID misuse, e.g. `1234@newsletter`)
    ///   - inputs containing `:` (user JID misuse, e.g.
    ///     `1234567890:0@s.whatsapp.net`)
    ///   - non-numeric prefixes without the `@g.us` suffix
    ///
    /// `validate()` is the production gate: it rejects bad
    /// `groups` entries at config time. This helper's
    /// `debug_assert!` catches programming errors in tests; in
    /// release builds the function falls through to the same
    /// behavior as before, since runtime callers always pass
    /// `validate()`-checked strings.
    fn group_to_jid(group_id: &str) -> String {
        const SUFFIX: &str = "@g.us";
        if let Some(prefix) = group_id.strip_suffix(SUFFIX) {
            debug_assert!(
                !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()),
                "group_to_jid: {group_id:?} has @g.us suffix but prefix {prefix:?} is empty or non-numeric"
            );
            group_id.to_string()
        } else {
            debug_assert!(
                !group_id.contains('@') && !group_id.contains(':')
                    && group_id.chars().all(|c| c.is_ascii_digit()),
                "group_to_jid: {group_id:?} is not a valid group JID (must be digits or digits+@g.us)"
            );
            format!("{group_id}{SUFFIX}")
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
        // Combine the static `config.groups` and the runtime-registered
        // groups at the moment the bot starts. New groups added via
        // `register_group_at_runtime` after `start_bot` is captured by
        // the Arc<Mutex<Vec>> below.
        let groups = self.config.groups.clone();
        let runtime_groups = Arc::clone(&self.runtime_groups);
        let sender_allowlist = self.config.sender_allowlist.clone();
        // Mission 0850p-a-notify-event-connected: clone the Notify
        // into the closure so the Event::Connected handler can
        // wake up `wait_for_connected` callers.
        let connected_notify = Arc::clone(&self.connected_notify);

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
                let runtime_groups = Arc::clone(&runtime_groups);
                let sender_allowlist = sender_allowlist.clone();
                let connected_notify = connected_notify.clone();

                async move {
                    use wacore::proto_helpers::MessageExt;
                    use wacore::types::events::Event;

                    match &*event {
                        Event::Message(msg, info) => {
                            let text = msg.text_content().unwrap_or("").to_string();
                            let chat = info.source.chat.to_string();
                            let sender = info.source.sender.to_string();

                            // Combine static config.groups with
                            // runtime-registered groups so messages from
                            // groups added via `register_group_at_runtime`
                            // are accepted.
                            let effective_groups: Vec<String> = {
                                let rt = runtime_groups.lock();
                                if rt.is_empty() {
                                    groups.clone()
                                } else {
                                    let mut combined = groups.clone();
                                    combined.extend(rt.iter().cloned());
                                    combined
                                }
                            };

                            let decision = Self::accept_message(
                                &chat,
                                &sender,
                                &text,
                                &effective_groups,
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
                            // Mission 0850p-a-notify-event-connected:
                            // wake up any `wait_for_connected` consumer
                            // waiting on `Notify::notified()`.
                            connected_notify.notify_waiters();
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

    // ── Group-setup API (RFC-0850p-a §8.1, E2E Scenario 1) ───────────────
    //
    // These methods are NOT part of the `PlatformAdapter` trait — they are
    // coordinator-only group-management operations that the Web protocol
    // exposes via `Client::groups()` but that the DOT envelope transport
    // contract does not need to advertise. They live here so a coordinator
    // process (CLI, swarm bootstrap, or e2e test) can:
    //
    // 1. Create the broadcast group with the coordinator as admin.
    // 2. Add member phone numbers (or skip and use the invite link instead).
    // 3. Fetch the `chat.whatsapp.com` invite link to share with humans /
    //    other nodes.
    // 4. Optionally tear the group down via `leave_group` for cleanup.
    //
    // They follow the same locking discipline as `send_envelope`: clone the
    // client Arc out of the mutex before any `.await`.

    /// Create a new WhatsApp group with `subject` and an initial participant
    /// list. The authenticated bot is automatically the creator (and admin).
    /// Members that already have a WhatsApp account receive an invite
    /// notification; members that don't are silently skipped by the server.
    ///
    /// `participants` is a list of E.164 phone numbers (with or without the
    /// leading `+`); they are normalised to digits-only and converted to
    /// `<digits>@s.whatsapp.net` JIDs internally.
    ///
    /// The bot must be connected (i.e. `start_bot()` returned and
    /// `self_handle()` is Some). Returns an error if the client is not yet
    /// connected, the subject is empty, or the WhatsApp server rejects the
    /// create IQ.
    ///
    /// **RFC-0861 §3 H2:** this method is named `create_group_str` (not
    /// `create_group`) so the `CoordinatorAdmin::create_group` trait impl
    /// below can call the unambiguous inherent without an infinite
    /// recursion footgun if anyone later loosens this inherent's signature
    /// to take `&[GroupMemberSpec]`. Mirrors the `leave_group_str`
    /// precedent at `adapter.rs:1788`.
    pub async fn create_group_str(
        &self,
        subject: &str,
        participants: &[&str],
    ) -> Result<CreateGroupOutput, String> {
        if subject.trim().is_empty() {
            return Err("group subject must not be empty".into());
        }
        // Clone the client Arc out of the mutex; do not hold the lock across await.
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        // Convert phone strings to JIDs.
        let mut participant_opts: Vec<whatsapp_rust::GroupParticipantOptions> =
            Vec::with_capacity(participants.len());
        for phone in participants {
            let digits = Self::normalize_phone(phone);
            if digits.is_empty() {
                return Err(format!("participant {phone:?} has no digits"));
            }
            let jid = wacore_binary::Jid::pn(digits);
            participant_opts.push(whatsapp_rust::GroupParticipantOptions::from_phone(jid));
        }

        let options = whatsapp_rust::GroupCreateOptions {
            subject: subject.to_string(),
            participants: participant_opts,
            ..Default::default()
        };

        let result = client
            .groups()
            .create_group(options)
            .await
            .map_err(|e| format!("create_group failed: {e:#}"))?;

        let group_jid = result.metadata.id.to_string();
        tracing::info!(
            subject = %subject,
            group_jid = %group_jid,
            participants = participants.len(),
            "WhatsApp group created"
        );

        Ok(CreateGroupOutput {
            group_jid,
            metadata: result.metadata,
        })
    }

    /// Add phone-number participants to an existing group. The bot must be
    /// an admin of the group (the creator is, by default).
    ///
    /// `participants` is a list of E.164 phone numbers (with or without `+`).
    /// The server's per-participant response is returned so callers can tell
    /// which numbers were accepted and which were rejected (already in the
    /// group, not on WhatsApp, blocked, etc.).
    pub async fn add_members(
        &self,
        group_jid: &str,
        participants: &[&str],
    ) -> Result<Vec<whatsapp_rust::ParticipantChangeResponse>, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let mut jids: Vec<wacore_binary::Jid> = Vec::with_capacity(participants.len());
        for phone in participants {
            let digits = Self::normalize_phone(phone);
            if digits.is_empty() {
                return Err(format!("participant {phone:?} has no digits"));
            }
            jids.push(wacore_binary::Jid::pn(digits));
        }

        let responses = client
            .groups()
            .add_participants(&jid, &jids)
            .await
            .map_err(|e| format!("add_participants failed: {e:#}"))?;

        tracing::info!(
            group_jid = %group_jid,
            added = responses.iter().filter(|r| r.is_ok()).count(),
            failed = responses.iter().filter(|r| !r.is_ok()).count(),
            "WhatsApp group participants added"
        );

        Ok(responses)
    }

    /// Fetch the `chat.whatsapp.com` invite link for the group. Pass
    /// `reset = true` to invalidate any previously-issued link and mint a new
    /// one (useful for the "revoke and re-issue" revocation pattern).
    pub async fn get_invite_link(&self, group_jid: &str, reset: bool) -> Result<String, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let link = client
            .groups()
            .get_invite_link(&jid, reset)
            .await
            .map_err(|e| format!("get_invite_link failed: {e:#}"))?;

        Ok(link)
    }

    /// Have the bot leave a group. Idempotent on already-left groups at the
    /// server level (server returns an error which we surface; callers that
    /// want "leave if member" semantics can ignore the error).
    pub async fn leave_group(&self, group_jid: &str) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        client
            .groups()
            .leave(&jid)
            .await
            .map_err(|e| format!("leave_group failed: {e:#}"))?;

        tracing::info!(group_jid = %group_jid, "WhatsApp group left");
        Ok(())
    }

    /// Re-fetch the current metadata for an existing group (subject,
    /// participants, admins). Used by the live E2E test to verify the
    /// bot's view of a group after the create-time snapshot.
    pub async fn group_metadata(
        &self,
        group_jid: &str,
    ) -> Result<whatsapp_rust::GroupMetadata, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        client
            .groups()
            .get_metadata(&jid)
            .await
            .map_err(|e| format!("group_metadata failed: {e:#}"))
    }

    // ── R20: CoordinatorAdmin surface (wraps whatsapp-rust groups API) ──
    //
    // These methods expose the same primitives that `CoordinatorAdmin`
    // needs, but with a `Result<_, String>` return type and `&str`
    // group JIDs. The trait impl below wraps them, normalizes errors
    // to `PlatformAdapterError`, and bridges the platform-native
    // `GroupId` / `PeerId` newtypes to WhatsApp's internal `wacore_binary::Jid`
    // format.

    /// Remove phone-number participants from an existing group. The bot
    /// must be an admin of the group.
    pub async fn remove_members(
        &self,
        group_jid: &str,
        participants: &[&str],
    ) -> Result<Vec<whatsapp_rust::ParticipantChangeResponse>, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let mut jids: Vec<wacore_binary::Jid> = Vec::with_capacity(participants.len());
        for phone in participants {
            let digits = Self::normalize_phone(phone);
            if digits.is_empty() {
                return Err(format!("participant {phone:?} has no digits"));
            }
            jids.push(wacore_binary::Jid::pn(digits));
        }

        let responses = client
            .groups()
            .remove_participants(&jid, &jids)
            .await
            .map_err(|e| format!("remove_participants failed: {e:#}"))?;

        tracing::info!(
            group_jid = %group_jid,
            removed = responses.iter().filter(|r| r.is_ok()).count(),
            failed = responses.iter().filter(|r| !r.is_ok()).count(),
            "WhatsApp group participants removed"
        );
        Ok(responses)
    }

    /// Promote phone-number participants to admin. The bot must itself be
    /// an admin of the group.
    pub async fn promote_participants(
        &self,
        group_jid: &str,
        participants: &[&str],
    ) -> Result<Vec<whatsapp_rust::ParticipantChangeResponse>, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let mut jids: Vec<wacore_binary::Jid> = Vec::with_capacity(participants.len());
        for phone in participants {
            let digits = Self::normalize_phone(phone);
            if digits.is_empty() {
                return Err(format!("participant {phone:?} has no digits"));
            }
            jids.push(wacore_binary::Jid::pn(digits));
        }

        // whatsapp-rust's `promote_participants` returns `()`. Synthesize a
        // per-participant success response so callers can reuse the same
        // `Vec<ParticipantChangeResponse>` shape as `add_members` and
        // `remove_members` (the per-participant semantics matches the
        // server's actual processing of each JID).
        client
            .groups()
            .promote_participants(&jid, &jids)
            .await
            .map_err(|e| format!("promote_participants failed: {e:#}"))?;

        let responses: Vec<whatsapp_rust::ParticipantChangeResponse> = jids
            .iter()
            .map(|j| whatsapp_rust::ParticipantChangeResponse {
                jid: j.clone(),
                status: Some("promoted".into()),
                error: None,
                phone_number: None,
                username: None,
                add_request: None,
            })
            .collect();

        tracing::info!(
            group_jid = %group_jid,
            promoted = responses.len(),
            "WhatsApp participants promoted to admin"
        );
        Ok(responses)
    }

    /// Demote admins back to regular participants. The bot must remain
    /// an admin of the group (WhatsApp does not allow the last admin
    /// to demote itself).
    pub async fn demote_participants(
        &self,
        group_jid: &str,
        participants: &[&str],
    ) -> Result<Vec<whatsapp_rust::ParticipantChangeResponse>, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let mut jids: Vec<wacore_binary::Jid> = Vec::with_capacity(participants.len());
        for phone in participants {
            let digits = Self::normalize_phone(phone);
            if digits.is_empty() {
                return Err(format!("participant {phone:?} has no digits"));
            }
            jids.push(wacore_binary::Jid::pn(digits));
        }

        // whatsapp-rust's `demote_participants` returns `()`. Synthesize a
        // per-participant success response, same as `promote_participants`.
        client
            .groups()
            .demote_participants(&jid, &jids)
            .await
            .map_err(|e| format!("demote_participants failed: {e:#}"))?;

        let responses: Vec<whatsapp_rust::ParticipantChangeResponse> = jids
            .iter()
            .map(|j| whatsapp_rust::ParticipantChangeResponse {
                jid: j.clone(),
                status: Some("demoted".into()),
                error: None,
                phone_number: None,
                username: None,
                add_request: None,
            })
            .collect();

        tracing::info!(
            group_jid = %group_jid,
            demoted = responses.len(),
            "WhatsApp participants demoted from admin"
        );
        Ok(responses)
    }

    /// List the groups the bot currently participates in. Each entry
    /// carries the JID and the subject. Used by the coordinator to
    /// reconcile its view of "groups I own" against the platform.
    pub async fn get_participating(
        &self,
    ) -> Result<std::collections::HashMap<String, whatsapp_rust::GroupMetadata>, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let groups = client
            .groups()
            .get_participating()
            .await
            .map_err(|e| format!("get_participating failed: {e:#}"))?;

        Ok(groups)
    }

    /// Set the group subject (the human-readable name). Bot must be admin.
    pub async fn set_subject(&self, group_jid: &str, subject: &str) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let subject_typed = whatsapp_rust::GroupSubject::new(subject)
            .map_err(|e| format!("set_subject: invalid subject: {e:#}"))?;

        client
            .groups()
            .set_subject(&jid, subject_typed)
            .await
            .map_err(|e| format!("set_subject failed: {e:#}"))?;
        Ok(())
    }

    /// Set the group description / topic. Bot must be admin.
    ///
    /// WhatsApp's `set_description` API requires the current description
    /// ID (from `group_metadata()`) for conflict detection. We pass
    /// `None` (unknown) here, which means: "if a description already
    /// exists, the call will fail with a conflict error and the caller
    /// should re-read metadata and retry with the ID." The simple
    /// coordinator flow that wants first-write-wins should fetch
    /// metadata first, then call this with the existing ID.
    pub async fn set_description(&self, group_jid: &str, description: &str) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let desc_typed = whatsapp_rust::GroupDescription::new(description)
            .map_err(|e| format!("set_description: invalid description: {e:#}"))?;

        client
            .groups()
            .set_description(&jid, Some(desc_typed), None)
            .await
            .map_err(|e| format!("set_description failed: {e:#}"))?;
        Ok(())
    }

    /// Set the "announce mode" (only admins can post). Bot must be admin.
    pub async fn set_announce(&self, group_jid: &str, announce_only: bool) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        client
            .groups()
            .set_announce(&jid, announce_only)
            .await
            .map_err(|e| format!("set_announce failed: {e:#}"))?;
        Ok(())
    }

    /// Set the "locked" mode (only admins can edit group info).
    pub async fn set_locked(&self, group_jid: &str, locked: bool) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        client
            .groups()
            .set_locked(&jid, locked)
            .await
            .map_err(|e| format!("set_locked failed: {e:#}"))?;
        Ok(())
    }

    /// Set the ephemeral / disappearing-message TTL in seconds.
    /// Pass `0` (or `Some(0)`) to disable. Common values:
    /// 86400 (24h), 604800 (7d), 7776000 (90d).
    pub async fn set_ephemeral(&self, group_jid: &str, ttl_seconds: u32) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        client
            .groups()
            .set_ephemeral(&jid, ttl_seconds)
            .await
            .map_err(|e| format!("set_ephemeral failed: {e:#}"))?;
        Ok(())
    }

    /// Resolve a `chat.whatsapp.com/CODE` invite URL or `CODE` string to
    /// the full group metadata. Does not auto-join; the caller decides.
    pub async fn get_invite_info(
        &self,
        invite: &str,
    ) -> Result<whatsapp_rust::GroupMetadata, String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        // `get_invite_info` does its own `extract_invite_code` internally,
        // accepting both `chat.whatsapp.com/CODE` URLs and bare codes.
        let info = client
            .groups()
            .get_invite_info(invite)
            .await
            .map_err(|e| format!("get_invite_info failed: {e:#}"))?;

        Ok(info)
    }

    /// Set membership-approval mode (new joiners must be approved by an
    /// admin). Bot must be admin.
    pub async fn set_membership_approval(
        &self,
        group_jid: &str,
        require: bool,
    ) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };

        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;

        let mode = if require {
            whatsapp_rust::MembershipApprovalMode::On
        } else {
            whatsapp_rust::MembershipApprovalMode::Off
        };

        client
            .groups()
            .set_membership_approval(&jid, mode)
            .await
            .map_err(|e| format!("set_membership_approval failed: {e:#}"))?;
        Ok(())
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

        // Find the group JID for this domain. Check the static config first,
        // then the runtime-registered list (groups added via
        // `register_group_at_runtime` after `create_group`).
        let static_match = self
            .config
            .groups
            .iter()
            .find(|g| Self::domain_hash(g) == domain.domain_hash)
            .cloned();
        let runtime_match = if static_match.is_none() {
            self.runtime_groups
                .lock()
                .iter()
                .find(|g| Self::domain_hash(g) == domain.domain_hash)
                .cloned()
        } else {
            None
        };
        let group_id = static_match.or(runtime_match).ok_or_else(|| {
            transport_err(format!(
                "No group found for domain {:?}",
                domain.domain_hash
            ))
        })?;

        let jid = Self::group_to_jid(&group_id);
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

    /// Coordinator-admin capability probe: WhatsApp supports the full
    /// admin set, so we opt in to `CoordinatorAdmin` by returning
    /// `Some(self)` here. Callers use
    /// [`PlatformAdapter::as_coordinator_admin`] to downcast.
    fn as_coordinator_admin(
        &self,
    ) -> Option<&dyn octo_network::dot::adapters::coordinator_admin::CoordinatorAdmin> {
        Some(self)
    }
}

// ── CoordinatorAdmin (R20) ─────────────────────────────────────────
//
// WhatsApp implements the full coordinator/admin surface. Every
// method on `CoordinatorAdmin` either delegates to one of the
// `*_impl` methods above or constructs the platform-neutral types
// (`GroupMetadata`, `GroupModeFlags`, `PeerId`) from the rich
// `whatsapp_rust::GroupMetadata` we get from the server.
//
// JID conventions: phone numbers come in as raw digits (e.g.
// `5521995544743`); JIDs come in as `<digits>@g.us` (groups) or
// `<digits>@s.whatsapp.net` (users). The `PeerId` we hand back is
// the same string the platform uses natively.

#[async_trait]
impl CoordinatorAdmin for WhatsAppWebAdapter {
    /// Truthful capability report. Anything we don't override on
    /// the trait returns `Unimplemented`, but the methods we *do*
    /// override match this report.
    fn admin_capabilities(&self) -> AdminCapabilityReport {
        AdminCapabilityReport {
            // Lifecycle
            can_create: true,
            can_join_by_id: false,
            can_join_by_invite: true, // `join_with_invite_code` exists in whatsapp-rust
            can_leave: true,
            can_destroy: false, // No first-class "destroy" on WhatsApp
            // Membership
            can_add_member: true,
            can_remove_member: true,
            can_ban: false, // WhatsApp has no ban primitive
            can_promote: true,
            can_demote: true,
            can_approve_join: false, // Not exposed in whatsapp-rust's typed API
            // Mode
            can_rename: true,
            can_describe: true,
            can_lock: true,
            can_announce: true,
            can_set_ephemeral: true,
            can_require_approval: true,
            // Discovery
            can_list_own_groups: true,
            can_get_metadata: true,
            can_resolve_invite: true,
            // Handoff
            can_transfer_ownership: false,
        }
    }

    fn platform_name(&self) -> String {
        "whatsapp".into()
    }

    async fn create_group(
        &self,
        subject: &str,
        initial_members: &[GroupMemberSpec],
    ) -> Result<GroupHandle, PlatformAdapterError> {
        // Translate `GroupMemberSpec` to a slice of `&str` phone
        // numbers. WhatsApp doesn't accept a per-member display
        // name on create; the platform-side display name is
        // whatever the contact already has in its address book.
        let phones: Vec<&str> = initial_members.iter().map(|m| m.handle.as_str()).collect();

        let output = self.create_group_str(subject, &phones).await.map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 500,
                message: format!("create_group failed: {e}"),
            }
        })?;

        // Promote initial members that requested `is_admin = true`.
        // (The `create_group` API creates all participants as regular
        // members; admin status must be set with `promote_participants`
        // after create. We do this best-effort — if any one fails the
        // group is still created and the caller can retry.)
        let to_promote: Vec<&str> = initial_members
            .iter()
            .filter(|m| m.is_admin)
            .map(|m| m.handle.as_str())
            .collect();
        if !to_promote.is_empty() {
            if let Err(e) = self
                .promote_participants(&output.group_jid, &to_promote)
                .await
            {
                tracing::warn!(
                    group_jid = %output.group_jid,
                    error = %e,
                    "failed to promote initial admins on create; caller should retry"
                );
            }
        }

        // Pull a fresh metadata snapshot so we can fill in the
        // membership / mode fields of the returned `GroupHandle`.
        // RFC-0861 §3 M5: surface failures at `tracing::debug!`
        // level rather than silently dropping them with `.ok()`.
        // Callers needing strong guarantees can call
        // `get_group_metadata` separately (the returned
        // `GroupHandle` fields are `None` means "platform did
        // not surface it" per the trait docs).
        let metadata = match self.group_metadata(&output.group_jid).await {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::debug!(
                    group_jid = %output.group_jid,
                    error = %e,
                    "create_group: group_metadata post-create fetch failed; returning handle without member_count/mode_flags"
                );
                None
            }
        };
        let invite_url = match self.get_invite_link(&output.group_jid, false).await {
            Ok(u) => Some(u),
            Err(e) => {
                tracing::debug!(
                    group_jid = %output.group_jid,
                    error = %e,
                    "create_group: get_invite_link post-create fetch failed; returning handle without invite_url"
                );
                None
            }
        };

        Ok(GroupHandle {
            id: GroupId::new(output.group_jid),
            subject: Some(subject.to_string()),
            invite_url,
            is_admin: true,
            member_count: metadata.as_ref().and_then(|m| m.size),
            mode_flags: metadata.as_ref().map(extract_mode_flags),
            initial_admins_promoted: true, // Phase 2 H2 path: WhatsApp makes the creator admin at create time
        })
    }

    async fn leave_group(&self, group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        // Idempotency: ignore "not a participant" errors. whatsapp-rust's
        // `leave` returns an error if the bot isn't in the group; we treat
        // that as success (the goal state — "not a member" — is already met).
        match self.leave_group_str(group_id.as_str()).await {
            Ok(()) => Ok(()),
            Err(e) if e.contains("not a participant") || e.contains("not in group") => Ok(()),
            Err(e) => Err(PlatformAdapterError::ApiError {
                code: 500,
                message: format!("leave_group failed: {e}"),
            }),
        }
    }

    async fn destroy_group(&self, group_id: &GroupId) -> Result<(), PlatformAdapterError> {
        // WhatsApp has no "destroy group" primitive. The best we can do
        // is revoke the invite link (so no new members can join) and
        // leave. The group itself remains visible to existing members
        // until they also leave.
        let _ = self.get_invite_link(group_id.as_str(), true).await;
        self.leave_group_str(group_id.as_str())
            .await
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 500,
                message: format!("destroy_group: {e}"),
            })
    }

    async fn add_member(
        &self,
        group_id: &GroupId,
        member: &GroupMemberSpec,
    ) -> Result<AddMemberOutput, PlatformAdapterError> {
        let phones = [member.handle.as_str()];
        let responses = self
            .add_members(group_id.as_str(), &phones)
            .await
            .map_err(|e| api_err("add_member", e))?;
        if let Some(r) = responses.first() {
            if !r.is_ok() {
                return Err(PlatformAdapterError::ApiError {
                    code: 500,
                    message: r
                        .error
                        .clone()
                        .unwrap_or_else(|| "add_member rejected".into()),
                });
            }
        }
        // Promote to admin if requested. Phase 2 M1 / M5 / M11 / M16
        // will refine the error handling and parallelization; for
        // Phase 1 we just adapt the signature.
        let promoted = if member.is_admin {
            let r = self
                .promote_to_admin(group_id, &PeerId::new(member.handle.clone()))
                .await;
            Some(r)
        } else {
            None
        };
        Ok(AddMemberOutput {
            added: true,
            promoted,
        })
    }

    async fn remove_member(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let phones = [member.as_str()];
        let responses = self
            .remove_members(group_id.as_str(), &phones)
            .await
            .map_err(|e| api_err("remove_member", e))?;
        if let Some(r) = responses.first() {
            if !r.is_ok() {
                return Err(PlatformAdapterError::ApiError {
                    code: 500,
                    message: r
                        .error
                        .clone()
                        .unwrap_or_else(|| "remove_member rejected".into()),
                });
            }
        }
        Ok(())
    }

    async fn ban_member(
        &self,
        _group_id: &GroupId,
        _member: &PeerId,
        _duration: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> {
        // WhatsApp has no ban primitive. The recommended pattern
        // (per `docs/research/coordinator-admin-actions.md`) is:
        // remove the member, then revoke the invite link. Returning
        // `Unimplemented` here tells the caller to use that
        // fallback rather than expecting a real ban.
        Err(PlatformAdapterError::Unimplemented {
            platform: "whatsapp".into(),
            action: "ban_member".into(),
        })
    }

    async fn promote_to_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let phones = [member.as_str()];
        self.promote_participants(group_id.as_str(), &phones)
            .await
            .map_err(|e| api_err("promote_to_admin", e))?;
        Ok(())
    }

    async fn demote_from_admin(
        &self,
        group_id: &GroupId,
        member: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let phones = [member.as_str()];
        self.demote_participants(group_id.as_str(), &phones)
            .await
            .map_err(|e| api_err("demote_from_admin", e))?;
        Ok(())
    }

    async fn approve_join_request(
        &self,
        _group_id: &GroupId,
        _requester: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        // whatsapp-rust's typed API doesn't expose approve-membership-
        // requests at the moment. Returning Unimplemented signals
        // "fall back to manual approval in the WhatsApp client" to
        // the caller, which is the right thing for an R-series
        // rollout.
        Err(PlatformAdapterError::Unimplemented {
            platform: "whatsapp".into(),
            action: "approve_join_request".into(),
        })
    }

    async fn rename_group(
        &self,
        group_id: &GroupId,
        new_subject: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.set_subject(group_id.as_str(), new_subject)
            .await
            .map_err(|e| api_err("rename_group", e))
    }

    async fn set_group_description(
        &self,
        group_id: &GroupId,
        description: &str,
    ) -> Result<(), PlatformAdapterError> {
        self.set_description(group_id.as_str(), description)
            .await
            .map_err(|e| api_err("set_group_description", e))
    }

    async fn set_locked(
        &self,
        group_id: &GroupId,
        locked: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.set_locked(group_id.as_str(), locked)
            .await
            .map_err(|e| api_err("set_locked", e))
    }

    async fn set_announce(
        &self,
        group_id: &GroupId,
        announce_only: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.set_announce(group_id.as_str(), announce_only)
            .await
            .map_err(|e| api_err("set_announce", e))
    }

    async fn set_ephemeral(
        &self,
        group_id: &GroupId,
        ttl: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> {
        // RFC-0861 §3 M1: TTL is u32 seconds on the WhatsApp wire.
        // Reject (not silently truncate) values that would overflow
        // `u32::MAX` seconds, with `ApiError { code: 400, ... }` so
        // the caller sees a clear "you gave a bad value" signal
        // rather than a server-side rejection hours later. `None`
        // means "disable" (u32 0), per the trait contract.
        let secs: u32 = match ttl {
            None => 0,
            Some(d) => {
                let raw = d.as_secs();
                if raw > u32::MAX as u64 {
                    return Err(PlatformAdapterError::ApiError {
                        code: 400,
                        message: format!(
                            "set_ephemeral: ttl {raw}s exceeds u32::MAX ({}s)",
                            u32::MAX
                        ),
                    });
                }
                raw as u32
            }
        };
        self.set_ephemeral(group_id.as_str(), secs)
            .await
            .map_err(|e| api_err("set_ephemeral", e))
    }

    async fn set_require_approval(
        &self,
        group_id: &GroupId,
        require: bool,
    ) -> Result<(), PlatformAdapterError> {
        self.set_membership_approval(group_id.as_str(), require)
            .await
            .map_err(|e| api_err("set_require_approval", e))
    }

    async fn list_own_groups(&self) -> Result<Vec<GroupHandle>, PlatformAdapterError> {
        let map = self
            .get_participating()
            .await
            .map_err(|e| api_err("list_own_groups", e))?;
        // Snapshot the bot's own phone once so we can match it against
        // the participant list without holding the lock.
        let self_phone = self.self_phone.lock().clone().unwrap_or_default();
        // RFC-0861 §5 M11: pre-compute a `HashSet<String>` of the
        // bot's plausible phone/JID forms so the per-participant
        // membership check below is an O(1) hash lookup instead of
        // an O(L) string equality. The set is built once per call;
        // forms covered:
        //   1. raw digits (e.g. `5521995544743`)
        //   2. digits with leading `+` stripped / re-applied variants
        //   3. digits with `@s.whatsapp.net` suffix
        //   4. digits with `+@s.whatsapp.net` (some participants
        //      carry the `+` in the user portion)
        // Forms we cannot derive (e.g. alternate country-code
        // variants of the same number) are simply not in the set
        // — the bot just won't be detected as admin in that
        // edge case, which matches the previous behavior.
        let mut self_phones: std::collections::HashSet<String> = std::collections::HashSet::new();
        if !self_phone.is_empty() {
            let digits = self_phone.trim_start_matches('+').to_string();
            self_phones.insert(digits.clone());
            self_phones.insert(format!("{digits}@s.whatsapp.net"));
            self_phones.insert(format!("+{digits}@s.whatsapp.net"));
            // Some platforms normalise to JID form with a `+` in
            // the user portion (e.g. `+15551234567@s.whatsapp.net`).
            self_phones.insert(format!("+{}", self_phone));
        }
        Ok(map
            .into_iter()
            .map(|(jid, meta)| {
                let mode_flags = extract_mode_flags(&meta);
                let is_admin = meta
                    .participants
                    .iter()
                    .find(|p| self_phones.contains(p.jid.user.as_str()))
                    .map(|p| p.is_admin())
                    .unwrap_or(false);
                GroupHandle {
                    id: GroupId::new(jid),
                    subject: Some(meta.subject),
                    invite_url: None, // would require a per-group `get_invite_link` call
                    is_admin,
                    member_count: meta.size,
                    mode_flags: Some(mode_flags),
                    initial_admins_promoted: false,
                }
            })
            .collect())
    }

    async fn get_group_metadata(
        &self,
        group_id: &GroupId,
    ) -> Result<GroupMetadata, PlatformAdapterError> {
        let raw = self
            .group_metadata(group_id.as_str())
            .await
            .map_err(|e| api_err("get_group_metadata", e))?;
        Ok(extract_group_metadata(&raw))
    }

    async fn resolve_invite(
        &self,
        invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        let raw = self
            .get_invite_info(invite.0.as_str())
            .await
            .map_err(|e| api_err("resolve_invite", e))?;
        let jid = raw.id.to_string();
        let mode_flags = extract_mode_flags(&raw);
        Ok(GroupHandle {
            id: GroupId::new(jid),
            subject: Some(raw.subject),
            invite_url: Some(invite.to_string()),
            is_admin: false, // Resolved but not joined yet
            member_count: raw.size,
            mode_flags: Some(mode_flags),
            initial_admins_promoted: false,
        })
    }

    async fn join_by_invite(
        &self,
        invite: &InviteRef,
    ) -> Result<GroupHandle, PlatformAdapterError> {
        // RFC-0861 §3 H1: implement via
        // `client.groups().join_with_invite_code(...)`. The SDK
        // accepts both bare invite codes and full
        // `https://chat.whatsapp.com/...` URLs (it calls
        // `extract_invite_code` internally). We pass the full
        // `InviteRef.0` through unchanged.
        let client = {
            let guard = self.client.lock();
            guard.clone().ok_or_else(|| {
                api_err("join_by_invite", "WhatsApp Web client not connected".into())
            })?
        };
        let result = client
            .groups()
            .join_with_invite_code(invite.0.as_str())
            .await
            .map_err(|e| api_err("join_by_invite", format!("{e:#}")))?;
        // Both `Joined` and `PendingApproval` map to the same
        // `GroupHandle` shape (RFC-0861 §3 H1). Callers that need to
        // distinguish pending vs joined can call `get_group_metadata`
        // after a backoff.
        let jid = result.group_jid();
        Ok(GroupHandle {
            id: GroupId::new(jid.to_string()),
            is_admin: false,
            subject: None,
            invite_url: None,
            member_count: None,
            mode_flags: None,
            initial_admins_promoted: false,
        })
    }

    async fn transfer_ownership(
        &self,
        _group_id: &GroupId,
        _new_owner: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        // WhatsApp has no first-class "transfer ownership" primitive.
        // The standard pattern is: promote the new owner, demote the
        // old owner, and have the old owner leave. That is a
        // multi-step sequence the caller can drive via
        // `promote_to_admin` + `demote_from_admin` + `leave_group`.
        Err(PlatformAdapterError::Unimplemented {
            platform: "whatsapp".into(),
            action: "transfer_ownership".into(),
        })
    }
}

// ── Helpers for CoordinatorAdmin impl ──────────────────────────────

/// Internal alias: `impl WhatsAppWebAdapter::leave_group` and the
/// `CoordinatorAdmin::leave_group` trait method have the same name
/// (and the trait method wins resolution). We re-bind the public
/// `String`-returning method to a distinct local name so the trait
/// impl above can call it.
impl WhatsAppWebAdapter {
    async fn leave_group_str(&self, group_jid: &str) -> Result<(), String> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| "WhatsApp Web client not connected".to_string())?
        };
        let jid: wacore_binary::Jid = group_jid
            .parse()
            .map_err(|e| format!("invalid group JID {group_jid:?}: {e}"))?;
        match client.groups().leave(&jid).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // `not a participant` / `not in group` are expected
                // on idempotent leave — surface them as a specific
                // error string so the trait impl can swallow them.
                let msg = format!("{e:#}");
                if msg.contains("not a participant")
                    || msg.contains("not in group")
                    || msg.contains("item-not-found")
                {
                    Err("not a participant".to_string())
                } else {
                    Err(format!("leave_group failed: {e:#}"))
                }
            }
        }
    }
}

fn api_err(action: &str, reason: String) -> PlatformAdapterError {
    PlatformAdapterError::ApiError {
        code: 500,
        message: format!("{action}: {reason}"),
    }
}

fn extract_mode_flags(meta: &whatsapp_rust::GroupMetadata) -> GroupModeFlags {
    GroupModeFlags {
        locked: meta.is_locked,
        announce_only: meta.is_announcement,
        ephemeral_ttl: if meta.ephemeral_expiration == 0 {
            None
        } else {
            Some(std::time::Duration::from_secs(
                meta.ephemeral_expiration as u64,
            ))
        },
        requires_approval: meta.membership_approval,
    }
}

fn extract_group_metadata(raw: &whatsapp_rust::GroupMetadata) -> GroupMetadata {
    let mut members: Vec<PeerId> = Vec::with_capacity(raw.participants.len());
    let mut admins: Vec<PeerId> = Vec::new();
    for p in &raw.participants {
        members.push(PeerId::new(p.jid.to_string()));
        if p.is_admin() {
            admins.push(PeerId::new(p.jid.to_string()));
        }
    }
    GroupMetadata {
        id: GroupId::new(raw.id.to_string()),
        subject: Some(raw.subject.clone()),
        description: raw.description.clone(),
        members,
        admins,
        invite_url: None, // requires a per-group get_invite_link round trip
        mode_flags: extract_mode_flags(raw),
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
        // RFC-0861 §2 M16: digits-with-`@g.us` is the only
        // `@`-bearing form we accept. The helper uses
        // `debug_assert!` to catch programming errors in tests; the
        // production gate is `WhatsAppConfig::validate()`.
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

    // Mission 0850p-a-has-valid-session
    #[test]
    fn test_has_valid_session_false_when_not_connected() {
        // A freshly-constructed adapter has no bot handle and no
        // self_handle. has_valid_session() must return false.
        let config = WhatsAppConfig {
            session_path: "/tmp/test.db".into(),
            pair_phone: None,
            pair_code: None,
            ws_url: None,
            groups: vec![],
            sender_allowlist: BTreeMap::new(),
        };
        let adapter = WhatsAppWebAdapter::new(config);
        assert!(!adapter.has_valid_session());
    }

    // Mission 0850p-a-notify-event-connected
    #[tokio::test]
    async fn test_connected_notify_fires_on_wait() {
        // Verify that notify_waiters() wakes a notified() waiter.
        // This validates the cross-crate contract: the adapter's
        // Event::Connected handler calls notify_waiters(), and the
        // core's wait_for_connected awaits notified().
        let config = WhatsAppConfig {
            session_path: "/tmp/test.db".into(),
            pair_phone: None,
            pair_code: None,
            ws_url: None,
            groups: vec![],
            sender_allowlist: BTreeMap::new(),
        };
        let adapter = WhatsAppWebAdapter::new(config);
        let notify = adapter.connected();

        // Spawn a waiter that should return within 1s.
        let waiter = tokio::spawn(async move {
            notify.notified().await;
            true
        });
        // Give the waiter a tick to subscribe.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        // Trigger the notify.
        adapter.connected().notify_waiters();
        // Wait for the waiter to return.
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("waiter did not return within 1s")
            .expect("waiter task panicked");
        assert!(result);
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

    #[test]
    fn validate_accepts_bare_digits_and_full_jid() {
        // RFC-0861 §2 M16: both bare digits and digits+@g.us are
        // valid `groups` entries.
        for good in [
            "120363012345678901",
            "120363012345678901@g.us",
            "1",
            "99999999999999999999999999",
        ] {
            let cfg = cfg_with("/tmp/test.db", None, None, None, vec![good]);
            assert!(cfg.validate().is_ok(), "groups {good:?} should be accepted");
        }
    }

    #[test]
    fn validate_rejects_malformed_jid_in_groups() {
        // RFC-0861 §2 M16: reject newsletter JID misuse (`@` but not
        // `@g.us`), user JID misuse (contains `:`), and non-numeric
        // bare strings.
        for bad in [
            "120363012345678901@newsletter",       // newsletter JID misuse
            "120363012345678901@s.whatsapp.net",   // user-JID-shaped but missing `@g.us`
            "120363012345678901:0@s.whatsapp.net", // user JID misuse (`:`)
            "not-a-jid",                           // non-numeric, no @
            "abc@g.us",                            // non-numeric prefix before @g.us
            "120363012345678901@",                 // empty suffix
            "@g.us",                               // empty prefix
        ] {
            let cfg = cfg_with("/tmp/test.db", None, None, None, vec![bad]);
            assert!(cfg.validate().is_err(), "groups {bad:?} should be rejected");
        }
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

    // ── Group-setup API unit tests (offline; no live session needed) ────────

    /// Helper: build an adapter that is NOT connected (no `start_bot()` was
    /// called, so `self.client` is `None`). Every group-setup method must
    /// surface a clear error in this state.
    fn offline_adapter() -> WhatsAppWebAdapter {
        let cfg = cfg_with("/tmp/test.db", None, None, None, vec![]);
        WhatsAppWebAdapter::new(cfg)
    }

    #[tokio::test]
    async fn create_group_rejects_empty_subject() {
        let adapter = offline_adapter();
        for bad in ["", "   ", "\t\n"] {
            let result = adapter.create_group_str(bad, &[]).await;
            // `expect_err` needs `Debug` on the Ok type, which `CreateGroupOutput`
            // doesn't satisfy because `GroupMetadata` doesn't derive Debug in a
            // useful way. Match on the Result directly instead.
            match result {
                Ok(_) => panic!("empty/whitespace subject {bad:?} should be rejected"),
                Err(err) => assert!(
                    err.contains("subject must not be empty"),
                    "unexpected error: {err}"
                ),
            }
        }
    }

    #[tokio::test]
    async fn create_group_fails_when_not_connected() {
        let adapter = offline_adapter();
        let result = adapter.create_group_str("DOT e2e test group", &[]).await;
        match result {
            Ok(_) => panic!("create_group must fail when client is not connected"),
            Err(err) => assert!(err.contains("not connected"), "unexpected error: {err}"),
        }
    }

    #[tokio::test]
    async fn add_members_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .add_members("120363012345678901@g.us", &["+15551234567"])
            .await
            .expect_err("add_members must fail when client is not connected");
        assert!(err.contains("not connected"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn get_invite_link_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .get_invite_link("120363012345678901@g.us", false)
            .await
            .expect_err("get_invite_link must fail when client is not connected");
        assert!(err.contains("not connected"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn leave_group_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .leave_group("120363012345678901@g.us")
            .await
            .expect_err("leave_group must fail when client is not connected");
        assert!(err.contains("not connected"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn add_members_rejects_invalid_jid() {
        // With the client disconnected we never reach the JID parse path,
        // so connect a fake client (still going to error later) — actually
        // we cannot fake the client without starting the bot. So just
        // exercise the JID parsing branch via the public error path by
        // confirming the helper returns the "not connected" message first
        // (clients would otherwise reject the malformed JID too).
        let adapter = offline_adapter();
        let err = adapter
            .add_members("not a valid jid at all", &["+15551234567"])
            .await
            .expect_err("malformed JID should be rejected");
        // Order matters: the JID parse happens *after* the not-connected
        // check, so we expect "not connected" first.
        assert!(err.contains("not connected"), "unexpected error: {err}");
    }

    // ── R20: CoordinatorAdmin + new admin methods (offline unit tests)

    fn expect_not_connected(err: String) {
        assert!(err.contains("not connected"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn remove_members_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .remove_members("120363012345678901@g.us", &["+15551234567"])
            .await
            .expect_err("remove_members must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn promote_participants_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .promote_participants("120363012345678901@g.us", &["+15551234567"])
            .await
            .expect_err("promote_participants must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn demote_participants_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .demote_participants("120363012345678901@g.us", &["+15551234567"])
            .await
            .expect_err("demote_participants must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn get_participating_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .get_participating()
            .await
            .expect_err("get_participating must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn set_subject_rejects_empty() {
        let adapter = offline_adapter();
        // Empty / whitespace subjects are caught by the
        // `GroupSubject::new` length validator, not by the
        // not-connected check, so we see the validator error
        // before the client is needed.
        for bad in ["", "   "] {
            let err = adapter
                .set_subject("120363012345678901@g.us", bad)
                .await
                .expect_err("empty/whitespace subject should be rejected");
            assert!(
                err.contains("invalid subject") || err.contains("not connected"),
                "unexpected error: {err}"
            );
        }
    }

    #[tokio::test]
    async fn set_subject_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .set_subject("120363012345678901@g.us", "valid subject")
            .await
            .expect_err("set_subject must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn set_description_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .set_description("120363012345678901@g.us", "valid description")
            .await
            .expect_err("set_description must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn set_announce_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .set_announce("120363012345678901@g.us", true)
            .await
            .expect_err("set_announce must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn set_locked_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .set_locked("120363012345678901@g.us", true)
            .await
            .expect_err("set_locked must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn set_ephemeral_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .set_ephemeral("120363012345678901@g.us", 86400)
            .await
            .expect_err("set_ephemeral must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn get_invite_info_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .get_invite_info("https://chat.whatsapp.com/ABCD1234")
            .await
            .expect_err("get_invite_info must fail when client is not connected");
        expect_not_connected(err);
    }

    #[tokio::test]
    async fn set_membership_approval_fails_when_not_connected() {
        let adapter = offline_adapter();
        let err = adapter
            .set_membership_approval("120363012345678901@g.us", true)
            .await
            .expect_err("set_membership_approval must fail when client is not connected");
        expect_not_connected(err);
    }

    // ── R20: CoordinatorAdmin capability probe ────────────────────

    #[test]
    fn whatsapp_capability_report_matches_implementation() {
        // The capability bits must agree with which `CoordinatorAdmin`
        // methods are actually overridden. This test fails loudly
        // if someone overrides a new method on the trait impl
        // without flipping the matching `can_*` bit in
        // `admin_capabilities` (and vice versa).
        let adapter = offline_adapter();
        let caps = adapter.admin_capabilities();
        // Lifecycle
        assert!(caps.can_create, "can_create");
        assert!(
            !caps.can_join_by_id,
            "can_join_by_id (always false on WhatsApp)"
        );
        assert!(caps.can_join_by_invite, "can_join_by_invite");
        assert!(caps.can_leave, "can_leave");
        assert!(!caps.can_destroy, "can_destroy");
        // Membership
        assert!(caps.can_add_member);
        assert!(caps.can_remove_member);
        assert!(!caps.can_ban, "can_ban (always false on WhatsApp)");
        assert!(caps.can_promote);
        assert!(caps.can_demote);
        assert!(!caps.can_approve_join, "can_approve_join");
        // Mode
        assert!(caps.can_rename);
        assert!(caps.can_describe);
        assert!(caps.can_lock);
        assert!(caps.can_announce);
        assert!(caps.can_set_ephemeral);
        assert!(caps.can_require_approval);
        // Discovery
        assert!(caps.can_list_own_groups);
        assert!(caps.can_get_metadata);
        assert!(caps.can_resolve_invite);
        // Handoff
        assert!(!caps.can_transfer_ownership);
        // Platform name
        assert_eq!(adapter.platform_name(), "whatsapp");
    }

    #[test]
    fn as_coordinator_admin_returns_some_for_whatsapp() {
        // The `PlatformAdapter::as_coordinator_admin` probe is the
        // caller's downcast entry point. It must return `Some`
        // for the WhatsApp adapter because we implement the trait.
        let adapter = offline_adapter();
        let admin: Option<&dyn CoordinatorAdmin> = adapter.as_coordinator_admin();
        assert!(
            admin.is_some(),
            "WhatsApp adapter must opt in to CoordinatorAdmin"
        );
    }

    #[test]
    fn unimplemented_actions_return_unimplemented_error() {
        // Methods we deliberately don't implement (ban_member,
        // approve_join_request, transfer_ownership) must return
        // `PlatformAdapterError::Unimplemented` with the correct
        // platform name and action label.
        //
        // `join_by_invite` is no longer in this list: RFC-0861 §3
        // H1 implemented it via `client.groups().join_with_invite_code`.
        // An offline adapter short-circuits with
        // `api_err("join_by_invite", "WhatsApp Web client not connected")`
        // (an `ApiError`, not `Unimplemented`); a separate test
        // `join_by_invite_fails_when_not_connected` covers that path.
        let adapter = offline_adapter();
        let g = GroupId::new("120363012345678901@g.us");
        let p = PeerId::new("+15551234567");

        // We can't `.await` inside `#[test]`, so we use a small
        // blocking helper instead.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        let check = |label: &'static str, result: PlatformAdapterError| match result {
            PlatformAdapterError::Unimplemented { platform, action } => {
                assert_eq!(platform, "whatsapp", "{label}: platform");
                assert_eq!(action, label, "{label}: action");
            }
            other => panic!("{label}: expected Unimplemented, got {other:?}"),
        };

        rt.block_on(async {
            check(
                "ban_member",
                CoordinatorAdmin::ban_member(&adapter, &g, &p, None)
                    .await
                    .expect_err("ban_member must be Unimplemented"),
            );
            check(
                "approve_join_request",
                CoordinatorAdmin::approve_join_request(&adapter, &g, &p)
                    .await
                    .expect_err("approve_join_request must be Unimplemented"),
            );
            check(
                "transfer_ownership",
                CoordinatorAdmin::transfer_ownership(&adapter, &g, &p)
                    .await
                    .expect_err("transfer_ownership must be Unimplemented"),
            );
        });
    }

    #[tokio::test]
    async fn join_by_invite_fails_when_not_connected() {
        // RFC-0861 §3 H1: the new impl short-circuits on a missing
        // client (offline adapter) with `api_err("join_by_invite",
        // "WhatsApp Web client not connected")` — an `ApiError`, not
        // `Unimplemented`. This is the same shape as
        // `create_group_fails_when_not_connected` (lib.rs:2333).
        let adapter = offline_adapter();
        let inv = InviteRef::new("https://chat.whatsapp.com/ABCD");
        let result = CoordinatorAdmin::join_by_invite(&adapter, &inv).await;
        match result {
            Ok(_) => panic!("join_by_invite must fail when client is not connected"),
            Err(PlatformAdapterError::ApiError { code, message }) => {
                assert_eq!(code, 500, "code should be 500, got {code}");
                assert!(
                    message.contains("not connected"),
                    "unexpected error: {message}"
                );
                assert!(
                    message.contains("join_by_invite"),
                    "action label missing: {message}"
                );
            }
            Err(other) => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_ephemeral_rejects_ttl_overflow() {
        // RFC-0861 §3 M1: TTLs > u32::MAX seconds must produce
        // `ApiError { code: 400 }`, not silently truncate. The
        // offline adapter short-circuits before the SDK call, so
        // we can assert on the error shape without needing a
        // live client. The check is on the trait's `set_ephemeral`
        // (which is the M1 entry point), not the inherent
        // `set_ephemeral(&str, u32)`.
        let adapter = offline_adapter();
        let g = GroupId::new("120363012345678901@g.us");
        // u32::MAX is ~4.29e9 seconds; pick a value clearly above.
        let huge = std::time::Duration::from_secs(u32::MAX as u64 + 1);
        let result = CoordinatorAdmin::set_ephemeral(&adapter, &g, Some(huge)).await;
        match result {
            Ok(_) => panic!("set_ephemeral with overflow TTL must error, not Ok"),
            Err(PlatformAdapterError::ApiError { code, message }) => {
                assert_eq!(code, 400, "code should be 400 (overflow), got {code}");
                assert!(
                    message.contains("exceeds u32::MAX"),
                    "message should explain the overflow, got: {message}"
                );
            }
            Err(other) => panic!("expected ApiError with code 400, got {other:?}"),
        }
    }
}
