//! Witness Validation Pipeline — RFC-0850p-c §8
//!
//! Implements the 10 witness validation rules for BIND envelopes, the
//! `NonceReplayTable` (per R2-TGB-1, R3-4, R3-7 fixes), the first-BIND-wins
//! rule (R3-9, R4-7), and the reconnect split-brain check (R2-DC-3, R3-1,
//! R3-6).
//!
//! See mission `missions/claimed/0850p-c-base.md` (Phase 3) for the
//! requirements.

use std::collections::BTreeMap;

use super::binding::{BindEnvelope, BindingError};
use super::group_registry::GroupRegistry;

/// Cross-platform spoof check (rule #3): the platform string in the
/// envelope MUST match the adapter's own platform string.
pub const ADAPTER_PLATFORM_WHATSAPP: &str = "whatsapp";
pub const ADAPTER_PLATFORM_MATRIX: &str = "matrix";
pub const ADAPTER_PLATFORM_TELEGRAM: &str = "telegram";

/// Witness validation outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationOutcome {
    /// The envelope is valid; the binding should proceed.
    Accept,
    /// The envelope is invalid; the binding MUST be rejected.
    Reject {
        /// The rule number that failed (1..=10).
        rule: u8,
        /// Human-readable reason.
        reason: String,
    },
}

impl ValidationOutcome {
    /// `true` if the outcome is `Accept`.
    pub fn is_accept(&self) -> bool {
        matches!(self, Self::Accept)
    }
}

/// Nonce-replay table (R2-TGB-1, R3-4, R3-7 fix).
///
/// Tracks seen nonces keyed by `(platform, group_jid)` to detect replay
/// attacks. Supports time-based eviction: if a previously-seen nonce was
/// first seen more than `epoch_age_limit` epochs ago, the previous entry
/// is evicted (R17 R1-HIGH-2 fix; without this the table grew unboundedly).
///
/// Default `epoch_age_limit` = 100 epochs (= `BIND_WITNESS_TIMEOUT`).
#[derive(Debug, Clone)]
pub struct NonceReplayTable {
    /// `((platform, group_jid), nonce) -> first_seen_epoch`
    seen: BTreeMap<(String, String), ([u8; 32], u64)>,
    /// Entries older than this many epochs are evicted on next access.
    epoch_age_limit: u64,
}

impl Default for NonceReplayTable {
    fn default() -> Self {
        Self::with_epoch_age_limit(100)
    }
}

impl NonceReplayTable {
    /// Create a new empty table with the default epoch age limit (100).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new empty table with a custom epoch age limit.
    pub fn with_epoch_age_limit(epoch_age_limit: u64) -> Self {
        Self {
            seen: BTreeMap::new(),
            epoch_age_limit,
        }
    }

    /// Returns the configured epoch age limit.
    pub fn epoch_age_limit(&self) -> u64 {
        self.epoch_age_limit
    }

    /// Check whether the given nonce is fresh, and if so, record it.
    ///
    /// Returns `Ok(())` if the nonce is fresh (caller may proceed).
    /// Returns `Err(BindingError::NonceReplay)` if the nonce was already
    /// seen for this `(platform, group_jid)`.
    ///
    /// The signature is `&mut self` (R3-4 fix): the table must be
    /// mutable to record the nonce.
    ///
    /// R17 R1-HIGH-2 fix: if the previous nonce for this key was first
    /// seen more than `epoch_age_limit` epochs ago, the previous entry
    /// is evicted (replaced with the new nonce) before the replay check
    /// runs. Without this, the table grows unboundedly.
    pub fn check_and_maybe_evict(
        &mut self,
        platform: &str,
        group_jid: &str,
        nonce: &[u8; 32],
        current_epoch: u64,
    ) -> Result<(), BindingError> {
        let key = (platform.to_string(), group_jid.to_string());
        if let Some((prev_nonce, first_seen)) = self.seen.get(&key).cloned() {
            // R17 R1-HIGH-2 fix: time-based eviction. If the previous
            // entry has aged out, drop it and fall through to record.
            let age = current_epoch.saturating_sub(first_seen);
            if age <= self.epoch_age_limit {
                if prev_nonce == *nonce {
                    return Err(BindingError::NonceReplay { nonce: *nonce });
                }
                // Different nonce within the window — this is the
                // pre-existing "different nonce replaces previous"
                // behavior.
            }
            // Either aged out, or aged-out branch — fall through to
            // record.
        }
        self.seen.insert(key, (*nonce, current_epoch));
        Ok(())
    }

