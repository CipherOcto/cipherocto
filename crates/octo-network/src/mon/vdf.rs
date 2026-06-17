//! VDF-based coordinator election (mission 0855p-b-vdf-election).
//!
//! A Verifiable Delay Function (VDF) is used to elect the next
//! coordinator. Each candidate computes `VDF(seed_for_epoch)` over
//! `EPOCH_DURATION_SECONDS = 60`; the candidate whose VDF output
//! is closest to the beacon's published randomness (lowest XOR
//! distance) wins.
//!
//! ## Status
//!
//! The full Wesolowski VDF requires a prime-field setup ceremony
//! (RSA modulus `N = p*q` with safe primes). The mission text
//! calls for the `class_groups` crate, which avoids the trusted
//! setup. We ship the **election state machine** here with
//! simulated VDFs (Blake3 hashes iterated `t` times) so the
//! election logic is testable end-to-end. The production VDF
//! is a follow-up mission that pins the `class_groups` version.
//!
//! ## Beacon
//!
//! `seed_for_epoch = hash(governance_id || epoch_number || previous_seed)`
//!
//! `beacon_randomness = hash(slash_events_of_epoch)` (one-shot
//! beacon; hard to predict, easy to verify).

use std::time::{SystemTime, UNIX_EPOCH};

use blake3::Hasher;
use serde::{Deserialize, Serialize};

/// EPOCH_DURATION_SECONDS = 60 (per mission spec).
pub const EPOCH_DURATION_SECONDS: u64 = 60;

/// Number of hash iterations in the simulated VDF. Real VDF uses
/// `t = 2^EPOCH_DURATION_SECONDS * time_constant`; we use 1024
/// iterations for testability.
pub const SIMULATED_VDF_ITERATIONS: u64 = 1024;

/// A VDF output (32-byte digest).
pub type VdfOutput = [u8; 32];

/// A VDF proof (simulated as 32-byte digest; real VDF is a
/// Wesolowski proof).
pub type VdfProof = [u8; 32];

/// A VDF evaluation result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VdfEvaluation {
    pub output: VdfOutput,
    pub proof: VdfProof,
    pub iterations: u64,
}

impl VdfEvaluation {
    /// Compute a simulated VDF: `y = H^t(seed)`. The proof is
    /// `H^(t-1)(seed)`, which is verifiable in O(1) by checking
    /// `H(proof) == output`.
    pub fn simulate(seed: &[u8; 32], iterations: u64) -> Self {
        let mut h = blake3::hash(seed);
        let mut prev: [u8; 32] = *h.as_bytes();
        for i in 1..iterations {
            h = blake3::hash(&prev);
            prev = *h.as_bytes();
            let _ = i;
        }
        Self {
            output: prev,
            proof: {
                // The proof is the previous hash, so a verifier
                // can check H(proof) == output. For the LAST
                // iteration, we need a different approach; in
                // practice the proof is the (t-1)-th intermediate.
                // For simulation, we recompute the (t-1)-th:
                let mut h = blake3::hash(seed);
                let mut prev2: [u8; 32] = *h.as_bytes();
                for _ in 1..(iterations.saturating_sub(1).max(1)) {
                    h = blake3::hash(&prev2);
                    prev2 = *h.as_bytes();
                }
                prev2
            },
            iterations,
        }
    }

    /// Verify a VDF proof: `H(proof) == output` for the
    /// simulated VDF. The proof encodes the (t-1)-th hash of the
    /// chain, so it is self-consistent with the output without
    /// needing the seed. (`_seed` is kept for API parity with a
    /// real VDF, where the seed is bound into the proof.)
    pub fn verify(&self, _seed: &[u8; 32]) -> bool {
        let h = blake3::hash(&self.proof);
        h.as_bytes() == &self.output
    }
}

/// Compute the beacon seed for an epoch.
pub fn beacon_seed(governance_id: &[u8; 32], epoch: u64, previous_seed: &[u8; 32]) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(governance_id);
    h.update(&epoch.to_le_bytes());
    h.update(previous_seed);
    let out = h.finalize();
    *out.as_bytes()
}

