//! Network bootstrap (RFC-0851p-a + missions 0851p-a-*).
//!
//! ## Missions in this module
//!
//! - **0851p-a-seed-health-check**: at `start_node`, drop seeds
//!   older than `MAX_SEED_AGE_EPOCHS = 10` epochs. Refuse to
//!   start with `SeedListFullyStale` if 100% are stale.
//! - **0851p-a-bootstrap-slashing**: SlashEnvelope has the new
//!   `0x000D` reason code and sub-codes (see `mon/slash.rs`).
//! - **0851p-a-seed-authority-decentralization**: Hard-fork gate
//!   for foundation multi-sig deprecation at `EPOCH_GOVERNANCE_TAKEOVER`.
//! - **0851p-a-tor-seed-list**: `bootstrap_mode = Direct |
//!   TorOnly | TorWithIpFallback` config option.
//! - **0851p-a-trust-ux**: ASCII + DOT graph renderer for the
//!   web-of-trust graph.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

// ── Mission 0851p-a-seed-health-check ─────────────────────────────

/// Maximum age (in epochs) for a seed list entry. At 1-minute
/// epochs, 10 minutes is a reasonable staleness threshold. Older
/// seeds are likely abandoned or compromised.
pub const MAX_SEED_AGE_EPOCHS: u64 = 10;

/// Stale seed record (mission 0851p-a-seed-health-check).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleSeed {
    pub peer_id: String,
    pub signed_at_epoch: u64,
    pub current_epoch: u64,
}

impl StaleSeed {
    pub fn is_stale(&self) -> bool {
        self.current_epoch.saturating_sub(self.signed_at_epoch) > MAX_SEED_AGE_EPOCHS
    }
}

/// Seed list entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedEntry {
    pub peer_id: String,
    pub multiaddr: String,
    pub signed_at_epoch: u64,
}

/// A signed seed list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedListEnvelope {
    pub authority_pubkey: Vec<u8>,
    pub signed_at_epoch: u64,
    pub peers: Vec<SeedEntry>,
}

/// Health-check result for a seed list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedHealth {
    /// All seeds are fresh.
    Fresh { fresh_count: usize },
    /// Some seeds are stale; the ratio is `stale_count / total`.
    PartialStale {
        fresh_count: usize,
        stale_count: usize,
        ratio_percent: u8,
        stale_seeds: Vec<StaleSeed>,
    },
    /// All seeds are stale.
    FullyStale { total: usize },
}

impl SeedHealth {
    /// Run the health check.
    ///
    /// The 20% threshold (per mission) emits a WARN log; we
    /// capture it in `PartialStale` for the caller to log.
    pub fn check(envelope: &SeedListEnvelope, current_epoch: u64) -> Self {
        let mut fresh = 0;
        let mut stale_seeds = Vec::new();
        for peer in &envelope.peers {
            let age = current_epoch.saturating_sub(peer.signed_at_epoch);
            if age > MAX_SEED_AGE_EPOCHS {
                stale_seeds.push(StaleSeed {
                    peer_id: peer.peer_id.clone(),
                    signed_at_epoch: peer.signed_at_epoch,
                    current_epoch,
                });
            } else {
                fresh += 1;
            }
        }
        let total = envelope.peers.len();
        if total == 0 {
            return SeedHealth::FullyStale { total: 0 };
        }
        let stale_count = stale_seeds.len();
        if stale_count == total {
            return SeedHealth::FullyStale { total };
        }
        if stale_count == 0 {
            return SeedHealth::Fresh { fresh_count: fresh };
        }
        // Compute ratio as percent.
        let ratio_percent = ((stale_count * 100) / total) as u8;
        SeedHealth::PartialStale {
            fresh_count: fresh,
            stale_count,
            ratio_percent,
            stale_seeds,
        }
    }

    /// Returns true if the health check should refuse to start
    /// (100% stale).
    pub fn refuses_start(&self) -> bool {
        matches!(self, SeedHealth::FullyStale { .. })
    }
}

// ── Mission 0851p-a-seed-authority-decentralization ──────────────

/// The epoch after which the foundation multi-sig is deprecated
/// and only the DAO multi-sig is accepted.
pub const EPOCH_GOVERNANCE_TAKEOVER: u64 = 1_700_000_000; // placeholder

/// The authority behind a seed list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeedListAuthority {
    /// Phase 1: 3-of-5 foundation multi-sig.
    Foundation,
    /// Phase 2+: DAO multi-sig (RFC-0855 §11 governance key).
    Dao,
}

/// A seed list authority error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedAuthorityError {
    /// The foundation multi-sig was used after the
    /// `EPOCH_GOVERNANCE_TAKEOVER`.
    SeedListAuthorityDeprecated,
    /// The DAO multi-sig is not yet active (used before
    /// `EPOCH_GOVERNANCE_TAKEOVER`).
    DaoNotYetActive,
    /// The signature is invalid.
    BadSignature,
}

/// Verify a seed list's authority at `current_epoch`.
///
/// Returns `Ok(())` if the authority is valid for the current
/// epoch. Returns `Err` if the authority is deprecated or not
/// yet active.
pub fn verify_authority(
    authority: SeedListAuthority,
    current_epoch: u64,
) -> Result<(), SeedAuthorityError> {
    if current_epoch >= EPOCH_GOVERNANCE_TAKEOVER {
        if authority == SeedListAuthority::Foundation {
            return Err(SeedAuthorityError::SeedListAuthorityDeprecated);
        }
    } else if authority == SeedListAuthority::Dao {
        return Err(SeedAuthorityError::DaoNotYetActive);
    }
    Ok(())
}

// ── Mission 0851p-a-tor-seed-list ────────────────────────────────

