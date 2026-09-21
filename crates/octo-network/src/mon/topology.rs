//! Mission Topology (RFC-0855 §5)

use serde::{Deserialize, Serialize};

use crate::mon::governance::GovernanceModel;
use crate::mon::lifecycle::MissionState;
use crate::mon::mission_id::{MissionId, MissionType};

/// Topology models (RFC-0855 §5.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum TopologyModel {
    Mesh = 0x0001,
    Hierarchical = 0x0002,
    Star = 0x0003,
    Swarm = 0x0004,
    Ring = 0x0005,
    Hybrid = 0x0006,
}

/// Minimum participants per topology model.
pub fn min_participants_for_topology(model: TopologyModel) -> u32 {
    match model {
        TopologyModel::Mesh => 2,
        TopologyModel::Hierarchical => 3,
        TopologyModel::Star => 2,
        TopologyModel::Swarm => 5,
        TopologyModel::Ring => 3,
        TopologyModel::Hybrid => 2,
    }
}

/// Topology commitment for deterministic replay (RFC-0855 §5.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct TopologyCommitment {
    pub mission_id: MissionId,
    pub model: TopologyModel,
    pub participant_root: [u8; 32],
    pub route_root: [u8; 32],
    pub epoch: u64,
    pub commitment: [u8; 32],
}

impl TopologyCommitment {
    /// Compute commitment = BLAKE3-256(participant_root || route_root || epoch)
    pub fn compute(
        mission_id: MissionId,
        model: TopologyModel,
        participant_root: [u8; 32],
        route_root: [u8; 32],
        epoch: u64,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&participant_root);
        hasher.update(&route_root);
        hasher.update(&epoch.to_be_bytes());
        let commitment = *hasher.finalize().as_bytes();
        Self {
            mission_id,
            model,
            participant_root,
            route_root,
            epoch,
            commitment,
        }
    }

    /// Render the topology commitment as ASCII or DOT
    /// graph output (Phase 11 G14 per RFC-0011-s
    /// §Substrate Mapping Table). Operates on the
    /// in-memory snapshot of the commitment; live
    /// topology-source adapter OUT OF SCOPE for Phase 11.
    /// Per-extension impl crates (Layer D) provide real
    /// topology-source adapters in follow-on missions.
    /// Deterministic across calls (no HashMap iteration,
    /// no randomness — `render_ascii` and `render_dot`
    /// iterate zero collections of any kind). REUSES
    /// `GraphFormat` enum from `mon::trust_graph`
    /// (Phase 1 0851p-a-trust-ux import); zero NEW
    /// types.
    pub fn render(&self, format: crate::mon::trust_graph::GraphFormat) -> String {
        match format {
            crate::mon::trust_graph::GraphFormat::Ascii => self.render_ascii(),
            crate::mon::trust_graph::GraphFormat::Dot => self.render_dot(),
        }
    }

    /// Private ASCII rendering helper for
    /// `TopologyCommitment::render`. Produces a single
    /// label line per RFC-0855 §5.1 topology-model
    /// field. Deterministic across calls (no HashMap
    /// iteration, no randomness).
    fn render_ascii(&self) -> String {
        let label = format!(
            "topology mission_id={} model={:?} epoch={}",
            hex::encode(self.mission_id.to_canonical_bytes()),
            self.model,
            self.epoch,
        );
        let mut out = String::new();
        out.push_str(&label);
        out.push('\n');
        out
    }

    /// Private DOT rendering helper for
    /// `TopologyCommitment::render`. Produces a
    /// `digraph G { ... }` block with deterministic
    /// `mission_id` key order. The model label is DOT-
    /// escaped via `escape_dot` (mirrors the Phase 1
    /// `TrustGraph::render_dot` convention at
    /// `mon::trust_graph::escape_dot`). Pipe output
    /// to `dot -Tpng` or `dot -Tsvg` for visualization.
    fn render_dot(&self) -> String {
        let mut out = String::new();
        out.push_str("digraph G {\n");
        out.push_str(&format!(
            "  mission_{} [label=\"{}\"];\n",
            hex::encode(self.mission_id.to_canonical_bytes()),
            escape_dot(&format!("{:?}", self.model)),
        ));
        out.push_str("}\n");
        out
    }
}

/// DOT label escaper for `render_dot` — mirrors the
/// Phase 1 `mon::trust_graph::escape_dot` precedent
/// (RFC-0855 §Substrate-faithfulness escaping
/// convention). Replaces `\` and `"` to prevent
/// malformed DOT output for any future
/// `TopologyModel` variant with String fields.
fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Mission descriptor flags (RFC-0855 §2.2)
pub const MISSION_FLAG_STEALTH: u64 = 0x0001;
pub const MISSION_FLAG_AUTO_RECOVER: u64 = 0x0002;
pub const MISSION_FLAG_PROOF_REQUIRED: u64 = 0x0004;
pub const MISSION_FLAG_EPHEMERAL: u64 = 0x0008;

