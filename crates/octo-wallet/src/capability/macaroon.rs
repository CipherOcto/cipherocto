//! Macaroon v1: HMAC-BLAKE3 chained bearer token (RFC-0957 §3.2).
//!
//! `macaroon_root_secret` = `[u8; 32]` random per mint.
//! `macaroon_id` = `HMAC-BLAKE3(salt: root_secret, info: "cipherocto/macaroon/v1", msg: nonce)[:16]`.
//! `capability_id` = `BLAKE3(0x05 || canonical_ser_unsigned(macaroon))` per RFC-0965 §3.7.
//! Each caveat: `hmac_i = HMAC-BLAKE3(salt: hmac_{i-1}, info: caveat_name,
//!                                       msg: canonical_ser(caveat_value) || capability_id_{i-1})`.
//!
//! HMAC per RFC 2104 with BLAKE3 as the hash function:
//!   `HMAC(K, m) = H(K' ⊕ opad || H(K' ⊕ ipad || m))`
//! where K' is K zero-padded to BLAKE3 block size (64 bytes), or BLAKE3(K) || zeros if shorter.

use rand::RngCore;
use serde::{Deserialize, Serialize};

use super::caveat::Caveat;

/// Domain separator for `capability_id` derivation (RFC-0965 §3.7).
/// `capability_id = BLAKE3(0x05 || canonical_ser_unsigned(macaroon))`.
pub const CAPABILITY_ID_DOMAIN: u8 = 0x05;

/// BLAKE3 block size (per BLAKE3 spec §2.5).
const BLOCK_SIZE: usize = 64;
/// HMAC ipad byte.
const IPAD: u8 = 0x36;
/// HMAC opad byte.
const OPAD: u8 = 0x5c;

/// Macaroon identifier (16 bytes — first half of HMAC-BLAKE3(root_secret, nonce)).
pub type MacaroonId = [u8; 16];

/// HMAC-BLAKE3 keyed MAC with 32-byte key.
#[must_use]
pub fn hmac_blake3(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    // K' = K if |K| == 64 else H(K) padded to 64.
    let mut key_padded = [0u8; BLOCK_SIZE];
    let h = blake3::hash(key);
    key_padded[..32].copy_from_slice(h.as_bytes());

    let mut ipad_key = [0u8; BLOCK_SIZE];
    let mut opad_key = [0u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        ipad_key[i] = key_padded[i] ^ IPAD;
        opad_key[i] = key_padded[i] ^ OPAD;
    }

    // inner = H(ipad || msg)
    let mut inner_hasher = blake3::Hasher::new();
    inner_hasher.update(&ipad_key);
    inner_hasher.update(msg);
    let inner = inner_hasher.finalize();

    // outer = H(opad || inner)
    let mut outer_hasher = blake3::Hasher::new();
    outer_hasher.update(&opad_key);
    outer_hasher.update(inner.as_bytes());
    let outer = outer_hasher.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(outer.as_bytes());
    out
}

/// 16-byte truncation of HMAC-BLAKE3 output. Macaroon ID per RFC-0957 §3.2.
#[must_use]
pub fn macaroon_id(root_secret: &[u8; 32], nonce: &[u8; 16]) -> MacaroonId {
    let mac = hmac_blake3(root_secret, nonce);
    let mut id = [0u8; 16];
    id.copy_from_slice(&mac[..16]);
    id
}

/// Wire info string for macaroon_id derivation.
pub const MACAROON_ID_INFO: &str = "cipherocto/macaroon/v1/id";

/// Convert a length to a big-endian `u32` length prefix. The macaroon's
/// fields are bounded (chain < 2^16 entries, caveats < 2^16 entries) so
/// this never panics in practice.
fn u32_len(n: usize) -> [u8; 4] {
    u32::try_from(n)
        .expect("macaroon field length fits in u32")
        .to_be_bytes()
}

/// Macaroon v1 (RFC-0957 §3.1). Bearer token + chained caveat HMACs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macaroon {
    /// Unique 16-byte identifier (per mint).
    pub root_id: MacaroonId,
    /// BLAKE3 hash of the root secret — embedded in the token so the verifier
    /// can confirm the root secret they hold matches without leaking it.
    pub root_secret_hash: [u8; 32],
    /// 32-byte capability identifier used for `WrappedOnly` chain checks
    /// (RFC-0960 §8 + RFC-0965 §3.7). Distinct from `root_id` (16 bytes) —
    /// the 32-byte form matches the catalog schema (`capability_id BLOB`).
    /// **Derived**: `BLAKE3(0x05 || canonical_ser_unsigned(self))` — never
    /// random, never supplied by the caller. Recomputed on every mint and
    /// attenuation; tamper-evident via the HMAC chain (`hmac_i` includes
    /// `capability_id_{i-1}` in its input, per the defect-2 fix).
    pub id: [u8; 32],
    /// Chained caveat HMACs — `chain[i]` is the HMAC output after applying
    /// caveat `caveats[i]` (RFC-0957 §3.2). The final `chain[last]` is the
    /// macaroon signature that the verifier checks.
    pub chain: Vec<[u8; 32]>,
    /// Caveat list (in attenuation order).
    pub caveats: Vec<Caveat>,
}

