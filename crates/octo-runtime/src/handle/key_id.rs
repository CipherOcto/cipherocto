//! `octo-runtime::handle::key_id` — key rotation discriminator
//! substrate (RFC-0011-c §F.5.1).
//!
//! `KeyId` is a typed version discriminator for the holder's signing
//! key; `KeySet` is a registry of `(key_id -> pubkey)` lookups plus
//! a grace-period acceptance window for in-flight tokens minted
//! during a rotation event.
//!
//! The discriminator mechanism is algorithm-independent (Layer B
//! substrate-faithful per `docs/architecture/design-principles.md` §Stable
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

#[cfg(feature = "octo-attach-key-rotation")]
use std::collections::BTreeMap;
/// Stable identity of one holder signing key within its DID document.
///
/// **This was a `u32` key-generation counter** and is now RFC-0009's
/// `VerificationMethodId`. They were the same concept expressed twice, which
/// is what made the v2 `key_id` byte suffix possible at all: the token bound a
/// *generation counter the verifier supplied*, while the RFC-0009 prefix binds
/// a *verification method id the token carries*. With the prefix on the token,
/// the counter has no source — a verifier holding only the token cannot know
/// which generation number to guess.
///
/// RFC-0009 §`VerificationMethodId` defines exactly this: "stable identity of
/// one key within one DID document", fixed width, and rotatable independently
/// of the key material. A monotonic counter cannot express that property — it
/// says *when* a key was issued, not *which* key — so the counter form could
/// not actually state what the grace window needs.
///
/// The name is kept so the `KeySet` API reads unchanged.
///
/// ALGORITHM-INDEPENDENCE NOTE: `KeySet.keys` / `KeySet.grace_keys` storage
/// still hardcodes a 32-byte pubkey size — the substrate-faithful minimum tied
/// to the current Layer A Ed25519 primitive. Variable-size pubkey storage (PQC
/// migration) is unchanged and remains open.
pub type KeyId = octo_wallet::VerificationMethodId;

/// Registry of `(key_id -> pubkey)` lookups plus a grace-period
/// acceptance window (RFC-0011-c §F.5.1 + RFC-0015-a §6.5
/// paired-acceptance bridge).
///
/// `keys` is the active lookup table; `grace_keys` holds the set of
/// key ids (plus their pubkeys) that have been rotated OUT of the
/// active set but whose in-flight tokens must still be accepted
/// during the grace-period window. The pubkey is retained in the
/// grace map so the grace fallback in
/// `verify_attach_handle_payload_v2` can attempt a per-pubkey
/// verify against the rotated-out key matching the CLAIMED `key_id`
/// (the standard rotation case: same key_id, pubkey changed).
///
/// `key_set` population POLICY is OUT OF SCOPE for the D2.1
/// substrate-faithful surface — see module rustdoc.
#[cfg(feature = "octo-attach-key-rotation")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeySet {
    /// Active `(key_id, pubkey)` table.
    keys: BTreeMap<KeyId, [u8; 32]>,
    /// Rotated-out pubkeys per `key_id`, retained so in-flight tokens still
    /// verify during the grace window.
    ///
    /// A **list**, not a single pubkey. With `KeyId` now being a stable
    /// verification method id rather than a generation counter, a rotation
    /// keeps the same id and changes the key — so one id can have several
    /// retired pubkeys over its life. The previous single-slot map could not
    /// represent that, and `insert` actively cleared grace on every write,
    /// which meant the "same key_id, pubkey rotated" case its own docs called
    /// *the standard rotation case* was unreachable.
    grace_keys: BTreeMap<KeyId, Vec<[u8; 32]>>,
}

