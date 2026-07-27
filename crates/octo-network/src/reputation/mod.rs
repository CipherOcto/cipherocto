//! Federation-side reputation types (mission 0855p-b + 0968 Phase 4).
//!
//! ## `SlashReputationStoreCompat`
//!
//! The 0855p-b replacement for the legacy pubkey-keyed
//! `octo_network::mon::reputation::SlashReputationStore`. Reads the
//! persisted `ReputationStore` via `query_attestations` to compute
//! `global_slash_count(did)` and the canonical RFC-0968 §10
//! `election_priority` formula. Legacy `priority_legacy` is preserved
//! as a back-compat field for the differential test (AC L33:
//! 1000-candidate set, byte-identical priority ordering).
//!
//! ## Authority model (RFC-0968-A1 amendment 28)
//!
//! Recorder signature is authoritative; coordinator / attestor
//! signatures are non-authoritative transport metadata. The store
//! reads only the recorder DID + the canonical event body — never
//! the gossip envelope.

mod slash_store;

pub use slash_store::{SlashReputationStoreCompat, HARD_THRESHOLD};