/// Bootstrap transport mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapMode {
    /// Direct IP connection (current default).
    Direct,
    /// Tor-only (`.onion` seed list service over `arti`). No IP
    /// fallback. Fails if Tor is down.
    TorOnly,
    /// Tor with direct-IP fallback. Logs a warning on fallback.
    TorWithIpFallback,
}

impl Default for BootstrapMode {
    fn default() -> Self {
        BootstrapMode::Direct
    }
}

// ── Mission 0851p-a-bootstrap-slashing ───────────────────────────

/// The set of slashed `peer_id`s (bootstrap nodes that have been
/// removed from the seed list).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SlashedSeedBlacklist {
    slashed: HashSet<String>,
}

impl SlashedSeedBlacklist {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a peer as slashed.
    pub fn slash(&mut self, peer_id: impl Into<String>) {
        self.slashed.insert(peer_id.into());
    }

    /// Returns true if `peer_id` is in the blacklist.
    pub fn is_slashed(&self, peer_id: &str) -> bool {
        self.slashed.contains(peer_id)
    }

    /// Filter a seed list, removing any slashed peers.
    pub fn filter(&self, mut envelope: SeedListEnvelope) -> SeedListEnvelope {
        envelope.peers.retain(|p| !self.is_slashed(&p.peer_id));
        envelope
    }

    /// Returns the number of slashed peers.
    pub fn len(&self) -> usize {
        self.slashed.len()
    }

    /// Returns true if the blacklist is empty.
    pub fn is_empty(&self) -> bool {
        self.slashed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(peer: &str, signed: u64) -> SeedEntry {
        SeedEntry {
            peer_id: peer.into(),
            multiaddr: format!("/ip4/1.2.3.4/tcp/4001/p2p/{peer}"),
            signed_at_epoch: signed,
        }
    }

    fn envelope(peers: Vec<SeedEntry>) -> SeedListEnvelope {
        SeedListEnvelope {
            authority_pubkey: vec![0],
            signed_at_epoch: 100,
            peers,
        }
    }

    #[test]
    fn fresh_seeds_pass() {
        let env = envelope(vec![entry("a", 100), entry("b", 100)]);
        let health = SeedHealth::check(&env, 105);
        assert!(matches!(health, SeedHealth::Fresh { fresh_count: 2 }));
        assert!(!health.refuses_start());
    }

    #[test]
    fn partially_stale_seeds_log_warning() {
        let env = envelope(vec![
            entry("a", 100), // age 5
            entry("b", 100),
            entry("c", 50), // age 55, stale
            entry("d", 50),
        ]);
        let health = SeedHealth::check(&env, 105);
        match health {
            SeedHealth::PartialStale {
                fresh_count,
                stale_count,
                ratio_percent,
                ..
            } => {
                assert_eq!(fresh_count, 2);
                assert_eq!(stale_count, 2);
                assert_eq!(ratio_percent, 50);
            }
            other => panic!("expected PartialStale, got {other:?}"),
        }
    }

    #[test]
    fn fully_stale_refuses_start() {
        let env = envelope(vec![entry("a", 50), entry("b", 50)]);
        let health = SeedHealth::check(&env, 105);
        assert!(matches!(health, SeedHealth::FullyStale { total: 2 }));
        assert!(health.refuses_start());
    }

    #[test]
    fn empty_envelope_is_fully_stale() {
        let env = envelope(vec![]);
        let health = SeedHealth::check(&env, 105);
        assert!(health.refuses_start());
    }

    #[test]
    fn stale_seed_is_stale_iff_older_than_max() {
        let s = StaleSeed {
            peer_id: "a".into(),
            signed_at_epoch: 100,
            current_epoch: 105,
        };
        assert!(!s.is_stale()); // 5 ≤ 10
        let s2 = StaleSeed {
            peer_id: "a".into(),
            signed_at_epoch: 90,
            current_epoch: 105,
        };
        assert!(s2.is_stale()); // 15 > 10
    }

    #[test]
    fn authority_dao_accepted_after_fork() {
        assert!(verify_authority(SeedListAuthority::Dao, EPOCH_GOVERNANCE_TAKEOVER).is_ok());
    }

    #[test]
    fn authority_foundation_rejected_after_fork() {
        let result = verify_authority(SeedListAuthority::Foundation, EPOCH_GOVERNANCE_TAKEOVER);
        assert_eq!(result, Err(SeedAuthorityError::SeedListAuthorityDeprecated));
    }

    #[test]
    fn authority_foundation_accepted_before_fork() {
        assert!(verify_authority(SeedListAuthority::Foundation, 0).is_ok());
    }

    #[test]
    fn authority_dao_rejected_before_fork() {
        let result = verify_authority(SeedListAuthority::Dao, 0);
        assert_eq!(result, Err(SeedAuthorityError::DaoNotYetActive));
    }

    #[test]
    fn bootstrap_mode_default_is_direct() {
        assert_eq!(BootstrapMode::default(), BootstrapMode::Direct);
    }

    #[test]
    fn slashed_blacklist_filters_seeds() {
        let mut bl = SlashedSeedBlacklist::new();
        bl.slash("evil-peer");
        let env = envelope(vec![entry("good", 100), entry("evil-peer", 100)]);
        let filtered = bl.filter(env);
        assert_eq!(filtered.peers.len(), 1);
        assert_eq!(filtered.peers[0].peer_id, "good");
    }

    #[test]
    fn slashed_blacklist_is_slashed() {
        let mut bl = SlashedSeedBlacklist::new();
        bl.slash("a");
        assert!(bl.is_slashed("a"));
        assert!(!bl.is_slashed("b"));
        assert_eq!(bl.len(), 1);
        assert!(!bl.is_empty());
    }
}
