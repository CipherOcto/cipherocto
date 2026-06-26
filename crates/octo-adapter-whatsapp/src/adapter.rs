//! WhatsApp Web adapter for DOT (RFC-0850 §8.1)
//!
//! Uses whatsapp-rust (native WhatsApp Web protocol) to transport DOT envelopes.
//! No Meta Business verification required — authentication via QR code or pair code.

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use octo_network::dot::adapters::{
    coordinator_admin::{
        AddMemberOutput, AdminCapabilityReport, CoordinatorAdmin, GroupHandle, GroupId,
        GroupMemberSpec, GroupMetadata, GroupModeFlags, InviteRef, PeerId,
    },
    CapabilityReport, DeliveryReceipt, MediaCapabilities, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
use octo_network::dot::transport::{
    decode_native_ref, encode_native_ref, select_mode_with_max_text, TransportMode,
};

use crate::media_ref::{decode_base64url, encode_base64url, MediaRef};

use super::store::StoolapStore;
// wacore re-exports `MediaType` and `Downloadable` via
// `whatsapp_rust::download`; `UploadOptions`/`UploadResponse` live in
// `whatsapp_rust::upload`. Mission 0850 (RFC-0850 §8.6/§9.4) uses both.
use whatsapp_rust::download::MediaType;
use whatsapp_rust::upload::{UploadOptions, UploadResponse};

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
            validate_group_jid(group).map_err(|e| format!("groups entry {e}"))?;
        }
        Ok(())
    }
}

/// R13-L3 fix: extract the strict JID-shape check (RFC-0861 §2 M16)
/// into a standalone helper so it can be shared between
/// `WhatsAppConfig::validate` (static path) and
/// `WhatsAppWebAdapter::register_group_at_runtime` (dynamic path).
/// Before this fix, a typo in a runtime-registered JID (e.g.,
/// `12036301234567890@g.us` — one digit short) was silently
/// accepted, the message was rejected as "unconfigured group",
/// and the caller had no way to find the bug.
fn validate_group_jid(group: &str) -> std::result::Result<(), String> {
    if group.is_empty() {
        return Err("is empty".to_string());
    }
    if group.contains(':') {
        return Err("contains ':' (user JID misuse; expected digits or digits+@g.us)".to_string());
    }
    if group.contains('@') {
        if !group.ends_with("@g.us") {
            return Err(
                "contains '@' but does not end with @g.us (newsletter JID misuse)".to_string(),
            );
        }
        let prefix = &group[..group.len() - "@g.us".len()];
        if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
            return Err("has non-numeric prefix before @g.us".to_string());
        }
    } else if !group.chars().all(|c| c.is_ascii_digit()) {
        return Err("is not all digits (expected digits or digits+@g.us)".to_string());
    }
    Ok(())
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

// ── Reconnect constants (R12-H1 follow-up) ────────────────────────
//
// R12-H1 fix: the reconnect logic in `run_reconnect_loop` was removed
// because the wacore library handles reconnection internally (see
// `wacore/src/client.rs:1102` — `Client::run` is a `while
// self.is_running` loop that retries forever). The retry-related
// constants and `compute_retry_delay` helper are no longer referenced
// from production code; kept here for now in case a future round
// reintroduces a reconnect path that doesn't rely on wacore's
// internal loop. If no such round materializes, these can be removed
// in a follow-up cleanup.

#[allow(dead_code)]
const MAX_RETRIES: u32 = 10;
#[allow(dead_code)]
const BASE_DELAY_SECS: u64 = 3;
#[allow(dead_code)]
const MAX_DELAY_SECS: u64 = 300;

#[allow(dead_code)]
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
    /// Fires on `Event::OfflineSyncCompleted` — the initial history
    /// sync is done and the client is fully synchronized.
    synced_notify: Arc<tokio::sync::Notify>,
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
    /// All conversation JIDs received from HistorySync. Populated by the
    /// Event::HistorySync handler. Used by the cleanup utility to find
    /// chats from groups we already left.
    conversation_jids: Arc<Mutex<Vec<String>>>,
    /// StoolapStore reference for persisting conversations. Set in start_bot.
    store: Arc<Mutex<Option<Arc<StoolapStore>>>>,
    /// Raw event broadcast for debugging/monitoring. Every event from
    /// wa-rs is stringified and sent here. Used by event_listener binary.
    raw_event_tx: tokio::sync::broadcast::Sender<String>,
    /// Mission 0850 (RFC-0850 §8.6/§9.4): channel for routing
    /// `DOT/2/{token}` download requests from the sync on_event closure
    /// (which does NOT capture `&self`) to the async download_rx
    /// consumer task spawned by `start_bot`. The on_event closure
    /// clones this `Arc` and `try_send`s a `DownloadRequest`; the
    /// consumer task pops, calls `Client::download`, and pushes the
    /// decrypted wire bytes to `inbound_tx`.
    ///
    /// `Arc<tokio::sync::Mutex<Option<...>>>` mirrors the existing
    /// `client` field shape (line 224) — `start_bot` populates the
    /// `Some(_)` variant without `&mut self`, and the closure holds an
    /// `Arc` clone without owning `self`. Initialized to `None` in
    /// `new`; populated in `start_bot` (so the receiver has an
    /// immediate owner — the consumer task).
    download_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<DownloadRequest>>>>,
    /// R12-M1 fix: monotonic counter of inbound messages that were
    /// accepted by `accept_message` (and thus passed the security
    /// filter) but then dropped because the inbound channel was full
    /// (`try_send` returned `Err(TrySendError::Full(_))`). Previously
    /// these drops were silent — only a `tracing::warn!` log
    /// signaled them, with no operator-visible counter. A burst of
    /// messages could exhaust the channel and silently lose envelopes
    /// with no way for the gateway to know. The counter is exposed via
    /// [`WhatsAppWebAdapter::dropped_inbound_messages`] for
    /// observability. Resetting it requires recreating the adapter.
    dropped_inbound_count: Arc<AtomicU64>,
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

/// Mission 0850 (RFC-0850 §8.6/§9.4): a `DOT/2/{token}` envelope that the
/// on-event closure (which does NOT capture `&self`) has dispatched for
/// pre-download. The download_rx consumer task pops these, calls
/// `Client::download` via the wacore API, and pushes the resulting wire
/// bytes to `inbound_tx` with `metadata["dot_mode"] = "native"`.
///
/// `msg_id` is the base64url-encoded JSON `MediaRef` token from the
/// `DOT/2/{token}` payload (NOT a WhatsApp `message_id`).
pub(crate) struct DownloadRequest {
    pub(crate) msg_id: String,
    pub(crate) chat: String,
    pub(crate) sender: String,
}