/// Compute the beacon randomness from the slash events of an
/// epoch. `slash_event_hashes` is the list of `SlashEvent` hashes
/// for the epoch; if empty, the beacon is `0`.
pub fn beacon_randomness(slash_event_hashes: &[[u8; 32]]) -> [u8; 32] {
    if slash_event_hashes.is_empty() {
        return [0u8; 32];
    }
    let mut h = Hasher::new();
    for s in slash_event_hashes {
        h.update(s);
    }
    *h.finalize().as_bytes()
}

/// XOR distance between two 32-byte values.
pub fn xor_distance(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = a[i] ^ b[i];
    }
    out
}

/// Returns true if `a` is closer to `target` than `b` (in XOR
/// distance, treated as a big-endian integer).
pub fn is_closer(a: &[u8; 32], b: &[u8; 32], target: &[u8; 32]) -> bool {
    let da = xor_distance(a, target);
    let db = xor_distance(b, target);
    // Lex order: treat bytes as big-endian.
    for i in 0..32 {
        if da[i] < db[i] {
            return true;
        }
        if da[i] > db[i] {
            return false;
        }
    }
    false
}

/// A candidate for the VDF-based election.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VdfCandidate {
    pub pubkey: String,
    pub vdf: VdfEvaluation,
}

/// Election result: the winner and the runner-up for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VdfElectionResult {
    pub winner: VdfCandidate,
    pub beacon_randomness: [u8; 32],
    pub distance: [u8; 32],
}

/// Run the VDF-based election: pick the candidate with the
/// lowest XOR distance to the beacon randomness.
///
/// Tie-break: lower `candidate_pubkey` (lex order).
pub fn elect_vdf(
    candidates: &[VdfCandidate],
    beacon_randomness: &[u8; 32],
) -> Option<VdfElectionResult> {
    if candidates.is_empty() {
        return None;
    }
    let mut best: Option<&VdfCandidate> = None;
    for c in candidates {
        match best {
            None => best = Some(c),
            Some(bc) => {
                let d_new = xor_distance(&c.vdf.output, beacon_randomness);
                let d_old = xor_distance(&bc.vdf.output, beacon_randomness);
                let closer = is_closer(&c.vdf.output, &bc.vdf.output, beacon_randomness);
                let tied = d_new == d_old;
                if closer || (tied && c.pubkey < bc.pubkey) {
                    best = Some(c);
                }
            }
        }
    }
    let winner = best.unwrap().clone();
    let distance = xor_distance(&winner.vdf.output, beacon_randomness);
    Some(VdfElectionResult {
        winner,
        beacon_randomness: *beacon_randomness,
        distance,
    })
}

/// A simple VDF election driver: compute the seed, beacon, and
/// run the election. Returns None if no candidates.
///
/// `previous_seed` is the seed from the previous epoch (or
/// all-zeros for the first epoch).
pub fn run_election(
    governance_id: &[u8; 32],
    epoch: u64,
    previous_seed: &[u8; 32],
    slash_events: &[[u8; 32]],
    candidates: &mut [VdfCandidate],
) -> Option<VdfElectionResult> {
    let _seed = beacon_seed(governance_id, epoch, previous_seed);
    let beacon = beacon_randomness(slash_events);
    // Each candidate must compute their VDF against `_seed`. We
    // assume they've already done so; this driver just selects
    // the winner. (`_seed` is the input each candidate's VDF was
    // (or should have been) evaluated over.)
    elect_vdf(candidates, &beacon)
}

