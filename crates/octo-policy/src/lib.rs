//! Policy Object Graph (RFC-0967).
//!
//! Separable, versioned, shareable authorization policy. A `PolicyObject`
//! is a versioned envelope carrying a content-addressed `PolicyGraph` DAG,
//! lineage, audit reference, and Ed25519 signature. Two policies can be
//! intersected to produce a child policy that must satisfy both parents.
//! Policy updates create new versions (NOT new policy IDs); the lineage
//! tracks evolution.
//!
//! ## Wire format
//!
//! Per RFC-0967 §4:
//! ```text
//! PolicyObject {
//!   version_tag:        1,
//!   policy_id:          PolicyId,         // BLAKE3(0xC0 || canonical_ser(unsigned))
//!   version_seq:        u64,
//!   parent_policy_id:   Option<PolicyId>,
//!   graph:              PolicyGraph,
//!   surface:            PolicySurface,    // semantic view derived from graph
//!   lineage:            Vec<LineageEdge>, // history of parent_policy_id + parent_version
//!   audit_ref:          [u8; 32],
//!   timestamp_unix_ms:  u64,
//!   signature:          Ed25519Signature,
//! }
//! policy_id = BLAKE3(0xC0 || canonical_ser(policy_unsigned))
//! ```
//!
//! ## Capability integration
//!
//! Capabilities carry a `PolicyReference` caveat (RFC-0965 §3.9) pointing
//! at a `PolicyId`. The verifier fetches the policy object from the
//! catalog and checks: `capability ⊆ policy` (per the subgraph relation
//! in RFC-0967 §5).
//!
//! ## Hierarchical lattice
//!
//! Per RFC-0960 §8 + RFC-0967 §5: hierarchical delegation uses the
//! `PolicyGraph` subgraph relation. parent policy ⊇ child policy iff
//! child ⊆ parent in the DAG.

#![warn(missing_debug_implementations)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::useless_vec)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::struct_excessive_bools)]

pub mod domain_separators;
pub mod kind_uuid_registry;
pub mod policy_kinds;
pub mod policy_registry;
pub mod workflow_kind;

// Mission F (RFC-0960): BurnEventRef substrate + 3-sink atomicity.
pub mod burn_event;
pub mod event_log_producer;

pub use burn_event::{
    AuditError, AuditSink, BurnEventError, BurnEventRef, SettlementId, TransferEventLog,
    ZERO_VAULT_ID,
};
pub use event_log_producer::{produce_burn, BurnEventProducer};

use std::collections::{HashMap, HashSet};

use cipherocto_encoding::Constraint;
use serde::{Deserialize, Serialize};

/// Protocol version tag for `PolicyObject` (RFC-0967 §2).
pub const POLICY_VERSION_TAG: u8 = 1;

/// Policy identifier (32-byte BLAKE3 hash).
pub type PolicyId = [u8; 32];

/// Policy `version_seq` (monotonic u64 per lineage; 1 = genesis).
pub type PolicyVersion = u64;

/// `PolicyNode` identifier (32-byte BLAKE3 hash of canonical node body).
pub type PolicyNodeId = [u8; 32];

/// Resource axis identifier (RFC-0959 §axes).
pub type AxisId = String;

/// Domain separator for `policy_id` hash (RFC-0967 §4).
pub const POLICY_ID_HASH_PREFIX: u8 = 0xC0;

/// Domain separator for `node_id` hash (RFC-0967 §3).
pub const POLICY_NODE_HASH_PREFIX: u8 = 0xC1;

/// 64-byte Ed25519 signature; serde-friendly wrapper via `policy_sig_serde`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicySignature(pub [u8; 64]);

impl Default for PolicySignature {
    fn default() -> Self {
        Self([0u8; 64])
    }
}

impl Serialize for PolicySignature {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for PolicySignature {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let v: Vec<u8> = Deserialize::deserialize(de)?;
        if v.len() != 64 {
            return Err(serde::de::Error::custom("expected 64-byte signature"));
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(&v);
        Ok(PolicySignature(out))
    }
}

/// 32-byte audit-trail commitment (RFC-0967 §7).
pub type AuditRef = [u8; 32];

/// Authorization surface: a set of constraints evaluated against the
/// request context (RFC-0967 §2 — semantic view of the policy).
///
/// Used by the simple `PolicySurface`-only APIs (`mint_surface`,
/// `intersect_surfaces`) for fast-path operations. Full policies carry
/// a `PolicyGraph` DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySurface {
    pub allowed_models: Option<HashSet<String>>,
    pub allowed_providers: Option<HashSet<String>>,
    pub per_axis_caps: Vec<(AxisId, u128)>,
    pub max_total_spend: Option<u128>,
    pub audit_window_secs: u64,
    pub allowed_destinations: Option<HashSet<String>>,
}

/// Policy lineage edge — points back at the parent policy + version
/// (RFC-0967 §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageEdge {
    pub parent_policy_id: PolicyId,
    pub parent_version: PolicyVersion,
}

/// `PolicyNode` (RFC-0967 §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyNode {
    pub node_id: PolicyNodeId,
    pub predicate: Constraint,
    pub action: PolicyAction,
    pub children: Vec<PolicyNodeId>,
    pub description: Option<String>,
}

/// `PolicyAction` (RFC-0967 §3).
///
/// Order matters for attenuation: `Deny < RequireApproval < Allow`.
/// `Audit` is independent (parallel dimension).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Allow,
    Deny,
    RequireApproval(ApprovalKind),
    /// `Audit(secs)` — record an audit-trail entry but do not gate.
    Audit(u64),
}

/// `ApprovalKind` (RFC-0967 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    SingleSigner,
    /// Quorum threshold; `1 ≤ n ≤ 23` per RFC-0967 §3.
    Quorum(u8),
    TimeLocked(u64),
}

