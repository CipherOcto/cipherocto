//! RFC-0862 v1.3 + v1.4 cross-instance DID coordination substrate.
//!
//! Per RFC-0862 v1.3 §Specification §Substrate types. This module
//! re-exports the newtype primitives, error enums, and `WalEntry` type
//! that the writer-election / drain / DID-write coordinator surfaces
//! depend on. The sealed trait surfaces (`WriterElection`,
//! `WriterElectionForceRelinquish`, `BootstrapOrchestrator`,
//! `DrainCoordinator`, `DidWriteCoordinator`, `WalWriter`, `WalReader`,
//! `WalNonceScanner`, non-deprecated `WalAppender`), the `octo_sync::did`
//! modules (`canonical_hash`, `EncodedDidDocument`), and the governance
//! verification helpers (`governance_signature_message`,
//! `verify_governance_attestation`, `ed25519_verify`) land in the
//! follow-on task per mission `0871e-f7-coordinator-impl` task #121.
//!
//! # Layer discipline
//!
//! `octo-sync` is a Layer B-substrate crate (per
//! [[cipherocto-design-principles]] §Layer direction). The concrete
//! `WriterElection` + `DidWriteCoordinator` impls (task #122) and the
//! multi-instance test harness (task #123) land here. The
//! `DidWriteCoordinator` trait itself lives in `octo-ident` (Layer B
//! substrate); `octo-sync` provides the concrete impl that downstream
//! crates (e.g., `octo-identity-resolver-node`) inject via
//! `IdentityResolverNodeConfig::write_coordinator`.
//!
//! # Why `ChainId` / `DidDocument` are not redefined here
//!
//! `ChainId` lives in `octo-ident` per RFC-0010 v1.4 §ChainId Namespace
//! Extension (typed 17-byte `ChainNamespace` form). `DidDocument` lives
//! in `octo-ident` per RFC-0010 v1.3 storage extension + v1.5 rich
//! fields. Substrate consumes these via the trait surface in task #121;
//! the newtype ports here are pure-substance (no `octo-ident` dep at
//! task #120).

pub mod hlc;
pub mod ids;
pub mod records;
pub mod state;
pub mod wal;

// Convenience re-exports for downstream callers.
pub use hlc::{ClockFn, HlcClock, HlcError, HlcTimestamp};
pub use ids::{
    ConfigError, OperatorId, OperatorSet, OperatorSignature, ShardKey, ShardMissionId, WriterNodeId,
};
pub use records::{
    ActualDrained, BootstrapError, DidWriteCoordinatorError, DrainCoordinatorError, NonceRecord,
    PeerIdentity, WriterElectionError,
};
pub use state::{ReplayState, WriterContext, WriterIdentity};
pub use wal::{
    WalEntry, ENTRY_TYPE_DID_REGISTER, ENTRY_TYPE_DID_REVOKE, ENTRY_TYPE_DRAIN,
    ENTRY_TYPE_NONCE_RECORD, WAL_MAGIC_V12, WAL_MAGIC_V13,
};
