//! Re-export shim — discharge channels now live in `octo-cap-macaroon`
//! (Layer 4 extension crate per RFC-0965 per-extension crate layout
//! mandate).
//!
//! Mission 0957-ext-macaroon Phase 2b-5 migration moved the canonical
//! implementation into `octo_cap_macaroon::discharge`. This shim
//! preserves the existing
//! `octo_wallet::capability::discharge::*` import paths for backward
//! compatibility. No new code lives here — all behavior lives in the
//! extension crate.

pub use octo_cap_macaroon::discharge::*;