/// Mission descriptor (RFC-0855 §2.2)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionDescriptor {
    pub mission_id: MissionId,
    pub descriptor_version: u64,
    pub mission_type: MissionType,
    pub creation_epoch: u64,
    pub governance_model: GovernanceModel,
    pub cryptographic_suite: u16,
    pub mission_root: [u8; 32],
    pub max_participants: u32,
    pub min_participants: u32,
    pub ttl_epochs: u64,
    pub flags: u64,
}

/// Mission state root (RFC-0855 §12.1)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct MissionStateRoot {
    pub mission_id: MissionId,
    pub state: MissionState,
    pub epoch: u64,
    pub state_root: [u8; 32],
    pub participant_root: [u8; 32],
    pub execution_root: [u8; 32],
    pub gossip_root: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mon::mission_id::MissionId;

    #[test]
    fn test_topology_model_repr() {
        assert_eq!(TopologyModel::Mesh as u16, 0x0001);
        assert_eq!(TopologyModel::Hybrid as u16, 0x0006);
    }

    #[test]
    fn test_min_participants() {
        assert_eq!(min_participants_for_topology(TopologyModel::Mesh), 2);
        assert_eq!(
            min_participants_for_topology(TopologyModel::Hierarchical),
            3
        );
        assert_eq!(min_participants_for_topology(TopologyModel::Swarm), 5);
    }

    #[test]
    fn test_topology_commitment_deterministic() {
        let peer = [1u8; 32];
        let nonce = [2u8; 32];
        let mid = MissionId::new(1, &peer, 100, &nonce, 1);
        let tc1 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xAA; 32], [0xBB; 32], 50);
        let tc2 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xAA; 32], [0xBB; 32], 50);
        assert_eq!(tc1.commitment, tc2.commitment);
    }

    #[test]
    fn test_topology_commitment_different_roots() {
        let peer = [1u8; 32];
        let nonce = [2u8; 32];
        let mid = MissionId::new(1, &peer, 100, &nonce, 1);
        let tc1 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xAA; 32], [0xBB; 32], 50);
        let tc2 = TopologyCommitment::compute(mid, TopologyModel::Mesh, [0xCC; 32], [0xBB; 32], 50);
        assert_ne!(tc1.commitment, tc2.commitment);
    }

    #[test]
    fn test_mission_flags() {
        assert_eq!(MISSION_FLAG_STEALTH, 0x0001);
        assert_eq!(MISSION_FLAG_EPHEMERAL, 0x0008);
    }

    // === Phase 11 G14 substrate tests (RFC-0011-s) ===

    fn phase11_mid() -> MissionId {
        let peer = [0u8; 32];
        let nonce = [0u8; 32];
        MissionId::new(1, &peer, 42, &nonce, 1)
    }

    fn phase11_commitment() -> TopologyCommitment {
        TopologyCommitment::compute(
            phase11_mid(),
            TopologyModel::Mesh,
            [0x11; 32],
            [0x22; 32],
            1234,
        )
    }

    #[test]
    fn tv_phase11_substrate_1_render_ascii_contains_label() {
        let tc = phase11_commitment();
        let out = tc.render(crate::mon::trust_graph::GraphFormat::Ascii);
        assert!(
            out.starts_with("topology mission_id="),
            "ascii render must start with `topology mission_id=`, got {out:?}"
        );
        assert!(out.contains("model=Mesh"));
        assert!(out.contains("epoch=1234"));
    }

    #[test]
    fn tv_phase11_substrate_2_render_dot_is_digraph() {
        let tc = phase11_commitment();
        let out = tc.render(crate::mon::trust_graph::GraphFormat::Dot);
        assert!(out.starts_with("digraph G {"));
        assert!(out.trim_end().ends_with('}'));
    }

    #[test]
    fn tv_phase11_substrate_3_render_deterministic_across_calls() {
        let tc1 = phase11_commitment();
        let tc2 = phase11_commitment();
        let ascii1 = tc1.render(crate::mon::trust_graph::GraphFormat::Ascii);
        let ascii2 = tc2.render(crate::mon::trust_graph::GraphFormat::Ascii);
        let dot1 = tc1.render(crate::mon::trust_graph::GraphFormat::Dot);
        let dot2 = tc2.render(crate::mon::trust_graph::GraphFormat::Dot);
        assert_eq!(ascii1, ascii2);
        assert_eq!(dot1, dot2);
    }

    #[test]
    fn tv_phase11_substrate_4_render_distinct_per_topology_model() {
        let peer = [0u8; 32];
        let nonce = [0u8; 32];
        let mid = MissionId::new(1, &peer, 42, &nonce, 1);
        let tc_mesh =
            TopologyCommitment::compute(mid, TopologyModel::Mesh, [0x11; 32], [0x22; 32], 1234);
        let tc_swarm =
            TopologyCommitment::compute(mid, TopologyModel::Swarm, [0x11; 32], [0x22; 32], 1234);
        let a_mesh = tc_mesh.render(crate::mon::trust_graph::GraphFormat::Ascii);
        let a_swarm = tc_swarm.render(crate::mon::trust_graph::GraphFormat::Ascii);
        assert!(
            a_mesh.contains("model=Mesh"),
            "mesh render must label model=Mesh"
        );
        assert!(
            a_swarm.contains("model=Swarm"),
            "swarm render must label model=Swarm"
        );
    }
}