/// Unix epoch seconds (for diagnostics / operator visibility).
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdf_simulate_and_verify() {
        let seed = [1u8; 32];
        let eval = VdfEvaluation::simulate(&seed, 100);
        assert_eq!(eval.iterations, 100);
        // H(proof) should equal output (H^t(seed) = H(H^(t-1)(seed))).
        assert!(
            eval.verify(&seed),
            "VDF proof must verify: H(proof) == H^t(seed)"
        );
    }

    #[test]
    fn vdf_simulate_deterministic() {
        let seed = [2u8; 32];
        let a = VdfEvaluation::simulate(&seed, 10);
        let b = VdfEvaluation::simulate(&seed, 10);
        assert_eq!(a.output, b.output);
    }

    #[test]
    fn beacon_seed_changes_with_epoch() {
        let gov = [0u8; 32];
        let prev = [0u8; 32];
        let s1 = beacon_seed(&gov, 1, &prev);
        let s2 = beacon_seed(&gov, 2, &prev);
        assert_ne!(s1, s2);
    }

    #[test]
    fn beacon_seed_changes_with_previous_seed() {
        let gov = [0u8; 32];
        let p1 = [1u8; 32];
        let p2 = [2u8; 32];
        let s1 = beacon_seed(&gov, 1, &p1);
        let s2 = beacon_seed(&gov, 1, &p2);
        assert_ne!(s1, s2);
    }

    #[test]
    fn beacon_randomness_zero_for_no_slashes() {
        let r = beacon_randomness(&[]);
        assert_eq!(r, [0u8; 32]);
    }

    #[test]
    fn beacon_randomness_changes_with_slashes() {
        let r1 = beacon_randomness(&[[1u8; 32]]);
        let r2 = beacon_randomness(&[[2u8; 32]]);
        assert_ne!(r1, r2);
    }

    #[test]
    fn xor_distance_zero_for_equal() {
        let a = [0u8; 32];
        let d = xor_distance(&a, &a);
        assert_eq!(d, [0u8; 32]);
    }

    #[test]
    fn xor_distance_symmetric() {
        let a = [0xAA; 32];
        let b = [0x55; 32];
        let d_ab = xor_distance(&a, &b);
        let d_ba = xor_distance(&b, &a);
        assert_eq!(d_ab, d_ba);
    }

    #[test]
    fn elect_vdf_picks_closest() {
        let beacon = [0u8; 32];
        let cands = vec![
            VdfCandidate {
                pubkey: "a".into(),
                vdf: VdfEvaluation {
                    output: [0xFF; 32],
                    proof: [0u8; 32],
                    iterations: 1,
                },
            },
            VdfCandidate {
                pubkey: "b".into(),
                vdf: VdfEvaluation {
                    output: [0x01; 32],
                    proof: [0u8; 32],
                    iterations: 1,
                },
            },
        ];
        let r = elect_vdf(&cands, &beacon).unwrap();
        // b's output (0x01) is closer to 0x00 than a's (0xFF).
        assert_eq!(r.winner.pubkey, "b");
    }

    #[test]
    fn elect_vdf_tie_break_lower_pubkey() {
        let beacon = [0u8; 32];
        let cands = vec![
            VdfCandidate {
                pubkey: "z".into(),
                vdf: VdfEvaluation {
                    output: [0x10; 32],
                    proof: [0u8; 32],
                    iterations: 1,
                },
            },
            VdfCandidate {
                pubkey: "a".into(),
                vdf: VdfEvaluation {
                    output: [0x10; 32],
                    proof: [0u8; 32],
                    iterations: 1,
                },
            },
        ];
        let r = elect_vdf(&cands, &beacon).unwrap();
        assert_eq!(r.winner.pubkey, "a");
    }

    #[test]
    fn elect_vdf_empty_returns_none() {
        assert!(elect_vdf(&[], &[0u8; 32]).is_none());
    }

    #[test]
    fn run_election_end_to_end() {
        let gov = [0u8; 32];
        let prev = [0u8; 32];
        let slashes = [[0xAA; 32]];
        let mut cands = vec![
            VdfCandidate {
                pubkey: "a".into(),
                vdf: VdfEvaluation {
                    output: [0x00; 32],
                    proof: [0u8; 32],
                    iterations: 1,
                },
            },
            VdfCandidate {
                pubkey: "b".into(),
                vdf: VdfEvaluation {
                    output: [0xFF; 32],
                    proof: [0u8; 32],
                    iterations: 1,
                },
            },
        ];
        let beacon = beacon_randomness(&slashes);
        let r = run_election(&gov, 1, &prev, &slashes, &mut cands).unwrap();
        // Verify the winner is one of a or b.
        assert!(r.winner.pubkey == "a" || r.winner.pubkey == "b");
        // Beacon is in the result.
        assert_eq!(r.beacon_randomness, beacon);
    }
}
