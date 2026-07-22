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
//! - [`sync`] — `CipheroctoTable` + `ReplicatedTables` sync subscription config

pub mod ask;
pub mod ask_repo;
pub mod migrations;
pub mod sync;

pub use ask::{
    cache_key_hash, settlement_cost, Ask, AskError, AskId, AxisConsumption, AxisRate,
    AxisRegistryError, CacheClassification, CachePolicy, ConsumedReceiptIndex, MicroOCTO_W,
    ModelRateTable, ModelRef, PricingAxis, PricingAxisRegistry, SettlementEnvelope,
    SettlementError,
};
pub use ask_repo::{AskRepository, AskRow, RepoError};
pub use migrations::{
    apply_pending, list_migrations, Migration, MigrationError, BUILTIN_MIGRATIONS,
};
pub use sync::{CipheroctoTable, ReplicatedTables};
