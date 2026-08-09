//! Re-export shim — wire format now lives in `octo-cap-macaroon` (Layer 4
//! extension crate per RFC-0965 per-extension crate layout mandate).
//!
//! Mission 0957-ext-macaroon Phase 2b-4 migration moved the canonical
//! implementation into `octo_cap_macaroon::wire`. This shim preserves
//! the existing `octo_wallet::capability::wire::*` import paths for
//! backward compatibility. No new code lives here — all behavior lives
//! in the extension crate.

pub use octo_cap_macaroon::wire::*;
