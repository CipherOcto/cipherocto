//! BIND envelope types (RFC-0850p-c + mission 0850p-c-partial-bindings).
//!
//! The BIND envelope is the binding record between a `domain_id`
//! and a physical group on a specific platform. It is signed by
//! the DomainCoordinator's key.
//!
//! ## Mission 0850p-c-partial-bindings
//!
//! `participant_filter: Option<Vec<PeerId>>` allows a BIND to
//! restrict DOT participation to a subset of the physical group.
//! Useful for large public groups (e.g., 1000-member WhatsApp
//! communities) where only a handful of members are DOT
//! participants.
//!
//! ## Mission 0850p-c-cross-node-rebind
//!
//! REBIND envelopes (`RebindPrepare`, `RebindCommit`, `RebindAbort`)
//! coordinate multi-platform re-bindings via a 2-phase commit.
//! See `rebind.rs` for the 2PC coordinator.

use serde::{Deserialize, Serialize};

/// The BIND envelope binding a `domain_id` to a physical group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindEnvelope {
    /// The mission domain id (RFC-0850p-c §2).
    pub domain_id: String,
    /// The platform identifier (e.g., "whatsapp", "matrix", "telegram").
    pub platform: String,
    /// The physical group identifier (e.g., WhatsApp group JID,
    /// Matrix room ID, Telegram supergroup ID).
    pub group_id: String,
    /// The DomainCoordinator's signature over the canonical
    /// serialized envelope.
    pub signature: Vec<u8>,
    /// Mission 0850p-c-partial-bindings: optional subset of
    /// physical-group members that are DOT participants.
    /// `None` means all physical-group members participate
    /// (backward-compatible default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_filter: Option<Vec<String>>,
    /// Mission 0855p-c-slash-small-groups: group size at binding
    /// time. Used to decide slash vs UNBIND on member misbehavior.
    /// Groups with < 4 members use slash (preserves small groups).
    #[serde(default)]
    pub member_count_at_bind: u16,
}

impl BindEnvelope {
    /// Create a new BIND envelope (signature is empty until signed).
    pub fn new(
        domain_id: impl Into<String>,
        platform: impl Into<String>,
        group_id: impl Into<String>,
    ) -> Self {
        Self {
            domain_id: domain_id.into(),
            platform: platform.into(),
            group_id: group_id.into(),
            signature: Vec::new(),
            participant_filter: None,
            member_count_at_bind: 0,
        }
    }

    /// Compute the canonical bytes that the signature covers.
    ///
    /// The signature covers `(domain_id, platform, group_id,
    /// participant_filter, member_count_at_bind)`. Any change to
    /// the filter or the group size invalidates the signature —
    /// this is the security property documented in missions
    /// 0850p-c-partial-bindings and 0855p-c-slash-small-groups.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        // Use a length-prefixed encoding to avoid ambiguity between
        // domain_id="foo" + group_id="bar" vs domain_id="foob" +
        // group_id="ar".
        let mut out = Vec::new();
        out.extend_from_slice(&(self.domain_id.len() as u32).to_le_bytes());
        out.extend_from_slice(self.domain_id.as_bytes());
        out.extend_from_slice(&(self.platform.len() as u32).to_le_bytes());
        out.extend_from_slice(self.platform.as_bytes());
        out.extend_from_slice(&(self.group_id.len() as u32).to_le_bytes());
        out.extend_from_slice(self.group_id.as_bytes());
        match &self.participant_filter {
            None => out.push(0),
            Some(peers) => {
                out.push(1);
                out.extend_from_slice(&(peers.len() as u32).to_le_bytes());
                for peer in peers {
                    out.extend_from_slice(&(peer.len() as u32).to_le_bytes());
                    out.extend_from_slice(peer.as_bytes());
                }
            }
        }
        // member_count_at_bind is signed as part of the envelope
        // (mission 0855p-c-slash-small-groups) so a DC cannot
        // misreport the group size to manipulate the slash vs
        // UNBIND decision.
        out.extend_from_slice(&self.member_count_at_bind.to_le_bytes());
        out
    }

    /// Returns true if `peer_id` is a DOT participant per this
    /// envelope's `participant_filter`. When the filter is
    /// `None`, every peer in the physical group is a participant.
    pub fn is_participant(&self, peer_id: &str) -> bool {
        match &self.participant_filter {
            None => true,
            Some(peers) => peers.iter().any(|p| p == peer_id),
        }
    }
}

// ── Mission 0850p-c-cross-node-rebind envelopes ──────────────────