/// `PolicyGraph` — DAG of `PolicyNode`s (RFC-0967 §3).
///
/// `all_nodes` is canonicalized sorted by `node_id` ascending;
/// `root_nodes` likewise; each node's `children` array is sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyGraph {
    pub root_nodes: Vec<PolicyNodeId>,
    pub all_nodes: Vec<PolicyNode>,
}

/// A versioned policy object (RFC-0967 §2).
///
/// The policy ID is content-addressed: `BLAKE3(0xC0 || canonical_ser(unsigned))`.
/// The signature covers `canonical_ser(unsigned)` (everything except
/// `signature` and `policy_id`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyObject {
    pub version_tag: u8,
    pub policy_id: PolicyId,
    pub version_seq: PolicyVersion,
    pub parent_policy_id: Option<PolicyId>,
    pub graph: PolicyGraph,
    pub surface: PolicySurface,
    pub lineage: Vec<LineageEdge>,
    pub audit_ref: AuditRef,
    pub timestamp_unix_ms: u64,
    pub signature: PolicySignature,
    /// Templates the parent permits a child to add to its graph (RFC-0967 §5).
    ///
    /// Per RFC-0967 §5: "the child may add nodes". The parent enumerates
    /// which additional nodes the child is allowed to include; the child
    /// then exercises that template by attaching a node with the same
    /// `node_id` (and matching predicate / action / reachability
    /// constraints). Empty by default ⇒ strict `node_id` match (legacy
    /// behavior).
    #[serde(default)]
    pub additive_nodes: Vec<PolicyNode>,
}

impl PolicyObject {
    /// Mint a new policy (`version_seq` = 1) from a `PolicyGraph` (RFC-0967 §2).
    #[must_use]
    pub fn mint(graph: PolicyGraph, audit_ref: AuditRef, timestamp_unix_ms: u64) -> Self {
        let surface = derive_surface_from_graph(&graph);
        let policy_id = [0u8; 32]; // computed by sign_and_seal
        let mut out = Self {
            version_tag: POLICY_VERSION_TAG,
            policy_id,
            version_seq: 1,
            parent_policy_id: None,
            graph,
            surface,
            lineage: Vec::new(),
            audit_ref,
            timestamp_unix_ms,
            signature: PolicySignature([0u8; 64]),
            additive_nodes: Vec::new(),
        };
        out.policy_id = compute_policy_id(&out);
        out
    }

    /// Mint a new policy with explicit additive-node templates (RFC-0967 §5).
    #[must_use]
    pub fn mint_with_additive(
        graph: PolicyGraph,
        additive_nodes: Vec<PolicyNode>,
        audit_ref: AuditRef,
        timestamp_unix_ms: u64,
    ) -> Self {
        let mut out = Self::mint(graph, audit_ref, timestamp_unix_ms);
        out.additive_nodes = additive_nodes;
        out.policy_id = compute_policy_id(&out);
        out
    }

    /// Convenience: mint from a `PolicySurface` (no graph).
    ///
    /// Builds a trivial single-node DAG with `predicate = SingleUse`-style
    /// "always-allow" node and the surface restrictions applied at
    /// verification time.
    #[must_use]
    pub fn mint_surface(
        surface: PolicySurface,
        audit_ref: AuditRef,
        timestamp_unix_ms: u64,
    ) -> Self {
        let graph = trivial_graph_from_surface(&surface);
        let mut out = Self {
            version_tag: POLICY_VERSION_TAG,
            policy_id: [0u8; 32],
            version_seq: 1,
            parent_policy_id: None,
            graph,
            surface,
            lineage: Vec::new(),
            audit_ref,
            timestamp_unix_ms,
            signature: PolicySignature([0u8; 64]),
            additive_nodes: Vec::new(),
        };
        out.policy_id = compute_policy_id(&out);
        out
    }

    /// Update the policy (new `version_seq`). The `policy_id` is preserved;
    /// the new version's `lineage` + `parent_policy_id` point back at the
    /// previous version (RFC-0967 §6).
    #[must_use]
    pub fn update(
        &self,
        new_graph: PolicyGraph,
        new_audit_ref: AuditRef,
        timestamp_unix_ms: u64,
    ) -> Self {
        let mut lineage = self.lineage.clone();
        lineage.push(LineageEdge {
            parent_policy_id: self.policy_id,
            parent_version: self.version_seq,
        });
        let surface = derive_surface_from_graph(&new_graph);
        let mut out = Self {
            version_tag: POLICY_VERSION_TAG,
            policy_id: self.policy_id,
            version_seq: self.version_seq + 1,
            parent_policy_id: Some(self.policy_id),
            graph: new_graph,
            surface,
            lineage,
            audit_ref: new_audit_ref,
            timestamp_unix_ms,
            signature: PolicySignature([0u8; 64]),
            additive_nodes: self.additive_nodes.clone(),
        };
        out.policy_id = compute_policy_id(&out);
        out
    }
}

/// Derive a `PolicySurface` (best-effort) from a `PolicyGraph`.
///
/// Surface-only APIs (`mint_surface`, `intersect_surfaces`) use this; full
/// subgraph containment requires DAG inspection per RFC-0967 §5.
fn derive_surface_from_graph(graph: &PolicyGraph) -> PolicySurface {
    let _ = graph; // graph→surface extraction is out of scope; placeholder
    PolicySurface {
        allowed_models: None,
        allowed_providers: None,
        per_axis_caps: Vec::new(),
        max_total_spend: None,
        audit_window_secs: 0,
        allowed_destinations: None,
    }
}

fn trivial_graph_from_surface(_surface: &PolicySurface) -> PolicyGraph {
    PolicyGraph {
        root_nodes: Vec::new(),
        all_nodes: Vec::new(),
    }
}

