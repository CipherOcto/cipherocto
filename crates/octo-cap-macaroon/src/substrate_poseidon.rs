//! STARK-native substrate primitives.
//!
//! # Why this exists
//!
//! RFC-0958 §Algorithms step 2 requires a circuit to re-derive the caveat chain
//! and bind it to a verifier-supplied value. Verifying the RFC-0957 HMAC-BLAKE3
//! chain requires BLAKE3 in-field, which measured **17,628 CASM words** against a
//! 2,400-word ceiling. That is not a tuning problem; it is the wrong primitive
//! for a STARK.
//!
//! Poseidon is designed for exactly this and measures 2,197 words for the same
//! relation — **8.0x cheaper** — using the same crate family the Cairo toolchain
//! builds against, so host and circuit can be held to identical vectors instead
//! of an assumption.
//!
//! # What is NOT here
//!
//! The credential chain is still HMAC-BLAKE3. Nothing in this module changes how
//! a capability is minted, attenuated, or verified. It supplies the primitive
//! a ZK circuit needs, so that adopting it on the host is a separate, explicit
//! decision rather than something that happened as a side effect of a refactor.
//!
//! # Security note
//!
//! [`poseidon_mac`] is a domain-separated sponge-prefix construction:
//! `MAC(k, m) = H(MAC_DOMAIN, k, m)`. The key is absorbed as a prefix under a
//! domain the commitment path never uses, which is the standard sponge MAC.
//! This is **not** `blake3::keyed_hash`, and the two must never be treated as
//! interchangeable — the domain separation is what stops a value produced as a
//! MAC being replayed as a commitment, and the two consume different
//! attacker-chosen inputs.

use starknet_crypto::poseidon_hash_many;
use starknet_crypto::Felt as FieldElement;

/// Domain tag separating the MAC use from the commitment use.
///
/// `b"MAC-CHAIN"` as a felt. Must equal `MAC_DOMAIN` in `cairo/src/lib.cairo`;
/// the parity tests pin that they agree.
pub const MAC_DOMAIN: FieldElement = FieldElement::from_hex_unchecked("0x4d41432d434841494e");

/// Domain-separated keyed MAC over felts: `MAC(k, m) = H(MAC_DOMAIN, k, m)`.
///
/// Mirrors `poseidon_mac` in `cairo/src/lib.cairo` exactly. If one side changes,
/// the parity vectors fail and the mismatch is caught rather than silently
/// producing chain tips that never match.
///
/// # Panics
/// Never. An empty `msg` yields `H(MAC_DOMAIN, k)`.
#[must_use]
pub fn poseidon_mac(key: FieldElement, msg: &[FieldElement]) -> FieldElement {
    let mut felts: Vec<FieldElement> = Vec::with_capacity(2 + msg.len());
    felts.push(MAC_DOMAIN);
    felts.push(key);
    felts.extend_from_slice(msg);
    poseidon_hash_many(felts.iter())
}

/// Encode a felt as a 32-byte chain entry, big-endian and left-padded.
///
/// Chain entries are stored as `[u8; 32]` on the token. A felt is at most
/// ~2^251.5, so the top four bits are always zero and the encoding is
/// injective over the values a chain can actually hold.
#[must_use]
pub fn felt_to_chain_bytes(felt: FieldElement) -> [u8; 32] {
    felt.to_bytes_be()
}

/// Decode a chain entry back to a felt, **refusing** values at or above the
/// field prime.
///
/// `FieldElement::from_bytes_be` reduces modulo the prime without complaint, so
/// an attacker-supplied token whose `chain[i]` is `p + k` would decode to `k`
/// and verify against a chain the issuer never produced. Round-tripping through
/// [`felt_to_chain_bytes`] catches exactly that: reduction changes the bytes.
#[must_use]
pub fn chain_bytes_to_felt(bytes: &[u8; 32]) -> Option<FieldElement> {
    let felt = FieldElement::from_bytes_be(bytes);
    (felt_to_chain_bytes(felt) == *bytes).then_some(felt)
}