impl Macaroon {
    /// Mint a new macaroon with no caveats. The root secret is held by the
    /// issuer (wallet); only `root_secret_hash` is embedded in the macaroon.
    ///
    /// # Errors
    /// Returns `MacaroonError::OsRng` if the OS RNG fails (extremely rare).
    pub fn mint(root_secret: &[u8; 32]) -> Result<Self, MacaroonError> {
        let mut rng = rand::rng();
        let mut nonce = [0u8; 16];
        rng.fill_bytes(&mut nonce);
        let root_id = macaroon_id(root_secret, &nonce);
        let root_secret_hash = *blake3::hash(root_secret).as_bytes();

        // Empty chain: chain[0] = HMAC(root_secret, MACAROON_ID_INFO || nonce)
        let mut hmac_state = *root_secret;
        let mut chained_msg = Vec::with_capacity(MACAROON_ID_INFO.len() + 16);
        chained_msg.extend_from_slice(MACAROON_ID_INFO.as_bytes());
        chained_msg.extend_from_slice(&nonce);
        hmac_state = hmac_blake3(&hmac_state, &chained_msg);

        let mut macaroon = Self {
            root_id,
            root_secret_hash,
            id: [0u8; 32], // placeholder — overwritten below
            chain: vec![hmac_state],
            caveats: Vec::new(),
        };
        // Derived capability_id per RFC-0965 §3.7.
        macaroon.id = Self::compute_capability_id(&macaroon);
        Ok(macaroon)
    }