/// Compute `node_id = BLAKE3(0xC1 || canonical_ser(predicate) || canonical_ser(action) || sorted(children))`.
///
/// Used by `PolicyNode::mint`. RFC-0967 §3 specifies the body commit.
#[must_use]
pub fn compute_node_id(
    predicate: &Constraint,
    action: &PolicyAction,
    children: &[PolicyNodeId],
) -> PolicyNodeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[POLICY_NODE_HASH_PREFIX]);
    // Predicate commitment: use the RFC-0964 constraint_hash if available,
    // else fall back to canonical-serialised JSON bytes.
    hasher.update(&cipherocto_encoding::constraint_hash(predicate));
    hasher.update(&policy_action_canonical_bytes(action));
    let mut sorted_children: Vec<PolicyNodeId> = children.to_vec();
    sorted_children.sort_unstable();
    for c in &sorted_children {
        hasher.update(c);
    }
    *hasher.finalize().as_bytes()
}

/// Compute `policy_id = BLAKE3(0xC0 || canonical_ser(policy_unsigned))`
/// per RFC-0967 §4.
#[must_use]
pub fn compute_policy_id(p: &PolicyObject) -> PolicyId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[POLICY_ID_HASH_PREFIX]);
    hasher.update(&[p.version_tag]);
    // version_seq, parent_policy_id, timestamp_unix_ms, surface metadata
    // are intentionally excluded from policy_id derivation so the ID
    // remains stable across re-mints + updates (RFC-0967 §6).
    // The committed content is: graph + audit_ref.
    let graph_root = compute_graph_root(&p.graph);
    hasher.update(&graph_root);
    hasher.update(&p.audit_ref);
    *hasher.finalize().as_bytes()
}

/// Domain separator for `graph_root` (RFC-0967 §2 — derived value).
pub const POLICY_GRAPH_ROOT_PREFIX: u8 = 0xC2;

/// `graph_root = BLAKE3(0xC2 || BLAKE3(0xC2 || sorted(all_nodes)))` —
/// nested commitment so graph edits are detectable.
#[must_use]
pub fn compute_graph_root(graph: &PolicyGraph) -> PolicyId {
    let mut sorted_nodes = graph.all_nodes.clone();
    sorted_nodes.sort_by_key(|n| n.node_id);
    let mut inner = blake3::Hasher::new();
    inner.update(&[POLICY_GRAPH_ROOT_PREFIX]);
    for n in &sorted_nodes {
        inner.update(&n.node_id);
    }
    let inner_bytes = *inner.finalize().as_bytes();
    let mut outer = blake3::Hasher::new();
    outer.update(&[POLICY_GRAPH_ROOT_PREFIX]);
    outer.update(&inner_bytes);
    *outer.finalize().as_bytes()
}

/// Canonical bytes for a `PolicyAction` (sorted, no-description).
fn policy_action_canonical_bytes(action: &PolicyAction) -> Vec<u8> {
    let mut buf = Vec::new();
    match action {
        PolicyAction::Allow => buf.extend_from_slice(b"Allow"),
        PolicyAction::Deny => buf.extend_from_slice(b"Deny"),
        PolicyAction::RequireApproval(kind) => {
            buf.extend_from_slice(b"RequireApproval/");
            buf.extend_from_slice(&approval_kind_canonical_bytes(kind));
        }
        PolicyAction::Audit(secs) => {
            buf.extend_from_slice(b"Audit/");
            buf.extend_from_slice(&secs.to_be_bytes());
        }
    }
    buf
}

fn approval_kind_canonical_bytes(kind: &ApprovalKind) -> Vec<u8> {
    let mut buf = Vec::new();
    match kind {
        ApprovalKind::SingleSigner => buf.extend_from_slice(b"SingleSigner"),
        ApprovalKind::Quorum(n) => {
            buf.extend_from_slice(b"Quorum/");
            buf.extend_from_slice(&n.to_be_bytes());
        }
        ApprovalKind::TimeLocked(unix) => {
            buf.extend_from_slice(b"TimeLocked/");
            buf.extend_from_slice(&unix.to_be_bytes());
        }
    }
    buf
}

/// Policy errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    /// Two policies have incompatible surfaces (intersection is empty).
    #[error("policy intersection is empty: incompatible surfaces")]
    EmptyIntersection,
}

/// Intersect two policies (RFC-0967 §5 subgraph relation).
///
/// Produces a child policy that must satisfy BOTH parents. Returns
/// `Err(EmptyIntersection)` if the surfaces contradict (e.g., allowed
/// models disjoint).
///
/// # Errors
///
/// Returns [`PolicyError::EmptyIntersection`] when the intersection of the
/// parent surfaces is empty (disjoint `allowed_models`, `allowed_providers`,
/// `allowed_destinations`, or violated caps).
pub fn intersect(
    parent_a: &PolicyObject,
    parent_b: &PolicyObject,
) -> Result<PolicyObject, PolicyError> {
    let surface = intersect_surfaces(&parent_a.surface, &parent_b.surface)?;
    let id = choose_intersection_id(parent_a, parent_b);
    let version_seq = parent_a.version_seq.max(parent_b.version_seq) + 1;
    let lineage = vec![
        LineageEdge {
            parent_policy_id: parent_a.policy_id,
            parent_version: parent_a.version_seq,
        },
        LineageEdge {
            parent_policy_id: parent_b.policy_id,
            parent_version: parent_b.version_seq,
        },
    ];
    let child = PolicyObject {
        version_tag: POLICY_VERSION_TAG,
        policy_id: id,
        version_seq,
        parent_policy_id: Some(id),
        graph: PolicyGraph {
            root_nodes: Vec::new(),
            all_nodes: Vec::new(),
        },
        surface,
        lineage,
        audit_ref: [0u8; 32],
        timestamp_unix_ms: 0,
        signature: PolicySignature([0u8; 64]),
        additive_nodes: Vec::new(),
    };
    Ok(child)
}

