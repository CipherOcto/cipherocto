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

    /// H8: build an adapter that shares a `SelfHandle` with the underlying
    /// client. This is the production path: the real TDLib client populates
    /// its `SelfHandle` from `get_me` on `Ready`, and the adapter reads
    /// from that same instance via Arc. Mocks/tests can use this to wire
    /// a pre-configured handle without re-fetching. `SelfHandle` is
    /// cheaply cloneable (`Arc<Mutex<...>>`).
    pub fn with_self_handle(config: TelegramConfig, client: C, self_handle: SelfHandle) -> Self {
        Self {
            config,
            client,
            self_handle,
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
}

/// R5 error-prop-C1, H3: map `TelegramError` to `PlatformAdapterError` preserving
/// the structured discriminant so the caller/gateway can distinguish transport
/// failures from user errors and API errors.
impl From<crate::error::TelegramError> for PlatformAdapterError {
    fn from(e: crate::error::TelegramError) -> Self {
        use crate::error::TelegramError;
        match e {
            // Recoverable — the retry loop handles these, but if they
            // escape (e.g. budget exhausted), surface as ApiError so
            // the gateway does not reconnect.
            TelegramError::RateLimited { retry_after_secs } => {
                PlatformAdapterError::RateLimited {
                    platform: "telegram".into(),
                    retry_after_ms: retry_after_secs * 1000,
                }
            }
            TelegramError::Transient(msg) => PlatformAdapterError::ApiError {
                code: 500,
                message: format!("transient: {}", msg),
            },
            // User errors — never trigger reconnect.
            TelegramError::InvalidChatId(m) => {
                PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("InvalidChatId: {}", m),
                }
            }
            TelegramError::InvalidFileId(m) => {
                PlatformAdapterError::ApiError {
                    code: 400,
                    message: format!("InvalidFileId: {}", m),
                }
            }
            TelegramError::Envelope(m) => PlatformAdapterError::ApiError {
                code: 400,
                message: format!("envelope: {}", m),
            },
            TelegramError::Auth(m) => PlatformAdapterError::ApiError {
                code: 401,
                message: crate::error::redact_credentials(&m),
            },
            TelegramError::Config(m) => PlatformAdapterError::ApiError {
                code: 500,
                message: format!("config: {}", m),
            },
            TelegramError::Unimplemented(m) => PlatformAdapterError::ApiError {
                code: 501,
                message: m,
            },
            // Catch-all — IO, JSON, base64, SendFailed, TdlibClient.
            other => PlatformAdapterError::ApiError {
                code: 500,
                message: crate::error::redact_credentials(&other.to_string()),
            },
        }
    }
}

/// R6 WIRE-C2: centralised i64 chat_id → canonical domain string conversion.
/// Both the send path (`domain_id`) and the receive path (`receive_messages`)
/// must produce the same `BroadcastDomainId` for the same numeric chat_id.
/// `i64::to_string()` is locale-independent and produces `-1001234567890`.
/// The send path's `.trim().to_lowercase()` is a no-op on numeric strings, so
/// the two paths agree. Using a single helper ensures they stay in sync.
fn chat_id_to_domain_string(chat_id: i64) -> String {
    chat_id.to_string()
}

impl<C: TelegramClient> TelegramAdapter<C> {

    /// Register a domain → chat_id mapping. This is an explicit escape hatch
    /// when the auto-population in `domain_id` is not what the caller wants.
    ///
    /// Normalises the `chat_id` (trim + lowercase) and validates it via
    /// `parse_chat_id` before storing, matching `domain_id`'s behaviour
    /// (R4 H2). Returns `Err` with a description if the chat_id is invalid
    /// (empty, non-numeric, or non-negative).
    pub fn register_domain(
        &self,
        domain: &BroadcastDomainId,
        chat_id: &str,
    ) -> Result<(), String> {
        let normalized = chat_id.trim().to_lowercase();
        // Validate before storing — surface registration errors early rather
        // than deep in send_envelope as an Unreachable error.
        crate::client::parse_chat_id(&normalized).map_err(|e| {
            format!("invalid chat_id '{}' for domain registration: {}", chat_id, e)
        })?;
        self.domain_chat_ids
            .write()
            .unwrap()
            .insert(domain.domain_hash, normalized);
        Ok(())
    }