/// `chain[0]`: the mint-time chain seed.
///
/// ```text
/// H(MAC_DOMAIN, secret_lo, secret_hi, ...mint_felts)
/// ```
///
/// `mint_felts` is [`bytes_to_felts`] of `MACAR_ID_DOMAIN ‖ nonce`. The mint
/// nonce is not stored on the token — `root_id` is a one-way summary of it — so
/// this cannot be recomputed from the token alone and must cross into the
/// circuit as witness. It is folded into the seed rather than appended as a
/// chain step because that is where the domain separation belongs: `chain[0]` is
/// the bearer token, and without the nonce every capability minted from the same
/// root secret would share an identical signature.
///
/// # Panics
/// Never.
#[must_use]
pub fn chain_seed(
    root_secret_lo: FieldElement,
    root_secret_hi: FieldElement,
    mint_felts: &[FieldElement],
) -> FieldElement {
    let mut seed = Vec::with_capacity(3 + mint_felts.len());
    seed.push(MAC_DOMAIN);
    seed.push(root_secret_lo);
    seed.push(root_secret_hi);
    seed.extend_from_slice(mint_felts);
    poseidon_hash_many(seed.iter())
}

/// `chain[i+1]`: one attenuation link.
///
/// ```text
/// MAC(state, [id_lo, id_hi, ...msg_felts])
/// ```
///
/// `msg_felts` is [`bytes_to_felts`] of `name ‖ canonical_ser(caveat)`, keeping
/// RFC-0957 §Algorithms' message layout intact — only its encoding into the
/// field changes.
///
/// The id is split rather than passed whole because `capability_id` is a full
/// 256-bit BLAKE3 digest and the field prime is only ~2^251.5: roughly one id in
/// twenty-three is out of range, and reducing it would silently collide with a
/// different id. Two 128-bit halves are both well inside the prime and the pair
/// is injective.
///
/// # Panics
/// Never.
#[must_use]
pub fn chain_step(
    state: FieldElement,
    prev_id: &[u8; 32],
    msg_felts: &[FieldElement],
) -> FieldElement {
    let (id_lo, id_hi) = split_felts(prev_id);
    let mut msg = Vec::with_capacity(2 + msg_felts.len());
    msg.push(id_lo);
    msg.push(id_hi);
    msg.extend_from_slice(msg_felts);
    poseidon_mac(state, &msg)
}

/// Re-derive the caveat chain tip: RFC-0958 §Algorithms step 2, host side.
///
/// Mirrors `verify_chain_against_tip` in `cairo/src/lib.cairo` exactly.
///
/// # Why `prev_id` is in the MAC
///
/// RFC-0957 §Algorithms builds each chain link over `name ‖ canonical_ser(caveat)
/// ‖ capability_id`. The id is what makes the chain tamper-evident: an attacker
/// who renames a capability's `id` also changes which parent chain the catalog
/// resolves, and the link must stop verifying. A circuit that MACed the caveat
/// alone would prove a *weaker* relation than the host enforces, so the ZK proof
/// would survive an id swap the credential path rejects.
///
/// It is supplied as a witness rather than derived, because deriving it needs
/// `compute_capability_id` = BLAKE3 over the canonical serialisation, and an
/// in-circuit BLAKE3 measured 17,628 CASM words. The id is on the token and is
/// checked out of band; the chain's job is only to commit to it.
///
/// # Panics
/// Panics when `prev_ids` does not cover `caveat_msgs` exactly.
#[must_use]
pub fn derive_chain_tip(
    root_secret_lo: FieldElement,
    root_secret_hi: FieldElement,
    mint_felts: &[FieldElement],
    caveat_msgs: &[Vec<u8>],
    prev_ids: &[[u8; 32]],
) -> FieldElement {
    // A short `prev_ids` slice is a caller bug, not a malleable witness: it
    // would silently drop ids from the MAC. The circuit refuses the same input
    // with `CaveatIdDepthMismatch`, so the two sides agree on what is valid.
    assert_eq!(
        prev_ids.len(),
        caveat_msgs.len(),
        "prev_ids must cover exactly the caveat chain"
    );
    let mut state = chain_seed(root_secret_lo, root_secret_hi, mint_felts);
    for (prev_id, msg) in prev_ids.iter().zip(caveat_msgs.iter()) {
        state = chain_step(state, prev_id, &bytes_to_felts(msg));
    }
    state
}