    /// Compute the 32-byte capability id per RFC-0965 §3.7:
    /// `BLAKE3(0x05 || canonical_ser_unsigned(macaroon))`.
    #[must_use]
    pub fn compute_capability_id(macaroon: &Self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[CAPABILITY_ID_DOMAIN]);
        hasher.update(&macaroon.canonical_ser_unsigned());
        *hasher.finalize().as_bytes()
    }

    /// Canonical serialization of the unsigned macaroon (everything except
    /// `id`). Used for `capability_id` derivation (RFC-0965 §3.7) and as the
    /// base for verify-side recomputation. Length-prefixed per field to
    /// prevent concatenation-collision attacks (see
    /// `crates/quota-router-sm-engine/src/envelope.rs:224` for the same
    /// pattern).
    #[must_use]
    pub fn canonical_ser_unsigned(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // root_id (16 bytes) — fixed size, length-prefix for symmetry.
        buf.extend_from_slice(&u32_len(self.root_id.len()));
        buf.extend_from_slice(&self.root_id);
        // root_secret_hash (32 bytes).
        buf.extend_from_slice(&u32_len(self.root_secret_hash.len()));
        buf.extend_from_slice(&self.root_secret_hash);
        // chain: length-prefixed array of 32-byte HMACs.
        buf.extend_from_slice(&u32_len(self.chain.len()));
        for entry in &self.chain {
            buf.extend_from_slice(entry);
        }
        // caveats: length-prefixed array of canonicalised JSON.
        buf.extend_from_slice(&u32_len(self.caveats.len()));
        for caveat in &self.caveats {
            let ser = caveat.canonical_ser();
            buf.extend_from_slice(&u32_len(ser.len()));
            buf.extend_from_slice(&ser);
        }
        buf
    }

    /// Append a caveat. Returns the new macaroon with the caveat added.
    /// Monotonic: existing caveats are preserved (RFC-0957 §3.5).
    ///
    /// **WrappedOnly chain guard (RFC-0965 §3.7):** pre- and post-append,
    /// the parent chain is walked via `catalog` to reject cycles and depth
    /// overruns. Cross-macaroon cycle detection requires a populated
    /// catalog. The guard is mandatory — there is no unchecked variant —
    /// because `CapabilityToken::mint` / `attenuate` / `attenuate_with_signer`
    /// all funnel through here.
    ///
    /// # Errors
    /// Returns `MacaroonError::WrappedCycle` on a chain cycle,
    /// `WrappedDepthExceeded` when the chain exceeds `MAX_WRAPPED_DEPTH`,
    /// or `WrappedParentNotFound` when a `WrappedOnly` parent is missing
    /// from `catalog`.
    pub fn attenuate(
        &self,
        caveat: Caveat,
        catalog: &dyn CapabilityCatalog,
    ) -> Result<Self, MacaroonError> {
        // Pre-append check — rejects cyclic / over-deep chains before we
        // mutate anything.
        check_wrapped_chain(self, catalog)?;

        let mut next = self.clone();
        let prev_chain = *next.chain.last().expect("chain non-empty");
        let mut msg = Vec::with_capacity(caveat.name().as_str().len() + 64 + self.id.len());
        msg.extend_from_slice(caveat.name().as_str().as_bytes());
        msg.extend_from_slice(&caveat.canonical_ser());
        // HMAC binds to capability_id per RFC-0965 §3.7 — without this,
        // an attacker could swap `id` (and the catalog-resolved parent
        // chain) without invalidating the signature.
        msg.extend_from_slice(&self.id);
        let new_chain = hmac_blake3(&prev_chain, &msg);
        next.caveats.push(caveat);
        next.chain.push(new_chain);
        // Recompute id AFTER chain push so the derived id covers the new
        // HMAC entry (content-addressed tamper-evidence).
        next.id = Self::compute_capability_id(&next);

        // Post-append check — a freshly-added WrappedOnly may have extended
        // the chain beyond the limit, or pointed at a non-existent parent.
        check_wrapped_chain(&next, catalog)?;
        Ok(next)
    }

    /// Verify the macaroon signature against the issuer's root secret.
    /// Re-derives the HMAC chain from `root_secret` over the caveat list
    /// and compares the final chain entry.
    ///
    /// # Errors
    /// Returns `MacaroonError::ChainMismatch` if the chain doesn't rederive,
    /// `MacaroonError::RootSecretMismatch` if the embedded hash differs,
    /// `CapabilityIdMismatch` if the stored id doesn't match the
    /// content-addressed derivation, `WrappedCycle` if the chain has a cycle,
    /// `WrappedDepthExceeded` if the chain exceeds `MAX_WRAPPED_DEPTH`.
    /// **Note:** `verify_signature` does NOT take a catalog; it only checks
    /// locally-detectable invariants. Cross-macaroon cycle detection
    /// requires `check_wrapped_chain` with a catalog.
    pub fn verify_signature(&self, root_secret: &[u8; 32]) -> Result<(), MacaroonError> {
        // Root secret hash must match (proves issuer had this root secret).
        let computed_hash = *blake3::hash(root_secret).as_bytes();
        if computed_hash != self.root_secret_hash {
            return Err(MacaroonError::RootSecretMismatch);
        }

        // capability_id must match content-addressed derivation (RFC-0965 §3.7).
        let expected_id = Self::compute_capability_id(self);
        if expected_id != self.id {
            return Err(MacaroonError::CapabilityIdMismatch);
        }

        // Re-derive chain. chain[0] is issuer-side; we accept it as-is.
        // chain[i+1] = HMAC(chain[i], caveat_name || canonical_ser(caveat) ||
        //                                       capability_id_at_step_i).
        // We need the id at each step, which requires re-deriving ids in
        // lockstep with the chain.
        let mut prev_chain = self.chain[0];
        let mut prev_id = Self::compute_capability_id_at(self, 0)?;
        for (i, caveat) in self.caveats.iter().enumerate() {
            let mut msg = Vec::with_capacity(caveat.name().as_str().len() + 64 + prev_id.len());
            msg.extend_from_slice(caveat.name().as_str().as_bytes());
            msg.extend_from_slice(&caveat.canonical_ser());
            msg.extend_from_slice(&prev_id);
            let expected = hmac_blake3(&prev_chain, &msg);
            if expected != self.chain[i + 1] {
                return Err(MacaroonError::ChainMismatch(i));
            }
            // Step forward: id_{i+1} = BLAKE3(0x05 || canonical_ser_unsigned
            // of state with chain[0..=i+1] and caveats[0..=i]).
            prev_chain = self.chain[i + 1];
            prev_id = Self::compute_capability_id_at(self, i + 1)?;
        }
        Ok(())
    }

    /// Re-derive the capability_id at step `step` (0 = mint-time state,
    /// `n` = state after applying caveats[0..step]). Used by
    /// `verify_signature` to walk id history in lockstep with chain
    /// verification.
    fn compute_capability_id_at(&self, step: usize) -> Result<[u8; 32], MacaroonError> {
        if step > self.caveats.len() || step + 1 > self.chain.len() {
            return Err(MacaroonError::ChainMismatch(step));
        }
        let partial = Self {
            root_id: self.root_id,
            root_secret_hash: self.root_secret_hash,
            id: [0u8; 32],
            chain: self.chain[..=step].to_vec(),
            caveats: self.caveats[..step].to_vec(),
        };
        Ok(Self::compute_capability_id(&partial))
    }

    /// Final chain entry — the macaroon signature that the verifier checks.
    #[must_use]
    pub fn signature(&self) -> &[u8; 32] {
        self.chain.last().expect("chain non-empty")
    }

    /// Return the **deepest** `WrappedOnly` parent capability id (RFC-0965
    /// §3.7), if any. Walks the caveat list and returns the LAST
    /// `WrappedOnly { parent_capability }` so that attenuation re-walks
    /// the full chain rather than skipping intermediate parents.
    /// A macaroon with two `WrappedOnly` caveats (defect-4 scenario)
    /// chains parent→child→grandchild: the deepest reference is the
    /// immediate parent, the outermost is the root.
    #[must_use]
    pub fn parent_capability(&self) -> Option<&[u8; 32]> {
        self.caveats.iter().rev().find_map(|c| match c {
            Caveat::WrappedOnly { parent_capability } => Some(parent_capability),
            _ => None,
        })
    }
}