#[cfg(feature = "octo-attach-key-rotation")]
impl KeySet {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or overwrite) a `(key_id, pubkey)` pair into the active set.
    ///
    /// **Grace entries are preserved.** Clearing them here would make a
    /// rotation unrepresentable: `move_to_grace(K)` then `insert(K, new)` is
    /// exactly how a key rotates under a stable verification method id, and
    /// the old code threw away the retired pubkey on the second call. Use
    /// [`KeySet::retire_grace_keys`] to drop grace entries deliberately.
    pub fn insert(&mut self, key_id: KeyId, pubkey: [u8; 32]) {
        self.keys.insert(key_id, pubkey);
    }

    /// Drop the grace entries for `key_id` (or every key when `None`).
    ///
    /// Explicit because [`KeySet::insert`] no longer implies it — the grace
    /// window ends when the operator says so, not as a side effect of
    /// publishing a new key.
    pub fn retire_grace_keys(&mut self, key_id: Option<KeyId>) {
        match key_id {
            Some(id) => {
                self.grace_keys.remove(&id);
            }
            None => self.grace_keys.clear(),
        }
    }

    /// Move `key_id` from the active set into the grace window
    /// (rotation event).
    ///
    /// No-op if `key_id` is unknown to the active set (the pubkey
    /// must already be in `keys`). After this call the active
    /// `lookup(key_id)` returns `None`; the grace map retains the
    /// pubkey so `verify_attach_handle_payload_v2` can attempt a
    /// grace-period verify against the same pubkey.
    pub fn move_to_grace(&mut self, key_id: KeyId) {
        if let Some(pk) = self.keys.remove(&key_id) {
            let slot = self.grace_keys.entry(key_id).or_default();
            if !slot.contains(&pk) {
                slot.push(pk);
            }
        }
    }

    /// Active lookup. Returns `None` if `key_id` is not in the
    /// active set; the caller decides whether to consult the grace
    /// window (the v2 verifier consults grace automatically).
    #[must_use]
    pub fn lookup(&self, key_id: KeyId) -> Option<&[u8; 32]> {
        self.keys.get(&key_id)
    }

    /// Grace-window lookup for the CLAIMED `key_id`. Returns
    /// `None` if `key_id` is not in the grace window. The
    /// `verify_attach_handle_payload_v2` grace fallback consults
    /// ONLY the grace entry matching the claimed `key_id` (never
    /// iterates every grace pubkey) — this binds the signature to
    /// a specific key per the typed-discriminator principle.
    #[must_use]
    pub fn grace_key(&self, key_id: KeyId) -> Option<&[u8; 32]> {
        self.grace_keys.get(&key_id).and_then(|v| v.first())
    }

    /// Every retired pubkey retained for `key_id`.
    ///
    /// The v2 verifier tries each of these against a token naming
    /// `key_id`. Scoping the walk to the token's *own* id is what keeps this
    /// safe: iterating grace pubkeys across **all** ids would let the holder
    /// of any one retired key forge a token claiming another.
    pub fn grace_keys_for(&self, key_id: KeyId) -> &[[u8; 32]] {
        self.grace_keys.get(&key_id).map_or(&[][..], Vec::as_slice)
    }

    /// Grace-period key ids in ascending order (diagnostic; not
    /// used by the verify path after the R1.5 grace-fix that
    /// scopes fallback to the claimed `key_id`).
    #[allow(dead_code)] // diagnostic; tests + future KeySet population policy per RFC-0015-a §6.5
    #[must_use]
    pub fn grace_period(&self) -> Vec<KeyId> {
        self.grace_keys.keys().copied().collect()
    }

    /// Number of retired pubkeys retained across every key id.
    #[allow(dead_code)] // diagnostic; grace-window bounds policy is RFC-0015-a §6.5
    #[must_use]
    pub fn grace_key_count(&self) -> usize {
        self.grace_keys.values().map(Vec::len).sum()
    }

    /// Diagnostic union of active + grace key ids (used by
    /// `AttachError::UnknownKeyId.known_keys`).
    #[must_use]
    pub fn known_key_ids(&self) -> Vec<KeyId> {
        let mut ids: Vec<KeyId> = self
            .keys
            .keys()
            .chain(self.grace_keys.keys())
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
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

#[cfg(all(test, feature = "octo-attach-key-rotation"))]
mod tests {
    use super::*;

    fn k(n: u8) -> octo_wallet::VerificationMethodId {
        octo_wallet::VerificationMethodId::derive(&format!("key-{n}"))
    }

    #[test]
    fn key_set_insert_lookup_roundtrip() {
        let mut ks = KeySet::new();
        let pk = [0xab; 32];
        ks.insert(k(7), pk);
        assert_eq!(ks.lookup(k(7)), Some(&pk));
        assert_eq!(ks.len(), 1);
    }

    #[test]
    fn key_set_move_to_grace_removes_from_active() {
        let mut ks = KeySet::new();
        ks.insert(k(1), [0x11; 32]);
        assert_eq!(ks.lookup(k(1)), Some(&[0x11; 32]));
        ks.move_to_grace(k(1));
        assert_eq!(ks.lookup(k(1)), None);
        assert!(ks.grace_period().contains(&k(1)));
    }

    /// Publishing a new pubkey under an existing id must **keep** the retired
    /// one, because that is precisely what a rotation is now: the id is
    /// stable and the key changes.
    ///
    /// This inverts the previous contract, where `insert` cleared grace on
    /// every write. That made a rotation unrepresentable — `move_to_grace(K)`
    /// followed by `insert(K, new)` is exactly the sequence, and the old
    /// `insert` discarded the retired key on the second call, so the
    /// grace window could never hold anything and never accepted anything.
    #[test]
    fn publishing_a_new_key_keeps_the_retired_one() {
        let mut ks = KeySet::new();
        ks.insert(k(1), [0x11; 32]);
        ks.move_to_grace(k(1));
        ks.insert(k(1), [0x22; 32]);

        assert_eq!(ks.lookup(k(1)), Some(&[0x22; 32]), "new key is active");
        assert_eq!(
            ks.grace_keys_for(k(1)),
            &[[0x11; 32]],
            "the retired key must still verify in-flight tokens"
        );

        // Grace now ends explicitly rather than as a side effect.
        ks.retire_grace_keys(Some(k(1)));
        assert!(ks.grace_keys_for(k(1)).is_empty());
        assert_eq!(
            ks.lookup(k(1)),
            Some(&[0x22; 32]),
            "active key is untouched"
        );
    }

    /// Several rotations under one id must accumulate, not overwrite: a key
    /// that has been rotated twice has two retired pubkeys, and a token
    /// signed under either is in flight.
    #[test]
    fn repeated_rotations_accumulate_grace_entries() {
        let mut ks = KeySet::new();
        ks.insert(k(1), [0x11; 32]);
        ks.move_to_grace(k(1));
        ks.insert(k(1), [0x22; 32]);
        ks.move_to_grace(k(1));
        ks.insert(k(1), [0x33; 32]);

        assert_eq!(ks.lookup(k(1)), Some(&[0x33; 32]));
        assert_eq!(ks.grace_keys_for(k(1)), &[[0x11; 32], [0x22; 32]]);
        assert_eq!(ks.grace_key_count(), 2);
    }

    #[test]
    fn key_set_known_key_ids_includes_both() {
        let mut ks = KeySet::new();
        ks.insert(k(1), [0x11; 32]);
        ks.insert(k(2), [0x22; 32]);
        ks.move_to_grace(k(1));
        let known = ks.known_key_ids();
        assert!(known.contains(&k(1)));
        assert!(known.contains(&k(2)));
        assert_eq!(known.len(), 2);
    }

    #[test]
    fn key_set_empty_lookup_miss() {
        let ks = KeySet::new();
        assert!(ks.is_empty());
        assert_eq!(ks.len(), 0);
        assert_eq!(ks.lookup(k(0)), None);
        assert_eq!(ks.lookup(k(99)), None);
        assert!(ks.grace_period().is_empty());
        assert!(ks.known_key_ids().is_empty());
    }
}