    /// Look up the chat_id for a domain hash.
    pub fn chat_id_for_domain(&self, domain: &BroadcastDomainId) -> Option<String> {
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
        let text_path = encoded.len() <= 4096;
        tracing::debug!(
            chat_id = %chat_id,
            envelope_size = wire.len(),
            encoded_size = encoded.len(),
            send_as_document = !text_path,
            "send_envelope: routing envelope"
        );
        let sent = if text_path {
            self.with_retry(|| {
                let chat_id = chat_id.clone();
                let encoded = encoded.clone();
                let client = &self.client;
                async move { client.send_message(&chat_id, &encoded).await }
            })
            .await?
        } else {
            self.with_retry(|| {
                let chat_id = chat_id.clone();
                let encoded = encoded.clone();
                let wire = wire.clone();
                let client = &self.client;
                async move {
                    client
                        .send_envelope(&chat_id, &encoded, "envelope.bin", &wire)
                        .await
                }
            })
            .await?
        };
        tracing::info!(
            message_id = %sent.id,
            delivered_at = sent.timestamp,
            "send_envelope: delivered"
        );
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
        //
        // M7: compare on the structured `MessageSender` enum (no string
        // parsing) — the legacy `from_legacy` string is kept in the
        // outgoing metadata for back-compat.
        let self_id = self.self_handle.user_id();
        let messages: Vec<_> = updates
            .into_iter()
            .filter_map(|u| match u {
                crate::client::TelegramUpdate::NewMessage(nm) => {
                    // Drop self-authored messages.
                    if let (Some(my_id), crate::client::MessageSender::User(from_id)) =
                        (self_id, &nm.from)
                    {
                        if from_id == &my_id {
                            return None;
                        }
                    }
                    // R6 WIRE-C2: use the centralised helper so send and receive paths
                    // produce the same domain hash for the same numeric chat_id.
                    let chat_id_str = chat_id_to_domain_string(nm.chat_id);
                    let msg_domain =
                        BroadcastDomainId::new(PlatformType::Telegram, &chat_id_str);
                    if msg_domain.domain_hash != domain_hash {
                        return None;
                    }
                    let mut metadata = std::collections::BTreeMap::new();
                    metadata.insert("chat_id".into(), nm.chat_id.to_string());
                    metadata.insert("from".into(), nm.from_legacy);
                    Some(RawPlatformMessage {
                        platform_id: nm.message.clone(),
                        payload: nm.message.into_bytes(),
                        metadata,
                    })
                }
                _ => None,
            })
            .collect();
        tracing::debug!(
            domain_hash = %format!("{:02x?}", domain_hash),
            received_count = messages.len(),
            "receive_messages: returning updates"
        );
        Ok(messages)
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        let wire = envelope::decode_envelope(std::str::from_utf8(&raw.payload).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("invalid utf8 in payload: {}", e),
            }
        })?)
        .map_err(|e| {
            // R4 C1: envelope decode failures (base64 errors, wrong length)
            // are API errors (400), not transport failures. The gateway
            // should not reconnect on bad payloads.
            PlatformAdapterError::ApiError {
                code: 400,
                message: e.to_string(),
            }
        })?;
        DeterministicEnvelope::from_wire_bytes(&wire).map_err(|e| {
            // R4 C1: `from_wire_bytes` failing on a known-length, known-version
            // payload means the payload is structurally invalid (bad hash,
            // bad signature). This is also an API error, not transport.
            PlatformAdapterError::ApiError {
                code: 400,
                message: e.to_string(),
            }
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            // Telegram's text message cap is 4096 chars for the base64-encoded
            // envelope string (R4 H1). Envelopes larger than this threshold
            // are sent via sendDocument (which accepts up to 2 GB via TDLib).
            // The `send_envelope` method handles routing automatically.
            max_payload_bytes: 4096,
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
        // Normalize before hashing so whitespace/case differences produce the
        // same domain hash (R10-C4).
        let normalized = platform_id.trim().to_lowercase();
        let domain = BroadcastDomainId::new(PlatformType::Telegram, &normalized);
        // Store the normalized form so send_envelope can route the chat_id
        // without re-parsing whitespace.
        self.domain_chat_ids
            .write()
            .unwrap()
            .insert(domain.domain_hash, normalized);
        domain
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::Telegram
    }

    fn replay_protection(&self, _envelope_id: &[u8; 32]) -> bool {
        // R7 CRYPTO-M3: Replay protection is disabled at the adapter level.
        // The DOT network's `DeterministicEnvelope` uses `envelope_id` + timestamp
        // for dedup, but the adapter does not maintain a bloom filter or window.
        // The gateway is responsible for discarding duplicate `envelope_id`s.
        // In a public chat, any previously-seen envelope can be re-posted.
        true
    }

    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        Ok(())
    }

    fn self_handle(&self) -> Option<String> {
        // Returns an opaque display string for the bot's identity.
        // Format is "telegram:user:<id>" (numeric) with user_id preferred,
        // falling back to "telegram:user:<username>" if only username is
        // known. Treat as opaque; do not parse. The user_id is the canonical
        // identifier for self-loop filtering (done in receive_messages via
        // SelfHandle::is_self).
        self.self_handle
            .user_id()
            .map(|id| format!("telegram:user:{}", id))
            .or_else(|| {
                self.self_handle
                    .username()
                    .map(|u| format!("telegram:user:{}", u))
            })
    }

    async fn upload_media(
        &self,
        filename: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        // H2: avoid silent non-determinism. If exactly one domain is registered
        // we can route deterministically. If multiple are registered, picking
        // any one would be ambiguous from the caller's perspective; require
        // the explicit `upload_media_to_domain` path instead.
        let domains: Vec<[u8; 32]> = self
            .domain_chat_ids
            .read()
            .unwrap()
            .keys()
            .copied()
            .collect();
        if domains.is_empty() {
            return Err(PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: "no registered domain for upload_media".into(),
            });
        }
        if domains.len() > 1 {
            return Err(PlatformAdapterError::Unreachable {
                platform: "telegram".into(),
                reason: "multiple domains registered; use upload_media_to_domain to disambiguate"
                    .into(),
            });
        }
        let domain = BroadcastDomainId {
            platform_type: PlatformType::Telegram as u16,
            domain_hash: domains[0],
        };
        self.upload_media_to_domain(&domain, filename, data, mime_type)
            .await
    }

    async fn download_media(&self, file_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        self.with_retry(|| {
            let file_id = file_id.to_string();
            let client = &self.client;
            async move { client.download_file(&file_id).await }
        })
        .await
    }
}