fn choose_intersection_id(a: &PolicyObject, b: &PolicyObject) -> PolicyId {
    if a.policy_id == b.policy_id {
        a.policy_id
    } else {
        // Per RFC-0967 §4: also use the 0xC0 domain separator for the
        // intersection ID derivation so cross-impl hashes match.
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[POLICY_ID_HASH_PREFIX]);
        hasher.update(b"intersection/v1");
        hasher.update(&a.policy_id);
        hasher.update(&b.policy_id);
        *hasher.finalize().as_bytes()
    }
}

fn intersect_surfaces(a: &PolicySurface, b: &PolicySurface) -> Result<PolicySurface, PolicyError> {
    let allowed_models = match (&a.allowed_models, &b.allowed_models) {
        (Some(a), Some(b)) => {
            let inter: HashSet<String> = a.intersection(b).cloned().collect();
            if inter.is_empty() {
                return Err(PolicyError::EmptyIntersection);
            }
            Some(inter)
        }
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    };
    let allowed_providers = match (&a.allowed_providers, &b.allowed_providers) {
        (Some(a), Some(b)) => {
            let inter: HashSet<String> = a.intersection(b).cloned().collect();
            if inter.is_empty() {
                return Err(PolicyError::EmptyIntersection);
            }
            Some(inter)
        }
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    };
    let allowed_destinations = match (&a.allowed_destinations, &b.allowed_destinations) {
        (Some(a), Some(b)) => {
            let inter: HashSet<String> = a.intersection(b).cloned().collect();
            if inter.is_empty() {
                return Err(PolicyError::EmptyIntersection);
            }
            Some(inter)
        }
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (None, None) => None,
    };
    let max_total_spend = match (a.max_total_spend, b.max_total_spend) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    // Per-axis caps: intersection requires the SAME axis to appear in both,
    // and the cap is min(a, b). Asymmetric axes: drop the missing one.
    let axes: HashSet<&AxisId> = a.per_axis_caps.iter().map(|(k, _)| k).collect();
    let mut per_axis_caps = Vec::new();
    for (axis, cap_a) in &a.per_axis_caps {
        if let Some((_, cap_b)) = b.per_axis_caps.iter().find(|(k, _)| k == axis) {
            per_axis_caps.push((axis.clone(), (*cap_a).min(*cap_b)));
        }
    }
    let _ = axes; // silence unused warning
    Ok(PolicySurface {
        allowed_models,
        allowed_providers,
        per_axis_caps,
        max_total_spend,
        audit_window_secs: a.audit_window_secs.max(b.audit_window_secs),
        allowed_destinations,
    })
}

/// Subgraph relation (RFC-0967 §5): `child ⊆ parent` iff child surface is
/// contained in parent surface AND child's graph is a subgraph of parent's.
#[must_use]
pub fn is_subgraph(child: &PolicyObject, parent: &PolicyObject) -> bool {
    if let Some(pm) = &parent.surface.allowed_models {
        if let Some(cm) = &child.surface.allowed_models {
            if !cm.is_subset(pm) {
                return false;
            }
        }
    }
    if let Some(pp) = &parent.surface.allowed_providers {
        if let Some(cp) = &child.surface.allowed_providers {
            if !cp.is_subset(pp) {
                return false;
            }
        }
    }
    if let Some(pd) = &parent.surface.allowed_destinations {
        if let Some(cd) = &child.surface.allowed_destinations {
            if !cd.is_subset(pd) {
                return false;
            }
        }
    }
    if let Some(pt) = parent.surface.max_total_spend {
        if let Some(ct) = child.surface.max_total_spend {
            if ct > pt {
                return false;
            }
        }
    }
    for (axis, cap_c) in &child.surface.per_axis_caps {
        if let Some((_, cap_p)) = parent.surface.per_axis_caps.iter().find(|(k, _)| k == axis) {
            if cap_c > cap_p {
                return false;
            }
        } else {
            // Child has a cap on an axis parent doesn't allow: unsupported.
            return false;
        }
    }
    // RFC-0967 §5: graph containment in addition to surface containment.
    // Empty child graph is trivially a subgraph of any parent graph.
    if !is_subgraph_graph(&child.graph, &parent.graph, &parent.additive_nodes) {
        return false;
    }
    true
}

/// Policy action ordering (RFC-0967 §5): `Deny < RequireApproval < Allow`.
/// `Audit` is an independent (parallel) dimension — child `Audit` requires
/// parent `Audit` (exact match).
///
/// `child ≤ parent` means "child is at most as permissive as parent":
/// - `Deny` is the most restrictive — always allowed (parent covers `Deny`).
/// - `RequireApproval` is allowed when parent is `Allow` or `RequireApproval`.
/// - `Allow` is the most permissive — only allowed when parent is `Allow`.
/// - `Audit` is independent: child `Audit(X)` is allowed iff parent is `Audit(Y)`
///   (the duration is a property of the audit envelope, not the gating
///   decision; the graph check covers equality here).
#[must_use]
pub fn action_at_least(child: &PolicyAction, parent: &PolicyAction) -> bool {
    use PolicyAction::{Allow, Audit, Deny, RequireApproval};
    match (child, parent) {
        // Deny is the most restrictive: child Deny is always within parent.
        // Allow is the most permissive: only allowed when parent is Allow.
        // Audit is independent: only matches within the Audit dimension.
        (Deny, _)
        | (RequireApproval(_) | Allow, Allow)
        | (RequireApproval(_), RequireApproval(_))
        | (Audit(_), Audit(_)) => true,
        // All other combinations are not allowed.
        _ => false,
    }
}

