//! Security sub-system for `octo-whatsapp`.
//!
//! Phase 5 §Security. Currently exposes:
//! - [`tokens`]: bearer-token store with rotation, grace period, and
//!   revocation list.
//! - [`auth`]: bearer-auth middleware + per-IP failure backoff used by
//!   the IPC server.
//!
//! Future phases add Prometheus metrics, OTLP tracing, and replay-nonce
//! tables. Per `docs/plans/2026-07-07-whatsapp-runtime-cli-mcp-phase5.md`
//! §Part A, token rotation is the first deliverable.

pub mod auth;
pub mod tokens;

pub use auth::{authenticate, AuthBackoff, AuthBackoffHandle};
pub use tokens::{GraceEntry, GraceFile, TokenDescriptor, TokenError, TokenStore};