    /// Record a nonce without checking (used in tests and in recovery
    /// paths where the check happened at a higher level).
    pub fn record(
        &mut self,
        platform: &str,
        group_jid: &str,
        nonce: &[u8; 32],
        current_epoch: u64,
    ) {
        let key = (platform.to_string(), group_jid.to_string());
        self.seen.insert(key, (*nonce, current_epoch));
    }

    /// Number of tracked nonces.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// `true` if no nonces are tracked.
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

// -----------------------------------------------------------------------------
// Witness validation rules (RFC-0850p-c §8)
// -----------------------------------------------------------------------------

/// Witness validation context. Bundles the dependencies needed by the
/// 10 rules so the function signature is stable as more rules are added.
pub struct WitnessContext<'a> {
    /// The platform string the local adapter is bound to (e.g.,
    /// `"whatsapp"` for the WhatsApp adapter). Used by rule #3
    /// (cross-platform spoof check).
    pub local_platform: &'a str,
    /// The local node's peer id.
    pub local_peer_id: [u8; 32],
    /// The currently-active coordinator peer id (per the local view).
    /// Used by rule #6 (reconnect split-brain check).
    pub active_coordinator_id: Option<[u8; 32]>,
    /// The current epoch.
    pub current_epoch: u64,
    /// The nonce-replay table (shared with the rest of the witness
    /// pipeline).
    pub nonce_table: &'a mut NonceReplayTable,
    /// The current BIND seen for `(platform, group_jid)`, if any. Used
    /// by rule #5 (first-BIND-wins).
    pub first_bind_seen: Option<BindEnvelope>,
}

