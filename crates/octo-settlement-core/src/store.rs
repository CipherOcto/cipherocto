//! `SettlementStore` trait per RFC-0014 §Module Layout `store`.
//!
//! Canonical CRUD-style interface for asks + settlements. Methods take
//! `&self` (interior mutability via `Arc<Mutex<Database>>` in the
//! concrete Stoolap impl per RFC-0014 §Rationale §Why
//! `SettlementStore::mint`/`settle`/`consume` use `&self`).
//!
//! Type-level append-only enforcement lives on the separate
//! [`AppendOnlyReceiptSink`](crate::sink::AppendOnlyReceiptSink) trait
//! (raw ledger path). The two-trait split keeps the canonical CRUD
//! interface ergonomic for domain crates while preserving the
//! append-only guarantee on the raw ledger path.

use crate::ask::Ask;
use crate::error::SettlementError;
use crate::receipt::Receipt;

/// Canonical settlement store trait. Concrete impls (e.g. `StoolapStore`
/// in `quota-router-sm-engine::store`) provide Stoolap-backed
/// persistence; tests use in-memory mocks.
pub trait SettlementStore {
    /// Mint a new ask (transition state → `Minted`). Returns the
    /// generated `ask_id` (32-byte BLAKE3-256 digest).
    fn mint(&self, ask: &Ask) -> Result<[u8; 32], SettlementError>;

    /// Settle an ask via router receipt (transition state → `Settled`).
    /// `ask_id` identifies the ask; `receipt` carries the router
    /// settlement signature.
    fn settle(&self, ask_id: &[u8; 32], receipt: &Receipt) -> Result<(), SettlementError>;

    /// Consume a settled ask (transition state → `Consumed`).
    /// `ask_id` identifies the ask to consume.
    fn consume(&self, ask_id: &[u8; 32]) -> Result<(), SettlementError>;

    /// Look up an ask by `ask_id`. Returns `None` if not found.
    fn get(&self, ask_id: &[u8; 32]) -> Result<Option<Ask>, SettlementError>;
}
