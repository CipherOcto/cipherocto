//! ZK capability substrate — **import-path guide** (RFC-0973 §Phase 5).
//!
//! # What moved
//!
//! This module used to be a `pub use octo_cap_zk::*` re-export shim, plus a
//! `to_zk_node_type` projection. Both are gone, deliberately:
//!
//! - The wallet is Layer B and `octo-cap-zk` is Layer E. Re-exporting it made
//!   the wallet depend on a strategy crate, which inverts the layer model that
//!   RFC-0973 Lifecycle Requirement 8 forbids. A core crate may not depend on a
//!   strategy crate to discover one.
//! - The ZK mint API is unchanged and still available. It now lives at
//!   `octo_cap_zk::{mint_with_zk, ProofBundle, PublicInputs, …}`, which is the
//!   path RFC-0965 §Per-Extension Crate Layout gives an extension crate. Import
//!   it from there.
//!
//! This module remains so the old import path resolves to a **compile error
//! that says what to do**, rather than to a bare "unresolved import".
//!
//! # What was here
//!
//! - `mint_with_zk`, `mint_with_zk_and_signers`, `ProofBundle`, `PublicInputs`,
//!   `PrivateWitness`, `ExecutionTrace`, `TraceStep`, `CapabilityClass`,
//!   `ZkMintError`, `proof_bundle_to_wire`, `proof_bundle_from_wire`,
//!   `bundled_casm_hash`, `COMPILED_CASM_BLAKE3_HASH`
//! - `to_zk_node_type`, a projection from the wallet's `NodeType` to
//!   `octo_cap_zk::NodeType`.
//!
//! # `to_zk_node_type` is deleted, and why that is the right call
//!
//! RFC-0973 §Phase 5 requires deciding the projection's disposition rather than
//! silently dropping it, because dropping it would dispose of the wallet-side
//! carrier of the RFC-0958 §NodeType mint gate.
//!
//! **It had no production consumer.** Every call site in the tree was inside
//! this file's own `mod tests`. Nothing outside the wallet called it, and no
//! wallet production path used it.
//!
//! **The mint-time ZK gate does not live here.** The gate that actually runs is
//! `NodeType::permits_zk_mint` in `octo_cap_zk`, enforced inside
//! `mint_with_zk_and_signers` — which returns `NodeTypeCannotMintZKCap` for
//! Wholesale. The wallet's own `crate::node::NodeType` carries the same
//! predicate, so the wallet-side gate survives the removal. Deleting the
//! projection removes a duplicate, not the gate.
//!
//! This reconciles with mission `0973-v36-c-substrate-corrections`, which
//! recorded that `NodeType` is retained in the wallet as the mint-time ZK gate.
//! That remains true: the wallet keeps `crate::node::NodeType` and its
//! `permits_zk_mint`. What is gone is the *mirror* into the extension crate,
//! which existed only because both types were foreign to either crate and the
//! orphan rule forbade a `From` impl.
//!
//! # Migration
//!
//! ```ignore
//! // before
//! use octo_wallet::capability::zk_mint::{PublicInputs, mint_with_zk};
//! use octo_wallet::capability::{PublicInputs, mint_with_zk};
//!
//! // after
//! use octo_cap_zk::{PublicInputs, mint_with_zk};
//! ```

/// The re-export shim was removed by RFC-0973 §Phase 5.
///
/// Import the ZK mint API from `octo_cap_zk` — it is a Layer-E extension crate
/// and owns that API under RFC-0965 §Per-Extension Crate Layout.
///
/// The wallet keeps `octo_wallet::capability::attestation`, which is the
/// RFC-0973 attestation substrate, and `octo_wallet::node::NodeType`, which
/// carries the mint-time ZK gate.
pub const ZK_MINT_MOVED: () = ();
