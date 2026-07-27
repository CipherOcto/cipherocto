//! Canonical constants for the reputation registry (RFC-0968 §10 declarations).
//!
//! Every constant is `pub const` so it is reachable from any crate without
//! re-declaration. Mission 0968 acceptance criteria require these declarations
//! to live in the RFC as canonical authority; the crate mirrors them to keep
//! runtime invariants and spec invariants byte-identical.

/// Minimum OCTO stake a recorder must post (RFC-0968 §10).
pub const MIN_RECORDER_OCTO_STAKE: u64 = 4_000;

/// Minimum role-token stake a recorder must post (RFC-0968 §10).
pub const MIN_RECORDER_ROLE_STAKE: u64 = 1_000;

/// Minimum dual-stake aggregate (octo + role) a recorder must post (RFC-0968 §10).
pub const MIN_RECORDER_DUAL_STAKE: u64 = 5_000;

/// Governance quorum: distinct signatures required on any authoritative proof
/// (suspension, retirement, slash). RFC-0968 amendment 24.
pub const GOVERNANCE_QUORUM: u32 = 3;

/// Attestor quorum: distinct attestor DIDs required to have observed a
/// gossip event before the federation accepts it as confirmed. RFC-0968
/// amendment 22 (I-P7). Absence of quorum fails-closed; `query_attestations`
/// + `attestor_quorum_reached` enforce this on the read path.
pub const MIN_ATTESTOR_QUORUM: u32 = 3;

/// Maximum candidates per attested `controller_id` per election (RFC-0968
/// amendment 58, Round 9 / Round 11 R11-M5). Reduced from 32 to 1.
pub const MAX_CANDIDATES_PER_CONTROLLER_PER_ELECTION: u64 = 1;

/// Auditor nonce TTL in seconds (RFC-0968 §3, Round A1 audit canonical decl).
/// Default 7 days.
pub const MAX_AUDITOR_NONCE_TTL_SECS: u64 = 7 * 86_400;

/// Maximum age of a `GovernanceSnapshot` before it is considered stale.
/// Used by `verify_governance_suspension`, `slash_recorder`,
/// `declare_retirement_eligible`. RFC-0968 amendment 24.
pub const MAX_GOVERNANCE_SNAPSHOT_AGE_SECS: u64 = 600;

/// Minimum finality depth (blocks) before a reputation anchor is considered
/// final. RFC-0955 §"Finality".
pub const MIN_REPUTATION_ANCHOR_FINALITY_BLOCKS: u64 = 12;

// ---------------------------------------------------------------------------
// BLAKE3 domain separators (RFC-0968 §21 + §10 Review Round 7).
//
// Each constant is a distinct byte string so domain-separated BLAKE3 calls
// produce different digests for the same input bytes under different domains.
// ---------------------------------------------------------------------------

pub const BLAKE3_REPUTATION_EVENT_DOMAIN: &[u8] = b"cipherocto/reputation/event/v1";
pub const BLAKE3_REPUTATION_AGGREGATE_DOMAIN: &[u8] = b"cipherocto/reputation/aggregate/v1";
pub const BLAKE3_REPUTATION_RECORDER_DOMAIN: &[u8] = b"cipherocto/reputation/recorder/v1";
pub const BLAKE3_REPUTATION_SUSPENSION_DOMAIN: &[u8] = b"cipherocto/reputation/suspension/v1";
pub const BLAKE3_REPUTATION_SLASH_DOMAIN: &[u8] = b"cipherocto/reputation/slash/v1";
pub const BLAKE3_REPUTATION_RETIREMENT_DOMAIN: &[u8] = b"cipherocto/reputation/retirement/v1";
pub const BLAKE3_REPUTATION_ANCHOR_DOMAIN: &[u8] = b"cipherocto/reputation/anchor/v1";
pub const BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN: &[u8] = b"cipherocto/reputation/audit_nonce/v1";
pub const BLAKE3_REPUTATION_CROSS_LAYER_DOMAIN: &[u8] = b"cipherocto/reputation/cross_layer/v1";
pub const BLAKE3_REPUTATION_SLIDING_WINDOW_DOMAIN: &[u8] =
    b"cipherocto/reputation/sliding_window/v1";