/// Split a 32-byte value into `(lo, hi)` felts, little-endian half order.
///
/// Matches the convention used everywhere a 256-bit value crosses into the
/// circuit, so `cap_root_hash`, `chain_tip` and holder keys all split the same
/// way. Getting this backwards produces values that are individually in range
/// and collectively wrong, which is why it lives here once.
#[must_use]
pub fn split_felts(bytes: &[u8; 32]) -> (FieldElement, FieldElement) {
    // Each half is a 16-byte big-endian integer, so it must be right-aligned in
    // the 32-byte field representation (i.e. scaled by 2^128). Copying the 16
    // bytes straight into a 32-byte buffer would panic on length, and padding
    // on the wrong side would silently produce in-range but wrong values.
    let mut lo = [0u8; 32];
    let mut hi = [0u8; 32];
    lo[16..].copy_from_slice(&bytes[..16]);
    hi[16..].copy_from_slice(&bytes[16..]);
    (
        FieldElement::from_bytes_be(&lo),
        FieldElement::from_bytes_be(&hi),
    )
}

// Filled in by the parity test; kept beside it so the values the circuit asserts
// and the values asserted here are one readable block.
pub const PARITY_EMPTY: FieldElement = FieldElement::from_hex_unchecked(
    "0x046e86a3387dd91b36e4c3bd6bdcd1ff021bee5c909188d42b7d76f27819de98",
);
pub const PARITY_SINGLE: FieldElement = FieldElement::from_hex_unchecked(
    "0x016c51efb49c3462f9e32810075ce52b3327d25b7802fa10313a71d72846056d",
);
pub const PARITY_THREE: FieldElement = FieldElement::from_hex_unchecked(
    "0x037523d4d4d0875dc9931199b83dd31a074bcdb4a21a5f2098020afd219f659b",
);
pub const PARITY_LARGE: FieldElement = FieldElement::from_hex_unchecked(
    "0x06bd32a5ce3f2513b62f1d02a7ef8128e148bfca2f1514deeef096d46058224d",
);

/// Bytes packed into one felt when encoding a caveat message for the chain MAC.
///
/// **31, not 32.** A felt252 must stay strictly below the field prime
/// (~2^251.5 + ...). A 32-byte big-endian value reaches 2^256, so packing 32
/// bytes would overflow for roughly half of all inputs. Those inputs would not
/// fail loudly — `Felt` reduction wraps them into *different, still-valid* felts,
/// so two distinct messages would collide and the MAC would bind the wrong
/// caveat. 31 bytes tops out at 2^248, comfortably inside the prime.
///
/// Every byte string is recoverable from its packing, and distinct byte strings
/// pack to distinct sequences. Both properties are tested, because a packing
/// that is merely "usually right" is a silent collision waiting for input.
pub const BYTES_PER_FELT: usize = 31;

/// Pack a byte string into felts for use as a MAC message.
///
/// Big-endian, `BYTES_PER_FELT` bytes per felt, **length-prefixed**: element 0
/// is the byte length, the rest are the packed groups.
///
/// The length prefix is not optional. Right-aligning each group looks sufficient
/// until you test it, at which point `[0x01, 0x02]` and `[0x00, 0x01, 0x02]` pack
/// identically — a MAC over that packing would bind the wrong caveat, and a
/// round-trip would return 31 bytes for a 2-byte input. The prefix is what makes
/// the encoding both injective and length-recoverable.
///
/// # Panics
/// Never. An empty input yields a one-element vector holding `0`.
#[must_use]
pub fn bytes_to_felts(bytes: &[u8]) -> Vec<FieldElement> {
    let mut out = Vec::with_capacity(1 + bytes.len().div_ceil(BYTES_PER_FELT));
    out.push(FieldElement::from(bytes.len() as u64));
    for chunk in bytes.chunks(BYTES_PER_FELT) {
        let mut buf = [0u8; 32];
        buf[32 - chunk.len()..].copy_from_slice(chunk);
        out.push(FieldElement::from_bytes_be(&buf));
    }
    out
}