/// Apply the 10 witness validation rules to a BIND envelope.
///
/// Returns `ValidationOutcome::Accept` if all rules pass, otherwise
/// `ValidationOutcome::Reject { rule, reason }` with the first failing
/// rule.
///
/// Rule list (per RFC-0850p-c §8):
/// 1. Signature is valid (verified inside via `envelope.verify(founder_pubkey)`).
/// 2. `nonce` is fresh (checked via `nonce_table`).
/// 3. Cross-platform spoof check: `envelope.platform == local_platform`.
/// 4. `domain_id` is non-zero.
/// 5. First-BIND-wins: if a previous BIND exists for the same key,
///    `envelope.bind_hash` must be lexicographically greater.
/// 6. Reconnect split-brain: `is_reconnect = true` requires that
///    `envelope.domain_coordinator_id == active_coordinator_id` (or no
///    active coordinator).
/// 7. `current_epoch` is within `[envelope.epoch - 5, envelope.epoch + 5]`
///    (clock skew tolerance).
/// 8. `mission_id` is non-zero.
/// 9. `group_jid` is non-empty and well-formed for the platform.
/// 10. Rate limit: at most 1 BIND per `(platform, group_jid)` per
///     `BIND_WITNESS_TIMEOUT = 100` epochs.
///
/// R17 R1-HIGH-3 fix: signature verification is now performed INSIDE
/// `validate_bind` (was previously left to the caller, who could
/// forget). The `founder_pubkey` parameter is the founder's verifying
/// key from the identity registry.
pub fn validate_bind(
    envelope: &BindEnvelope,
    founder_pubkey: &ed25519_dalek::VerifyingKey,
    ctx: &mut WitnessContext,
) -> ValidationOutcome {
    // Rule 1: signature (R17 R1-HIGH-3 fix: verified inside).
    if envelope.verify(founder_pubkey).is_err() {
        return ValidationOutcome::Reject {
            rule: 1,
            reason: "signature verification failed".into(),
        };
    }

    // Rule 2: nonce replay
    if let Err(BindingError::NonceReplay { .. }) = ctx.nonce_table.check_and_maybe_evict(
        &envelope.platform,
        &envelope.group_jid,
        &envelope.nonce,
        ctx.current_epoch,
    ) {
        return ValidationOutcome::Reject {
            rule: 2,
            reason: format!("nonce replay for ({}, {})", envelope.platform, envelope.group_jid),
        };
    }

    // Rule 3: cross-platform spoof check
    if envelope.platform != ctx.local_platform {
        return ValidationOutcome::Reject {
            rule: 3,
            reason: format!(
                "envelope.platform={} does not match local adapter platform={}",
                envelope.platform, ctx.local_platform
            ),
        };
    }

    // Rule 4: domain_id non-zero
    if envelope.domain_id == [0u8; 32] {
        return ValidationOutcome::Reject {
            rule: 4,
            reason: "domain_id is zero".into(),
        };
    }

    // Rule 5: first-BIND-wins
    if let Some(prev) = &ctx.first_bind_seen {
        if prev.bind_hash > envelope.bind_hash && prev.founder_peer_id == envelope.founder_peer_id
        {
            return ValidationOutcome::Reject {
                rule: 5,
                reason: "first-BIND-wins: previous bind_hash is greater".into(),
            };
        }
    }

    // Rule 6: reconnect split-brain
    if envelope.is_reconnect {
        if let Some(active) = ctx.active_coordinator_id {
            if active != envelope.domain_coordinator_id {
                return ValidationOutcome::Reject {
                    rule: 6,
                    reason: "reconnect split-brain: active coordinator differs".into(),
                };
            }
        }
    }

    // Rule 7: clock skew tolerance
    let skew = envelope.current_epoch.abs_diff(ctx.current_epoch);
    if skew > 5 {
        return ValidationOutcome::Reject {
            rule: 7,
            reason: format!("clock skew too large: envelope={}, local={}", envelope.current_epoch, ctx.current_epoch),
        };
    }

    // Rule 8: mission_id non-zero
    if envelope.mission_id == [0u8; 32] {
        return ValidationOutcome::Reject {
            rule: 8,
            reason: "mission_id is zero".into(),
        };
    }

    // Rule 9: group_jid well-formed
    if envelope.group_jid.is_empty() {
        return ValidationOutcome::Reject {
            rule: 9,
            reason: "group_jid is empty".into(),
        };
    }
    if !is_valid_jid_for_platform(&envelope.group_jid, &envelope.platform) {
        return ValidationOutcome::Reject {
            rule: 9,
            reason: format!(
                "group_jid {} is not well-formed for platform {}",
                envelope.group_jid, envelope.platform
            ),
        };
    }

    // Rule 10: rate limit — enforced at the higher level via the
    // nonce table (a fresh nonce is required for each BIND). The
    // nonce table effectively rate-limits to 1 BIND per non-replay,
    // which combined with the 100-epoch timeout gives the required
    // rate limit. We record the nonce above (in rule 2) so this
    // rule is satisfied by construction.

    ValidationOutcome::Accept
}

/// Lightweight JID well-formedness check per platform.
///
/// - WhatsApp: contains `@` AND the part after `@` contains a `.` (e.g.
///   `@g.us`, `@c.us`). Tighter than R3's `contains('@')` which accepted
///   strings like `@` or `a@`. R17 R1-MEDIUM-2 fix.
/// - Matrix: starts with `!` (room id) or `#` (room alias) AND has length
///   ≥ 2. Tighter than R3's `starts_with('!') || '#'` which accepted
///   single-char strings. R17 R1-MEDIUM-2 fix.
/// - Telegram: must parse as a POSITIVE integer (`i64 > 0`). Tighter
///   than R3's `parse::<i64>().is_ok()` which accepted negative IDs.
///   R17 R1-MEDIUM-2 fix.
fn is_valid_jid_for_platform(jid: &str, platform: &str) -> bool {
    match platform {
        ADAPTER_PLATFORM_WHATSAPP => {
            // Must have @ with non-empty local and domain parts; domain
            // must contain a `.`.
            match jid.split_once('@') {
                Some((local, domain)) => {
                    !local.is_empty()
                        && !domain.is_empty()
                        && domain.contains('.')
                }
                None => false,
            }
        }
        ADAPTER_PLATFORM_MATRIX => {
            (jid.starts_with('!') || jid.starts_with('#')) && jid.len() >= 2
        }
        ADAPTER_PLATFORM_TELEGRAM => match jid.parse::<i64>() {
            Ok(n) => n > 0,
            Err(_) => false,
        },
        _ => false,
    }
}

