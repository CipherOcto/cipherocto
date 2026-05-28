//! Deterministic Overlay Mempool — RFC-0857
//!
//! Module structure:
//! - `intent` — OverlayIntent struct and IntentType/ExecutionClass enums
//! - `admission` — Deterministic admission pipeline
//! - `ordering` — Canonical intent ordering
//! - `pool` — Mission-scoped mempool with capacity limits
//! - `eviction` — Deterministic eviction
//! - `propagation` — DGP integration
//! - `economics` — Fee model and distribution
//! - `error` — DomError enum

pub mod admission;
pub mod economics;
pub mod error;
pub mod eviction;
pub mod intent;
pub mod ordering;
pub mod pool;
pub mod propagation;

pub use error::DomError;
pub use intent::{ExecutionClass, IntentType, OverlayIntent};
pub use ordering::canonical_sort;
pub use pool::MempoolPool;
