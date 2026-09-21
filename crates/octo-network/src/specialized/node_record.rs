//! `SpecializedNodeRecord` — RFC-0871 Specialized Node
//! Protocol Envelope substrate projection (RFC-0011-q
//! Phase 9 G11).
//!
//! Layer B substrate addition per RFC-0011-h
//! §Substrate-Additions Companion Missions row G11.
//! Per-extension transport impl crates (Layer D) are
//! OUT OF SCOPE per per-extension crate pattern; this
//! module exposes only the Layer B substrate projection
//! consumed by `octo network node show` (read) +
//! `octo network node bind <node_id_hex> --holder-did
//! <did>` (mutating) per RFC-0011-q Phase 9
//! §Subcommand Taxonomy.
//!
//! `BTreeMap` chosen over `HashMap` for deterministic
//! iteration order per RFC-0011-h §Output Envelope
//! order determinism.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `HolderDid` — substrate-faithful holder DID newtype
/// per RFC-0871 §Identity Projection. Local newtype
/// (not `octo-did::Did` or `octo-wallet::Did`) to avoid
/// adding a new cross-crate dependency on the substrate
/// slice per per-extension crate pattern. Per-extension
/// impl crates in Layer D own DID validation
/// (parse + canonicalize + signature check); CLI layer
/// passes the `--holder-did` string as-is without
/// validation per RFC-0011-q §Implicit Assumptions
/// row 3.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolderDid(String);

impl HolderDid {
    /// Construct a `HolderDid` from a string slice.
    /// No format validation at substrate layer —
    /// per-extension impl crates own DID validation
    /// per RFC-0011-q §Implicit Assumptions row 3.
    pub fn new(did: impl Into<String>) -> Self {
        Self(did.into())
    }

    /// Borrow the underlying DID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for HolderDid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// `SpecializedNodeRecord` — substrate-faithful
/// specialized node record per RFC-0871. Per
/// RFC-0011-q Phase 9 §Substrate Mapping Table, this
/// struct is the Layer B substrate projection consumed
/// by `octo network node show` + `octo network node
/// bind`. Per-extension transport impl crates (Layer
/// D) are OUT OF SCOPE.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecializedNodeRecord {
    /// 32-byte canonical node identifier.
    pub node_id: [u8; 32],
    /// Holder DID post-bind (None pre-bind).
    pub holder_did: Option<HolderDid>,
    /// Node class (Builder + Provider + Storage +
    /// Bandwidth + Orchestrator).
    pub node_class: NodeClass,
    /// Creation epoch (RFC-0855 §epoch).
    pub creation_epoch: u64,
    /// Operator metadata. `BTreeMap` for deterministic
    /// iteration order per RFC-0011-h §Output Envelope
    /// order determinism.
    pub metadata: BTreeMap<String, String>,
}

/// `NodeClass` — closed enum of specialized node
/// classes per RFC-0871 §Node Taxonomy. Matches
/// existing role surface (RFC-0011-d role taxonomy);
/// additive type per [[cipherocto-design-principles]]
/// §Extension over enumeration (does NOT introduce a
/// new central enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeClass {
    /// Builder node — assembles and validates
    /// envelopes (RFC-0855).
    Builder,
    /// Provider node — serves quota / routing /
    /// federation traffic (RFC-0870).
    Provider,
    /// Storage node — persists envelopes and ledger
    /// snapshots (RFC-0855 §storage).
    Storage,
    /// Bandwidth node — relays envelopes through
    /// transport adapters (RFC-0855 §relay).
    Bandwidth,
    /// Orchestrator node — coordinates mission
    /// execution across the network (RFC-0011-d
    /// role taxonomy).
    Orchestrator,
}

/// `SpecializedNodeRecordAccess` — trait abstraction
/// over specialized node record operations per
/// RFC-0011-q Phase 9 per-extension crate pattern.
/// Trait in Layer B (octo-network); concrete per-
/// transport impl crates (substrate-ext-specialized-
/// node-*) in Layer D, OUT OF SCOPE.
pub trait SpecializedNodeRecordAccess: Send + Sync {
    /// Load a specialized node record by node_id
    /// (returns `None` if not found in local
    /// registry).
    fn load(&self, node_id: &[u8; 32]) -> Option<SpecializedNodeRecord>;

    /// Bind a node to a holder DID (reversible:
    /// returns `AlreadyBound` if previously bound
    /// to a different DID; returns `NotFound` if
    /// the node is not in the local registry).
    /// `&self` (not `&mut self`) so the trait
    /// remains object-safe behind
    /// `Arc<dyn SpecializedNodeRecordAccess>`
    /// per RFC-0011-h §User extensibility
    /// registry pattern. Concrete per-extension
    /// impl crates (Layer D) own the data and
    /// use interior mutability (`Mutex`/
    /// `RwLock`) internally.
    fn bind_to_did(
        &self,
        node_id: &[u8; 32],
        holder_did: &HolderDid,
    ) -> Result<(), SpecializedNodeError>;

    /// Return the local node_id (32-byte canonical).
    fn node_id(&self) -> [u8; 32];
}

/// `SpecializedNodeError` — error enum for the trait
/// surface. `#[non_exhaustive]` for forward-compatible
/// variant growth per RFC-0011-h §Error Handling
/// pattern (reuses slot 89 `NetworkSubstrateUnavailable`
/// at the CLI layer).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecializedNodeError {
    /// Node not found in local registry.
    NotFound,
    /// Node already bound to a different DID
    /// (re-bind returns this error; rebind via
    /// separate `unbind` flow is OUT OF SCOPE for
    /// this trait-only phase).
    AlreadyBound,
    /// Holder DID string is malformed (CLI parse
    /// layer pre-validates canonical format; this
    /// variant surfaces substrate-side validation
    /// failures from per-extension impl crates).
    InvalidDid(String),
    /// Internal error with opaque message (per-
    /// extension impl crate reports substrate-side
    /// fault; mapped to slot 89 at CLI layer).
    Internal(String),
}

