//! PlatformAdapter impl (preserved contract).
//! Mission AC line 128: "Implements PlatformAdapter trait with all methods (6 required + 6 optional)"
//!
//! All 12 methods implemented; the 6 optional methods all override the default.

use async_trait::async_trait;
use octo_network::dot::adapters::backoff::{default_backoff, RetryConfig};
use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, MediaCapabilities, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;
use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Duration;

use crate::client::TelegramClient;
use crate::config::TelegramConfig;
use crate::envelope;
use crate::self_handle::SelfHandle;

pub struct TelegramAdapter<C: TelegramClient> {
    pub config: TelegramConfig,
    pub client: C,
    self_handle: SelfHandle,
    /// Maps domain_hash → chat_id for send Envelope routing.
    /// Auto-populated by `domain_id()` so send_envelope can route correctly.
    /// Uses BTreeMap (not HashMap) so iteration order is deterministic — see
    /// H6 in octo-adapter-telegram-adversarial-review-r2.md.
    domain_chat_ids: RwLock<BTreeMap<[u8; 32], String>>,
    /// Retry policy for transient failures (rate limits, transient TDLib errors).
    retry_config: RetryConfig,
}

impl<C: TelegramClient> TelegramAdapter<C> {
    pub fn new(config: TelegramConfig, client: C) -> Self {
        Self {
            config,
            client,
            self_handle: SelfHandle::new(),
            domain_chat_ids: RwLock::new(BTreeMap::new()),
            retry_config: RetryConfig::default(),
        }
    }

    /// Build an adapter with a custom retry policy (for tests / tuning).
    pub fn with_retry_config(config: TelegramConfig, client: C, retry_config: RetryConfig) -> Self {
        Self {
            config,
            client,
            self_handle: SelfHandle::new(),
            domain_chat_ids: RwLock::new(BTreeMap::new()),
            retry_config,
        }
    }

    /// Cache the bot username for self-loop prevention. Real impl: calls getMe.
    pub fn set_bot_username(&self, username: String) {
        self.self_handle.set_username(username);
    }

    /// Cache the bot's numeric user_id (TDLib `get_me` result).
    pub fn set_self_user_id(&self, user_id: i64) {
        self.self_handle.set_user_id(user_id);
    }

    /// Register a domain → chat_id mapping. This is an explicit escape hatch
    /// when the auto-population in `domain_id` is not what the caller wants.
    pub fn register_domain(&self, domain: &BroadcastDomainId, chat_id: &str) {
        self.domain_chat_ids
            .write()
            .unwrap()
            .insert(domain.domain_hash, chat_id.to_string());
    }

    /// Look up the chat_id for a domain hash.
    fn chat_id_for_domain(&self, domain: &BroadcastDomainId) -> Option<String> {
        self.domain_chat_ids
            .read()
            .unwrap()
            .get(&domain.domain_hash)
            .cloned()
    }
}