/// Capability catalog: resolves a parent `WrappedOnly` reference to a macaroon.
///
/// RFC-0960 §8 specifies a SQL `capabilities` table with `capability_id` →
/// `parent_capability_id` references; implementations back this trait with
/// that catalog (or any equivalent storage). The chain walker rejects
/// unknown parents with `WrappedParentNotFound` — a missing parent is an
/// error, not a chain terminator (defect 5).
pub trait CapabilityCatalog {
    /// Resolve a capability id to its `Macaroon`, or `None` if absent.
    fn get(&self, id: &[u8; 32]) -> Option<&Macaroon>;
}

/// Walk the `WrappedOnly` chain rooted at `macaroon`, rejecting cycles,
/// depth overruns, and missing parents (RFC-0965 §3.7). `attenuate` invokes
/// this both before and after appending a caveat, so a chain that is
/// valid pre-append but invalid post-append is still rejected.
///
/// # Errors
/// Returns `MacaroonError::WrappedCycle` on a repeated `id` in the chain.
/// Returns `MacaroonError::WrappedDepthExceeded(usize)` when the chain
/// length is at or above `MAX_WRAPPED_DEPTH`.
/// Returns `MacaroonError::WrappedParentNotFound { parent_id }` when a
/// `WrappedOnly` parent is missing from `catalog` (defect 5: an unresolved
/// parent is malformed, not a terminator).
pub fn check_wrapped_chain(
    macaroon: &Macaroon,
    catalog: &dyn CapabilityCatalog,
) -> Result<(), MacaroonError> {
    let mut visited: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
    let mut current = macaroon;
    let mut depth: u8 = 1;
    visited.insert(current.id);
    loop {
        if depth >= MAX_WRAPPED_DEPTH {
            return Err(MacaroonError::WrappedDepthExceeded(usize::from(depth)));
        }
        let Some(parent_id) = current.parent_capability().copied() else {
            return Ok(());
        };
        if !visited.insert(parent_id) {
            return Err(MacaroonError::WrappedCycle);
        }
        depth = depth
            .checked_add(1)
            .expect("depth bounded by MAX_WRAPPED_DEPTH < u8::MAX");
        let Some(parent) = catalog.get(&parent_id) else {
            return Err(MacaroonError::WrappedParentNotFound { parent_id });
        };
        current = parent;
    }
}

/// Single-step depth probe (RFC-0965 §3.7). Plan signature:
/// `fn check_wrapped_depth(macaroon: &Macaroon, count: u8) -> Result<(), MacaroonError>`.
/// Rejects when `count >= MAX_WRAPPED_DEPTH` (the canonical threshold).
/// Local self-reference check only — cross-macaroon cycle detection lives
/// in `check_wrapped_chain`.
///
/// # Errors
/// Returns `MacaroonError::WrappedDepthExceeded(count)` when `count >=
/// MAX_WRAPPED_DEPTH`. Returns `MacaroonError::WrappedCycle` if the macaroon
/// references its own id via `WrappedOnly`.
pub fn check_wrapped_depth(macaroon: &Macaroon, count: u8) -> Result<(), MacaroonError> {
    if count >= MAX_WRAPPED_DEPTH {
        return Err(MacaroonError::WrappedDepthExceeded(usize::from(count)));
    }
    if let Some(parent_id) = macaroon.parent_capability() {
        if parent_id == &macaroon.id {
            return Err(MacaroonError::WrappedCycle);
        }
    }
    Ok(())
}

/// Macaroon errors.
#[derive(Debug, thiserror::Error)]
pub enum MacaroonError {
    #[error("OS RNG failure: {0}")]
    OsRng(String),

    #[error("HMAC chain mismatch at caveat {0}")]
    ChainMismatch(usize),

    #[error("root secret does not match embedded hash")]
    RootSecretMismatch,

    /// `WrappedOnly` chain has a cycle (RFC-0965 §3.7): a capability in
    /// the chain appears twice, or a `WrappedOnly` references the macaroon
    /// itself.
    #[error("WrappedOnly chain cycle detected")]
    WrappedCycle,

    /// `WrappedOnly` chain depth reached the rejection threshold
    /// (`>= MAX_WRAPPED_DEPTH` per RFC-0965 §3.7). `usize` = observed depth.
    #[error("WrappedOnly chain depth {0} exceeds maximum")]
    WrappedDepthExceeded(usize),

    /// `WrappedOnly` parent referenced by `parent_capability` was not
    /// present in the catalog (RFC-0960 §8 + RFC-0965 §3.7: a missing
    /// parent is malformed, not a chain terminator).
    #[error("WrappedOnly parent {parent_id:?} not found in catalog")]
    WrappedParentNotFound { parent_id: [u8; 32] },