/// A REBIND operation has three envelope types forming a 2PC:
/// PREPARE → COMMIT/ABORT.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RebindEnvelope {
    /// Phase 1: the initiator asks all N-1 other platforms to
    /// prepare the new binding. Each prepares locally (allocates
    /// resources, validates the new group) but does not commit.
    #[serde(rename = "rebind_prepare")]
    Prepare(RebindPrepare),
    /// Phase 2 (success): all N-1 voted PREPARED, the initiator
    /// broadcasts COMMIT and all parties switch atomically.
    #[serde(rename = "rebind_commit")]
    Commit(RebindCommit),
    /// Phase 2 (failure): at least one platform voted ABORT or
    /// the timeout elapsed, the initiator broadcasts ABORT and all
    /// parties roll back.
    #[serde(rename = "rebind_abort")]
    Abort(RebindAbort),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebindPrepare {
    pub domain_id: String,
    pub new_bind: BindEnvelope,
    /// The 30s deadline (epoch seconds) after which the initiator
    /// will treat any non-vote as a default ABORT.
    pub deadline_epoch: u64,
    /// Signature of the DomainCoordinator initiating the REBIND.
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebindCommit {
    pub domain_id: String,
    pub new_bind: BindEnvelope,
    /// Hash of the PREPARED responses (sha256 of sorted
    /// `{platform, signature}` pairs). Verifies that all
    /// platforms actually agreed.
    pub prepared_evidence: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebindAbort {
    pub domain_id: String,
    pub reason: RebindAbortReason,
    /// The platforms that voted ABORT (or timed out). Sorted.
    pub dissenters: Vec<String>,
    pub signature: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebindAbortReason {
    /// At least one platform voted ABORT.
    VoteAbort,
    /// The 30s timeout elapsed with a non-vote.
    Timeout,
    /// Lost the lex `domain_id` tie-break to a concurrent REBIND.
    LostTieBreak,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_includes_filter() {
        // The canonical encoding must include the filter, so a
        // signature is bound to it.
        let mut env = BindEnvelope::new("d1", "whatsapp", "group1");
        let base_canon = env.canonical_bytes();
        env.participant_filter = Some(vec!["peer-a".into(), "peer-b".into()]);
        let filtered_canon = env.canonical_bytes();
        assert_ne!(base_canon, filtered_canon);
    }

    #[test]
    fn canonical_bytes_includes_member_count() {
        // The canonical encoding must include member_count_at_bind,
        // so a DC cannot lie about group size to manipulate the
        // slash vs UNBIND decision (mission 0855p-c-slash-small-groups).
        let mut env = BindEnvelope::new("d1", "whatsapp", "group1");
        let canon_3 = {
            env.member_count_at_bind = 3;
            env.canonical_bytes()
        };
        let canon_5 = {
            env.member_count_at_bind = 5;
            env.canonical_bytes()
        };
        assert_ne!(canon_3, canon_5);
    }

    #[test]
    fn is_participant_default_includes_all() {
        let env = BindEnvelope::new("d1", "whatsapp", "g");
        assert!(env.is_participant("any-peer"));
        assert!(env.is_participant("another"));
    }

    #[test]
    fn is_participant_with_filter() {
        let mut env = BindEnvelope::new("d1", "whatsapp", "g");
        env.participant_filter = Some(vec!["a".into(), "b".into()]);
        assert!(env.is_participant("a"));
        assert!(env.is_participant("b"));
        assert!(!env.is_participant("c"));
        assert!(!env.is_participant("anyone-else"));
    }

    #[test]
    fn participant_filter_roundtrip_serde() {
        let mut env = BindEnvelope::new("d1", "whatsapp", "g");
        env.participant_filter = Some(vec!["a".into(), "b".into()]);
        let json = serde_json::to_string(&env).unwrap();
        let back: BindEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
    }

    #[test]
    fn bind_envelope_backward_compat_no_filter() {
        // Envelopes without a filter must deserialize correctly.
        let env = BindEnvelope::new("d1", "whatsapp", "g");
        let json = serde_json::to_string(&env).unwrap();
        let back: BindEnvelope = serde_json::from_str(&json).unwrap();
        assert!(back.participant_filter.is_none());
    }

    #[test]
    fn rebind_envelope_serde_roundtrip() {
        let env = RebindEnvelope::Prepare(RebindPrepare {
            domain_id: "d1".into(),
            new_bind: BindEnvelope::new("d1", "matrix", "room1"),
            deadline_epoch: 1700000000,
            signature: vec![1, 2, 3],
        });
        let json = serde_json::to_string(&env).unwrap();
        let back: RebindEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
    }
}