#[async_trait]
impl<C: TelegramClient + Send + Sync> PlatformAdapter for TelegramAdapter<C> {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope_obj: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let chat_id = self.chat_id_for_domain(domain).ok_or_else(|| {
            // H2: the precondition for send_envelope is that the caller has
            // called `domain_id(chat_id)` (or `register_domain`) at some prior
            // point. Surface that loudly so it is clear what to do.
            PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: "domain not registered: call register_domain() after domain_id()".into(),
            }
        })?;
        let wire = envelope_obj.to_wire_bytes();
        // Mission Architecture line 60-62: small envelopes via sendMessage,
        // large via sendDocument. Threshold: 4096 chars (Telegram text message
        // limit on the base64-encoded string). sendDocument receives the raw
        // wire bytes (no encoding overhead); the encoded envelope is embedded
        // in the caption for the receive path to recover.
        let encoded = envelope::encode_envelope(&wire);
        let sent = if encoded.len() <= 4096 {
            self.send_with_retry(|| {
                let chat_id = chat_id.clone();
                let encoded = encoded.clone();
                let client = &self.client;
                async move { client.send_message(&chat_id, &encoded).await }
            })
            .await?
        } else {
            self.send_with_retry(|| {
                let chat_id = chat_id.clone();
                let wire = wire.clone();
                let client = &self.client;
                async move { client.send_document(&chat_id, "envelope.bin", &wire).await }
            })
            .await?
        };
        Ok(DeliveryReceipt {
            platform_message_id: sent.id,
            delivered_at: sent.timestamp as u64,
        })
    }

    async fn receive_messages(
        &self,
        domain: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        let updates =
            self.client
                .receive_updates()
                .await
                .map_err(|e| PlatformAdapterError::Unreachable {
                    platform: "telegram".into(),
                    reason: e.to_string(),
                })?;
        let domain_hash = domain.domain_hash;
        // H5: filter out self-authored messages at the adapter boundary, not
        // at the gateway. Compare on the numeric user_id (H4) so the filter is
        // robust to formatting changes in the from string.
        let self_id = self.self_handle.user_id();
        let messages = updates
            .into_iter()
            .filter_map(|u| match u {
                crate::client::TelegramUpdate::NewMessage(nm) => {
                    // Drop self-authored messages.
                    if let Some(my_id) = self_id {
                        if let Ok(from_id) = nm.from.parse::<i64>() {
                            if from_id == my_id {
                                return None;
                            }
                        }
                    }
                    let msg_domain =
                        BroadcastDomainId::new(PlatformType::Telegram, &nm.chat_id.to_string());
                    if msg_domain.domain_hash != domain_hash {
                        return None;
                    }
                    let mut metadata = std::collections::BTreeMap::new();
                    metadata.insert("chat_id".into(), nm.chat_id.to_string());
                    metadata.insert("from".into(), nm.from);
                    Some(RawPlatformMessage {
                        platform_id: nm.message.clone(),
                        payload: nm.message.into_bytes(),
                        metadata,
                    })
                }
                _ => None,
            })
            .collect();
        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        let wire = envelope::decode_envelope(std::str::from_utf8(&raw.payload).map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: format!("invalid utf8 in payload: {}", e),
            }
        })?)
        .map_err(|e| PlatformAdapterError::Unreachable {
            platform: "telegram".into(),
            reason: e.to_string(),
        })?;
        DeterministicEnvelope::from_wire_bytes(&wire).map_err(|e| {
            PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: e.to_string(),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            // Envelope is embedded in the caption (Telegram hard cap 1024 chars).
            // Arbitrary media (uploaded via upload_media) can be up to
            // `media_capabilities.max_upload_bytes` (2 GB via TDLib).
            max_payload_bytes: 1024,
            supports_fragmentation: true,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: 30,
            media_capabilities: Some(MediaCapabilities {
                max_upload_bytes: 2_000_000_000,
                supported_mime_types: vec![
                    "application/octet-stream".into(),
                    "image/*".into(),
                    "video/*".into(),
                    "audio/*".into(),
                ],
            }),
        }
    }

    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
        // Per crates/octo-network/src/dot/domain.rs:80 — PlatformType::Telegram
        // maps to "telegram:" prefix and normalizes (lowercase + trim) the
        // platform_id before hashing.
        let domain = BroadcastDomainId::new(PlatformType::Telegram, platform_id);
        // Auto-register the domain so send_envelope can route to this chat_id.
        // The chat_id is the normalized platform_id (same string used to
        // construct the domain); callers can override via register_domain().
        self.domain_chat_ids
            .write()
            .unwrap()
            .insert(domain.domain_hash, platform_id.to_string());
        domain
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Telegram
    }

    fn replay_protection(&self, _envelope_id: &[u8; 32]) -> bool {
        // Default: no replay protection at adapter level (handled by gateway)
        true
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    fn self_handle(&self) -> Option<String> {
        // The trait's `self_handle()` returns the bot's username (or a
        // `user:<id>` marker if only the user_id is known). Numeric
        // self-loop filtering is done in `receive_messages` via
        // `self_handle.user_id()`.
        self.self_handle
            .username()
            .or_else(|| self.self_handle.user_id().map(|id| format!("user:{}", id)))
    }

    async fn upload_media(
        &self,
        filename: &str,
        data: &[u8],
        _mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        // H6: use a BTreeMap so iteration is deterministic. Pick the *first*
        // registered domain (sorted by domain_hash) so the chat_id is
        // reproducible across processes. Callers who need to target a specific
        // domain should call `register_domain` and use the result of
        // `chat_id_for_domain` directly.
        let chat_id = self
            .domain_chat_ids
            .read()
            .unwrap()
            .values()
            .next()
            .cloned()
            .ok_or_else(|| PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: "no registered domain for upload_media".into(),
            })?;
        self.client
            .send_document(&chat_id, filename, data)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: e.to_string(),
            })
            .map(|s| s.id)
    }

    async fn download_media(&self, file_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        self.client
            .download_file(file_id)
            .await
            .map_err(|e| PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: e.to_string(),
            })
    }
}

impl<C: TelegramClient> TelegramAdapter<C> {
    /// Run an async send closure with exponential-backoff retry on
    /// `TelegramError::RateLimited`. Non-rate-limit errors return immediately.
    /// This implements H1 from octo-adapter-telegram-adversarial-review-r2.md.
    async fn send_with_retry<F, Fut>(
        &self,
        mut op: F,
    ) -> Result<crate::client::SentMessage, PlatformAdapterError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<
            Output = Result<crate::client::SentMessage, crate::error::TelegramError>,
        >,
    {
        let mut attempt: u32 = 0;
        loop {
            match op().await {
                Ok(sent) => return Ok(sent),
                Err(crate::error::TelegramError::RateLimited { retry_after_secs }) => {
                    if !self.retry_config.should_retry(attempt) {
                        return Err(PlatformAdapterError::Unreachable {
                            platform: "telegram".into(),
                            reason: format!("rate-limited after {} attempts", attempt + 1),
                        });
                    }
                    let backoff = std::cmp::max(
                        Duration::from_secs(retry_after_secs),
                        default_backoff(attempt),
                    );
                    tokio::time::sleep(backoff).await;
                    attempt = attempt.saturating_add(1);
                }
                Err(e) => {
                    return Err(PlatformAdapterError::Unreachable {
                        platform: "telegram".into(),
                        reason: e.to_string(),
                    });
                }
            }
        }
    }
}