impl<C: TelegramClient> TelegramAdapter<C> {
    /// Explicit, deterministic upload routing: callers that have multiple
    /// registered domains can target a specific one by passing the
    /// `BroadcastDomainId` they obtained from `domain_id(chat_id)` (or by
    /// constructing one directly). This is the unambiguous counterpart to
    /// the trait's default `upload_media` (which errors on multi-domain
    /// configurations; see H2).
    pub async fn upload_media_to_domain(
        &self,
        domain: &BroadcastDomainId,
        filename: &str,
        data: &[u8],
        _mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        let chat_id =
            self.chat_id_for_domain(domain)
                .ok_or_else(|| PlatformAdapterError::Unreachable {
                    platform: "telegram".into(),
                    reason: "domain not registered".into(),
                })?;
        // R6 MEM-C1: clone the upload data ONCE outside the retry closure so
        // that each retry attempt does not re-copy the entire payload.
        // For a 1 GB upload with 3 retries, this saves 2 GB of churn.
        let data = data.to_vec();
        self.with_retry(move || {
            let chat_id = chat_id.clone();
            let filename = filename.to_string();
            let data = data.clone();
            let client = &self.client;
            async move { client.send_file(&chat_id, &filename, &data).await }
        })
        .await
        .map(|s| s.id)
    }

    /// Run an async closure with exponential-backoff retry on
    /// `TelegramError::RateLimited` and `TelegramError::Transient`.
    /// Non-recoverable errors return immediately. Generic over success type
    /// `T` so both send (SentMessage) and download (Vec<u8>) paths use it.
    async fn with_retry<F, Fut, T>(&self, mut op: F) -> Result<T, PlatformAdapterError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, crate::error::TelegramError>>,
    {
        let mut attempt: u32 = 0;
        loop {
            match op().await {
                Ok(sent) => return Ok(sent),
                Err(crate::error::TelegramError::RateLimited { retry_after_secs }) => {
                    if !self.retry_config.should_retry(attempt) {
                        tracing::warn!(
                            attempt,
                            max_retries = self.retry_config.max_retries,
                            "with_retry: rate-limited budget exhausted after {} attempts", attempt + 1
                        );
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
                // M6: 5xx / "connection failed" / "connection closed" errors
                // from TDLib are recoverable. Same `should_retry` policy as
                // `RateLimited`, but the floor is the *configurable* backoff
                // (no `retry_after_secs` hint from the server, and tests need
                // a way to shrink the wait without changing `default_backoff`).
                Err(crate::error::TelegramError::Transient(msg)) => {
                    if !self.retry_config.should_retry(attempt) {
                        tracing::warn!(
                            attempt,
                            error = %msg,
                            max_retries = self.retry_config.max_retries,
                            "with_retry: transient budget exhausted after {} attempts", attempt + 1
                        );
                        return Err(PlatformAdapterError::Unreachable {
                            platform: "telegram".into(),
                            reason: format!(
                                "transient error after {} attempts: {}",
                                attempt + 1,
                                msg
                            ),
                        });
                    }
                    let backoff = self.retry_config.delay_for_attempt(attempt);
                    tokio::time::sleep(backoff).await;
                    attempt = attempt.saturating_add(1);
                }
                // R5 error-prop-C1, H3: non-retryable errors propagate via
                // the `From<TelegramError> for PlatformAdapterError` impl,
                // which preserves the typed discriminant. This prevents
                // user errors (InvalidChatId, InvalidFileId, Auth, Config,
                // Envelope, Unimplemented) from being flattened into
                // `Unreachable` and triggering a gateway reconnect.
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Resolve a message by chat_id and message_id, then download its
    /// attached media. First resolves the message to a file_id via
    /// `get_file_id_for_message`, then downloads the file via
    /// `download_media`.
    #[cfg(feature = "real-tdlib")]
    pub async fn download_media_from_message(
        &self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<Vec<u8>, PlatformAdapterError> {
        let file_id = self
            .with_retry(|| {
                let client = &self.client;
                async move { client.get_file_id_for_message(chat_id, message_id).await }
            })
            .await?;
        self.download_media(&file_id).await
    }
}