impl std::fmt::Display for SpecializedNodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecializedNodeError::NotFound => write!(f, "specialized node not found"),
            SpecializedNodeError::AlreadyBound => {
                write!(f, "specialized node already bound to a different DID")
            }
            SpecializedNodeError::InvalidDid(msg) => {
                write!(f, "specialized node holder DID invalid: {msg}")
            }
            SpecializedNodeError::Internal(msg) => {
                write!(f, "specialized node internal error: {msg}")
            }
        }
    }
}

impl std::error::Error for SpecializedNodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node_id(seed: u8) -> [u8; 32] {
        let mut id = [0u8; 32];
        id[0] = seed;
        id
    }

    #[test]
    fn tv_phase9_substrate_1_holder_did_new_and_as_str() {
        let did = HolderDid::new(
            "did:octo:0xababababababababababababababababababababababababababababababababab",
        );
        assert_eq!(
            did.as_str(),
            "did:octo:0xababababababababababababababababababababababababababababababababab"
        );
        assert_eq!(did.as_ref(), did.as_str());
    }

    #[test]
    fn tv_phase9_substrate_2_specialized_node_record_field_assignment() {
        let did = HolderDid::new(
            "did:octo:0xababababababababababababababababababababababababababababababababab",
        );
        let rec = SpecializedNodeRecord {
            node_id: sample_node_id(1),
            holder_did: Some(did),
            node_class: NodeClass::Builder,
            creation_epoch: 100,
            metadata: BTreeMap::new(),
        };
        assert_eq!(rec.creation_epoch, 100);
        assert_eq!(rec.node_class, NodeClass::Builder);
        assert!(rec.holder_did.is_some());
    }

    #[test]
    fn tv_phase9_substrate_3_node_class_serde_lowercase() {
        // JSON serialize roundtrip preserves
        // #[serde(rename_all = "lowercase")] for all 5 variants
        assert_eq!(
            serde_json::to_string(&NodeClass::Builder).expect("serialize Builder"),
            "\"builder\""
        );
        assert_eq!(
            serde_json::to_string(&NodeClass::Provider).expect("serialize Provider"),
            "\"provider\""
        );
        assert_eq!(
            serde_json::to_string(&NodeClass::Storage).expect("serialize Storage"),
            "\"storage\""
        );
        assert_eq!(
            serde_json::to_string(&NodeClass::Bandwidth).expect("serialize Bandwidth"),
            "\"bandwidth\""
        );
        assert_eq!(
            serde_json::to_string(&NodeClass::Orchestrator).expect("serialize Orchestrator"),
            "\"orchestrator\""
        );
        // Deserialize roundtrip covers all 5 variants — a serde
        // rename regression on any one variant will fail this test
        let parsed_builder: NodeClass =
            serde_json::from_str("\"builder\"").expect("parse lowercase builder");
        assert_eq!(parsed_builder, NodeClass::Builder);
        let parsed_provider: NodeClass =
            serde_json::from_str("\"provider\"").expect("parse lowercase provider");
        assert_eq!(parsed_provider, NodeClass::Provider);
        let parsed_storage: NodeClass =
            serde_json::from_str("\"storage\"").expect("parse lowercase storage");
        assert_eq!(parsed_storage, NodeClass::Storage);
        let parsed_bandwidth: NodeClass =
            serde_json::from_str("\"bandwidth\"").expect("parse lowercase bandwidth");
        assert_eq!(parsed_bandwidth, NodeClass::Bandwidth);
        let parsed_orchestrator: NodeClass =
            serde_json::from_str("\"orchestrator\"").expect("parse lowercase orchestrator");
        assert_eq!(parsed_orchestrator, NodeClass::Orchestrator);
    }

    #[test]
    fn tv_phase9_substrate_4_btreemap_deterministic_ordering() {
        let mut rec = SpecializedNodeRecord {
            node_id: sample_node_id(2),
            holder_did: None,
            node_class: NodeClass::Storage,
            creation_epoch: 0,
            metadata: BTreeMap::new(),
        };
        // Insertion order is reverse alphabetical
        rec.metadata.insert("z".to_string(), "1".to_string());
        rec.metadata.insert("m".to_string(), "2".to_string());
        rec.metadata.insert("a".to_string(), "3".to_string());
        // Iteration order must be ascending (BTreeMap)
        let keys: Vec<&String> = rec.metadata.keys().collect();
        assert_eq!(keys, vec!["a", "m", "z"]);
    }

    #[test]
    fn tv_phase9_substrate_5_specialized_error_display() {
        let err_not_found = SpecializedNodeError::NotFound;
        let err_already = SpecializedNodeError::AlreadyBound;
        let err_invalid = SpecializedNodeError::InvalidDid("bad".to_string());
        let err_internal = SpecializedNodeError::Internal("boom".to_string());
        assert_eq!(err_not_found.to_string(), "specialized node not found");
        assert_eq!(
            err_already.to_string(),
            "specialized node already bound to a different DID"
        );
        assert_eq!(
            err_invalid.to_string(),
            "specialized node holder DID invalid: bad"
        );
        assert_eq!(
            err_internal.to_string(),
            "specialized node internal error: boom"
        );
    }

    #[test]
    fn tv_phase9_substrate_6_holder_did_partial_eq() {
        let a = HolderDid::new(
            "did:octo:0xcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
        );
        let b = HolderDid::new(
            "did:octo:0xcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
        );
        let c = HolderDid::new(
            "did:octo:0xefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef",
        );
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