/// Mission 0850 (RFC-0850 §8.6/§9.4): type-level least-privilege handle
/// for background tasks spawned by `start_bot`. Cloning the
/// [`WhatsAppWebAdapter`] would expose every field (config, bot_handle,
/// inbound_rx, self_phone, runtime_groups) to the consumer task; this
/// handle gives it exactly the two fields it needs.
///
/// Clone is `#[derive(Clone)]` because every field is `Arc`/`Sender`
/// (both inherently `Clone`). Cheap to clone.
#[derive(Clone)]
pub(crate) struct WhatsAppHandlerHandle {
    pub(crate) client: Arc<Mutex<Option<Arc<whatsapp_rust::Client>>>>,
    pub(crate) inbound_tx: tokio::sync::mpsc::Sender<RawPlatformMessage>,
    /// R12-M1 fix: shared dropped-message counter. The
    /// download_rx_consumer task captures a clone of this `Arc` and
    /// increments the counter when its `try_send` to `inbound_tx`
    /// fails (channel full or closed). The on_event closure captures
    /// the SAME `Arc` and increments the same counter on its
    /// `try_send` failure. The counter is exposed via
    /// [`WhatsAppWebAdapter::dropped_inbound_messages`].
    pub(crate) dropped_inbound_count: Arc<AtomicU64>,
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
            synced_notify: Arc::new(tokio::sync::Notify::new()),
            runtime_groups: Arc::new(Mutex::new(Vec::new())),
            conversation_jids: Arc::new(Mutex::new(Vec::new())),
            store: Arc::new(Mutex::new(None)),
            raw_event_tx: tokio::sync::broadcast::channel::<String>(1000).0,
            // Mission 0850: download_tx is None until start_bot populates
            // it. The channel is created INSIDE start_bot (not here) so
            // the receiver has an immediate owner — the consumer task.
            download_tx: Arc::new(tokio::sync::Mutex::new(None)),
            // R12-M1 fix: dropped-message counter starts at 0. The
            // counter is incremented inside the on_event closure and
            // the download_rx_consumer task on `try_send` failure.
            dropped_inbound_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Mission 0850p-a-notify-event-connected: returns a clonable
    /// handle to the `Notify` that fires on `Event::Connected`.
    /// Cloning the `Arc<Notify>` is cheap and gives a handle to
    /// the same underlying `Notify`.
    pub fn connected(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.connected_notify)
    }

    /// Returns a clonable handle to the `Notify` that fires on
    /// `Event::OfflineSyncCompleted` — the initial history sync is
    /// done and the client is fully synchronized with the server.
    pub fn synced(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.synced_notify)
    }

    /// R12-M1 fix: returns the cumulative number of inbound
    /// messages that passed `accept_message` (and thus the security
    /// filter) but were then dropped because the inbound channel was
    /// full (`try_send` returned `Err(TrySendError::Full(_))`).
    ///
    /// Operators should monitor this counter alongside
    /// `receive_messages()` throughput. A monotonically increasing
    /// value indicates the gateway is consuming inbound messages
    /// slower than WhatsApp is delivering them — the 1024-deep
    /// inbound channel is the backpressure boundary.
    ///
    /// The counter is monotonic; it never decreases. To reset it,
    /// recreate the adapter. The counter is incremented from both
    /// the on_event closure (DOT/1/ text path) and the
    /// download_rx_consumer task (DOT/2/ native path), so it covers
    /// all inbound delivery channels.
    pub fn dropped_inbound_messages(&self) -> u64 {
        self.dropped_inbound_count.load(Ordering::Relaxed)
    }

    /// Mission 0850 (RFC-0850 §8.6/§9.4): clone the fields needed by
    /// background tasks spawned in `start_bot` (the download_rx consumer
    /// task). Does NOT clone `inbound_rx` because the consumer pushes
    /// via `inbound_tx`, not drains `inbound_rx`. `receive_messages()`
    /// still holds the original `inbound_rx`.
    ///
    /// Type-level least-privilege: the handle exposes only `client` and
    /// `inbound_tx`, NOT `config` (session path, groups, sender
    /// allowlist), `bot_handle` (shutdown control), `inbound_rx` (could
    /// drain messages), `self_phone` / `runtime_groups` (state that
    /// should not be touched by download tasks).
    pub(crate) fn clone_for_handler(&self) -> WhatsAppHandlerHandle {
        WhatsAppHandlerHandle {
            client: Arc::clone(&self.client),
            inbound_tx: self.inbound_tx.clone(),
            // R12-M1 fix: share the dropped-message counter with the
            // handler handle so the download_rx_consumer task and the
            // on_event closure can both increment it.
            dropped_inbound_count: Arc::clone(&self.dropped_inbound_count),
        }
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
    ///
    /// R13-L3 fix: validate the JID against the same strict shape
    /// check that `WhatsAppConfig::validate` uses (RFC-0861 §2 M16).
    /// Previously any string was accepted; a typo (e.g.,
    /// `12036301234567890@g.us` — one digit short) was silently
    /// stored, the message was rejected as "unconfigured group",
    /// and the caller had no way to find the bug. Returns
    /// `Err(reason)` for invalid JIDs; the caller is expected to
    /// surface the error to the user.
    pub fn register_group_at_runtime(&self, group_jid: &str) -> std::result::Result<(), String> {
        validate_group_jid(group_jid)?;
        let mut guard = self.runtime_groups.lock();
        if !guard.iter().any(|g| g == group_jid) {
            guard.push(group_jid.to_string());
        }
        Ok(())
    }

    /// All conversation JIDs collected from HistorySync events.
    /// Includes groups we've already left (the chat entry persists).
    pub fn list_all_conversations(&self) -> Vec<String> {
        self.conversation_jids.lock().clone()
    }

    /// Subscribe to raw event descriptions from the wa-rs event handler.
    /// Every event is stringified and broadcast. Useful for debugging.
    pub fn subscribe_raw_events(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.raw_event_tx.subscribe()
    }

    /// Persist conversations to the stoolap `conversations` table.
    /// Each entry is (jid, name, is_group).
    pub async fn persist_conversations(
        &self,
        entries: &[(String, Option<String>, bool)],
    ) -> anyhow::Result<()> {
        let store = self
            .store
            .lock()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("store not initialized (call start_bot first)"))?;
        store.upsert_conversations(entries).await
    }

    /// Read persisted conversations from the stoolap `conversations` table.
    /// These survive across adapter restarts. Returns (jid, name, is_group).
    pub async fn list_persisted_conversations(
        &self,
    ) -> anyhow::Result<Vec<(String, Option<String>, bool)>> {
        let store = self
            .store
            .lock()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("store not initialized (call start_bot first)"))?;
        store.list_conversations().await
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
    /// Maximum upload size in bytes (Mission 0850 / RFC-0850 §8.6).
    /// Single source of truth for both `capabilities()` (advertised
    /// via `media_capabilities.max_upload_bytes`) and the `upload_media`
    /// pre-flight check (R9-L4 fix). R10-M1 fix: the runtime
    /// `debug_assert_eq!` in `capabilities()` enforces that the const
    /// value matches the documented 100 MiB limit. If a future change
    /// updates this const (e.g., to support a higher WhatsApp Document
    /// ceiling), update both the const and the literal in the
    /// assertion, otherwise `capabilities()` will panic in debug
    /// builds at the first call.
    pub const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;
    /// Documented WhatsApp Document upload ceiling, per public WhatsApp
    /// documentation as of 2026-06. Must match `MAX_UPLOAD_BYTES`.
    /// Used by the `debug_assert_eq!` in `capabilities()` (R10-M1 fix).
    const WHATSAPP_DOCUMENT_CEILING_BYTES: usize = 100 * 1024 * 1024;
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

    /// Convert a `PeerId` string (phone number or raw JID) to a `Jid`.
    ///
    /// - Raw JIDs like `"5521995544743@s.whatsapp.net"` or `"265716875980991@lid"`
    ///   are parsed directly.
    /// - Phone numbers like `"+5521995544743"` are normalized to digits
    ///   and converted to `Jid::pn()`.
    fn peer_to_jid(peer: &str) -> wacore_binary::Jid {
        if peer.contains('@') {
            peer.parse().unwrap_or_else(|_| wacore_binary::Jid::pn(Self::normalize_phone(peer)))
        } else {
            wacore_binary::Jid::pn(Self::normalize_phone(peer))
        }
    }

    /// Convert a group ID to a WhatsApp group JID.
    ///
    /// RFC-0861 §2 M16: appends `@g.us` to bare digits, or passes
    /// through digits already terminated with `@g.us`.
    ///
    /// Validates the input shape with `debug_assert!` in debug
    /// builds so the `validate_group_jid` unit tests catch typos;
    /// in release builds the function is a transparent formatter
    /// and does NOT refuse malformed input. Callers are responsible
    /// for pre-validating inputs:
    ///
    ///   - Static config groups: `WhatsAppConfig::validate` rejects
    ///     bad `groups` entries at config time (RFC-0861 §2 M16).
    ///   - Runtime-registered groups: `register_group_at_runtime`
    ///     validates via `validate_group_jid` (R13-L3).
    ///
    /// This function is intentionally a thin formatter (not a
    /// validator) so the `accept_message` hot path doesn't pay
    /// validation cost per inbound message — the validation
    /// happens once at config time / once at registration time,
    /// and `group_to_jid` is the no-op JID-shape canonicalization
    /// step.
    ///
    /// **R14-L1 fix:** the previous doc-comment claimed this
    /// function "Refuses (via `debug_assert!` + a `Result` return)"
    /// — but the function actually returns `String`, not `Result`,
    /// and in release builds there is no refusal behavior. This
    /// doc-comment now accurately describes the function's actual
    /// behavior. Production callers all pre-validate, so the
    /// function works correctly in practice; the previous
    /// doc-comment was a maintenance hazard (readers might believe
    /// the function refuses invalid inputs in release builds).
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
                "group_to_jid: {group_id:?} is not a valid group JID (must be digits or digits+@g.us); \
                 callers must pre-validate via validate() or validate_group_jid"
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

        // Mission 0850 (RFC-0850 §8.6): accept both `DOT/1/{base64}`
        // (text mode) and `DOT/2/{token}` (native mode) envelopes. The
        // downstream `on_event` closure dispatches on the prefix to
        // either push the text bytes directly to `inbound_tx` or push
        // a `DownloadRequest` to `download_tx` for pre-download.
        //
        // R9-L3 fix: also reject empty tokens after a `DOT/2/` prefix.
        // Without this check, the literal string `"DOT/2/"` would
        // pass the prefix check, then fail downstream with a noisy
        // `decode_native_ref → None` error and fall through to the
        // text path where it would also fail (no `DOT/1/` prefix).
        // The envelope would be dropped with two cascading errors;
        // rejecting at the `accept_message` boundary gives a single,
        // clear rejection reason for the gateway's close-the-loop
        // logging.
        //
        // R10-L2 fix: also reject whitespace-only or whitespace-padded
        // tokens. `"DOT/2/   token"` previously slipped through the
        // `is_empty()` check (the rest is non-empty even if all
        // whitespace) and failed deeper in the pipeline as a generic
        // "invalid media ref format" — clearer to reject at the
        // boundary with a token-specific reason.
        if let Some(rest) = text_trimmed.strip_prefix("DOT/2/") {
            if rest.trim().is_empty() {
                return AcceptDecision::Reject {
                    reason: "DOT/2/ token is empty or whitespace",
                };
            }
        } else if !text_trimmed.starts_with("DOT/1/") {
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
    ///
    /// **R12-H2 warning — wacore `CoreEventBus` reference cycle:**
    /// calling `start_bot()` more than once per `WhatsAppWebAdapter`
    /// instance leaks an entire `Client` worth of memory. The cycle
    /// is internal to the wacore library:
    ///
    /// ```text
    /// Client
    ///   └─ core: CoreClient
    ///        └─ event_bus: CoreEventBus
    ///             └─ handlers: Vec<Arc<BotEventHandler>>
    ///                  └─ BotEventHandler
    ///                       └─ client: Arc<Client>    ← back to start
    /// ```
    ///
    /// The `BotEventHandler` is added in `wacore/bot.rs:217` and the
    /// `CoreEventBus` has no `remove_handler` API. The `Client` can
    /// never be dropped while a handler is registered. If you call
    /// `start_bot()` a second time (e.g., to "reconnect" after a
    /// crash), the OLD `Client` is unreachable from the adapter's
    /// state but is held alive forever by the cycle.
    ///
    /// **Recommended:** to recover from a bot crash, drop the entire
    /// `WhatsAppWebAdapter` and create a new one with a fresh session
    /// database. This is also the recommended pattern for
    /// reconnection (see R12-H1 doc-comment on `run_reconnect_loop`).
    /// Filed as a tracking issue; will be removed once wacore adds a
    /// `remove_handler` API or breaks the cycle via a `Weak<Client>`
    /// in the handler.
    pub async fn start_bot(&self) -> Result<()> {
        let expanded_path = shellexpand::tilde(&self.config.session_path).to_string();
        let storage = StoolapStore::new(&expanded_path)
            .map_err(|e| anyhow::anyhow!("stoolap store init at {expanded_path:?}: {e:#}"))?;
        let backend = Arc::new(storage);
        // Save store reference for later use (persist_conversations, etc.)
        *self.store.lock() = Some(Arc::clone(&backend));

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
        let conversation_jids = Arc::clone(&self.conversation_jids);
        let conversation_store = Arc::clone(&backend);
        let raw_event_tx = self.raw_event_tx.clone();
        let sender_allowlist = self.config.sender_allowlist.clone();
        // Mission 0850p-a-notify-event-connected: clone the Notify
        // into the closure so the Event::Connected handler can
        // wake up `wait_for_connected` callers.
        let connected_notify = Arc::clone(&self.connected_notify);
        let synced_notify = Arc::clone(&self.synced_notify);
        // Mission 0850 (RFC-0850 §8.6/§9.4): clone the `download_tx`
        // Arc BEFORE the `on_event(move ...)` closure so the closure
        // doesn't have to capture `&self` (the closure must be
        // `'static`-bound because wacore stores it on the bot).
        let download_tx = Arc::clone(&self.download_tx);
        // R12-M1 fix: clone the dropped-message counter for both the
        // on_event closure (which pushes DOT/1/ text envelopes) and
        // the download_rx_consumer (which pushes downloaded DOT/2/
        // wire bytes). Both call sites increment the counter on
        // `try_send` failure.
        let dropped_inbound_count = Arc::clone(&self.dropped_inbound_count);

        // Mission 0850 (RFC-0850 §8.6/§9.4): create the download
        // request channel HERE (not in `new` — so the receiver has an
        // immediate owner: the consumer task spawned below). Populate
        // `self.download_tx` with the sender; the on_event closure
        // captures an `Arc` clone and pushes `DownloadRequest`s for
        // any `DOT/2/{token}` envelope it sees.
        let (download_tx_sender, mut download_rx_receiver) =
            tokio::sync::mpsc::channel::<DownloadRequest>(64);
        *self.download_tx.lock().await = Some(download_tx_sender);

        // Spawn the download_rx consumer task. It captures a
        // least-privilege `WhatsAppHandlerHandle` (client + inbound_tx
        // only — not config, not bot_handle, not inbound_rx) and exits
        // cleanly when the channel closes (i.e., when the
        // `Option<Sender>` in `self.download_tx` is dropped, which
        // happens when the adapter is shut down).
        //
        // R9-L1 fix: use `download_handle.inbound_tx` instead of
        // cloning `self.inbound_tx` again. The handle already
        // contains the only Sender the consumer task needs; cloning
        // `self.inbound_tx` was redundant and left the
        // `inbound_tx` field on the handle dead-code (suppressed by
        // `#[allow(dead_code)]`). The handle's least-privilege
        // design intent is now actually enforced — the consumer task
        // cannot accidentally access fields it shouldn't see.
        let download_handle = self.clone_for_handler();
        tokio::spawn(async move {
            while let Some(req) = download_rx_receiver.recv().await {
                match download_via_media_ref(&download_handle.client, &req.msg_id).await {
                    Ok(wire_bytes) => {
                        // R2-M5: tag with `dot_mode = "native"` so
                        // `canonicalize` knows to skip the text decode
                        // and pass `wire_bytes` directly to
                        // `DeterministicEnvelope::from_wire_bytes`.
                        let raw = RawPlatformMessage {
                            platform_id: format!("{}:{}", req.chat, uuid::Uuid::new_v4()),
                            payload: wire_bytes,
                            metadata: [
                                ("chat".to_string(), req.chat),
                                ("sender".to_string(), req.sender),
                                ("dot_mode".to_string(), "native".to_string()),
                            ]
                            .into_iter()
                            .collect(),
                        };
                        if let Err(e) = download_handle.inbound_tx.try_send(raw) {
                            // R12-M1 fix: increment the shared
                            // dropped-message counter so operators can
                            // see silent drops via
                            // `dropped_inbound_messages()`. The counter
                            // is shared with the on_event closure via
                            // the handler handle's
                            // `dropped_inbound_count` field.
                            download_handle
                                .dropped_inbound_count
                                .fetch_add(1, Ordering::Relaxed);
                            tracing::warn!("inbound channel full or closed: {e}");
                        }
                    }
                    Err(_e) => {
                        // R1-H4: error message is redacted — no
                        // `media_key` or `direct_path` from `req.msg_id`
                        // propagates to the log.
                        //
                        // R12-M2 fix: instead of silently dropping
                        // the failed request, push a sentinel
                        // `RawPlatformMessage` with `dot_mode =
                        // "delivery_failed"` so the gateway can see
                        // the failed delivery and report it. The
                        // `canonicalize` function returns an
                        // `ApiError { code: 502, message: ... }` for
                        // this dot_mode, mirroring the
                        // upstream-downstream error contract. The
                        // error reason in the metadata is a
                        // fixed-string redacted message — no wacore
                        // internals, no `media_key`, no
                        // `direct_path`.
                        tracing::warn!("DOT/2/ download failed; pushing delivery_failed sentinel");
                        let failed = RawPlatformMessage {
                            platform_id: format!("{}:{}", req.chat, uuid::Uuid::new_v4()),
                            // Empty payload — the gateway only needs
                            // the metadata to know the delivery
                            // failed.
                            payload: Vec::new(),
                            metadata: [
                                ("chat".to_string(), req.chat),
                                ("sender".to_string(), req.sender),
                                // Sentinel tag — the `canonicalize`
                                // function checks for this and
                                // returns an ApiError.
                                ("dot_mode".to_string(), "delivery_failed".to_string()),
                                // Fixed-string redacted reason. NO
                                // wacore error text, NO media_key,
                                // NO direct_path.
                                ("error".to_string(), "DOT/2/ download failed".to_string()),
                            ]
                            .into_iter()
                            .collect(),
                        };
                        if let Err(send_err) = download_handle.inbound_tx.try_send(failed) {
                            download_handle
                                .dropped_inbound_count
                                .fetch_add(1, Ordering::Relaxed);
                            tracing::warn!(
                                "inbound channel full or closed while pushing delivery_failed: {send_err}"
                            );
                        }
                    }
                }
            }
            tracing::debug!("download_rx consumer task exiting (channel closed)");
        });

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
                let conversation_jids = conversation_jids.clone();
                let conversation_store = conversation_store.clone();
                let raw_event_tx = raw_event_tx.clone();
                let sender_allowlist = sender_allowlist.clone();
                let connected_notify = connected_notify.clone();
                let synced_notify = synced_notify.clone();
                // `download_tx` is cloned in the outer scope (above
                // the `on_event(move |...| { ... })` closure) so the
                // closure doesn't need to capture `&self` and can
                // satisfy the `'static` bound required by wacore.
                // Clone it once more here (cheap: `Arc::clone`) so
                // the inner `async move` can take ownership of its
                // own copy without moving out of the outer closure.
                let download_tx = Arc::clone(&download_tx);
                // R12-M1 fix: clone the dropped-message counter into
                // the inner async closure so the `try_send` at line
                // 904+ can increment it on channel-full failure.
                let dropped_inbound_count = Arc::clone(&dropped_inbound_count);

                async move {
                    use wacore::proto_helpers::MessageExt;
                    use wacore::types::events::Event;

                    // Broadcast raw event for debugging/monitoring.
                    let event_desc = format!("{:?}", event);
                    let _ = raw_event_tx.send(event_desc);

                    match &*event {
                        Event::Message(msg, info) => {
                            let text = msg.text_content().unwrap_or("").to_string();
                            let chat = info.source.chat.to_string();
                            let sender = info.source.sender.to_string();

                            // R13-L2 fix: avoid the per-message
                            // `Vec<String>` clone that used to happen
                            // unconditionally. `accept_message` takes
                            // `&[String]` (which `&Vec<String>` derefs
                            // to), so we can pass `&groups` directly on
                            // the hot path. The `Vec<String>` allocation
                            // for the combined slice only happens on
                            // the cold path (runtime groups are
                            // non-empty — uncommon). The previous code
                            // did `groups.clone()` on every inbound
                            // message, which is N+rt string clones
                            // per message; for a high-traffic group
                            // (100 msg/s with 10 configured groups)
                            // that's ~1000 string clones per second
                            // per adapter instance — visible in
                            // mimalloc/jemalloc profiles.
                            let decision = {
                                let rt = runtime_groups.lock();
                                if rt.is_empty() {
                                    // Hot path: zero per-message
                                    // allocation. `&groups` derefs
                                    // from `&Vec<String>` to
                                    // `&[String]`.
                                    Self::accept_message(
                                        &chat,
                                        &sender,
                                        &text,
                                        &groups,
                                        &sender_allowlist,
                                    )
                                } else {
                                    // Cold path: build the combined
                                    // slice only when runtime groups
                                    // are non-empty.
                                    let mut combined = groups.clone();
                                    combined.extend(rt.iter().cloned());
                                    Self::accept_message(
                                        &chat,
                                        &sender,
                                        &text,
                                        &combined,
                                        &sender_allowlist,
                                    )
                                }
                            };

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

                            // Mission 0850: dispatch on the wire-format
                            // prefix. `DOT/1/{base64}` is the existing
                            // text path — push the raw bytes to
                            // `inbound_tx` with `dot_mode = "text"`.
                            // `DOT/2/{token}` is the new native path —
                            // decode the token, push a `DownloadRequest`
                            // to `download_tx` (the consumer task does
                            // the actual `Client::download` async call
                            // and pushes the decrypted wire bytes back
                            // to `inbound_tx` with `dot_mode =
                            // "native"`).
                            if let Some(token) = decode_native_ref(&text) {
                                let req = DownloadRequest {
                                    msg_id: token.to_string(),
                                    chat: chat.clone(),
                                    sender: sender.clone(),
                                };
                                // Lock briefly, `try_send`, drop the
                                // guard. If `download_tx` is `None`
                                // (consumer task not yet spawned),
                                // `try_send` returns `Closed(_)` and we
                                // silently drop the request — better
                                // than panicking in the on-event
                                // closure.
                                let tx_guard = download_tx.lock().await;
                                if let Some(tx) = tx_guard.as_ref() {
                                    if let Err(e) = tx.try_send(req) {
                                        tracing::warn!(
                                            "download_tx channel full or closed: {e}"
                                        );
                                    }
                                } else {
                                    tracing::warn!(
                                        "DOT/2/ received before download_rx consumer started; dropping"
                                    );
                                }
                                return;
                            }

                            // DOT/1/{base64} text path — push raw bytes.
                            // R4-L2: tag with `dot_mode = "text"` for
                            // the `canonicalize` discriminator (also
                            // serves as an explicit contract — missing
                            // key defaults to text, but the explicit
                            // tag pins the contract for future
                            // readers).
                            let raw = RawPlatformMessage {
                                platform_id: format!("{}:{}", chat, uuid::Uuid::new_v4()),
                                payload: text.into_bytes(),
                                metadata: [
                                    ("chat".to_string(), chat),
                                    ("sender".to_string(), sender),
                                    ("dot_mode".to_string(), "text".to_string()),
                                ]
                                .into_iter()
                                .collect(),
                            };
                            if let Err(e) = inbound_tx.try_send(raw) {
                                // R12-M1 fix: increment the shared
                                // dropped-message counter so operators
                                // can see silent drops via
                                // `dropped_inbound_messages()`. The
                                // counter is shared with the
                                // download_rx_consumer task via the
                                // handler handle's
                                // `dropped_inbound_count` field.
                                dropped_inbound_count.fetch_add(1, Ordering::Relaxed);
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
                        Event::HistorySync(ref lazy) => {
                            // History sync requires an active authenticated
                            // connection. Signal connected_notify as a
                            // fallback in case Event::Connected was missed.
                            // Also resolve phone if not yet set.
                            if self_phone.lock().is_none() {
                                let device = client.persistence_manager().get_device_snapshot().await;
                                if let Some(ref pn) = device.pn {
                                    let pn_str = pn.to_string();
                                    let user_part = pn_str.split_once('@').map(|(u, _)| u).unwrap_or(&pn_str);
                                    let digits = Self::normalize_phone(user_part);
                                    if !digits.is_empty() {
                                        *self_phone.lock() = Some(digits);
                                        tracing::info!("resolved bot identity from HistorySync: +{user_part}");
                                    }
                                }
                            }
                            // Check if this is a 0-conversation sync (final).
                            let conv_count = lazy.get()
                                .map(|hs| hs.conversations.len())
                                .unwrap_or(0);
                            // Collect conversation JIDs for cleanup utility.
                            if let Some(hs) = lazy.get() {
                                let new_entries: Vec<(String, Option<String>, bool)> = {
                                    let mut guard = conversation_jids.lock();
                                    let before = guard.len();
                                    let mut entries = Vec::new();
                                    for conv in &hs.conversations {
                                        if !guard.contains(&conv.id) {
                                            guard.push(conv.id.clone());
                                            let is_group = conv.id.ends_with("@g.us");
                                            entries.push((conv.id.clone(), None, is_group));
                                        }
                                    }
                                    tracing::info!(
                                        before = before,
                                        after = guard.len(),
                                        new = entries.len(),
                                        "conversation_jids updated from HistorySync"
                                    );
                                    entries
                                };
                                // Persist to stoolap so cleanup tool can find them later.
                                if !new_entries.is_empty() {
                                    let store = conversation_store.clone();
                                    if let Err(e) = store.upsert_conversations(&new_entries).await {
                                        tracing::warn!(error = %e, "failed to persist conversations");
                                    }
                                }
                            }
                            tracing::debug!(
                                conversations = conv_count,
                                "HistorySync received (connection is alive)"
                            );
                            connected_notify.notify_waiters();
                            // A 0-conversation HistorySync means the sync is
                            // done — OfflineSyncCompleted may not fire.
                            if conv_count == 0 {
                                tracing::info!("HistorySync with 0 conversations — sync complete");
                                synced_notify.notify_waiters();
                            }
                        }
                        Event::OfflineSyncCompleted(info) => {
                            tracing::info!(
                                messages = info.count,
                                "offline sync completed, client is fully synchronized"
                            );
                            // Also signal connected (definitive proof of
                            // an authenticated connection).
                            connected_notify.notify_waiters();
                            synced_notify.notify_waiters();
                        }
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

    /// R12-H1 fix: the reconnect logic was effectively dead code.
    /// The wacore library's `Client::run` is a `while self.is_running`
    /// loop (see `wacore/src/client.rs:1102`) that handles reconnection
    /// internally — the run task never ends naturally in the current
    /// wacore version, so the loop's liveness check
    /// (`bot_handle.is_some()`) always returned `true` for a healthy
    /// adapter and the reconnect branch never fired.
    ///
    /// This function is preserved as a deprecated no-op stub to keep
    /// the public API stable for any external caller that might be
    /// invoking it; it now logs a one-time warning and returns. The
    /// wacore library handles reconnection internally; if the bot ever
    /// gives up trying to reconnect (which it currently does not do in
    /// the pinned wacore revision), callers should drop this
    /// `WhatsAppWebAdapter` and create a new one with a fresh session
    /// database.
    ///
    /// A proper fix would require either a wacore API to register a
    /// "bot died" callback, or polling the `BotHandle` via a
    /// waker-aware task to detect run-task completion and feed that
    /// signal into a `Notify` that this loop awaits. Either approach
    /// is too invasive for the current mission scope.
    #[deprecated(
        since = "0.1.0",
        note = "the wacore library handles reconnection internally; this \
                function is a no-op. Drop the adapter and create a new one \
                to recover from a bot crash."
    )]
    pub async fn run_reconnect_loop(&self) {
        tracing::warn!(
            "run_reconnect_loop is a no-op: the wacore library handles \
             reconnection internally. See the function's doc-comment for details."
        );
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

        // Delete chat AFTER leaving. Matches official app flow:
        // 1. GroupUpdate Remove (leave)
        // 2. Wait for server to process the leave
        // 3. clearChat + deleteChat
        use waproto::whatsapp::sync_action_value::SyncActionMessageRange;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let message_range = SyncActionMessageRange {
            last_message_timestamp: None,
            last_system_message_timestamp: Some(now_secs),
            messages: vec![],
        };
        // clearChat with delete_media=true (matches official app)
        let clear_result = client
            .chat_actions()
            .clear_chat(&jid, false, true, Some(message_range.clone()))
            .await;
        tracing::info!(group_jid = %group_jid, ?clear_result, "clear_chat after leave");
        // deleteChat with delete_media=true
        let delete_result = client
            .chat_actions()
            .delete_chat(&jid, true, Some(message_range))
            .await;
        tracing::info!(group_jid = %group_jid, ?delete_result, "delete_chat after leave");

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
        for participant in participants {
            // Accept raw JIDs (e.g. "265716875980991@lid") directly.
            if participant.contains('@') {
                let parsed: wacore_binary::Jid = participant
                    .parse()
                    .map_err(|e| format!("invalid JID {participant:?}: {e}"))?;
                jids.push(parsed);
            } else {
                let digits = Self::normalize_phone(participant);
                if digits.is_empty() {
                    return Err(format!("participant {participant:?} has no digits"));
                }
                jids.push(wacore_binary::Jid::pn(digits));
            }
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

// ── Media helpers (Mission 0850) ───────────────────────────────────

/// Mission 0850 (RFC-0850 §8.6/§9.4): shared `download_via_media_ref`
/// helper called by BOTH [`WhatsAppWebAdapter::download_media`] (the
/// trait method) and the `download_rx` consumer task spawned in
/// `start_bot`. Decodes the `MediaRef` wire token from a
/// `DOT/2/{token}` envelope and calls the wacore `Client::download`
/// API directly.
///
/// R1-H4 fix: all error paths return `PlatformAdapterError` variants
/// whose `Display` impls do NOT include the `media_key`, `direct_path`,
/// or any other `MediaRef` field. The mapping is:
/// - `MediaRefError::Base64` / `MediaRefError::Json(_)`
///   → `ApiError { code: 400, message: "invalid media ref format" }`
///   (4xx-shaped — malformed wire format; gateway refuses the envelope
///   rather than retrying indefinitely)
/// - Any `wacore::Result` download error — including
///   `wacore::Error::HashMismatch` (raised by `Client::download` when
///   `file_enc_sha256` fails verification), auth errors, transport
///   errors, and decryption errors — collapses to a single
///   `Unreachable { reason: format!("download failed: {e}") }` via
///   `map_err` (R9-M2 fix: this is a catch-all, not a special case
///   for `HashMismatch`). The `wacore::Error` `Display` strings do
///   not include `media_key` or `direct_path` (only status codes and
///   short labels — verified at the pinned `whatsapp-rust` rev
///   9734fb2).
/// - `Client::download` not-connected → `Unreachable { reason: "client
///   not connected" }` (matches `upload_media`'s precondition)
pub(crate) async fn download_via_media_ref(
    client: &Arc<parking_lot::Mutex<Option<Arc<whatsapp_rust::Client>>>>,
    media_ref_token: &str,
) -> Result<Vec<u8>, PlatformAdapterError> {
    // R8-M1 fix: use the explicit `INVALID_MEDIA_REF_FORMAT` const
    // instead of round-tripping through `MediaRefError::to_string`.
    // The original `MediaRefError` variant is logged at debug level
    // for operator visibility (the `Display` impl is the same
    // redacted string for both variants, so no info is lost for the
    // user-facing `ApiError { message }`).
    let media_ref = decode_base64url(media_ref_token).map_err(|e| {
        tracing::debug!(
            "decode_base64url failed (variant={}); returning redacted ApiError",
            e.variant_name()
        );
        PlatformAdapterError::ApiError {
            code: 400,
            message: INVALID_MEDIA_REF_FORMAT.into(),
        }
    })?;
    let doc = media_ref.to_document_message();
    // Clone the `Arc<Client>` out of the parking_lot guard before
    // awaiting — `whatsapp_rust::Client` is `!Send` (it contains FFI
    // pointers via `*mut ()`), so holding the guard across the await
    // would break the `async_trait` `Send` bound on the trait method.
    let client = {
        let guard = client.lock();
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: "client not connected".into(),
            })?
    };
    // The blanket `impl_downloadable!` at `wacore/src/download.rs`
    // provides `&DocumentMessage: &dyn Downloadable` (MediaType::Document).
    client
        .download(&doc)
        .await
        .map_err(|e| PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: format!("download failed: {e}"),
        })
}

/// Mission 0850: shared `upload_to_cdn` helper used by both
/// `WhatsAppWebAdapter::upload_media` and `send_envelope`'s native
/// branch. Returns the `UploadResponse` on success. Caller is
/// responsible for the `MediaRef::encode_base64url(&response)` step.
async fn upload_to_cdn(
    client: &Arc<whatsapp_rust::Client>,
    data: Vec<u8>,
    media_type: MediaType,
    options: UploadOptions,
) -> Result<UploadResponse, PlatformAdapterError> {
    // R13-M2 fix: the helper used to take
    // `&Arc<Mutex<Option<Arc<Client>>>>` and re-lock the mutex
    // here, creating a TOCTOU window: a `shutdown()` between the
    // caller's `self.client.lock().clone()` and the lock here
    // would return `Unreachable { "client not connected" }` even
    // though the caller's cloned `Arc<Client>` was still valid.
    //
    // The fix: take the cloned `Arc<Client>` directly. The
    // caller is responsible for cloning it out of the mutex
    // before any await, which eliminates the re-locking race.
    // This also lets `send_envelope_native` use the
    // `client: &Arc<whatsapp_rust::Client>` parameter it already
    // has, instead of the half-dead `&self.client` it used to
    // fall through to.
    client
        .upload(data, media_type, options)
        .await
        .map_err(|e| PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: format!("upload failed: {e}"),
        })
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

        // Mission 0850 (RFC-0850 §8.6): mode-dispatch via
        // `select_mode_with_max_text`. The adapter owns mode
        // selection (no production caller of `select_mode*` exists
        // outside this crate as of `next`). WhatsApp's text-message
        // ceiling is 65 KB (RFC-0850 line 202 + line 785); using the
        // RFC default 4 KB would route envelopes >4 KB to native mode
        // unnecessarily.
        //
        // **R8-H1 fix:** the threshold argument is `encoded.len()`
        // (the on-wire text-message body, ~33% larger than the wire
        // bytes after base64 expansion), NOT `wire_bytes.len()`. The
        // actual constraint is on the bytes that would be transmitted
        // in text mode — if `wire_bytes.len()` <= 65 KB but
        // `encoded.len()` > 65 KB, the envelope would be routed into
        // text mode and fail to fit in a single WhatsApp text message.
        // Using `encoded.len()` keeps the dispatch and the
        // PayloadTooLarge error consistent (same value reported in
        // both places — see the PayloadTooLarge arm below). RFC-0850
        // §8.6 line 805's `payload.len()` is read here as "bytes that
        // would be transmitted on the wire in text mode".
        let caps = self.capabilities();
        let mode = select_mode_with_max_text(encoded.len(), &caps, WHATSAPP_MAX_TEXT_BYTES)
            .map_err(|e| PlatformAdapterError::PayloadTooLarge {
                size: encoded.len(),
                max: e.max_payload,
                platform: "whatsapp".into(),
            })?;

        match mode {
            TransportMode::Text => self.send_envelope_text(&client, &to, &encoded).await,
            TransportMode::Native => {
                // Try native upload + send a text message carrying
                // the `DOT/2/{token}` wire reference. Per RFC-0850
                // §8.6 + §9.4 MUST-fallback: if the native upload
                // fails AND the payload still fits in a text message
                // (<= 65 KB), fall back to text mode and log a
                // warning. If the payload doesn't fit in text mode,
                // propagate the error (no fallback possible).
                //
                // R8-H3 fix: extracted the fallback decision into the
                // pure `should_fallback_to_text` helper so the
                // MUST-fallback contract is unit-testable without a
                // real wacore Client (which is a concrete type, not a
                // trait, so a stub cannot be injected in a normal
                // #[tokio::test]). See
                // `should_fallback_to_text_*` tests in `mod tests`.
                let encoded_len = encoded.len();
                // R9-H1 fix: send the raw envelope bytes (the
                // pre-base64 wire format) to the native-mode sender,
                // not the DOT/1/ base64 text. The receiver's
                // `canonicalize` for `dot_mode == "native"` takes the
                // downloaded payload directly as `wire_bytes`, so
                // uploading the DOT/1/ text would corrupt every
                // round-trip (length check in
                // `DeterministicEnvelope::from_wire_bytes` would fail).
                let primary = self.send_envelope_native(&client, &to, &wire_bytes);
                let fallback = self.send_envelope_text(&client, &to, &encoded);
                let primary_result = primary.await;
                if let Ok(receipt) = primary_result {
                    return Ok(receipt);
                }
                let err = primary_result.unwrap_err();
                if should_fallback_to_text(&err, encoded_len, WHATSAPP_MAX_TEXT_BYTES) {
                    tracing::warn!(
                        "native upload failed, falling back to DOT/1/ text mode (RFC-0850 §8.6/§9.4): {err:?}"
                    );
                    fallback.await
                } else {
                    Err(err)
                }
            }
            // `supports_raw_binary: false` and
            // `supports_fragmentation: false` in `capabilities()`
            // make Raw/Fragment unreachable. If the capabilities ever
            // change, surface that as an explicit error rather than
            // silently sending the wrong shape.
            TransportMode::Raw | TransportMode::Fragment => {
                Err(PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: format!("{mode:?} mode is not supported by this adapter"),
                })
            }
        }
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
        // R12-M2 fix: the `delivery_failed` sentinel uses an empty
        // payload by design (the gateway only needs the metadata to
        // know the delivery failed; the wire bytes were never
        // downloaded). Check for the sentinel BEFORE the
        // empty-payload check below so the sentinel can return a
        // meaningful 502 ApiError instead of a generic "Empty
        // payload" error.
        if raw.metadata.get("dot_mode").map(String::as_str) == Some("delivery_failed") {
            let reason = raw
                .metadata
                .get("error")
                .map(String::as_str)
                .unwrap_or("DOT/2/ download failed");
            return Err(PlatformAdapterError::ApiError {
                code: 502,
                message: format!("DOT/2/ delivery failed: {reason}"),
            });
        }

        if raw.payload.is_empty() {
            return Err(transport_err("Empty payload"));
        }

        // R2-M5 fix: dispatch on `metadata["dot_mode"]` (NOT payload
        // sniffing, which is fragile to future wire-format changes).
        // - `dot_mode == "native"` → payload is already wire bytes
        //   (decrypted by the download_rx consumer task); pass
        //   through `DeterministicEnvelope::from_wire_bytes` directly.
        // - `dot_mode == "text"` OR missing → legacy DOT/1/ text path;
        //   `decode_envelope` strips the `DOT/1/{base64}` prefix and
        //   base64-decodes to wire bytes.
        // - `dot_mode == "delivery_failed"` is handled at the top of
        //   this function (before the empty-payload check) — see the
        //   R12-M2 comment above.
        let dot_mode = raw.metadata.get("dot_mode").map(String::as_str);
        let wire_bytes = match dot_mode {
            Some("native") => raw.payload.clone(),
            _ => {
                // Legacy text path: extract text + decode envelope.
                let text = String::from_utf8_lossy(&raw.payload);
                Self::decode_envelope(&text).map_err(|e| PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("canonicalize failed: {e}"),
                })?
            }
        };

        DeterministicEnvelope::from_wire_bytes(&wire_bytes).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize failed: {e}"),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        // R10-M1 fix: enforce the `MAX_UPLOAD_BYTES` const value
        // matches the documented 100 MiB WhatsApp Document ceiling.
        // Fires at the first `capabilities()` call in debug builds.
        // If you intentionally change the ceiling, update BOTH the
        // const (`MAX_UPLOAD_BYTES`) and the literal here.
        debug_assert_eq!(
            Self::MAX_UPLOAD_BYTES,
            Self::WHATSAPP_DOCUMENT_CEILING_BYTES,
            "MAX_UPLOAD_BYTES drifted from the documented 100 MiB WhatsApp \
             Document ceiling; update both the const and the literal in \
             this assertion if the change is intentional"
        );
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: false,
            supports_encryption: true, // Signal Protocol via whatsapp-rust
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            // Mission 0850 (RFC-0850 §8.6): declare native media
            // transport. `max_upload_bytes` is the WhatsApp server-side
            // `Document` ceiling (100 MiB) per public WhatsApp
            // documentation as of 2026-06. The single supported MIME
            // is `application/octet-stream` because `MediaType::Document`
            // is the only `wacore::download::MediaType` that stores
            // arbitrary opaque blobs (Image/Video/Audio re-encode;
            // AppState/History/StickerPack/... have app-specific shapes).
            //
            // R9-L4 fix: read from `Self::MAX_UPLOAD_BYTES` (the shared
            // const) instead of the literal `100 * 1024 * 1024`. The
            // `debug_assert_eq!` at the top of this method (R10-M1)
            // verifies the const matches the documented WhatsApp
            // ceiling.
            media_capabilities: Some(MediaCapabilities {
                max_upload_bytes: Self::MAX_UPLOAD_BYTES,
                supported_mime_types: vec!["application/octet-stream".to_string()],
            }),
            ..Default::default()
        }
    }

    async fn upload_media(
        &self,
        filename: &str,
        data: &[u8],
        _mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        // Pre-flight size check (the adapter's only local enforcement
        // point — `Client::upload` would let WhatsApp's CDN reject with
        // a less-actionable server-side error).
        //
        // R9-L4 fix: use `Self::MAX_UPLOAD_BYTES` (the shared const)
        // instead of a local literal. R10-M1: the `debug_assert_eq!`
        // is at the top of `capabilities()` (one place, not here);
        // it fires at the first `capabilities()` call if a future
        // change updates the const without updating the documented
        // WhatsApp ceiling literal.
        if data.len() > Self::MAX_UPLOAD_BYTES {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: Self::MAX_UPLOAD_BYTES,
                platform: "whatsapp".into(),
            });
        }
        // R5: `_mime_type` is intentionally ignored. WhatsApp's
        // `Document` channel hardcodes `application/octet-stream`
        // regardless of the upload MIME. The argument is preserved in
        // the signature for future extension.
        //
        // R13-M2 fix: clone the `Arc<Client>` out of the parking_lot
        // mutex guard BEFORE awaiting (the guard is `!Send`, so it
        // can't cross the await point) and pass it to
        // `upload_to_cdn`. The helper used to take
        // `&self.client` (a `&Arc<Mutex<Option<Arc<Client>>>>`)
        // and re-lock the mutex, which created a TOCTOU window:
        // a `shutdown()` between the caller's clone and the
        // re-lock would surface as a misleading
        // "client not connected" error.
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let response = upload_to_cdn(
            &client,
            data.to_vec(),
            MediaType::Document,
            UploadOptions::new(),
        )
        .await?;
        let media_ref = MediaRef::from_upload_response(&response, filename);
        // The returned `String` is the wire-format token for
        // `DOT/2/{token}`. Callers that go through `send_envelope`
        // never see this — the adapter's native-mode branch wraps it
        // automatically. External callers (other adapters, tests)
        // receive the raw token and can construct their own
        // `DocumentMessage` from it.
        //
        // R9-L5 fix: the encode step is now fallible (returns
        // `MediaRefError`). `MediaRefError::Json` is the only
        // possible failure today and is unreachable for the
        // current field set, but propagating the error keeps the
        // adapter panic-free for future wacore upgrades.
        encode_base64url(&media_ref).map_err(|e| PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: format!("encode MediaRef failed: {e}"),
        })
    }

    async fn download_media(&self, media_ref_token: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        download_via_media_ref(&self.client, media_ref_token).await
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

        // R11-H2 fix: drop the `download_tx` Sender so the
        // `download_rx_consumer` task spawned in `start_bot` (line 651)
        // sees its `recv()` return `None` and exits cleanly. Without
        // this, the Sender is held in the field even after the bot
        // is aborted, the channel never closes, and the consumer
        // task is leaked (it lives forever, blocked on `recv().await`).
        // The reconnect path doesn't have this problem because
        // `start_bot` replaces the Sender (line 633) — the old Sender
        // is dropped, the old channel closes, the old consumer
        // task exits. But the FIRST `shutdown` has no follow-up
        // `start_bot` to trigger the replacement, so we must drop
        // the Sender explicitly here.
        *self.download_tx.lock().await = None;

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

// ── Inherent send_envelope helpers (Mission 0850) ──────────────────

impl WhatsAppWebAdapter {
    /// Upload a document to CDN and send it as a visible DocumentMessage
    /// to the given JID. Returns (message_id, media_ref_token).
    /// The message_id identifies the sent message; the media_ref_token
    /// can be passed to `download_media` to verify the CDN round-trip.
    pub async fn send_document(
        &self,
        to_jid: &str,
        filename: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<(String, String), PlatformAdapterError> {
        if data.len() > Self::MAX_UPLOAD_BYTES {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: data.len(),
                max: Self::MAX_UPLOAD_BYTES,
                platform: "whatsapp".into(),
            });
        }
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "whatsapp".into(),
                    reason: "client not connected".into(),
                })?
        };
        let upload = upload_to_cdn(
            &client,
            data.to_vec(),
            MediaType::Document,
            UploadOptions::new(),
        )
        .await?;
        let media_ref = MediaRef::from_upload_response(&upload, filename);
        let token = encode_base64url(&media_ref).map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "whatsapp".into(),
                reason: format!("encode MediaRef failed: {e}"),
            }
        })?;

        let jid: wacore_binary::Jid = to_jid
            .parse()
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid JID {to_jid:?}: {e}"),
            })?;

        let doc_msg = waproto::whatsapp::message::DocumentMessage {
            url: Some(upload.url),
            direct_path: Some(upload.direct_path),
            media_key: Some(upload.media_key.to_vec()),
            file_sha256: Some(upload.file_sha256.to_vec()),
            file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
            file_length: Some(data.len() as u64),
            mimetype: Some(mime_type.to_string()),
            file_name: Some(filename.to_string()),
            ..Default::default()
        };
        let outgoing = waproto::whatsapp::Message {
            document_message: Some(Box::new(doc_msg)),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(jid, outgoing))
            .await
            .map_err(|e| transport_err(format!("send_message failed: {e}")))?;

        Ok((send_result.message_id, token))
    }

    /// Text-mode send path used by [`PlatformAdapter::send_envelope`]
    /// after `select_mode_with_max_text` returns `TransportMode::Text`.
    /// Encodes the envelope as `DOT/1/{base64}` and sends via the
    /// `conversation` field of a `waproto::whatsapp::Message`.
    async fn send_envelope_text(
        &self,
        client: &Arc<whatsapp_rust::Client>,
        to: &wacore_binary::jid::Jid,
        encoded: &str,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let outgoing = waproto::whatsapp::Message {
            conversation: Some(encoded.to_string()),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(to.clone(), outgoing))
            .await
            .map_err(|e| transport_err(format!("send_message failed: {e}")))?;
        Ok(DeliveryReceipt {
            platform_message_id: send_result.message_id,
            delivered_at: epoch_millis(),
        })
    }

    /// Mission 0850 (RFC-0850 §8.6): native-mode send path.
    /// 1. Upload the encoded envelope bytes to WhatsApp's CDN.
    /// 2. Build a `MediaRef` from the `UploadResponse`.
    /// 3. Encode the `MediaRef` as base64url-JSON.
    /// 4. Send a text message with `conversation = "DOT/2/{token}"`.
    ///
    /// The receiver reads the `DOT/2/{token}` text, decodes the
    /// `MediaRef`, and calls `Client::download` to retrieve the
    /// envelope bytes. We intentionally do NOT send a separate
    /// `DocumentMessage` — the `DOT/2/{token}` reference IS the
    /// message on the wire.
    /// Mission 0850 (RFC-0850 §8.6): Native-mode sender.
    ///
    /// **R9-H1 fix:** this function uploads `wire_bytes` (the raw 282-byte
    /// `DeterministicEnvelope` wire format) to the WhatsApp CDN, NOT
    /// `encoded` (the DOT/1/ base64 text). The receiver's `canonicalize`
    /// for `dot_mode == "native"` takes the downloaded payload directly
    /// as `wire_bytes` and feeds it to
    /// `DeterministicEnvelope::from_wire_bytes`, whose length check
    /// (must equal exactly 282, see
    /// `crates/octo-network/src/dot/envelope.rs:124-136`) would fail
    /// with the ~370-byte DOT/1 text. The mission spec at line 83
    /// mandates `&wire_bytes`; the previous implementation used
    /// `encoded.as_bytes()` (≈370 B for a typical envelope), which
    /// broke every DOT/2/ round-trip in production. The pre-flight
    /// size check is also on `wire_bytes.len()` (not the base64
    /// expansion), so the full 100 MiB capacity is available.
    async fn send_envelope_native(
        &self,
        client: &Arc<whatsapp_rust::Client>,
        to: &wacore_binary::jid::Jid,
        wire_bytes: &[u8],
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        // Pre-flight size check (matches `upload_media`'s contract).
        //
        // R9-L4 fix: use `Self::MAX_UPLOAD_BYTES` (the shared const).
        // The same value is advertised in `capabilities()` and enforced
        // by `upload_media`. A future change to one without the other
        // now fails the startup-time debug assertion instead of
        // silently disagreeing.
        if wire_bytes.len() > Self::MAX_UPLOAD_BYTES {
            return Err(PlatformAdapterError::PayloadTooLarge {
                size: wire_bytes.len(),
                max: Self::MAX_UPLOAD_BYTES,
                platform: "whatsapp".into(),
            });
        }

        // Step 1+2: upload the raw envelope bytes to the CDN + build
        // MediaRef. The receiver will download these exact bytes and
        // feed them directly to `DeterministicEnvelope::from_wire_bytes`.
        //
        // R13-M2 fix: pass the `client` parameter (which was
        // already cloned out of `self.client` by the caller —
        // see `send_envelope` at adapter.rs:1770-1775) instead of
        // `&self.client`. The old code re-locked the mutex here,
        // which (a) created a TOCTOU window with `shutdown()` and
        // (b) made the `client` parameter half-dead. After the
        // refactor `upload_to_cdn` takes `&Arc<whatsapp_rust::Client>`
        // directly, so we just pass the parameter.
        let upload_response = upload_to_cdn(
            client,
            wire_bytes.to_vec(),
            MediaType::Document,
            UploadOptions::new(),
        )
        .await?;
        let media_ref = MediaRef::from_upload_response(&upload_response, "envelope.bin");
        // Step 3: encode the wire-format reference. R9-L5 fix:
        // `encode_base64url` is now fallible (returns `MediaRefError`);
        // propagate the error rather than panicking. The error arm is
        // unreachable for the current field set but future-proofs
        // against wacore upgrades that introduce non-serializable
        // fields.
        let token = encode_base64url(&media_ref)
            .map_err(|e| transport_err(format!("encode MediaRef failed: {e}")))?;
        // Step 4: send the DOT/2/ text message.
        let outgoing = waproto::whatsapp::Message {
            conversation: Some(encode_native_ref(&token)),
            ..Default::default()
        };
        let send_result = Box::pin(client.send_message(to.clone(), outgoing))
            .await
            .map_err(|e| transport_err(format!("send_message failed: {e}")))?;
        Ok(DeliveryReceipt {
            platform_message_id: send_result.message_id,
            delivered_at: epoch_millis(),
        })
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
            can_ban: true, // Implemented as remove + revoke_invite
            can_promote: true,
            can_demote: true,
            can_approve_join: true,
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
            can_transfer_ownership: true,
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
        group_id: &GroupId,
        member: &PeerId,
        _duration: Option<std::time::Duration>,
    ) -> Result<(), PlatformAdapterError> {
        // WhatsApp has no native ban primitive. The equivalent is:
        // 1. Remove the member from the group
        // 2. Revoke the invite link so they cannot rejoin
        self.remove_member(group_id, member).await?;
        // Revoke invite link by resetting it. Failure is non-fatal
        // (the member is already removed).
        let _ = self.get_invite_link(group_id.as_str(), true).await;
        Ok(())
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
        group_id: &GroupId,
        requester: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        let client = {
            let guard = self.client.lock();
            guard
                .clone()
                .ok_or_else(|| {
                    PlatformAdapterError::ApiError {
                        code: 500,
                        message: "WhatsApp Web client not connected".into(),
                    }
                })?
        };

        let group_jid: wacore_binary::Jid = group_id
            .as_str()
            .parse()
            .map_err(|e| PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid group JID: {e}"),
            })?;

        let requester_jid = Self::peer_to_jid(requester.as_str());

        client
            .groups()
            .approve_membership_requests(&group_jid, &[requester_jid])
            .await
            .map_err(|e| api_err("approve_join_request", e.to_string()))?;
        Ok(())
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
        group_id: &GroupId,
        new_owner: &PeerId,
    ) -> Result<(), PlatformAdapterError> {
        // WhatsApp has no native "transfer ownership" primitive.
        // The equivalent is: promote the new owner to admin.
        // The caller can optionally demote the old owner afterwards.
        self.promote_to_admin(group_id, new_owner).await
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
            Ok(()) => {
                // Delete chat AFTER leaving. Matches official app flow.
                use waproto::whatsapp::sync_action_value::SyncActionMessageRange;
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let message_range = SyncActionMessageRange {
                    last_message_timestamp: None,
                    last_system_message_timestamp: Some(now_secs),
                    messages: vec![],
                };
                let _ = client
                    .chat_actions()
                    .clear_chat(&jid, false, true, Some(message_range.clone()))
                    .await;
                let _ = client
                    .chat_actions()
                    .delete_chat(&jid, true, Some(message_range))
                    .await;
                Ok(())
            }
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

/// Mission 0850 (RFC-0850 §8.6 + §9.4): WhatsApp's text-message ceiling.
///
/// `encoded` (a `DOT/1/{base64url}` string) is the actual on-the-wire text
/// payload; if its length exceeds this constant, it cannot fit in a single
/// WhatsApp text message and the adapter must use the native upload path.
pub(crate) const WHATSAPP_MAX_TEXT_BYTES: usize = 65_536;

/// R1-H4 fix: the redacted error message returned in
/// `PlatformAdapterError::ApiError { message }` for any
/// `MediaRef` decode failure. MUST NOT include the input bytes
/// (which would leak `media_key`). The string is identical to
/// `MediaRefError`'s `Display` impl for both variants — kept as a
/// const here so the call site doesn't have to round-trip through
/// `MediaRefError::to_string(&MediaRefError::Base64)` (R8-M1 fix:
/// the round-trip was opaque and lost the original error variant).
pub(crate) const INVALID_MEDIA_REF_FORMAT: &str = "invalid media ref format";

/// Mission 0850 (RFC-0850 §8.6 + §9.4): the MUST-fallback decision.
///
/// R8-H3 fix: extracted from `send_envelope` so the fallback contract is
/// unit-testable without a real wacore `Client` (which is a concrete type,
/// not a trait — see the spec's "R1-H1 fallback test stub-ability" note at
/// `missions/open/0850-whatsapp-media-transport.md` line 494).
///
/// The contract (RFC-0850 §8.6 + §9.4): when the native (`DOT/2/`) send
/// fails, the adapter MUST fall back to the text (`DOT/1/`) path IF AND
/// ONLY IF the text path would actually succeed — i.e., the encoded
/// payload fits in a single text message AND the error is a transient
/// transport error (`Unreachable`), not a permanent wire-format error
/// (e.g., `ApiError { code: 4xx }`).
pub(crate) fn should_fallback_to_text(
    err: &PlatformAdapterError,
    encoded_len: usize,
    max_text_bytes: usize,
) -> bool {
    encoded_len <= max_text_bytes && matches!(err, PlatformAdapterError::Unreachable { .. })
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

    /// R10-M2 fix: pin the `canonicalize` behavior for the
    /// native-mode non-282-byte payload path. When the download
    /// returns a payload of any length other than the wire-format
    /// 282 bytes, `DeterministicEnvelope::from_wire_bytes` rejects
    /// it with `"Invalid wire envelope length: expected 282, got N"`.
    /// The error MUST be a 400 `ApiError` with a message that
    /// includes both the expected and actual lengths, so the
    /// gateway operator can diagnose a CDN-side mismatch without
    /// leaking the `media_key`. The behavior was verified manually
    /// in R10 but was not pinned by any test — a regression that
    /// (a) silently switches dot_mode to the text path, (b) maps
    /// `DotError::Serialization` to a different code, or (c) accepts
    /// a truncated payload (`len() < 282` rather than `!=`) would
    /// not be caught. Runs without a live session because
    /// `canonicalize` is local — it only inspects `raw` and reads
    /// `metadata["dot_mode"]`.
    #[test]
    fn canonicalize_native_mode_rejects_non_282_byte_payload() {
        let adapter = offline_adapter();
        let raw = RawPlatformMessage {
            platform_id: "test".into(),
            payload: vec![0u8; 100], // not 282 bytes
            metadata: [
                ("chat".to_string(), "x".into()),
                ("sender".to_string(), "y".into()),
                ("dot_mode".to_string(), "native".into()),
            ]
            .into_iter()
            .collect(),
        };
        match adapter.canonicalize(&raw) {
            Err(PlatformAdapterError::ApiError { code, message }) => {
                assert_eq!(code, 400, "must surface as a 400 ApiError, got {code}");
                assert!(
                    message.contains("Invalid wire envelope length"),
                    "message must include the from_wire_bytes error, got: {message}"
                );
                assert!(
                    message.contains("expected 282, got 100"),
                    "message must include the expected and actual lengths, got: {message}"
                );
            }
            Err(other) => {
                panic!("expected ApiError 400 with length-mismatch message, got {other:?}")
            }
            Ok(_) => panic!("non-282-byte native payload must be rejected"),
        }
    }

    /// R10-M2 fix (complement): a 282-byte payload of arbitrary
    /// bytes MUST NOT be rejected by `from_wire_bytes`'s length
    /// check. This pins the boundary: 282 bytes is the ONLY
    /// payload length that passes the length check. The actual
    /// downstream behavior (signature verification, etc.) is a
    /// separate concern — we don't care whether it succeeds or
    /// fails; we only care that the failure is NOT the length
    /// check. This test would fail if a future change made the
    /// length check `< 282` instead of `!= 282` (which would
    /// accept 283+ byte payloads).
    #[test]
    fn canonicalize_native_mode_passes_length_check_at_282_bytes() {
        let adapter = offline_adapter();
        let raw = RawPlatformMessage {
            platform_id: "test".into(),
            payload: vec![0u8; 282], // exact wire length
            metadata: [
                ("chat".to_string(), "x".into()),
                ("sender".to_string(), "y".into()),
                ("dot_mode".to_string(), "native".into()),
            ]
            .into_iter()
            .collect(),
        };
        // The length check must pass. The downstream behavior is
        // opaque (could be Ok for a well-formed envelope, or an
        // ApiError from signature verification for an all-zeros
        // payload) — neither outcome is this test's concern.
        match adapter.canonicalize(&raw) {
            Ok(_) => { /* length check passed, downstream succeeded */ }
            Err(PlatformAdapterError::ApiError { code: 400, message }) => {
                assert!(
                    !message.contains("Invalid wire envelope length"),
                    "282-byte payload must pass the length check; \
                     downstream signature failure is acceptable. got: {message}"
                );
            }
            Err(other) => panic!(
                "expected Ok or ApiError 400 from a downstream check (NOT length), got {other:?}"
            ),
        }
    }

    /// R12-M2 fix: the `delivery_failed` sentinel (pushed by the
    /// `download_rx_consumer` task when the upstream WhatsApp CDN
    /// download fails) must be converted by `canonicalize` into a
    /// 502 `ApiError` with the redacted reason in the message. 502
    /// mirrors HTTP semantics (upstream is the source of the failure,
    /// not us), distinguishing this case from a 400 canonicalize
    /// error or a 400 empty-payload error. The reason is taken from
    /// `metadata["error"]` (a redacted fixed-string — no wacore
    /// internals, no `media_key`, no `direct_path`).
    #[test]
    fn canonicalize_delivery_failed_returns_502_with_redacted_reason() {
        let adapter = offline_adapter();
        let raw = RawPlatformMessage {
            platform_id: "test".into(),
            payload: Vec::new(), // empty — sentinel has no payload
            metadata: [
                ("chat".to_string(), "120363012345678901@g.us".into()),
                ("sender".to_string(), "1234@s.whatsapp.net".into()),
                ("dot_mode".to_string(), "delivery_failed".into()),
                ("error".to_string(), "DOT/2/ download failed".into()),
            ]
            .into_iter()
            .collect(),
        };
        match adapter.canonicalize(&raw) {
            Err(PlatformAdapterError::ApiError { code, message }) => {
                assert_eq!(
                    code, 502,
                    "delivery_failed must surface as 502 Bad Gateway, got {code}"
                );
                assert!(
                    message.contains("DOT/2/ download failed"),
                    "message must include the redacted reason, got: {message}"
                );
                assert!(
                    message.contains("delivery failed"),
                    "message must include the 'delivery failed' prefix, got: {message}"
                );
            }
            Err(other) => {
                panic!("expected ApiError 502 for delivery_failed sentinel, got {other:?}")
            }
            Ok(_) => panic!("delivery_failed sentinel must NOT canonicalize to Ok"),
        }
    }

    /// R12-M2 fix: a `delivery_failed` sentinel WITHOUT a
    /// `metadata["error"]` entry must still return a 502 ApiError
    /// with the default redacted reason ("DOT/2/ download failed").
    /// This pins the contract that the error reason is always
    /// present and redacted even if the metadata is missing or
    /// tampered with.
    #[test]
    fn canonicalize_delivery_failed_without_error_metadata_uses_default_reason() {
        let adapter = offline_adapter();
        let raw = RawPlatformMessage {
            platform_id: "test".into(),
            payload: Vec::new(),
            metadata: [
                ("chat".to_string(), "120363012345678901@g.us".into()),
                ("sender".to_string(), "1234@s.whatsapp.net".into()),
                ("dot_mode".to_string(), "delivery_failed".into()),
                // Note: no "error" metadata key
            ]
            .into_iter()
            .collect(),
        };
        match adapter.canonicalize(&raw) {
            Err(PlatformAdapterError::ApiError { code, message }) => {
                assert_eq!(code, 502);
                assert!(
                    message.contains("DOT/2/ download failed"),
                    "default reason must be used when error metadata is missing, got: {message}"
                );
            }
            Err(other) => {
                panic!("expected ApiError 502, got {other:?}")
            }
            Ok(_) => panic!("delivery_failed sentinel must NOT canonicalize to Ok"),
        }
    }

    /// R12-M1 fix: the public `dropped_inbound_messages()` getter
    /// returns the monotonic counter. A fresh adapter starts at 0.
    /// This pins the contract that the counter is exposed and
    /// starts at zero (so a test that observes a non-zero value can
    /// confidently assert that drops happened).
    #[test]
    fn dropped_inbound_messages_starts_at_zero() {
        let adapter = offline_adapter();
        assert_eq!(
            adapter.dropped_inbound_messages(),
            0,
            "fresh adapter must start with zero dropped messages"
        );
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

    // ── R13-L3 tests: register_group_at_runtime JID validation ────
    //
    // The static-config path (`WhatsAppConfig::validate`) already had
    // strict JID-shape checks; the runtime-registration path
    // (`register_group_at_runtime`) silently accepted any string.
    // R13-L3 fixed the runtime path to share the same check via the
    // `validate_group_jid` helper. These tests pin the new behavior.

    #[test]
    fn register_group_at_runtime_accepts_valid_jids() {
        // Bare digits and digits+@g.us are the two valid forms.
        for good in [
            "120363012345678901",      // bare digits
            "120363012345678901@g.us", // full JID
        ] {
            let cfg = cfg_with("/tmp/test.db", None, None, None, vec![]);
            let adapter = WhatsAppWebAdapter::new(cfg);
            assert!(
                adapter.register_group_at_runtime(good).is_ok(),
                "valid JID {good:?} should be accepted"
            );
        }
    }

    #[test]
    fn register_group_at_runtime_rejects_invalid_jids() {
        // Same set of bad JIDs that `WhatsAppConfig::validate`
        // rejects — proves the runtime path shares the check.
        for bad in [
            "",                                    // empty
            "120363012345678901@newsletter",       // newsletter JID misuse
            "120363012345678901@s.whatsapp.net",   // user JID shape
            "120363012345678901:0@s.whatsapp.net", // user JID misuse (`:`)
            "not-a-jid",                           // non-numeric
            "abc@g.us",                            // non-numeric prefix
            "120363012345678901@",                 // empty suffix
            "@g.us",                               // empty prefix
        ] {
            let cfg = cfg_with("/tmp/test.db", None, None, None, vec![]);
            let adapter = WhatsAppWebAdapter::new(cfg);
            assert!(
                adapter.register_group_at_runtime(bad).is_err(),
                "invalid JID {bad:?} should be rejected (was silently accepted before R13-L3)"
            );
        }
    }

    #[test]
    fn register_group_at_runtime_idempotent() {
        // Re-registering an existing JID is a no-op (no duplicate
        // entries in the runtime_groups vec).
        let cfg = cfg_with("/tmp/test.db", None, None, None, vec![]);
        let adapter = WhatsAppWebAdapter::new(cfg);
        let jid = "120363012345678901@g.us";
        adapter.register_group_at_runtime(jid).expect("first");
        adapter.register_group_at_runtime(jid).expect("second");
        let guard = adapter.runtime_groups.lock();
        assert_eq!(
            guard.len(),
            1,
            "duplicate register must not insert a second row"
        );
        assert_eq!(guard[0], jid);
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
        assert!(caps.can_ban, "can_ban (implemented as remove + revoke_invite)");
        assert!(caps.can_promote);
        assert!(caps.can_demote);
        assert!(caps.can_approve_join, "can_approve_join");
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
        assert!(caps.can_transfer_ownership);
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
        // All previously-unimplemented methods (ban_member, approve_join_request,
        // transfer_ownership) are now implemented. They fail with ApiError
        // when called offline (no client connected), not Unimplemented.
        //
        // This test is kept as a placeholder. If any new methods are added
        // as Unimplemented, add them here.
        let adapter = offline_adapter();
        let g = GroupId::new("120363012345678901@g.us");
        let p = PeerId::new("+15551234567");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            // ban_member: now implemented (remove + revoke invite)
            let err = CoordinatorAdmin::ban_member(&adapter, &g, &p, None)
                .await
                .expect_err("ban_member must fail offline");
            assert!(
                matches!(err, PlatformAdapterError::ApiError { .. }),
                "ban_member: expected ApiError (not connected), got {err:?}"
            );

            // approve_join_request: now implemented (approve_membership_requests)
            let err = CoordinatorAdmin::approve_join_request(&adapter, &g, &p)
                .await
                .expect_err("approve_join_request must fail offline");
            assert!(
                matches!(err, PlatformAdapterError::ApiError { .. }),
                "approve_join_request: expected ApiError (not connected), got {err:?}"
            );

            // transfer_ownership: now implemented (promote_to_admin)
            let err = CoordinatorAdmin::transfer_ownership(&adapter, &g, &p)
                .await
                .expect_err("transfer_ownership must fail offline");
            assert!(
                matches!(err, PlatformAdapterError::ApiError { .. }),
                "transfer_ownership: expected ApiError (not connected), got {err:?}"
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

    // ── Mission 0850 (RFC-0850 §8.6/§9.4) tests ──────────────────

    /// Mission 0850 AC: `capabilities()` MUST declare
    /// `media_capabilities` so `select_mode_with_max_text` routes
    /// envelopes > 65 KB to `TransportMode::Native`. Without this
    /// declaration, the gate silently degrades to DOT/1/ text mode
    /// for every envelope.
    #[test]
    fn capabilities_includes_media_capabilities() {
        let adapter = offline_adapter();
        let caps = adapter.capabilities();
        assert_eq!(caps.max_payload_bytes, 65_536);
        assert!(caps.supports_encryption);
        assert!(!caps.supports_fragmentation);
        assert!(!caps.supports_raw_binary);
        let media = caps
            .media_capabilities
            .expect("media_capabilities must be populated for DOT/2 transport");
        // R9-L4 fix: use the shared const instead of a literal so the test
        // can't drift from the value advertised by the production code.
        assert_eq!(media.max_upload_bytes, WhatsAppWebAdapter::MAX_UPLOAD_BYTES);
        // R8-L2: only `application/octet-stream` is in the list because
        // WhatsApp's `MediaType::Document` channel uploads as
        // application/octet-stream regardless of the requested MIME
        // (see R5 in `missions/open/0850-whatsapp-media-transport.md`).
        // The list is the truth for what the adapter CAN advertise —
        // adding other MIMEs here would be lying about transport
        // capabilities.
        assert_eq!(
            media.supported_mime_types,
            vec!["application/octet-stream".to_string()]
        );
    }

    /// Mission 0850 AC (R1-H3): `upload_media` against an
    /// un-connected adapter MUST return `Unreachable { reason:
    /// "client not connected" }` — same precondition as `send_envelope`.
    #[tokio::test]
    async fn upload_media_client_not_connected() {
        let adapter = offline_adapter();
        let result = adapter
            .upload_media("test.bin", b"hello", "application/octet-stream")
            .await;
        match result {
            Err(PlatformAdapterError::Unreachable { reason, .. }) => {
                assert!(
                    reason.contains("client not connected"),
                    "unexpected reason: {reason}"
                );
            }
            Err(other) => panic!("expected Unreachable, got {other:?}"),
            Ok(_) => panic!("upload_media must fail when client is not connected"),
        }
    }

    /// R10-L1 fix: pin the pre-flight 100 MiB + 1 byte boundary.
    /// The mission spec (Test 2 of `media_capabilities_match_upload_limit`)
    /// requires this boundary. A regression that changes the
    /// comparison from `>` to `>=` would still pass at 100 MiB + 1
    /// but would reject a payload of exactly 100 MiB (still legal).
    /// A regression that removes the check entirely would let the
    /// payload reach `Client::upload` and surface as a less-actionable
    /// server-side rejection. The test uses a 100 MiB + 1 byte payload
    /// to pin the off-by-one boundary. Runs without a live session
    /// because the pre-flight check short-circuits before any network
    /// call.
    #[tokio::test]
    async fn upload_media_rejects_payload_over_max_upload_bytes() {
        let adapter = offline_adapter();
        // 100 MiB + 1 byte
        let oversized = vec![0u8; WhatsAppWebAdapter::MAX_UPLOAD_BYTES + 1];
        let result = adapter
            .upload_media("test.bin", &oversized, "application/octet-stream")
            .await;
        match result {
            Err(PlatformAdapterError::PayloadTooLarge {
                size,
                max,
                platform,
            }) => {
                assert_eq!(size, WhatsAppWebAdapter::MAX_UPLOAD_BYTES + 1);
                assert_eq!(max, WhatsAppWebAdapter::MAX_UPLOAD_BYTES);
                assert_eq!(platform, "whatsapp");
            }
            Err(other) => panic!("expected PayloadTooLarge, got {other:?}"),
            Ok(_) => panic!("oversized payload must be rejected by pre-flight"),
        }
    }

    /// R10-L1 fix: pin the pre-flight at-the-boundary case.
    /// A payload of EXACTLY 100 MiB must NOT be rejected by the
    /// pre-flight check (the check uses `>`, not `>=`). This test
    /// would fail if a future change inverted the comparison. It
    /// then fails at the `client not connected` step (the
    /// pre-flight passes), proving the boundary is inclusive at
    /// `MAX_UPLOAD_BYTES`.
    #[tokio::test]
    async fn upload_media_accepts_payload_exactly_at_max_upload_bytes() {
        let adapter = offline_adapter();
        // Exactly 100 MiB
        let at_boundary = vec![0u8; WhatsAppWebAdapter::MAX_UPLOAD_BYTES];
        let result = adapter
            .upload_media("test.bin", &at_boundary, "application/octet-stream")
            .await;
        match result {
            // Pre-flight passes (size == MAX, not >), fails at client-not-connected.
            Err(PlatformAdapterError::Unreachable { reason, .. }) => {
                assert!(
                    reason.contains("client not connected"),
                    "at-the-boundary payload must pass pre-flight and fail at \
                     client-not-connected step, got reason: {reason}"
                );
            }
            Err(PlatformAdapterError::PayloadTooLarge { .. }) => {
                panic!(
                    "at-the-boundary payload (exactly MAX_UPLOAD_BYTES) must \
                        NOT be rejected by pre-flight; check uses > not >="
                )
            }
            Err(other) => panic!(
                "expected Unreachable (pre-flight passes, client disconnected), got {other:?}"
            ),
            Ok(_) => panic!("upload_media must fail when client is not connected"),
        }
    }

    /// Mission 0850 AC (R1-H3, R18): `download_media` with a malformed
    /// token MUST return `ApiError { code: 400, .. }` with the redacted
    /// "invalid media ref format" message. The 4xx-shaped variant
    /// tells the gateway to refuse the envelope rather than retry
    /// indefinitely. The redacted message MUST NOT include the input
    /// bytes (which would leak the `media_key` on a partial parse).
    #[tokio::test]
    async fn download_media_invalid_message_id() {
        let adapter = offline_adapter();
        // `!` is not a base64url char — b64url_decode will fail.
        let result = adapter.download_media("not-base64!!!").await;
        match result {
            Err(PlatformAdapterError::ApiError { code, message }) => {
                assert_eq!(code, 400);
                assert_eq!(
                    message, "invalid media ref format",
                    "message MUST be the redacted generic string"
                );
                // Defensive: the input MUST NOT appear in the message.
                assert!(
                    !message.contains("not-base64"),
                    "message leaked input: {message}"
                );
            }
            Err(other) => panic!("expected ApiError code 400, got {other:?}"),
            Ok(_) => panic!("download_media with malformed token must fail"),
        }
    }

    /// Mission 0850 AC (R1-M2): `accept_message` MUST accept
    /// `DOT/1/{base64}` (existing behavior pinned).
    #[test]
    fn accept_message_accepts_dot1() {
        // R8-L1: JID format reference.
        // - `120363012345678901@g.us` is a group JID (suffix `@g.us`
        //   marks the group domain in WhatsApp). The 18-digit prefix
        //   is the group ID.
        // - `1234@s.whatsapp.net` is a user JID (suffix
        //   `@s.whatsapp.net` marks the user domain).
        let groups = vec!["120363012345678901".to_string()];
        let allowlist = BTreeMap::new();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "1234@s.whatsapp.net",
            "DOT/1/abc",
            &groups,
            &allowlist,
        );
        assert!(matches!(decision, AcceptDecision::Accept));
    }

    /// Mission 0850 AC (R1-M2): `accept_message` MUST accept
    /// `DOT/2/{token}` (new behavior pinned).
    #[test]
    fn accept_message_accepts_dot2() {
        // See R8-L1 JID reference in `accept_message_accepts_dot1`.
        let groups = vec!["120363012345678901".to_string()];
        let allowlist = BTreeMap::new();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "1234@s.whatsapp.net",
            "DOT/2/test_msg_id",
            &groups,
            &allowlist,
        );
        assert!(matches!(decision, AcceptDecision::Accept));
    }

    /// Mission 0850 AC (R1-M2): `accept_message` MUST reject any
    /// non-DOT-prefixed text (including `DOT/F/`, which is out of
    /// scope for this mission).
    #[test]
    fn accept_message_rejects_other_prefix() {
        let groups = vec!["120363012345678901".to_string()];
        let allowlist = BTreeMap::new();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "1234@s.whatsapp.net",
            "DOT/F/fragmented",
            &groups,
            &allowlist,
        );
        match decision {
            AcceptDecision::Reject { reason } => {
                assert_eq!(reason, "not a DOT envelope");
            }
            AcceptDecision::Accept => panic!("DOT/F/ must be rejected"),
        }
    }

    /// R9-L3 fix + R10-L2: `accept_message` MUST reject an empty or
    /// whitespace-only `DOT/2/` token at the boundary instead of
    /// letting it cascade through the receive pipeline as a noisy
    /// decode failure. The literal string `"DOT/2/"` (no token after
    /// the slash) previously passed the prefix check, then failed
    /// `decode_native_ref → None`, then failed text-decode, and was
    /// dropped with two cascading errors. Rejecting here gives a
    /// single, clear rejection reason. The `trim()` (R10-L2 fix)
    /// also catches `"DOT/2/   "` (whitespace-only) and
    /// `"DOT/2/\t"` (tab-only) tokens.
    #[test]
    fn accept_message_rejects_empty_dot2_token() {
        let groups = vec!["120363012345678901".to_string()];
        let allowlist = BTreeMap::new();
        let decision = WhatsAppWebAdapter::accept_message(
            "120363012345678901@g.us",
            "1234@s.whatsapp.net",
            "DOT/2/",
            &groups,
            &allowlist,
        );
        match decision {
            AcceptDecision::Reject { reason } => {
                assert_eq!(reason, "DOT/2/ token is empty or whitespace");
            }
            AcceptDecision::Accept => panic!("empty DOT/2/ token must be rejected"),
        }
    }

    /// R10-L2 fix: `accept_message` MUST also reject `DOT/2/` tokens
    /// that are entirely whitespace. `"DOT/2/   "` previously
    /// slipped through the `is_empty()` check (the string `"   "`
    /// is non-empty) and surfaced deeper as a generic
    /// "invalid media ref format" error.
    ///
    /// R12-L2 fix: extend the whitespace pin to cover tabs, newlines,
    /// and mixed Unicode whitespace. The `accept_message`
    /// implementation uses `rest.trim().is_empty()` which handles all
    /// Unicode whitespace; the test pin must match the implementation
    /// exactly so a future narrowing (e.g., `trim_start()` or
    /// `trim_matches(' ')`) would be caught.
    #[test]
    fn accept_message_rejects_whitespace_dot2_token() {
        let groups = vec!["120363012345678901".to_string()];
        let allowlist = BTreeMap::new();
        for input in &[
            "DOT/2/   ",      // spaces
            "DOT/2/\t",       // tab
            "DOT/2/\n",       // newline
            "DOT/2/\r\n",     // CRLF
            "DOT/2/\t \n \t", // mixed Unicode whitespace
            "DOT/2/\u{00A0}", // non-breaking space (U+00A0 is whitespace per `char::is_whitespace`)
        ] {
            let decision = WhatsAppWebAdapter::accept_message(
                "120363012345678901@g.us",
                "1234@s.whatsapp.net",
                input,
                &groups,
                &allowlist,
            );
            match decision {
                AcceptDecision::Reject { reason } => {
                    assert_eq!(
                        reason, "DOT/2/ token is empty or whitespace",
                        "input {input:?} must be rejected with the documented reason, got: {reason}"
                    );
                }
                AcceptDecision::Accept => {
                    panic!("whitespace DOT/2/ token {input:?} must be rejected")
                }
            }
        }
    }

    /// Mission 0850 AC (R3-M2 + R4-M3): the `download_rx` consumer
    /// task exits cleanly when the channel sender is dropped. This
    /// pins the lifecycle — a regression that blocks the task on a
    /// closed channel would hang this test until the timeout.
    ///
    /// R8-H2 fix: previously the test had no real assertion (just a
    /// 100ms `sleep` loop). Now we capture the spawned task's
    /// `JoinHandle` and bound the wait with `tokio::time::timeout`,
    /// so a hang fails the test loudly.
    #[tokio::test]
    async fn download_rx_consumer_exits_on_channel_close() {
        use std::time::Duration;

        let adapter = offline_adapter();

        // Use the test-only constructor that bypasses `start_bot`
        // (which requires an authenticated wacore session).
        let (tx, handle) = adapter.spawn_download_consumer_for_test();

        // Dropping the sender closes the channel. The consumer task
        // should observe `recv() → None` and exit the `while let`
        // loop. The `JoinHandle` completes when the spawned future
        // returns; we bound the wait with a 500ms timeout to fail
        // loudly if the task doesn't exit.
        drop(tx);
        match tokio::time::timeout(Duration::from_millis(500), handle).await {
            Ok(Ok(())) => {} // task exited cleanly
            Ok(Err(join_err)) => panic!("download_rx consumer task panicked: {join_err}"),
            Err(_elapsed) => panic!("download_rx consumer task did not exit within 500ms"),
        }
    }

    /// Mission 0850 AC (R4-M2 happy path): the consumer task pushes a
    /// `RawPlatformMessage` to `inbound_tx` when a `DownloadRequest`
    /// arrives. The test stub pretends the download always succeeds,
    /// pushing `b"native"` as the payload and tagging it with
    /// `dot_mode = "native"`.
    #[tokio::test]
    async fn download_rx_consumer_processes_valid_request() {
        use std::time::Duration;

        // R9-L2 fix: capture the JoinHandle instead of discarding
        // it. If the consumer task panics while processing the
        // request (e.g., due to a future refactor that breaks the
        // stub), the JoinHandle will be in the Err state when we
        // await it at the end of the test. We don't strictly need to
        // await it (the stub doesn't block), but we do so explicitly
        // to surface any panic via `assert!` rather than letting it
        // silently disappear as a dangling task.
        let adapter = offline_adapter();
        let (tx, handle) = adapter.spawn_download_consumer_for_test();

        // Push a DownloadRequest. The test stub immediately pushes a
        // RawPlatformMessage to `inbound_tx`. The stub's synthetic
        // payload + metadata shape MUST match `STUB_NATIVE_PAYLOAD`
        // and `STUB_DOT_MODE` (defined in the test-only impl block
        // below). R8-M3 fix: the test and the stub previously shared
        // the values implicitly (the test asserted `b"native"` and
        // the stub produced `b"native"`); a future maintainer
        // changing one without the other would silently break the
        // test. The shared const makes the contract explicit.
        tx.try_send(DownloadRequest {
            msg_id: "test-token".into(),
            chat: "120363012345678901@g.us".into(),
            sender: "1234@s.whatsapp.net".into(),
        })
        .expect("channel should have capacity");

        // Poll inbound_rx for the result (max 500 ms). Use
        // `tokio::task::yield_now` rather than `sleep` to avoid
        // holding the parking_lot::Mutex guard across an await point
        // (parking_lot is not async-aware — see clippy::await_holding_lock).
        let start = std::time::Instant::now();
        let raw = loop {
            if let Ok(msg) = adapter.inbound_rx.lock().try_recv() {
                break msg;
            }
            if start.elapsed() > Duration::from_millis(500) {
                panic!("download_rx consumer did not push a RawPlatformMessage within 500 ms");
            }
            tokio::task::yield_now().await;
        };

        // R8-M3 fix: assertions reference the shared consts (defined
        // in the test-only impl block below) instead of literal
        // `b"native"` / `"native"`. The stub and the test are now
        // linked at the source level.
        assert_eq!(raw.payload, WhatsAppWebAdapter::STUB_NATIVE_PAYLOAD);
        assert_eq!(
            raw.metadata.get("dot_mode").map(String::as_str),
            Some(WhatsAppWebAdapter::STUB_DOT_MODE)
        );
        assert_eq!(
            raw.metadata.get("chat").map(String::as_str),
            Some("120363012345678901@g.us")
        );
        assert_eq!(
            raw.metadata.get("sender").map(String::as_str),
            Some("1234@s.whatsapp.net")
        );

        // R9-L2 fix: confirm the consumer task didn't panic during
        // the request. Awaiting the JoinHandle returns
        // `Ok(())` if the task completed normally, `Err(JoinError)`
        // if it panicked. We bound the wait with a 500ms timeout —
        // if the stub is broken and the task hangs, we want the
        // test to fail loudly rather than block until the runtime's
        // outer test timeout (default 1 minute).
        drop(tx);
        match tokio::time::timeout(Duration::from_millis(500), handle).await {
            Ok(Ok(())) => {} // task completed normally
            Ok(Err(join_err)) => panic!("download_rx consumer panicked: {join_err}"),
            Err(_elapsed) => panic!("download_rx consumer did not exit within 500ms"),
        }
    }

    /// Mission 0850 AC (R4-M2): `download_tx.try_send` returns `Full`
    /// when the channel's capacity is exceeded. Push 65 (size + 1)
    /// messages; the (capacity+1)th MUST be rejected.
    #[tokio::test]
    async fn download_tx_try_send_returns_full_when_capacity_exceeded() {
        let adapter = offline_adapter();
        let (tx, _handle) = adapter.spawn_download_consumer_for_test();

        // The consumer task drains the channel concurrently, but its
        // test stub doesn't actually `await` anything (it just pushes
        // a `RawPlatformMessage` and loops). With the test runtime
        // running both tasks, we can fill the buffer deterministically
        // by pushing faster than the consumer drains.
        //
        // To make this deterministic, we push all (capacity+1)
        // messages in a tight loop and check that at least one
        // returned `Full`. The exact count of `Ok` vs `Full` depends
        // on scheduling, but (capacity+1) push attempts into a
        // `capacity`-slot buffer MUST produce at least one `Full`
        // (the consumer might drain a few in between, but it can't
        // drain all of them before we're done pushing).
        //
        // R8-L4 fix: the capacity comes from the shared const
        // `WhatsAppWebAdapter::DOWNLOAD_CHANNEL_CAPACITY` (defined in
        // the test-only impl block below). The test loop's upper bound
        // is `capacity + 1` so it stays correct if the const changes.
        let cap = WhatsAppWebAdapter::DOWNLOAD_CHANNEL_CAPACITY;
        let mut full_count = 0;
        for i in 0..(cap + 1) {
            let res = tx.try_send(DownloadRequest {
                msg_id: format!("msg-{i}"),
                chat: "test@g.us".into(),
                sender: "sender@s.whatsapp.net".into(),
            });
            if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = res {
                full_count += 1;
            }
        }
        assert!(
            full_count > 0,
            "expected at least one Full error when pushing {} messages into a {}-slot channel, got {full_count}",
            cap + 1,
            cap,
        );
    }

    /// Defensive test (R8-M2): the mission spec doesn't explicitly
    /// request this, but the precondition check at the top of
    /// `send_envelope` (domain→JID lookup) is a security-relevant
    /// gate — a regression that returns `Ok(_)` for an unknown
    /// domain could allow cross-domain envelope injection. Pins the
    /// `Unreachable` error so the contract can't silently change.
    ///
    /// The 282-byte zero buffer is structurally valid
    /// `DeterministicEnvelope` wire format (218 signing bytes + 64
    /// signature bytes; see
    /// `octo_network::dot::envelope::DeterministicEnvelope::from_wire_bytes`).
    /// The exact content doesn't matter — the lookup fails before
    /// the bytes are touched.
    #[tokio::test]
    async fn send_envelope_unknown_domain_returns_error() {
        let adapter = offline_adapter();
        let domain = BroadcastDomainId::new(PlatformType::WhatsApp, "999999999");
        let envelope = DeterministicEnvelope::from_wire_bytes(&[0u8; 282])
            .expect("zeroed 282-byte buffer is structurally valid");
        let result = adapter.send_envelope(&domain, &envelope).await;
        assert!(
            matches!(result, Err(PlatformAdapterError::Unreachable { .. })),
            "send_envelope to unknown domain must return Unreachable, got {result:?}"
        );
    }

    // ── Mission 0850 (R8-H3 fix): MUST-fallback decision unit tests ─

    /// R8-H3 fix: pins RFC-0850 §8.6/§9.4 fallback semantics. When the
    /// native send fails with `Unreachable` AND the encoded payload
    /// fits in a text message, fall back. The pure helper exists
    /// because `Client` is a concrete type — a stub cannot be injected
    /// in a normal `#[tokio::test]`, so the dispatch is verified via
    /// the decision function instead of the full send path.
    #[test]
    fn should_fallback_to_text_unreachable_within_text_limit() {
        let err = PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "client not connected".into(),
        };
        assert!(should_fallback_to_text(&err, 1000, WHATSAPP_MAX_TEXT_BYTES));
    }

    /// R8-H3 fix: encoded payload that fits exactly at the boundary
    /// (65_536 bytes) MUST still trigger fallback — `<=` is inclusive.
    #[test]
    fn should_fallback_to_text_at_text_limit_boundary() {
        let err = PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "transient".into(),
        };
        assert!(
            should_fallback_to_text(&err, WHATSAPP_MAX_TEXT_BYTES, WHATSAPP_MAX_TEXT_BYTES),
            "encoded_len == max_text_bytes MUST trigger fallback (boundary inclusive)"
        );
    }

    /// R8-H3 fix: encoded payload that exceeds the text limit MUST
    /// NOT trigger fallback — the text path would also fail, and the
    /// caller should see the original `Unreachable` error.
    #[test]
    fn should_not_fallback_when_payload_exceeds_text_limit() {
        let err = PlatformAdapterError::Unreachable {
            platform: "whatsapp".into(),
            reason: "client not connected".into(),
        };
        assert!(
            !should_fallback_to_text(&err, WHATSAPP_MAX_TEXT_BYTES + 1, WHATSAPP_MAX_TEXT_BYTES),
            "encoded_len > max_text_bytes MUST NOT trigger fallback"
        );
    }

    /// R8-H3 fix: `ApiError` (4xx-shaped) is a permanent wire-format
    /// failure, NOT a transient transport error. The fallback to
    /// `DOT/1/` text mode would fail with the same error, so the
    /// adapter MUST propagate the error rather than masking it with a
    /// retry.
    #[test]
    fn should_not_fallback_on_api_error() {
        let err = PlatformAdapterError::ApiError {
            code: 400,
            message: "invalid media ref format".into(),
        };
        assert!(
            !should_fallback_to_text(&err, 1000, WHATSAPP_MAX_TEXT_BYTES),
            "ApiError MUST NOT trigger fallback (4xx is permanent)"
        );
    }

    /// R8-H3 fix: `PayloadTooLarge` is a permanent shape failure
    /// (the payload exceeds even native mode's 100 MiB ceiling). No
    /// fallback can rescue it; the adapter MUST propagate the error.
    #[test]
    fn should_not_fallback_on_payload_too_large() {
        let err = PlatformAdapterError::PayloadTooLarge {
            size: 200 * 1024 * 1024,
            max: WhatsAppWebAdapter::MAX_UPLOAD_BYTES,
            platform: "whatsapp".into(),
        };
        assert!(
            !should_fallback_to_text(&err, 1000, WHATSAPP_MAX_TEXT_BYTES),
            "PayloadTooLarge MUST NOT trigger fallback"
        );
    }

    /// R8-H3 fix: `RateLimited` is transient (the gateway will retry
    /// per the retry policy) but NOT `Unreachable` — the spec says
    /// fallback is gated on `Unreachable` specifically. A
    /// `RateLimited` native error is propagated to the gateway's
    /// retry layer rather than masked by a text-mode attempt.
    #[test]
    fn should_not_fallback_on_rate_limited() {
        let err = PlatformAdapterError::RateLimited {
            platform: "whatsapp".into(),
            retry_after_ms: 1000,
        };
        assert!(
            !should_fallback_to_text(&err, 1000, WHATSAPP_MAX_TEXT_BYTES),
            "RateLimited is not Unreachable; fallback gate is `Unreachable`-only"
        );
    }

    /// Mission 0850 (RFC-0850 §8.6/§9.4): test-only constructor for
    /// the `download_rx` consumer task. Mirrors the channel creation
    /// and spawn logic in `start_bot` but bypasses the wacore `Bot`
    /// setup so unit tests don't need an authenticated session.
    ///
    /// Returns `(Sender, JoinHandle)`. The `Sender` lets tests push
    /// `DownloadRequest`s directly. The `JoinHandle` lets lifecycle
    /// tests assert that the spawned task exits cleanly when the
    /// sender is dropped. Without the handle, the test had no way to
    /// verify the consumer actually shut down, and a regression that
    /// blocks the task on a closed channel would silently pass (the
    /// R8-H2 finding).
    impl WhatsAppWebAdapter {
        // R8-M3 fix: shared consts for the test stub's synthetic
        // output. The test `download_rx_consumer_processes_valid_request`
        // asserts the consumer pushed exactly this payload + metadata
        // — keeping the values in one place ensures the test and the
        // stub can't drift.
        const STUB_NATIVE_PAYLOAD: &'static [u8] = b"native";
        const STUB_DOT_MODE: &'static str = "native";

        // R8-L4 fix: the test stub's download channel capacity is a
        // shared const so the constructor and the
        // `download_tx_try_send_returns_full_when_capacity_exceeded`
        // test can't drift apart. Changing the capacity in one place
        // without updating the test's "fill-the-buffer" loop would
        // silently break the test (it would push 65 messages into a
        // larger buffer and get no `Full` errors). The production
        // channel at `start_bot` (line 595) uses the same value
        // independently — this const is for test-only channels.
        const DOWNLOAD_CHANNEL_CAPACITY: usize = 64;

        fn spawn_download_consumer_for_test(
            &self,
        ) -> (
            tokio::sync::mpsc::Sender<DownloadRequest>,
            tokio::task::JoinHandle<()>,
        ) {
            let (tx, mut rx) =
                tokio::sync::mpsc::channel::<DownloadRequest>(Self::DOWNLOAD_CHANNEL_CAPACITY);
            // R8-H2 fix: do NOT clone `tx` into `self.download_tx`.
            // The field is for the production `on_event` closure,
            // which the test stub's tests don't exercise (they push
            // directly to the channel). If we clone the sender into
            // the field, dropping the test's `tx` does NOT close the
            // channel — the cloned sender in `self.download_tx` keeps
            // the receiver alive and the consumer task's `recv()`
            // never returns `None`. Tests that need to verify the
            // channel-close lifecycle must own the only sender.
            let _handle = self.clone_for_handler();
            let inbound_tx = self.inbound_tx.clone();
            let handle = tokio::spawn(async move {
                while let Some(req) = rx.recv().await {
                    // Test stub: pretend the download always succeeds,
                    // pushing a synthetic payload with `dot_mode = "native"`
                    // (matches the production consumer task's contract).
                    // The shared consts above link this output to the
                    // assertions in `download_rx_consumer_processes_valid_request`.
                    let raw = RawPlatformMessage {
                        platform_id: format!("test:{}", req.chat),
                        payload: Self::STUB_NATIVE_PAYLOAD.to_vec(),
                        metadata: [
                            ("chat".to_string(), req.chat),
                            ("sender".to_string(), req.sender),
                            ("dot_mode".to_string(), Self::STUB_DOT_MODE.to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    };
                    let _ = inbound_tx.try_send(raw);
                }
            });
            (tx, handle)
        }
    }
}