pub const BLAKE3_REPUTATION_PARITY_DOMAIN: &[u8] = b"cipherocto/reputation/parity/v1";
pub const BLAKE3_REPUTATION_GOVERNANCE_SNAPSHOT_DOMAIN: &[u8] =
    b"cipherocto/governance/snapshot/v1";
pub const BLAKE3_REPUTATION_GOVERNANCE_PROOF_DOMAIN: &[u8] = b"cipherocto/governance/proof/v1";

/// Separate family from reputation domains — the set of governance pubkeys
/// authorised to sign authoritative proofs. RFC-0968 §10 Review Round 7.
pub const BLAKE3_GOVERNANCE_SET_DOMAIN: &[u8] = b"cipherocto/governance/set/v1";

#[cfg(test)]
mod tests {
    use super::*;

    /// All domain separators must be byte-distinct so a digest computed
    /// under one domain can never collide with a digest computed under
    /// another for the same input bytes.
    #[test]
    fn domains_are_byte_distinct() {
        let all: &[(&str, &[u8])] = &[
            ("event", BLAKE3_REPUTATION_EVENT_DOMAIN),
            ("aggregate", BLAKE3_REPUTATION_AGGREGATE_DOMAIN),
            ("recorder", BLAKE3_REPUTATION_RECORDER_DOMAIN),
            ("suspension", BLAKE3_REPUTATION_SUSPENSION_DOMAIN),
            ("slash", BLAKE3_REPUTATION_SLASH_DOMAIN),
            ("retirement", BLAKE3_REPUTATION_RETIREMENT_DOMAIN),
            ("anchor", BLAKE3_REPUTATION_ANCHOR_DOMAIN),
            ("audit_nonce", BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN),
            ("cross_layer", BLAKE3_REPUTATION_CROSS_LAYER_DOMAIN),
            ("sliding_window", BLAKE3_REPUTATION_SLIDING_WINDOW_DOMAIN),
            ("parity", BLAKE3_REPUTATION_PARITY_DOMAIN),
            ("gov_snapshot", BLAKE3_REPUTATION_GOVERNANCE_SNAPSHOT_DOMAIN),
            ("gov_proof", BLAKE3_REPUTATION_GOVERNANCE_PROOF_DOMAIN),
            ("gov_set", BLAKE3_GOVERNANCE_SET_DOMAIN),
        ];
        assert_eq!(all.len(), 14, "must enumerate all 14 domain separators");
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(
                    all[i].1, all[j].1,
                    "domain {} collides with {}",
                    all[i].0, all[j].0
                );
            }
        }
    }

    #[test]
    fn stake_minima_are_independent() {
        // Per Round 8 rename: MIN_RECORDER_OCTO_STAKE and MIN_RECORDER_ROLE_STAKE
        // are independent minima, NOT subsumed by MIN_RECORDER_DUAL_STAKE.
        const { assert!(MIN_RECORDER_OCTO_STAKE < MIN_RECORDER_DUAL_STAKE) };
        const { assert!(MIN_RECORDER_ROLE_STAKE < MIN_RECORDER_DUAL_STAKE) };
        const {
            assert!(
                MIN_RECORDER_OCTO_STAKE + MIN_RECORDER_ROLE_STAKE == MIN_RECORDER_DUAL_STAKE,
                "dual-stake aggregate = octo + role"
            );
        };
    }

    #[test]
    fn candidates_per_controller_is_one() {
        // Round 9 / Round 11 R11-M5 reduced from 32 to 1.
        const { assert!(MAX_CANDIDATES_PER_CONTROLLER_PER_ELECTION == 1) };
    }

    #[test]
    fn auditor_nonce_ttl_is_seven_days() {
        const { assert!(MAX_AUDITOR_NONCE_TTL_SECS == 7 * 86_400) };
    }
}