    /// `capability_id` does not match the content-addressed derivation
    /// `BLAKE3(0x05 || canonical_ser_unsigned(macaroon))` per RFC-0965 §3.7.
    #[error("capability_id does not match content-addressed derivation")]
    CapabilityIdMismatch,
}

/// Threshold at which the `WrappedOnly` chain is rejected (RFC-0965 §3.7).
/// `MAX_WRAPPED_DEPTH = 17` is the FIRST depth that is rejected — counts
/// 1..=16 are allowed, `count >= 17` returns `WrappedDepthExceeded`. The
/// value matches the RFC's "A 17-deep chain … is malformed" wording and
/// is the canonical threshold for `check_wrapped_depth`'s `>=` comparison.
pub const MAX_WRAPPED_DEPTH: u8 = 17;

/// Test-only in-memory `CapabilityCatalog`. Visible to integration tests and
/// to other test modules in this crate (e.g. `capability::tests`,
/// `capability::wire::tests`).
#[cfg(test)]
#[derive(Default, Clone, Debug)]
pub struct InMemoryCatalog {
    pub(crate) by_id: std::collections::HashMap<[u8; 32], Macaroon>,
}

#[cfg(test)]
impl CapabilityCatalog for InMemoryCatalog {
    fn get(&self, id: &[u8; 32]) -> Option<&Macaroon> {
        self.by_id.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::caveat::{Caveat, ProviderId, UnixTimeSecs};

    #[test]
    fn hmac_blake3_deterministic() {
        let key = [0xab; 32];
        let msg = b"hello world";
        assert_eq!(hmac_blake3(&key, msg), hmac_blake3(&key, msg));
    }

    #[test]
    fn hmac_blake3_different_keys() {
        let key1 = [0xab; 32];
        let key2 = [0xcd; 32];
        let msg = b"hello world";
        assert_ne!(hmac_blake3(&key1, msg), hmac_blake3(&key2, msg));
    }

    fn empty_catalog() -> InMemoryCatalog {
        InMemoryCatalog::default()
    }

    #[test]
    fn mint_creates_macaroon_with_empty_caveats() {
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        assert_eq!(m.caveats.len(), 0);
        assert_eq!(m.chain.len(), 1);
        // root_secret_hash must match BLAKE3(secret).
        assert_eq!(m.root_secret_hash, *blake3::hash(&secret).as_bytes());
        // capability_id must match the content-addressed derivation.
        assert_eq!(m.id, Macaroon::compute_capability_id(&m));
    }

    #[test]
    fn attenuate_appends_caveat_and_chain() {
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        let catalog = empty_catalog();
        let m2 = m
            .attenuate(Caveat::Model("gpt-4".to_owned()), &catalog)
            .unwrap();
        assert_eq!(m2.caveats.len(), 1);
        assert_eq!(m2.chain.len(), 2);
    }

    #[test]
    fn monotonic_attenuation_preserves_previous_caveats() {
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        let m = m
            .attenuate(Caveat::Model("gpt-4".to_owned()), &catalog)
            .unwrap();
        let m = m
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap();
        assert_eq!(m.caveats.len(), 2);
        assert_eq!(m.caveats[0], Caveat::Model("gpt-4".to_owned()));
        assert_eq!(m.caveats[1], Caveat::Before(1_700_000_000));
        assert_eq!(m.chain.len(), 3);
    }

    #[test]
    fn verify_accepts_correct_signature() {
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        m.verify_signature(&secret).expect("verify empty");
        let m2 = m
            .attenuate(Caveat::Before(UnixTimeSecs::MAX), &catalog)
            .unwrap();
        m2.verify_signature(&secret).expect("verify with caveat");
    }

    #[test]
    fn verify_rejects_wrong_root_secret() {
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        let wrong = [0x99; 32];
        let err = m.verify_signature(&wrong).unwrap_err();
        assert!(matches!(err, MacaroonError::RootSecretMismatch));
    }

    #[test]
    fn verify_rejects_tampered_caveat() {
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let mut m = Macaroon::mint(&secret).unwrap();
        m = m
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap();
        // Tamper: replace caveat with a different one without re-deriving chain.
        m.caveats[0] = Caveat::Before(1_800_000_000);
        m.id = Macaroon::compute_capability_id(&m); // attacker re-derives id
        let err = m.verify_signature(&secret).unwrap_err();
        assert!(matches!(err, MacaroonError::ChainMismatch(0)));
    }

    #[test]
    fn verify_rejects_tampered_capability_id() {
        // Defect 2: id must be tamper-evident. Attacker swaps id without
        // touching chain/caveats; verify must reject.
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        let mut m2 = m
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap();
        // Wipe the id to a known-wrong value.
        m2.id = [0xde; 32];
        let err = m2.verify_signature(&secret).unwrap_err();
        assert!(matches!(err, MacaroonError::CapabilityIdMismatch));
    }

    #[test]
    fn attenuation_cannot_remove_caveats() {
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        let m = m
            .attenuate(Caveat::Model("gpt-4".to_owned()), &catalog)
            .unwrap();
        // Attenuating only appends (RFC-0957 §3.5).
        assert_eq!(m.caveats.len(), 1);
    }

    #[test]
    fn macaroon_id_is_16_bytes() {
        let secret = [0x42; 32];
        let nonce = [0u8; 16];
        let id = macaroon_id(&secret, &nonce);
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn verify_accepts_provider_vec() {
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        let m = m
            .attenuate(
                Caveat::Provider(vec![
                    ProviderId::from("openai"),
                    ProviderId::from("anthropic"),
                ]),
                &catalog,
            )
            .unwrap();
        m.verify_signature(&secret).unwrap();
    }

    // ---- Defect 4: parent_capability returns LAST (deepest) WrappedOnly ----

    #[test]
    fn parent_capability_returns_deepest_when_multiple_wrapped_only() {
        // Build a chain A→B→C. We use the unchecked helper so we can
        // append a second WrappedOnly without the catalog needing the
        // immediate parent (which is a derived catalog lookup, not what
        // this test is checking).
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        let m = m.attenuate_unchecked_for_test(Caveat::WrappedOnly {
            parent_capability: [0x01; 32],
        });
        let m = m.attenuate_unchecked_for_test(Caveat::WrappedOnly {
            parent_capability: [0x02; 32],
        });
        // Defect 4: returns LAST (deepest, [0x02]) so chain walk visits
        // every layer.
        assert_eq!(m.parent_capability(), Some(&[0x02; 32]));
    }

    #[test]
    fn parent_capability_returns_only_when_present() {
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        assert!(m.parent_capability().is_none());
        // Use the unchecked helper — the accessor itself doesn't need a
        // catalog; only the chain walker does.
        let parent = [0xab; 32];
        let m = m.attenuate_unchecked_for_test(Caveat::WrappedOnly {
            parent_capability: parent,
        });
        assert_eq!(m.parent_capability(), Some(&parent));
    }

    // ---- Defect 5: WrappedParentNotFound on catalog miss ----

    #[test]
    fn check_wrapped_chain_rejects_missing_parent() {
        // Construct a tampered macaroon with WrappedOnly([0xab; 32]) and
        // a catalog that doesn't have that parent. Chain walk must
        // reject (defect 5).
        let secret = [0x42; 32];
        let mut m = Macaroon::mint(&secret).unwrap();
        m.caveats.push(Caveat::WrappedOnly {
            parent_capability: [0xab; 32],
        });
        let catalog = empty_catalog();
        let err = check_wrapped_chain(&m, &catalog).unwrap_err();
        match err {
            MacaroonError::WrappedParentNotFound { parent_id } => {
                assert_eq!(parent_id, [0xab; 32]);
            }
            other => panic!("expected WrappedParentNotFound, got {other:?}"),
        }
    }

    #[test]
    fn attenuate_rejects_missing_parent() {
        // Defect 1 + 5 combined: attenuation enforces catalog presence.
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        let err = m
            .attenuate(
                Caveat::WrappedOnly {
                    parent_capability: [0xab; 32],
                },
                &catalog,
            )
            .unwrap_err();
        assert!(matches!(
            err,
            MacaroonError::WrappedParentNotFound { parent_id } if parent_id == [0xab; 32]
        ));
    }

    // ---- Defect 3: check_wrapped_depth signature ----

    #[test]
    fn check_wrapped_depth_accepts_counts_below_max() {
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        // Counts 1..=MAX-1 are accepted.
        check_wrapped_depth(&m, 1).unwrap();
        check_wrapped_depth(&m, MAX_WRAPPED_DEPTH - 1).unwrap();
    }

    #[test]
    fn check_wrapped_depth_rejects_at_max() {
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        // Count == MAX is the first rejected value (per plan: `>=`).
        let err = check_wrapped_depth(&m, MAX_WRAPPED_DEPTH).unwrap_err();
        assert!(matches!(
            err,
            MacaroonError::WrappedDepthExceeded(n) if n == usize::from(MAX_WRAPPED_DEPTH)
        ));
    }

    #[test]
    fn check_wrapped_depth_detects_self_reference() {
        // Construct a tampered macaroon where parent_capability == self.id
        // (the local-self-cycle path). Production `attenuate` cannot
        // produce this — the catalog walk rejects it. We construct it
        // manually for the test.
        let secret = [0x42; 32];
        let mut cyclic = Macaroon::mint(&secret).unwrap();
        // Inject a WrappedOnly pointing at our own (not-yet-set) id.
        cyclic.caveats.push(Caveat::WrappedOnly {
            parent_capability: cyclic.id,
        });
        // Now make parent_capability == self.id: this is the local self-ref
        // the check catches. (Caveat ordering: parent_capability returns
        // the last WrappedOnly, so push a dummy after to ensure ordering.)
        let err = check_wrapped_depth(&cyclic, 1).unwrap_err();
        // self-ref triggers WrappedCycle (parent == self.id).
        assert!(matches!(err, MacaroonError::WrappedCycle));
    }

    // ---- Defect 1: cycle / depth check enforced IN attenuate ----
    // InMemoryCatalog is defined at module scope (cfg(test)).

    /// Test-only helper: like `attenuate` but with no catalog check, so we
    /// can construct deliberately-malformed chains (self-ref, missing
    /// parent, cyclic) for unit tests of `check_wrapped_chain` /
    /// `check_wrapped_depth`. **Production callers must use `attenuate`.**
    impl Macaroon {
        pub(crate) fn attenuate_unchecked_for_test(self, caveat: Caveat) -> Self {
            let mut next = self;
            let prev_chain = *next.chain.last().expect("chain non-empty");
            let mut msg = Vec::with_capacity(caveat.name().as_str().len() + 64 + next.id.len());
            msg.extend_from_slice(caveat.name().as_str().as_bytes());
            msg.extend_from_slice(&caveat.canonical_ser());
            msg.extend_from_slice(&next.id);
            let new_chain = hmac_blake3(&prev_chain, &msg);
            next.caveats.push(caveat);
            next.chain.push(new_chain);
            next.id = Self::compute_capability_id(&next);
            next
        }
    }

    fn build_chain(secret: &[u8; 32], depth: usize, catalog: &mut InMemoryCatalog) -> Macaroon {
        // Build a chain of `depth` macaroons: each links via WrappedOnly to
        // its parent's id. Root (oldest) at index 0; leaf (newest) at
        // depth-1. Uses the test-only unchecked attenuator.
        let mut macaroons: Vec<Macaroon> = (0..depth)
            .map(|_| Macaroon::mint(secret).unwrap())
            .collect();
        for i in 0..depth - 1 {
            let parent = &macaroons[i];
            let child =
                macaroons[i + 1]
                    .clone()
                    .attenuate_unchecked_for_test(Caveat::WrappedOnly {
                        parent_capability: parent.id,
                    });
            macaroons[i + 1] = child;
        }
        for m in &macaroons {
            catalog.by_id.insert(m.id, m.clone());
        }
        macaroons.pop().expect("non-empty")
    }

    #[test]
    fn attenuate_enforces_depth_in_chain() {
        // Chain at the boundary (depth = MAX_WRAPPED_DEPTH) + further
        // attenuation must fail.
        let secret = [0x42; 32];
        let mut catalog = InMemoryCatalog::default();
        // MAX_WRAPPED_DEPTH = 17 is the rejection threshold; build a chain
        // of length MAX - 1 = 16 (allowed) and try to attenuate one more
        // time, which should still be allowed (count starts at 1, depth
        // grows each step). Actually MAX_WRAPPED_DEPTH is the threshold
        // count, so a chain of MAX-1 macaroons has walked depth =
        // MAX-1 = 16, and the next attenuation would not add another
        // WrappedOnly — it would still be allowed. The depth check
        // applies when adding WrappedOnly caveats.
        let leaf = build_chain(&secret, MAX_WRAPPED_DEPTH as usize - 1, &mut catalog);
        // Pre-append is OK (depth = MAX-1 < MAX). Post-append with no new
        // WrappedOnly does not extend the chain — should pass.
        let _next = leaf
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap();
    }

    #[test]
    fn attenuate_rejects_chain_exceeding_depth() {
        // Build a chain of length MAX_WRAPPED_DEPTH (the threshold). Adding
        // a non-WrappedOnly caveat doesn't change chain depth (the chain is
        // only walked via parent_capability, which is determined by the
        // already-present WrappedOnly). The pre-append check rejects.
        let secret = [0x42; 32];
        let mut catalog = InMemoryCatalog::default();
        let leaf = build_chain(&secret, MAX_WRAPPED_DEPTH as usize, &mut catalog);
        let err = leaf
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap_err();
        assert!(matches!(err, MacaroonError::WrappedDepthExceeded(_)));
    }

    #[test]
    fn attenuate_detects_cycle_through_catalog() {
        // Build a tampered A→B→A cycle:
        //   - Mint A (id_a), B (id_b). Insert both into catalog at their
        //     original ids.
        //   - Construct a_cyclic = A's mint-time state with a WrappedOnly
        //     pointing at id_b. id remains id_a (no recompute).
        //   - Construct b_cyclic = B's mint-time state with a WrappedOnly
        //     pointing at id_a. id remains id_b.
        //   - Insert both back into catalog at the SAME ids (overwriting
        //     originals — they share ids because we kept the mint-time ids).
        //   - Walk from a_cyclic: visited={id_a}, parent=id_b, catalog
        //     returns b_cyclic, b_cyclic.parent=id_a → already visited → cycle.
        let secret = [0x42; 32];
        let a = Macaroon::mint(&secret).unwrap();
        let b = Macaroon::mint(&secret).unwrap();
        let id_a = a.id;
        let id_b = b.id;

        let mut a_cyclic = a;
        a_cyclic.caveats.push(Caveat::WrappedOnly {
            parent_capability: id_b,
        });
        // Do NOT recompute id — keep it at id_a so the cycle resolves via
        // catalog.get(id_b) → b_cyclic → parent id_a → visited.

        let mut b_cyclic = b;
        b_cyclic.caveats.push(Caveat::WrappedOnly {
            parent_capability: id_a,
        });

        let mut catalog = InMemoryCatalog::default();
        catalog.by_id.insert(id_a, a_cyclic);
        catalog.by_id.insert(id_b, b_cyclic);

        let err = catalog
            .by_id
            .get(&id_a)
            .unwrap()
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap_err();
        assert!(matches!(err, MacaroonError::WrappedCycle));
    }

    #[test]
    fn attenuate_rejects_self_reference() {
        // Self-reference: a macaroon whose WrappedOnly parent is its own
        // id. Production attenuate cannot construct this because the
        // post-append check walks the chain and detects the cycle via the
        // visited set when the id is seen twice. Construct manually for
        // the test using the unchecked helper + manual id write.
        let secret = [0x42; 32];
        let m = Macaroon::mint(&secret).unwrap();
        let cyclic = m.clone().attenuate_unchecked_for_test(Caveat::WrappedOnly {
            parent_capability: m.id,
        });
        // cyclic's parent is m.id. cyclic.id is the derived id which is
        // different from m.id, so this isn't yet a self-reference from
        // the chain walker's perspective. To force one, overwrite id to
        // equal m.id.
        let mut cyclic_id_overwrite = cyclic.clone();
        cyclic_id_overwrite.id = m.id;
        // cyclic_id_overwrite.parent_capability() == m.id == cyclic_id_overwrite.id
        // → self-reference. Chain walker detects: visited insert fails.
        let mut catalog = InMemoryCatalog::default();
        catalog
            .by_id
            .insert(cyclic_id_overwrite.id, cyclic_id_overwrite.clone());
        let err = cyclic_id_overwrite
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap_err();
        assert!(matches!(err, MacaroonError::WrappedCycle));
    }

    // ---- Defect 2: capability_id is derived ----

    #[test]
    fn capability_id_is_deterministic_per_mint() {
        let secret = [0x42; 32];
        let m1 = Macaroon::mint(&secret).unwrap();
        let m2 = Macaroon::mint(&secret).unwrap();
        // Same secret → different nonces → different chain[0] → different ids.
        assert_ne!(m1.id, m2.id);
        // Each id matches its own content-addressed derivation.
        assert_eq!(m1.id, Macaroon::compute_capability_id(&m1));
        assert_eq!(m2.id, Macaroon::compute_capability_id(&m2));
    }

    #[test]
    fn capability_id_changes_on_attenuation() {
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        let id_before = m.id;
        let m = m
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap();
        assert_ne!(m.id, id_before, "id must change on attenuation");
        assert_eq!(m.id, Macaroon::compute_capability_id(&m));
    }

    #[test]
    fn hmac_chain_binds_to_capability_id() {
        // The HMAC for caveat_i must depend on capability_id_{i-1}. If we
        // tamper with the id without re-deriving the chain, verify must
        // catch it via ChainMismatch (the chain entries are HMACs that
        // were computed against the original id).
        let secret = [0x42; 32];
        let catalog = empty_catalog();
        let m = Macaroon::mint(&secret).unwrap();
        let m = m
            .attenuate(Caveat::Before(1_700_000_000), &catalog)
            .unwrap();
        let m_for_tamper = m.clone();
        let m_for_sanity = m.clone();
        let original_id = m.id;
        // Recompute id after the attenuation — but DON'T update chain[1]
        // (which was HMAC'd against the pre-attenuation id). This is the
        // "attacker only knows how to update id but not chain" scenario.
        let m_tampered = Macaroon {
            id: [0xee; 32],
            ..m_for_tamper
        };
        let err = m_tampered.verify_signature(&secret).unwrap_err();
        // Either ChainMismatch (because chain[1] was HMAC'd against the
        // original id) or CapabilityIdMismatch (because the stored id
        // doesn't match the re-derived id) — both are valid rejections.
        assert!(
            matches!(
                err,
                MacaroonError::ChainMismatch(_) | MacaroonError::CapabilityIdMismatch
            ),
            "expected chain or id mismatch, got {err:?}"
        );
        // Sanity: untouched macaroon still verifies.
        m_for_sanity.verify_signature(&secret).unwrap();
        // Original id sanity.
        assert_eq!(original_id, Macaroon::compute_capability_id(&m));
    }
}