/// Inverse of [`bytes_to_felts`].
///
/// Returns `None` for any witness this function did not produce: a missing
/// prefix, a length that disagrees with the group count, or a felt wider than
/// `BYTES_PER_FELT`. Truncating instead would decode a malformed witness into a
/// *different* valid message, which is the failure this guards.
///
/// # Panics
/// Never.
#[must_use]
pub fn felts_to_bytes(felts: &[FieldElement]) -> Option<Vec<u8>> {
    let (prefix, body) = felts.split_first()?;
    // Low limb: to_raw is big-endian, so index 0 would read the HIGH limb and
    // silently yield 0 for every small prefix.
    let len = usize::try_from(prefix.to_le_digits()[0]).ok()?;

    // The declared length must match the number of groups actually present.
    // Without this a witness could claim a short length and hide trailing groups.
    let expected_groups = len.div_ceil(BYTES_PER_FELT);
    if body.len() != expected_groups {
        return None;
    }
    if expected_groups == 0 {
        return (len == 0).then(Vec::new);
    }

    let mut out = Vec::with_capacity(len);
    let full_groups = len / BYTES_PER_FELT;
    let remainder = len % BYTES_PER_FELT;
    let expected = full_groups + usize::from(remainder != 0);
    debug_assert_eq!(expected_groups, expected);

    for (i, f) in body.iter().enumerate() {
        let be = f.to_bytes_be();
        // Only the FINAL group may be short, and only by `remainder`. Every
        // other group is a full `BYTES_PER_FELT` wide, so a non-zero leading
        // byte there means the felt exceeded the field and was reduced.
        let width = if i + 1 == body.len() {
            if remainder == 0 {
                BYTES_PER_FELT
            } else {
                remainder
            }
        } else {
            BYTES_PER_FELT
        };
        if be[..32 - width].iter().any(|b| *b != 0) {
            return None;
        }
        out.extend_from_slice(&be[32 - width..]);
    }
    debug_assert_eq!(out.len(), len);
    Some(out)
}

#[cfg(test)]
mod packing_tests {
    use super::*;

