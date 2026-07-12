//! Query layer — comprehensive message + events query backed by a
//! SQL store (stoolap), Tantivy FTS sidecar, and a hybrid embedder.
//!
//! Gated behind the `query` cargo feature. Phase 0 of
//! `docs/plans/2026-07-11-whatsapp-query-layer-design.md`.
//!
//! Submodules:
//! - [`schema`]: idempotent SQL DDL.
//! - [`ingester`]: write-path that mirrors `InboundEvent` into the
//!   `events` / `messages` tables with `INSERT OR IGNORE` so replays
//!   from the NDJSON canonical log are safe.
//! - [`embedder`]: text-to-vector encoder (local candle + remote opt-in).
//!
//! Tantivy sidecar + the `QueryService` land in later phases (see
//! the plan doc).

#![cfg(feature = "query")]

pub mod embedder;
pub mod embedder_job;
pub mod ingester;
pub mod schema;
pub mod subsystem;
pub mod tantivy_sidecar;

pub use embedder::{EmbedError, Embedder, HybridEmbedder, LocalCandleEmbedder};
pub use embedder_job::{EmbedderJob, EmbedderQueue, JobConfig, JobMetrics, JobMetricsSnapshot};
pub use ingester::{QueryError, QueryIngester};
pub use schema::{migrate, SCHEMA_VERSION};
pub use subsystem::{open_subsystem, QuerySubsystem, SubsystemError};
pub use tantivy_sidecar::{IndexedMessage, TantivyError, TantivySidecar, TextHit};