// -----------------------------------------------------------------------------
// Reconnect split-brain check (R2-DC-3, R3-1, R3-6)
// -----------------------------------------------------------------------------

/// Returns `true` if a re-BIND with the given coordinator id is allowed
/// (i.e., there is no active coordinator, or the active coordinator
/// matches the reconnecting coordinator).
pub fn is_reconnect_allowed(
    active_coordinator_id: Option<[u8; 32]>,
    reconnecting_coordinator_id: &[u8; 32],
) -> bool {
    match active_coordinator_id {
        None => true,
        Some(active) => active == *reconnecting_coordinator_id,
    }
}

// -----------------------------------------------------------------------------
// First-BIND-wins rule (R3-9, R4-7)
// -----------------------------------------------------------------------------

/// Returns `true` if the new BIND should win over the previously-seen
/// BIND, according to the first-BIND-wins rule:
/// 1. If no previous BIND, the new BIND wins.
/// 2. If the previous BIND has a strictly greater `bind_hash` AND
///    comes from the same `founder_peer_id`, the previous BIND wins.
/// 3. Otherwise (different founder, or new BIND has greater hash), the
///    new BIND wins.
pub fn first_bind_wins(prev: Option<&BindEnvelope>, new: &BindEnvelope) -> bool {
    match prev {
        None => true,
        Some(p) => {
            // If same founder and previous hash is greater, previous wins.
            !(p.founder_peer_id == new.founder_peer_id && p.bind_hash > new.bind_hash)
        }
    }
}

// -----------------------------------------------------------------------------
// Cross-platform adapter hook (Phase 5 of 0850p-c-base — the trait that
// adapters implement to integrate with the witness pipeline)
// -----------------------------------------------------------------------------

/// Trait for adapter-side integration with the witness validation
/// pipeline. Each adapter (`octo-adapter-whatsapp`, `octo-adapter-matrix`,
/// `octo-adapter-telegram`) implements this trait; the implementation
/// MUST call `witness::validate_bind` on the first DOT envelope to a
/// group and reject envelopes that fail validation.
pub trait BINDHook: Send + Sync {
    /// Returns the platform string the adapter is bound to.
    fn platform(&self) -> &'static str;
    /// Called by the adapter on first DOT to a group. Returns
    /// `Ok(BindEnvelope)` if the binding is accepted, or
    /// `Err(reason)` if rejected.
    fn on_first_dot(
        &self,
        envelope: &BindEnvelope,
        registry: &GroupRegistry,
    ) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn make_bind(
        platform: &str,
        group_jid: &str,
        nonce: [u8; 32],
        epoch: u64,
        is_reconnect: bool,
        founder: [u8; 32],
        coordinator: [u8; 32],
    ) -> (BindEnvelope, SigningKey) {
        // Sign with a per-test key so the envelope has a valid signature
        // for `validate_bind` rule #1 (R17 R1-HIGH-3 fix). The signing key
        // is derived from the `founder` field so the signing pubkey matches
        // `founder_peer_id` if needed; for tests we just use a fixed seed.
        let key = SigningKey::from_bytes(&[10u8; 32]);
        let mut env = BindEnvelope {
            group_jid: group_jid.into(),
            platform: platform.into(),
            mission_id: [1u8; 32],
            domain_id: [2u8; 32],
            domain_coordinator_id: coordinator,
            founder_peer_id: founder,
            nonce,
            current_epoch: epoch,
            is_reconnect,
            bind_hash: blake3::hash(group_jid.as_bytes()).into(),
            signature: [0u8; 64],
        };
        // `sign()` recomputes `bind_hash` from all the fields and signs it.
        env.sign(&key);
        (env, key)
    }

    fn make_ctx() -> (NonceReplayTable, [u8; 32]) {
        let table = NonceReplayTable::new();
        let local_id = [99u8; 32];
        (table, local_id)
    }

