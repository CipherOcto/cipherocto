//! `octo-runtime::handle::key_id` — key rotation discriminator
//! substrate (RFC-0011-c §F.5.1).
//!
//! `KeyId` is a typed version discriminator for the holder's signing
//! key; `KeySet` is a registry of `(key_id -> pubkey)` lookups plus
//! a grace-period acceptance window for in-flight tokens minted
//! during a rotation event.
//!
//! The discriminator mechanism is algorithm-independent (Layer B
//! substrate-faithful per [[cipherocto-design-principles]] §Stable
//! Abstractions Principle): the wire form + lookup logic are
//! forward-compatible regardless of which signature algorithm(s)
//! the `key_set` eventually holds (classic Ed25519 / hybrid
//! Ed25519+PQC / PQC-only). The `key_set` population POLICY (which
//! keys exist, when they rotate, grace-period bounds, PQC algorithm
//! choice) is out of scope for the D2.1 substrate-faithful surface
//! — that lands as a follow-on per RFC-0015-a §6.5 paired-acceptance
//! bridge once PQC direction is known.
//!
//! The `sign_attach_handle_payload_v2` + `verify_attach_handle_payload_v2`
//! functions in `signing.rs` (and the `AttachError::UnknownKeyId`
//! variant in `error.rs`) consume `KeyId` + `KeySet` and are gated
//! on the `octo-attach-key-rotation` Cargo feature; the substrate-
//! faithful baseline (default build) keeps the v1 single-pubkey
//! verify path byte-identical.

use std::collections::{BTreeMap, BTreeSet};

/// Typed version discriminator for a holder signing key
/// (RFC-0011-c §F.5.1).
///
/// `u32` covers 4B key generations; the canonical-acceptance default
/// (RFC-0015-a §6.5 paired-acceptance bridge) uses `key_id == 0` as
/// the bootstrap slot, so a `NonZeroU32` newtype is intentionally
/// not used — the substrate-faithful surface allows the bootstrap
/// slot to be the explicit zero.
pub type KeyId = u32;

/// Registry of `(key_id -> pubkey)` lookups plus a grace-period
/// acceptance window (RFC-0011-c §F.5.1).
///
/// `keys` is the active lookup table; `grace_ids` holds the set of
/// key ids that have been rotated OUT of the active set but whose
/// in-flight tokens must still be accepted during the grace-period
/// window. `verify_attach_handle_payload_v2` consults `keys` first,
/// then `grace_ids` (try-each; each verify is O(1)).
///
/// `key_set` population POLICY is OUT OF SCOPE for the D2.1
/// substrate-faithful surface — see module rustdoc.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeySet {
    /// Active `(key_id, pubkey)` table.
    keys: BTreeMap<KeyId, [u8; 32]>,
    /// Set of rotated-out key ids whose in-flight tokens still
    /// verify during the grace-period window.
    grace_ids: BTreeSet<KeyId>,
}

impl KeySet {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a `(key_id, pubkey)` pair into the
    /// active set.
    ///
    /// If `key_id` was previously in the grace window, it is removed
    /// from grace (re-promotion to the active set).
    pub fn insert(&mut self, key_id: KeyId, pubkey: [u8; 32]) {
        self.grace_ids.remove(&key_id);
        self.keys.insert(key_id, pubkey);
    }

    /// Move `key_id` from the active set into the grace window
    /// (rotation event).
    ///
    /// No-op if `key_id` is unknown. After this call the active
    /// `lookup(key_id)` returns `None`; the grace set retains the
    /// id so `verify_attach_handle_payload_v2` can attempt a
    /// grace-period verify against the same pubkey.
    pub fn move_to_grace(&mut self, key_id: KeyId) {
        self.keys.remove(&key_id);
        self.grace_ids.insert(key_id);
    }

    /// Active lookup. Returns `None` if `key_id` is not in the
    /// active set; the caller decides whether to consult the grace
    /// window (the v2 verifier consults grace automatically).
    #[must_use]
    pub fn lookup(&self, key_id: KeyId) -> Option<&[u8; 32]> {
        self.keys.get(&key_id)
    }

    /// Grace-period key ids in ascending order.
    #[must_use]
    pub fn grace_period(&self) -> Vec<KeyId> {
        self.grace_ids.iter().copied().collect()
    }

    /// Diagnostic union of active + grace key ids (used by
    /// `AttachError::UnknownKeyId.known_keys`).
    #[must_use]
    pub fn known_key_ids(&self) -> Vec<KeyId> {
        self.keys
            .keys()
            .chain(self.grace_ids.iter())
            .copied()
            .collect()
    }

    /// Number of active (non-grace) key ids.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// `true` iff no active key ids are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_set_insert_lookup_roundtrip() {
        let mut ks = KeySet::new();
        let pk = [0xab; 32];
        ks.insert(7, pk);
        assert_eq!(ks.lookup(7), Some(&pk));
        assert_eq!(ks.len(), 1);
    }

    #[test]
    fn key_set_move_to_grace_removes_from_active() {
        let mut ks = KeySet::new();
        ks.insert(1, [0x11; 32]);
        assert_eq!(ks.lookup(1), Some(&[0x11; 32]));
        ks.move_to_grace(1);
        assert_eq!(ks.lookup(1), None);
        assert!(ks.grace_period().contains(&1));
    }

    #[test]
    fn key_set_re_promote_clears_grace() {
        let mut ks = KeySet::new();
        ks.insert(1, [0x11; 32]);
        ks.move_to_grace(1);
        assert!(ks.grace_period().contains(&1));
        ks.insert(1, [0x22; 32]);
        assert!(!ks.grace_period().contains(&1));
        assert_eq!(ks.lookup(1), Some(&[0x22; 32]));
    }

    #[test]
    fn key_set_known_key_ids_includes_both() {
        let mut ks = KeySet::new();
        ks.insert(1, [0x11; 32]);
        ks.insert(2, [0x22; 32]);
        ks.move_to_grace(1);
        let known = ks.known_key_ids();
        assert!(known.contains(&1));
        assert!(known.contains(&2));
        assert_eq!(known.len(), 2);
    }

    #[test]
    fn key_set_empty_lookup_miss() {
        let ks = KeySet::new();
        assert!(ks.is_empty());
        assert_eq!(ks.len(), 0);
        assert_eq!(ks.lookup(0), None);
        assert_eq!(ks.lookup(99), None);
        assert!(ks.grace_period().is_empty());
        assert!(ks.known_key_ids().is_empty());
    }
}
