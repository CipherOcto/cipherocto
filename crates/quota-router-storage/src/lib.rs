//! Cipherocto-side persistence for the quota-router domain (moved from octo-core 2026-07-21).
//!
//! Owns the `asks` table schema, migration runner, DAO, and sync subscription
//! config for the quota-router marketplace. Per [[stoolap-general-purpose-db]],
//! cipherocto owns consumer schema; the fork is a general-purpose DB.
//!
//! Modules:
//! - [`ask`] — `Ask`, `AskId`, `PricingAxis`, `SettlementEnvelope`, `ConsumedReceiptIndex` types
//! - [`migrations`] — migration runner with version tracking (`apply_pending`)
//! - [`ask_repo`] — `AskRepository` DAO (put/get/cheapest/list_by_asker/delete)
//! - [`marketplace`] — in-memory `MarketplaceIndex` for ordered lookups (RFC-0959 §Roles)
//! - [`sync`] — `CipheroctoTable` + `ReplicatedTables` sync subscription config
//! - [`cache_key`] — `cache_key()` BLAKE3 keyed-hash (RFC-0959 §Data Structures)
//! - [`circuit_breaker`] — anti-fraud monitor state machine (RFC-0959 §Lifecycle)
//! - [`anti_fraud`] — multi-layer AntiFraudMonitor wrapping the breaker (RFC-0959 §Adversary A5)
//! - [`axis_registry_toml`] — TOML parser for `pricing-axes.toml` (RFC-0959 §Data Structures)

pub mod anti_fraud;
pub mod ask;
pub mod ask_repo;
pub mod axis_registry_toml;
pub mod cache_key;
pub mod circuit_breaker;
pub mod consumed_receipt_repo;
pub mod marketplace;
pub mod migrations;
pub mod settlement_event_repo;
pub mod sync;

pub use anti_fraud::{
    AntiFraudMonitor, AskerHitRate, FraudSignal, FraudSignalKind, MultiLayerCacheStatus,
    ProviderCacheControl, RecordOutcome, ReputationDelta,
};
pub use ask::{
    cache_key_hash, compute_cost, compute_settlement_hash, settlement_cost,
    sign_settlement_receipt, verify_settlement_receipt, Ask, AskError, AskId, AskSigned,
    AskSignedError, AskUnsignedPayload, AskerDid, AxesConsumed, AxisConsumption, AxisId, AxisRate,
    AxisRegistryError, CacheClassification, CachePolicy, ConsumedReceiptIndex, Ed25519PublicKey,
    Ed25519Signature, MicroOCTO_W, MicroOCTO_WNewtype, ModelRateTable, ModelRef, ModelRefError,
    NodeType, NodeTypeParseError, OCTO_WAmount, PricingAxis, PricingAxisRegistry,
    SettlementEnvelope, SettlementError, SettlementEvent, SettlementReceipt, TokenCount,
    SETTLEMENT_HASH_DOMAIN,
};
pub use ask_repo::{AskRepository, AskRow, RepoError};
pub use axis_registry_toml::{
    is_snake_case, load_from_path as load_axis_registry_from_path,
    load_from_str as load_axis_registry_from_str, AxisRegistryTomlError, DEFAULT_MVP_TOML,
};
pub use cache_key::{
    cache_key, cache_key_from_bytes, cache_key_hash_value, canonical_prompt_bytes, CACHE_KEY_DOMAIN,
};
pub use circuit_breaker::{
    AxisClassification, CircuitBreaker, CircuitBreakerError, CircuitState, TransitionEvent,
    TransitionReason, CACHE_HIT_RATE_TRIP_THRESHOLD, MIN_PROMPT_DIVERSITY, RECOVERY_COOLDOWN_SECS,
    RECOVERY_OBSERVE_SECS, WINDOW_SIZE,
};
pub use consumed_receipt_repo::ConsumedReceiptRepository;
pub use marketplace::{MarketplaceIndex, ACTIVE_ASK_CAP};
pub use migrations::{
    apply_pending, list_migrations, Migration, MigrationError, BUILTIN_MIGRATIONS,
};
pub use settlement_event_repo::{
    PersistedSettlementEvent, SettlementEventInsert, SettlementEventRepository,
};
pub use sync::{CipheroctoTable, ReplicatedTables};