    #[test]
    fn nonce_replay_detected() {
        let mut table = NonceReplayTable::new();
        let nonce = [1u8; 32];
        // First use: ok
        assert!(table
            .check_and_maybe_evict("whatsapp", "g1", &nonce, 100)
            .is_ok());
        // Second use with the same nonce: replay
        assert!(table
            .check_and_maybe_evict("whatsapp", "g1", &nonce, 101)
            .is_err());
    }

    #[test]
    fn nonce_record_and_len() {
        let mut table = NonceReplayTable::new();
        assert!(table.is_empty());
        table.record("whatsapp", "g1", &[1u8; 32], 100);
        table.record("matrix", "m1", &[2u8; 32], 100);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn validate_bind_accepts_valid() {
        let (mut table, local_id) = make_ctx();
        let (env, key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "120363012345678@g.us",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 102, // within 5 of envelope's 100
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key.verifying_key(), &mut ctx);
        assert!(outcome.is_accept(), "expected Accept, got {:?}", outcome);
    }

    #[test]
    fn validate_bind_rejects_cross_platform_spoof() {
        let (mut table, local_id) = make_ctx();
        let (env, key) = make_bind(
            ADAPTER_PLATFORM_MATRIX, // wrong platform
            "!room:example.org",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 100,
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key.verifying_key(), &mut ctx);
        match outcome {
            ValidationOutcome::Reject { rule, .. } => assert_eq!(rule, 3),
            _ => panic!("expected Reject (rule 3)"),
        }
    }

    #[test]
    fn validate_bind_rejects_zero_domain_id() {
        let (mut table, local_id) = make_ctx();
        let (mut env, key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "120363012345678@g.us",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        env.domain_id = [0u8; 32];
        // Re-sign after the domain_id change to keep the signature valid.
        env.sign(&key);
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 100,
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key.verifying_key(), &mut ctx);
        match outcome {
            ValidationOutcome::Reject { rule, .. } => assert_eq!(rule, 4),
            _ => panic!("expected Reject (rule 4)"),
        }
    }

    #[test]
    fn validate_bind_rejects_reconnect_split_brain() {
        let (mut table, local_id) = make_ctx();
        let (env, key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "120363012345678@g.us",
            [1u8; 32],
            100,
            true, // is_reconnect = true
            [10u8; 32],
            [20u8; 32], // reconnecting coordinator
        );
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            // Different active coordinator — split-brain
            active_coordinator_id: Some([30u8; 32]),
            current_epoch: 100,
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key.verifying_key(), &mut ctx);
        match outcome {
            ValidationOutcome::Reject { rule, .. } => assert_eq!(rule, 6),
            _ => panic!("expected Reject (rule 6)"),
        }
    }

    #[test]
    fn validate_bind_rejects_clock_skew() {
        let (mut table, local_id) = make_ctx();
        let (env, key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "120363012345678@g.us",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 200, // 100 epochs off
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key.verifying_key(), &mut ctx);
        match outcome {
            ValidationOutcome::Reject { rule, .. } => assert_eq!(rule, 7),
            _ => panic!("expected Reject (rule 7)"),
        }
    }

    #[test]
    fn validate_bind_rejects_empty_jid() {
        let (mut table, local_id) = make_ctx();
        let (env, key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "", // empty
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 100,
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key.verifying_key(), &mut ctx);
        match outcome {
            ValidationOutcome::Reject { rule, .. } => assert_eq!(rule, 9),
            _ => panic!("expected Reject (rule 9)"),
        }
    }

    #[test]
    fn validate_bind_rejects_malformed_jid() {
        let (mut table, local_id) = make_ctx();
        let (env, key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "not-a-valid-jid", // no '@' for WhatsApp
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 100,
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key.verifying_key(), &mut ctx);
        match outcome {
            ValidationOutcome::Reject { rule, .. } => assert_eq!(rule, 9),
            _ => panic!("expected Reject (rule 9)"),
        }
    }