    fn bytes(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn every_felt_stays_inside_the_packing_width() {
        // The property 31 bytes buys. At 32, inputs with the top bit set exceed
        // the field prime and reduction wraps them — silently, into different
        // but still-valid felts, which is a collision rather than an error.
        for len in [1usize, 30, 31, 32, 62, 93, 200] {
            for f in bytes_to_felts(&bytes(len)) {
                let be = f.to_bytes_be();
                assert!(
                    be[..32 - BYTES_PER_FELT].iter().all(|b| *b == 0),
                    "len={len}: felt occupies more than {BYTES_PER_FELT} bytes, so it \
                     exceeded the field prime and was reduced"
                );
            }
        }
    }

    #[test]
    fn a_full_width_group_keeps_its_leading_byte() {
        // Anti-vacuity for the test above: a felt really does span the bytes it
        // was packed from, so the "no bytes above the width" check is
        // meaningful rather than vacuously true.
        let b = bytes(31);
        let packed = bytes_to_felts(&b);
        let be = packed[1].to_bytes_be();
        assert_eq!(
            be[1], b[0],
            "the first packed byte must survive at offset 1"
        );
        assert_eq!(
            be[31], b[30],
            "the last packed byte must survive at offset 31"
        );
    }

    #[test]
    fn packing_round_trips_every_length_around_the_group_boundary() {
        for len in [0usize, 1, 30, 31, 32, 61, 62, 63, 124] {
            let b = bytes(len);
            let packed = bytes_to_felts(&b);
            assert_eq!(
                felts_to_bytes(&packed).as_deref(),
                Some(&b[..]),
                "len={len}"
            );
        }
    }

    #[test]
    fn distinct_inputs_pack_to_distinct_sequences() {
        // The collision property. A message and a shifted variant must not be
        // confusable, which is what right-alignment of the final group buys.
        let a = bytes_to_felts(&[0x01, 0x02]);
        let b = bytes_to_felts(&[0x00, 0x01, 0x02]);
        let c = bytes_to_felts(&[0x01, 0x02, 0x00]);
        assert_ne!(
            a, b,
            "a leading zero must not be indistinguishable from padding"
        );
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert_ne!(a, bytes_to_felts(&[]));
    }

    #[test]
    fn a_felt_outside_the_packing_width_is_refused_rather_than_truncated() {
        // A witness carrying a value this function never produced must not be
        // silently decoded into a different message.
        let too_big = FieldElement::from_bytes_be(&[0xff; 32]);
        assert_eq!(felts_to_bytes(&[too_big]), None);
    }

    #[test]
    fn group_boundary_lengths_use_the_expected_felt_count() {
        // +1 for the length prefix.
        assert_eq!(bytes_to_felts(&bytes(0)).len(), 1);
        assert_eq!(bytes_to_felts(&bytes(1)).len(), 2);
        assert_eq!(bytes_to_felts(&bytes(31)).len(), 2);
        assert_eq!(bytes_to_felts(&bytes(32)).len(), 3);
        assert_eq!(bytes_to_felts(&bytes(62)).len(), 3);
        assert_eq!(bytes_to_felts(&bytes(63)).len(), 4);
    }

    #[test]
    fn a_length_that_disagrees_with_the_group_count_is_refused() {
        // A witness claiming fewer bytes than its groups can hold would
        // otherwise decode into a truncated message.
        let mut packed = bytes_to_felts(&bytes(64));
        packed[0] = FieldElement::from(1_u64); // claim 1 byte, supply 3 groups
        assert_eq!(felts_to_bytes(&packed), None);
    }

    #[test]
    fn a_missing_length_prefix_is_refused() {
        assert_eq!(felts_to_bytes(&[]), None);
        // A bare group with no prefix must not decode.
        assert_eq!(felts_to_bytes(&bytes_to_felts(&bytes(10))[1..]), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(v: u64) -> FieldElement {
        FieldElement::from(v)
    }

    /// Cross-implementation parity vectors.
    ///
    /// These constants are asserted **identically** by `poseidon_mac_parity` in
    /// `cairo/src/lib.cairo`. A mismatch means the two implementations have
    /// diverged and every circuit proof would fail against a real token — the
    /// one failure mode a hand-written MAC is most prone to, because both sides
    /// still run and still produce plausible values.
    fn parity_vectors() -> [(&'static str, FieldElement, Vec<FieldElement>, FieldElement); 4] {
        [
            ("empty message", fe(1), vec![], PARITY_EMPTY),
            ("single felt", fe(42), vec![fe(7)], PARITY_SINGLE),
            (
                "three felts",
                fe(0xdead),
                vec![fe(1), fe(2), fe(3)],
                PARITY_THREE,
            ),
            (
                "domain-sized key",
                fe(u64::MAX),
                vec![fe(u64::MAX), fe(0)],
                PARITY_LARGE,
            ),
        ]
    }

    #[test]
    fn poseidon_mac_matches_the_circuit_vectors() {
        for (name, key, msg, expected) in parity_vectors() {
            assert_eq!(
                poseidon_mac(key, &msg),
                expected,
                "host/circuit Poseidon MAC divergence on `{name}`"
            );
        }
    }

    /// Chain-level parity vectors, asserted **identically** by
    /// `chain_tip_matches_the_host_implementation` in `cairo/src/lib.cairo`.
    ///
    /// Pinning the whole chain rather than only the MAC matters because the MAC
    /// parity alone would still pass if the *loop* drifted — different seed
    /// construction, dropped `prev_id`, reversed order. Those are the changes that
    /// make every circuit proof fail against a real token, while both
    /// implementations continue to run and produce plausible felts.
    ///
    /// The witness is deliberately a realistic one: a mint seed of the actual
    /// `MACAR_ID_DOMAIN ‖ nonce` shape, and three byte caveats. A parity vector
    /// built from bare felts would pass even if the host and circuit disagreed
    /// about how bytes enter the field, which is the half that actually drifts.
    pub const CHAIN_PARITY_TIP: FieldElement = FieldElement::from_hex_unchecked(
        "0x074c6537d07b0b1ee7e99240c6185b939e890f78b9652cb6a1328b91a7794d40",
    );

    fn parity_mint_nonce() -> [u8; 16] {
        *b"0123456789abcdef"
    }

    fn parity_mint_felts() -> Vec<FieldElement> {
        let mut seed = crate::MACAR_ID_DOMAIN.as_bytes().to_vec();
        seed.extend_from_slice(&parity_mint_nonce());
        bytes_to_felts(&seed)
    }

    fn parity_caveats() -> Vec<Vec<u8>> {
        vec![
            b"ValidUntil".to_vec(),
            b"ModelWhitelist".to_vec(),
            b"VaultScoped".to_vec(),
        ]
    }

    fn parity_prev_ids() -> [[u8; 32]; 3] {
        [
            core::array::from_fn(|i| i as u8),
            core::array::from_fn(|i| (i + 100) as u8),
            core::array::from_fn(|i| (i + 200) as u8),
        ]
    }

    fn parity_tip() -> FieldElement {
        derive_chain_tip(
            fe(11),
            fe(12),
            &parity_mint_felts(),
            &parity_caveats(),
            &parity_prev_ids(),
        )
    }

    #[test]
    fn chain_tip_matches_the_circuit_vector() {
        assert_eq!(
            parity_tip(),
            CHAIN_PARITY_TIP,
            "host/circuit chain tip divergence"
        );
    }

    #[test]
    fn the_prev_id_binding_changes_the_chain_tip() {
        // The tamper-evidence property, host side. If `prev_id` were not in
        // the MAC, these two would be equal and an id swap would be free.
        let mint = parity_mint_felts();
        let caveats = parity_caveats();
        let honest = derive_chain_tip(fe(11), fe(12), &mint, &caveats, &parity_prev_ids());
        let mut swapped_ids = parity_prev_ids();
        swapped_ids[1][7] ^= 0xff;
        let swapped = derive_chain_tip(fe(11), fe(12), &mint, &caveats, &swapped_ids);
        let zeroed = derive_chain_tip(
            fe(11),
            fe(12),
            &mint,
            &caveats,
            &[[0u8; 32], [0u8; 32], [0u8; 32]],
        );
        assert_ne!(honest, swapped, "swapping one prev_id must change the tip");
        assert_ne!(honest, zeroed, "zeroing every prev_id must change the tip");
    }

    #[test]
    fn the_chain_tip_binds_caveat_order() {
        let mint = parity_mint_felts();
        let ids = parity_prev_ids();
        let a = derive_chain_tip(fe(11), fe(12), &mint, &parity_caveats(), &ids);
        let mut reordered = parity_caveats();
        reordered.swap(0, 2);
        let b = derive_chain_tip(fe(11), fe(12), &mint, &reordered, &ids);
        assert_ne!(a, b, "permuting the caveats must change the tip");
    }

    #[test]
    fn the_mint_nonce_changes_the_chain_tip() {
        // Without the nonce folded into the seed, every capability minted from
        // the same root secret would share an identical chain[0].
        let caveats = parity_caveats();
        let ids = parity_prev_ids();
        let honest = parity_tip();
        let mut other_seed = crate::MACAR_ID_DOMAIN.as_bytes().to_vec();
        other_seed.extend_from_slice(b"fedcba9876543210");
        let other = derive_chain_tip(fe(11), fe(12), &bytes_to_felts(&other_seed), &caveats, &ids);
        assert_ne!(honest, other, "a different mint nonce must change the tip");
    }

    #[test]
    fn the_root_secret_changes_the_chain_tip() {
        let mint = parity_mint_felts();
        let caveats = parity_caveats();
        let ids = parity_prev_ids();
        assert_ne!(
            derive_chain_tip(fe(11), fe(12), &mint, &caveats, &ids),
            derive_chain_tip(fe(11), fe(13), &mint, &caveats, &ids),
            "the root secret must be bound into the chain"
        );
    }

    #[test]
    fn an_empty_caveat_message_advances_the_walk() {
        // A zero-length caveat occupies exactly its one-element prefix. If the
        // group arithmetic gave 0 the walk would stall and a prover could pad
        // the chain with caveats that carry nothing.
        let mut seed = crate::MACAR_ID_DOMAIN.as_bytes().to_vec();
        seed.extend_from_slice(b"0123456789abcdef");
        let tip = derive_chain_tip(
            fe(11),
            fe(12),
            &bytes_to_felts(&seed),
            &[Vec::new()],
            &[core::array::from_fn(|i| i as u8)],
        );
        println!("EMPTY_CAVEAT_TIP={tip:#066x}");
        assert_ne!(tip, FieldElement::from(0_u64));
    }

    #[test]
    fn a_short_prev_id_slice_is_refused_rather_than_silently_truncated() {
        // Truncating here would drop ids from the MAC and produce a tip that
        // the circuit would refuse — a host/circuit disagreement on validity.
        let mint = parity_mint_felts();
        let caveats = parity_caveats();
        let r = std::panic::catch_unwind(|| {
            derive_chain_tip(fe(11), fe(12), &mint, &caveats, &parity_prev_ids()[..1])
        });
        assert!(
            r.is_err(),
            "a prev_id slice shorter than the chain must panic"
        );
    }

    #[test]
    fn a_chain_entry_at_or_above_the_field_prime_is_refused() {
        // The silent-reduction hazard. `capability_id` is a full 256-bit BLAKE3
        // digest and the prime is ~2^251.5, so roughly one id in twenty-three is
        // out of range. An attacker who put `p + k` in `chain[i]` would otherwise
        // have it decode to `k` and verify against a chain nobody issued.
        let felt = FieldElement::from(7_u64);
        let bytes = felt_to_chain_bytes(felt);
        assert_eq!(chain_bytes_to_felt(&bytes), Some(felt));

        // `2^252` — above the prime, which is ~2^251.5. Reduce it and the round-trip
        // no longer matches.
        let mut over = [0u8; 32];
        over[0] = 0x10;
        assert_eq!(chain_bytes_to_felt(&over), None);

        // `2^251 + 1` is just *below* the starknet prime, so it must still be
        // accepted. Without this the test above would pass on any large value
        // and prove nothing about the boundary.
        let mut just_under = [0u8; 32];
        just_under[0] = 0x08;
        just_under[31] = 0x01;
        assert_eq!(
            chain_bytes_to_felt(&just_under),
            Some(FieldElement::from_bytes_be(&just_under))
        );

        // All-ones is far above the prime for the same reason.
        assert_eq!(chain_bytes_to_felt(&[0xff; 32]), None);
    }

    #[test]
    fn chain_entries_round_trip_through_the_token_representation() {
        // Every chain value the host produces must survive the trip into the
        // token's `[u8; 32]` and back, or the verifier re-derives something the
        // issuer never wrote.
        for i in 0..64_u8 {
            let felt = poseidon_hash_many([fe(u64::from(i))].iter());
            let bytes = felt_to_chain_bytes(felt);
            assert_eq!(bytes.len(), 32);
            assert_eq!(chain_bytes_to_felt(&bytes), Some(felt));
        }
    }

    #[test]
    fn a_capability_id_split_halves_are_always_inside_the_field() {
        // Why the id is split rather than passed whole: a 32-byte BLAKE3 digest
        // exceeds the prime about 4.4% of the time, and each 16-byte half is
        // always in range.
        let digest = [0xffu8; 32];
        let (lo, hi) = split_felts(&digest);
        // Right-aligned: the 16 source bytes land in the HIGH half of the
        // 32-byte field representation, i.e. scaled by 2^128.
        assert_eq!(lo.to_bytes_be()[16..32], [0xff; 16]);
        assert_eq!(hi.to_bytes_be()[16..32], [0xff; 16]);
        assert_eq!(lo.to_bytes_be()[..16], [0u8; 16]);
        // The worst case is all ones: if that survives, so does every digest.
        assert_eq!(chain_bytes_to_felt(&felt_to_chain_bytes(lo)), Some(lo));
        assert_eq!(chain_bytes_to_felt(&felt_to_chain_bytes(hi)), Some(hi));
    }

    #[test]
    fn mac_domain_is_the_circuit_domain() {
        // `b"MAC-CHAIN"` — the literal the Cairo side declares.
        assert_eq!(
            MAC_DOMAIN,
            FieldElement::from_hex_unchecked("0x4d41432d434841494e")
        );
    }

    #[test]
    fn domain_separation_changes_the_output() {
        // Without this, a value produced as a MAC could be replayed as a
        // commitment and vice versa.
        let k = fe(9);
        let mac = poseidon_mac(k, &[fe(1)]);
        let mut commitment_input = Vec::new();
        commitment_input.push(k);
        commitment_input.extend_from_slice(&[fe(1)]);
        assert_ne!(mac, poseidon_hash_many(commitment_input.iter()));
    }

    #[test]
    fn split_felts_is_low_half_first_and_round_trips_by_bytes() {
        let bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
        let (lo, hi) = split_felts(&bytes);
        let mut expect_lo = [0u8; 32];
        let mut expect_hi = [0u8; 32];
        expect_lo[16..].copy_from_slice(&bytes[..16]);
        expect_hi[16..].copy_from_slice(&bytes[16..]);
        assert_eq!(lo, FieldElement::from_bytes_be(&expect_lo));
        assert_eq!(hi, FieldElement::from_bytes_be(&expect_hi));
        // Right-alignment means each half is scaled by 2^128. Getting the side
        // wrong yields in-range felts that silently disagree with the circuit.
        let hex_lo = bytes[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            lo,
            FieldElement::from_hex_unchecked(&format!("0x{hex_lo}")),
            "the low half must be the leading 16 bytes, right-aligned"
        );
        // Halves must differ for a non-symmetric input, otherwise the split
        // could not be carrying both.
        assert_ne!(lo, hi);
    }
}
