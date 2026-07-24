//! Inference Task Market (RFC-0918).
//!
//! Public surface for the submodules:
//! - [`orders`]   — `TaskSpec`, `TaskType`, `TaskMarket` (wraps `OrderBook`).
//! - [`escrow`]   — task-scoped escrow lifecycle (wraps `marketplace::escrow`).
//! - [`dispute`]  — dispute creation + resolution.
//! - [`slashing`] — task-market-facing wrapper around `SlashingLedger`.
//!
//! Cross-references: `marketplace::orderbook` (Gap 5 OrderBook),
//! `marketplace::escrow` (Gap 5 Escrow), `marketplace::slashing` (Gap 5.3).
//!
//! Status: implements Phase 1 (Core Market) skeleton of RFC-0918.

pub mod dispute;
pub mod escrow;
pub mod orders;
pub mod slashing;

pub use dispute::{Dispute, DisputeError, DisputeReason, Evidence};
pub use escrow::{TaskEscrow, TaskEscrowError};
pub use orders::{TaskMarket, TaskSpec, TaskType};
pub use slashing::{TaskMarketSlashing, TaskSlashError};