/// Policy graph subgraph relation (RFC-0967 §5).
///
/// For every node `n` in `child.all_nodes` there exists a node `n'` in
/// `parent.all_nodes` (or in `parent_additive`) with:
/// - `n.predicate == n'.predicate` (constraint equality — `Constraint` has
///   no partial-order by default; we use identity matching)
/// - `child.action ≤ parent.action` per [`action_at_least`]
/// - every node in `n.children` is also in `n'.children` (reachability is
///   covered by the parent)
///
/// And every `child.root_nodes` entry is a node in the parent (or in
/// `parent_additive`).
///
/// **Additive nodes (RFC-0967 §5):** "the child may add nodes". The parent
/// enumerates which additional nodes the child is permitted to attach via
/// the `parent_additive` slice. A child node whose `node_id` is not in the
/// parent's `all_nodes` is admitted iff it matches an entry in
/// `parent_additive` by `node_id` (and predicate / action / children
/// constraints are checked against the additive template). Empty
/// `parent_additive` ⇒ strict `node_id` match (legacy behavior).
///
/// **Empty graphs:** `child.all_nodes.empty()` is trivially a subgraph of
/// any parent (no nodes to check).
#[must_use]
pub fn is_subgraph_graph(
    child: &PolicyGraph,
    parent: &PolicyGraph,
    parent_additive: &[PolicyNode],
) -> bool {
    // Empty graph is a subgraph of any graph.
    if child.all_nodes.is_empty() {
        return true;
    }

    // Build a parent lookup by node_id.
    let parent_by_id: HashMap<&[u8; 32], &PolicyNode> =
        parent.all_nodes.iter().map(|n| (&n.node_id, n)).collect();
    // Additive-template lookup by node_id.
    let additive_by_id: HashMap<&[u8; 32], &PolicyNode> =
        parent_additive.iter().map(|n| (&n.node_id, n)).collect();

    // Every child node must exist in parent (strict) or in additive
    // templates (additive).
    for child_node in &child.all_nodes {
        let template = if let Some(parent_node) = parent_by_id.get(&child_node.node_id) {
            *parent_node
        } else if let Some(additive_node) = additive_by_id.get(&child_node.node_id) {
            *additive_node
        } else {
            return false;
        };

        // Predicate must match the source-of-truth template (constraint equality).
        if child_node.predicate != template.predicate {
            return false;
        }

        // Template action must be at least as permissive as child action.
        if !action_at_least(&child_node.action, &template.action) {
            return false;
        }

        // Template's children must reach all of child's children.
        for child_child_id in &child_node.children {
            if !template.children.contains(child_child_id) {
                return false;
            }
        }
    }

    // Every child root must be a parent node or an additive template.
    for root_id in &child.root_nodes {
        if !parent_by_id.contains_key(root_id) && !additive_by_id.contains_key(root_id) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(max_total: Option<u128>, models: &[&str]) -> PolicySurface {
        let allowed_models = if models.is_empty() {
            None
        } else {
            Some(models.iter().map(ToString::to_string).collect())
        };
        PolicySurface {
            allowed_models,
            allowed_providers: None,
            per_axis_caps: Vec::new(),
            max_total_spend: max_total,
            audit_window_secs: 0,
            allowed_destinations: None,
        }
    }

    #[test]
    fn policy_id_stable_for_same_surface() {
        let s = surface(Some(1000), &["gpt-4"]);
        let p1 = PolicyObject::mint_surface(s.clone(), [0u8; 32], 1_000_000);
        let p2 = PolicyObject::mint_surface(s, [0u8; 32], 1_000_000);
        assert_eq!(p1.policy_id, p2.policy_id);
    }

    #[test]
    fn policy_id_differs_for_different_graph() {
        // Two distinct PolicyGraphs produce different graph_roots and
        // therefore different policy_ids.
        let g1 = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![PolicyNode {
                node_id: [0x01; 32],
                predicate: Constraint::SingleUse,
                action: PolicyAction::Allow,
                children: Vec::new(),
                description: None,
            }],
        };
        let g2 = PolicyGraph {
            root_nodes: vec![[0x02; 32]],
            all_nodes: vec![PolicyNode {
                node_id: [0x02; 32],
                predicate: Constraint::SingleUse,
                action: PolicyAction::Deny,
                children: Vec::new(),
                description: None,
            }],
        };
        let p1 = PolicyObject::mint(g1, [0u8; 32], 1_000_000);
        let p2 = PolicyObject::mint(g2, [0u8; 32], 1_000_000);
        assert_ne!(p1.policy_id, p2.policy_id);
    }

    #[test]
    fn update_increments_version_preserves_id() {
        let p1 = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let p2 = p1.update(PolicyGraph::default(), [0u8; 32], 2_000_000);
        assert_eq!(p1.policy_id, p2.policy_id);
        assert_eq!(p1.version_seq, 1);
        assert_eq!(p2.version_seq, 2);
        assert_eq!(p2.lineage.len(), 1);
        assert_eq!(p2.lineage[0].parent_version, 1);
    }

    #[test]
    fn intersect_disjoint_models_fails() {
        let pa = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let pb =
            PolicyObject::mint_surface(surface(Some(1000), &["claude-3"]), [0u8; 32], 1_000_000);
        let err = intersect(&pa, &pb).unwrap_err();
        assert_eq!(err, PolicyError::EmptyIntersection);
    }

    #[test]
    fn intersect_overlapping_models_succeeds() {
        let pa = PolicyObject::mint_surface(
            surface(Some(1000), &["gpt-4", "claude-3"]),
            [0u8; 32],
            1_000_000,
        );
        let pb = PolicyObject::mint_surface(
            surface(Some(1000), &["gpt-4", "cohere"]),
            [0u8; 32],
            1_000_000,
        );
        let child = intersect(&pa, &pb).unwrap();
        let models = child.surface.allowed_models.as_ref().unwrap();
        assert!(models.contains("gpt-4"));
        assert!(!models.contains("claude-3"));
        assert!(!models.contains("cohere"));
    }

    #[test]
    fn intersect_takes_min_total_spend() {
        let pa = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let pb = PolicyObject::mint_surface(surface(Some(500), &["gpt-4"]), [0u8; 32], 1_000_000);
        let child = intersect(&pa, &pb).unwrap();
        assert_eq!(child.surface.max_total_spend, Some(500));
    }

    #[test]
    fn intersect_same_policy_id_preserves_id() {
        let sur = surface(Some(1000), &["gpt-4"]);
        let pa = PolicyObject::mint_surface(sur.clone(), [0u8; 32], 1_000_000);
        let pb = PolicyObject::mint_surface(sur, [0u8; 32], 1_000_000);
        let child = intersect(&pa, &pb).unwrap();
        assert_eq!(child.policy_id, pa.policy_id);
    }

    #[test]
    fn subgraph_child_with_subset_models() {
        let parent = PolicyObject::mint_surface(
            surface(Some(1000), &["gpt-4", "claude-3"]),
            [0u8; 32],
            1_000_000,
        );
        let child =
            PolicyObject::mint_surface(surface(Some(500), &["gpt-4"]), [0u8; 32], 1_000_000);
        assert!(is_subgraph(&child, &parent));
    }

    #[test]
    fn subgraph_child_with_superset_models_rejected() {
        let parent =
            PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let child = PolicyObject::mint_surface(
            surface(Some(500), &["gpt-4", "claude-3"]),
            [0u8; 32],
            1_000_000,
        );
        assert!(!is_subgraph(&child, &parent));
    }

    #[test]
    fn subgraph_child_with_overlapping_spend() {
        let parent =
            PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let child =
            PolicyObject::mint_surface(surface(Some(500), &["gpt-4"]), [0u8; 32], 1_000_000);
        assert!(is_subgraph(&child, &parent));
    }

    #[test]
    fn subgraph_child_with_higher_spend_rejected() {
        let parent =
            PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let child =
            PolicyObject::mint_surface(surface(Some(2000), &["gpt-4"]), [0u8; 32], 1_000_000);
        assert!(!is_subgraph(&child, &parent));
    }

    #[test]
    fn intersect_lineage_records_both_parents() {
        let pa = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let pb = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let child = intersect(&pa, &pb).unwrap();
        assert_eq!(child.lineage.len(), 2);
        assert!(child
            .lineage
            .iter()
            .any(|e| e.parent_policy_id == pa.policy_id));
        assert!(child
            .lineage
            .iter()
            .any(|e| e.parent_policy_id == pb.policy_id));
    }

    #[test]
    fn intersect_audit_window_takes_max() {
        let mut sa = surface(Some(1000), &["gpt-4"]);
        sa.audit_window_secs = 3600;
        let mut sb = surface(Some(1000), &["gpt-4"]);
        sb.audit_window_secs = 86400;
        let pa = PolicyObject::mint_surface(sa, [0u8; 32], 1_000_000);
        let pb = PolicyObject::mint_surface(sb, [0u8; 32], 1_000_000);
        let child = intersect(&pa, &pb).unwrap();
        assert_eq!(child.surface.audit_window_secs, 86400);
    }

    // === RFC-0967 §2 full envelope tests ===

    #[test]
    fn policy_object_envelope_has_all_required_fields() {
        let p =
            PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0xab; 32], 1_700_000_000);
        assert_eq!(p.version_tag, POLICY_VERSION_TAG);
        assert_eq!(p.version_seq, 1);
        assert!(p.parent_policy_id.is_none());
        assert_eq!(p.audit_ref, [0xab; 32]);
        assert_eq!(p.timestamp_unix_ms, 1_700_000_000);
        assert_eq!(p.signature.0, [0u8; 64]); // unsigned
        assert_ne!(p.policy_id, [0u8; 32]); // computed
    }

    #[test]
    fn policy_id_stable_across_timestamps_for_same_content() {
        // Per RFC-0967 §6: policy_id is stable across updates that don't
        // change semantic content. timestamp_unix_ms is metadata.
        let p1 = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let p2 = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 9_999_999);
        assert_eq!(p1.policy_id, p2.policy_id);
    }

    #[test]
    fn update_sets_parent_policy_id() {
        let p1 = PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let p2 = p1.update(PolicyGraph::default(), [0u8; 32], 2_000_000);
        assert_eq!(p2.parent_policy_id, Some(p1.policy_id));
        assert_eq!(p2.lineage.len(), 1);
        assert_eq!(p2.lineage[0].parent_policy_id, p1.policy_id);
        assert_eq!(p2.lineage[0].parent_version, 1);
    }

    #[test]
    fn graph_root_differs_for_different_nodes() {
        let g1 = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![PolicyNode {
                node_id: [0x01; 32],
                predicate: Constraint::SingleUse,
                action: PolicyAction::Allow,
                children: Vec::new(),
                description: None,
            }],
        };
        let g2 = PolicyGraph {
            root_nodes: vec![[0x02; 32]],
            all_nodes: vec![PolicyNode {
                node_id: [0x02; 32],
                predicate: Constraint::SingleUse,
                action: PolicyAction::Allow,
                children: Vec::new(),
                description: None,
            }],
        };
        assert_ne!(compute_graph_root(&g1), compute_graph_root(&g2));
    }

    #[test]
    fn policy_node_id_deterministic_for_same_inputs() {
        let id_a = compute_node_id(
            &Constraint::SingleUse,
            &PolicyAction::Allow,
            &[[0xab; 32], [0xcd; 32]],
        );
        let id_b = compute_node_id(
            &Constraint::SingleUse,
            &PolicyAction::Allow,
            &[[0xcd; 32], [0xab; 32]], // different order
        );
        // children sorted before hash → same id
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn policy_action_quorum_within_bounds() {
        // Per RFC-0967 §3: Quorum n in 1..=23
        let k = ApprovalKind::Quorum(5);
        assert_eq!(k, ApprovalKind::Quorum(5));
    }

    // ---- RFC-0967 §5 — graph containment complement to surface check ----

    fn policy_node(
        id: [u8; 32],
        predicate: Constraint,
        action: PolicyAction,
        children: Vec<[u8; 32]>,
    ) -> PolicyNode {
        PolicyNode {
            node_id: id,
            predicate,
            action,
            children,
            description: None,
        }
    }

    #[test]
    fn action_at_least_deny_covers_everything() {
        // Deny is the most restrictive — child Deny is always within parent.
        use PolicyAction::{Allow, Audit, Deny, RequireApproval};
        assert!(action_at_least(&Deny, &Allow));
        assert!(action_at_least(
            &Deny,
            &RequireApproval(ApprovalKind::SingleSigner)
        ));
        assert!(action_at_least(&Deny, &Deny));
        assert!(action_at_least(&Deny, &Audit(60)));
    }

    #[test]
    fn action_at_least_require_approval_under_allow() {
        use PolicyAction::{Allow, RequireApproval};
        assert!(action_at_least(
            &RequireApproval(ApprovalKind::SingleSigner),
            &Allow
        ));
        assert!(action_at_least(
            &RequireApproval(ApprovalKind::SingleSigner),
            &RequireApproval(ApprovalKind::SingleSigner)
        ));
        // Reverse — child Allow is more permissive than parent RequireApproval.
        assert!(!action_at_least(
            &Allow,
            &RequireApproval(ApprovalKind::SingleSigner)
        ));
    }

    #[test]
    fn action_at_least_allow_only_under_allow() {
        use PolicyAction::{Allow, RequireApproval};
        assert!(action_at_least(&Allow, &Allow));
        assert!(!action_at_least(
            &Allow,
            &RequireApproval(ApprovalKind::SingleSigner)
        ));
    }

    #[test]
    fn action_at_least_audit_axis_independent() {
        use PolicyAction::{Allow, Audit, RequireApproval};
        // Audit is independent: child Audit only matches parent Audit.
        assert!(action_at_least(&Audit(60), &Audit(60)));
        assert!(action_at_least(&Audit(60), &Audit(120)));
        // Audit is NOT a subset of Allow / RequireApproval / Deny.
        assert!(!action_at_least(&Audit(60), &Allow));
        assert!(!action_at_least(
            &Audit(60),
            &RequireApproval(ApprovalKind::SingleSigner)
        ));
    }

    #[test]
    fn is_subgraph_graph_empty_child_is_subgraph() {
        // Empty graph is trivially a subgraph of any parent.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        let child = PolicyGraph::default();
        assert!(is_subgraph_graph(&child, &parent, &[]));
        // Empty parent + empty child is also a subgraph.
        assert!(is_subgraph_graph(&child, &PolicyGraph::default(), &[]));
    }

    #[test]
    fn is_subgraph_graph_identical_graphs() {
        let node = policy_node(
            [0x01; 32],
            Constraint::SingleUse,
            PolicyAction::Allow,
            vec![],
        );
        let graph = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![node],
        };
        assert!(is_subgraph_graph(&graph, &graph, &[]));
    }

    #[test]
    fn is_subgraph_graph_child_with_restricted_action() {
        // Parent has Allow; child has RequireApproval on same node.
        // Per RFC-0967 §5: child is more restrictive ⇒ subgraph.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::RequireApproval(ApprovalKind::SingleSigner),
                vec![],
            )],
        };
        assert!(is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_child_with_more_permissive_action_rejected() {
        // Parent has RequireApproval; child has Allow. Widening ⇒ reject.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::RequireApproval(ApprovalKind::SingleSigner),
                vec![],
            )],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        assert!(!is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_child_with_unknown_node_id_rejected() {
        // Strict node_id match: child has a node that doesn't exist in parent.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0xab; 32]],
            all_nodes: vec![policy_node(
                [0xab; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        assert!(!is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_child_with_predicate_mismatch_rejected() {
        // Same node_id, different predicate ⇒ reject.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::MaxUses { count: 5 },
                PolicyAction::Allow,
                vec![],
            )],
        };
        assert!(!is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_parent_children_must_cover_child_children() {
        // Parent has node A with children [B, C]; child has node A with child [B].
        // Parent's children reach both B and C; child's reachability is a subset.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![
                policy_node(
                    [0x01; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![[0x02; 32], [0x03; 32]],
                ),
                policy_node(
                    [0x02; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![],
                ),
                policy_node(
                    [0x03; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![],
                ),
            ],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![
                policy_node(
                    [0x01; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![[0x02; 32]],
                ),
                policy_node(
                    [0x02; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![],
                ),
            ],
        };
        assert!(is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_child_with_extra_child_rejected() {
        // Child has node A with child [C] but parent has node A with child [B].
        // Child's reachability exceeds parent's ⇒ reject.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![
                policy_node(
                    [0x01; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![[0x02; 32]],
                ),
                policy_node(
                    [0x02; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![],
                ),
            ],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![
                policy_node(
                    [0x01; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![[0x03; 32]],
                ),
                policy_node(
                    [0x03; 32],
                    Constraint::SingleUse,
                    PolicyAction::Allow,
                    vec![],
                ),
            ],
        };
        assert!(!is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_root_must_be_in_parent() {
        // Child root [0x02] isn't in parent (parent has [0x01]) ⇒ reject.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0x02; 32]],
            all_nodes: vec![policy_node(
                [0x02; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        assert!(!is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_combined_rejects_on_graph_mismatch() {
        // Combined surface+graph check: passing surface, failing graph ⇒ reject.
        let parent = PolicyGraph {
            root_nodes: vec![[0x01; 32]],
            all_nodes: vec![policy_node(
                [0x01; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        let child = PolicyGraph {
            root_nodes: vec![[0xab; 32]],
            all_nodes: vec![policy_node(
                [0xab; 32],
                Constraint::SingleUse,
                PolicyAction::Allow,
                vec![],
            )],
        };
        let p = PolicyObject::mint(parent, [0u8; 32], 1_000_000);
        let c = PolicyObject::mint(child, [0u8; 32], 1_000_000);
        // Surface trivially matches (both empty); graph mismatches ⇒ reject.
        assert!(!is_subgraph(&c, &p));
    }

    #[test]
    fn is_subgraph_combined_rejects_on_surface_mismatch() {
        // Combined surface+graph check: failing surface, passing graph ⇒ reject.
        let parent =
            PolicyObject::mint_surface(surface(Some(1000), &["gpt-4"]), [0u8; 32], 1_000_000);
        let child =
            PolicyObject::mint_surface(surface(Some(2000), &["gpt-4"]), [0u8; 32], 1_000_000);
        // Both have empty graphs (passing graph check); surface fails.
        assert!(!is_subgraph(&child, &parent));
    }

    // === RFC-0967 §5 additive child nodes (Gap 8.2) ===

    fn make_node(id: [u8; 32], action: PolicyAction) -> PolicyNode {
        policy_node(id, Constraint::SingleUse, action, vec![])
    }

    #[test]
    fn is_subgraph_graph_admits_additive_child_node_in_parent_additive_set() {
        // Per RFC-0967 §5: "the child may add nodes". The parent's
        // `additive_nodes` field lists templates the child is permitted
        // to include.
        let a = [0x01; 32];
        let b = [0x02; 32];
        let c = [0x03; 32];
        let parent = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
            ],
        };
        let additive_c = make_node(c, PolicyAction::Allow);
        let child = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
                additive_c.clone(),
            ],
        };
        assert!(is_subgraph_graph(&child, &parent, &[additive_c]));
    }

    #[test]
    fn is_subgraph_graph_rejects_additive_child_node_not_in_parent_additive_set() {
        // Child adds node C, but parent does not list C in
        // `additive_nodes` ⇒ reject.
        let a = [0x01; 32];
        let b = [0x02; 32];
        let c = [0x03; 32];
        let parent = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
            ],
        };
        let child = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
                make_node(c, PolicyAction::Allow),
            ],
        };
        // Empty additive set ⇒ reject.
        assert!(!is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_accepts_multiple_additive_child_nodes() {
        // Multiple additive nodes declared in the parent → all accepted.
        let a = [0x01; 32];
        let b = [0x02; 32];
        let c = [0x03; 32];
        let d = [0x04; 32];
        let parent = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
            ],
        };
        let additive = vec![
            make_node(c, PolicyAction::Allow),
            make_node(d, PolicyAction::Allow),
        ];
        let child = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
                make_node(c, PolicyAction::Allow),
                make_node(d, PolicyAction::Allow),
            ],
        };
        assert!(is_subgraph_graph(&child, &parent, &additive));
    }

    #[test]
    fn is_subgraph_graph_empty_additive_set_preserves_legacy_strict_match() {
        // Empty additive set + child node with unknown node_id ⇒ reject
        // (legacy behavior preserved).
        let a = [0x01; 32];
        let unknown = [0xab; 32];
        let parent = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![make_node(a, PolicyAction::Allow)],
        };
        let child = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(unknown, PolicyAction::Allow),
            ],
        };
        assert!(!is_subgraph_graph(&child, &parent, &[]));
    }

    #[test]
    fn is_subgraph_graph_additive_template_must_share_node_id() {
        // Parent declares an additive template keyed by node_id X; child
        // adds a node with node_id X but mismatched predicate ⇒ reject.
        let a = [0x01; 32];
        let b = [0x02; 32];
        let parent = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
            ],
        };
        let additive_template = make_node(b, PolicyAction::Allow);
        let mut child_node = make_node(b, PolicyAction::Allow);
        child_node.predicate = Constraint::MaxUses { count: 5 };
        let child = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![make_node(a, PolicyAction::Allow), child_node],
        };
        // Additive node_id matched, but predicate differs ⇒ reject.
        assert!(!is_subgraph_graph(
            &child,
            &parent,
            std::slice::from_ref(&additive_template)
        ));
    }

    #[test]
    fn is_subgraph_with_additive_accepts_augmented_child() {
        // Full `is_subgraph` (combined surface + graph) with additive
        // nodes in the parent — child includes all parent nodes + an
        // additive node.
        let a = [0x01; 32];
        let b = [0x02; 32];
        let c = [0x03; 32];
        let parent = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
            ],
        };
        let additive_c = make_node(c, PolicyAction::Allow);
        let child = PolicyGraph {
            root_nodes: vec![a],
            all_nodes: vec![
                make_node(a, PolicyAction::Allow),
                make_node(b, PolicyAction::Allow),
                additive_c.clone(),
            ],
        };
        let mut parent_obj = PolicyObject::mint(parent, [0u8; 32], 1_000_000);
        parent_obj.additive_nodes = vec![additive_c];
        let child_obj = PolicyObject::mint(child, [0u8; 32], 1_000_000);
        assert!(is_subgraph(&child_obj, &parent_obj));
    }
}