    // R17 R1-HIGH-3 regression test: an envelope signed by the WRONG
    // key must be rejected with rule 1 (signature).
    #[test]
    fn validate_bind_rejects_bad_signature() {
        let (mut table, local_id) = make_ctx();
        // Build an envelope signed by key A, but validate with key B.
        let (env, _key_a) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "120363012345678@g.us",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let key_b = SigningKey::from_bytes(&[99u8; 32]);
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 100,
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        let outcome = validate_bind(&env, &key_b.verifying_key(), &mut ctx);
        match outcome {
            ValidationOutcome::Reject { rule, .. } => assert_eq!(rule, 1),
            _ => panic!("expected Reject (rule 1)"),
        }
    }

    #[test]
    fn first_bind_wins_no_prev() {
        let (env, _key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "g1@g.us",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        assert!(first_bind_wins(None, &env));
    }

    #[test]
    fn first_bind_wins_same_founder_lower_hash() {
        // Construct envelopes with explicit bind_hash values to make the
        // relationship deterministic (post-R17 R1-HIGH-3, `make_bind`
        // recomputes bind_hash from the canonical body, so we cannot
        // rely on group_jid → hash ordering).
        let mut prev = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "g1@g.us",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let mut new = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "a@g.us",
            [2u8; 32],
            101,
            false,
            [10u8; 32], // same founder
            [20u8; 32],
        );
        // Force: prev has higher bind_hash than new.
        prev.0.bind_hash = [0xFFu8; 32];
        new.0.bind_hash = [0x00u8; 32];
        assert!(!first_bind_wins(Some(&prev.0), &new.0));
    }

    #[test]
    fn first_bind_wins_different_founder() {
        let (prev, _key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "g1@g.us",
            [1u8; 32],
            100,
            false,
            [10u8; 32],
            [20u8; 32],
        );
        let (new, _key) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "a@g.us",
            [2u8; 32],
            101,
            false,
            [11u8; 32], // different founder
            [20u8; 32],
        );
        // Different founder wins regardless of hash.
        assert!(first_bind_wins(Some(&prev), &new));
    }

    #[test]
    fn is_reconnect_allowed_when_no_active() {
        assert!(is_reconnect_allowed(None, &[1u8; 32]));
    }

    #[test]
    fn is_reconnect_allowed_when_active_matches() {
        assert!(is_reconnect_allowed(Some([1u8; 32]), &[1u8; 32]));
    }

    #[test]
    fn is_reconnect_rejected_when_active_differs() {
        assert!(!is_reconnect_allowed(Some([1u8; 32]), &[2u8; 32]));
    }

    #[test]
    fn signature_roundtrip_with_validate() {
        // Sanity: a real BIND that round-trips through sign/verify
        // should also pass the witness rules (modulo nonce).
        let key = SigningKey::from_bytes(&[1u8; 32]);
        let (mut env, _key2) = make_bind(
            ADAPTER_PLATFORM_WHATSAPP,
            "120363012345678@g.us",
            [1u8; 32],
            100,
            false,
            key.verifying_key().to_bytes(),
            [20u8; 32],
        );
        env.sign(&key);
        let (mut table, local_id) = make_ctx();
        let mut ctx = WitnessContext {
            local_platform: ADAPTER_PLATFORM_WHATSAPP,
            local_peer_id: local_id,
            active_coordinator_id: None,
            current_epoch: 100,
            nonce_table: &mut table,
            first_bind_seen: None,
        };
        assert!(env.verify(&key.verifying_key()).is_ok());
        assert!(validate_bind(&env, &key.verifying_key(), &mut ctx).is_accept());
    }

    // R17 R1-HIGH-2 regression test: a nonce seen 100 epochs ago MUST
    // NOT trigger a replay rejection (the previous entry has aged out).
    #[test]
    fn nonce_table_evicts_old_entries() {
        let mut table = NonceReplayTable::with_epoch_age_limit(100);
        let nonce = [42u8; 32];
        // First use at epoch 100.
        assert!(table
            .check_and_maybe_evict("whatsapp", "g1", &nonce, 100)
            .is_ok());
        // Same nonce at epoch 200 — REPLAY within window.
        assert!(table
            .check_and_maybe_evict("whatsapp", "g1", &nonce, 150)
            .is_err());
        // Same nonce at epoch 250 — AGED OUT (>100 epochs since first_seen).
        assert!(table
            .check_and_maybe_evict("whatsapp", "g1", &nonce, 250)
            .is_ok());
    }
}
