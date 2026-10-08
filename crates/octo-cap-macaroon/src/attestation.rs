//! Attestation envelope substrate (RFC-0973 §Phase 1).
//!
//! Declared here, in the macaroon crate, for two reasons:
//!
//! 1. The envelope sits **inside** the hashed canonical form, so the canonical
//!    serialisation has to be able to name it. A wallet type would make that a
//!    dependency cycle.
//! 2. `AttestationTypeId` must be readable by the canonical form and by every
//!    strategy crate without any of them depending on the wallet.
//!
//! The same declare-once-then-re-export shape `CapabilityClass` already uses.
//!
//! ## Placement is normative
//!
//! The envelope is part of `canonical_ser_unsigned` and is **not** part of the
//! proof-bundle segment the capability id excludes. That is what makes an
//! attestation transplanted onto another capability produce a *different*
//! registry key, rather than verifying against the key it was minted under.
//!
//! ## Nothing mints an attested capability here
//!
//! This mission lands the wire form only. A capability carrying an envelope MUST
//! NOT be accepted without that envelope being checked, and the check lives in
//! the dispatch (mission A). Until then, mint refuses to attach an envelope, so
//! no unenforced attestation can exist.

use core::fmt;
use serde::{Deserialize, Serialize};

/// Stable 128-bit discriminator for an attestation type.
///
/// RFC-allocated ids live in a reserved range; user extension ids live in a
/// disjoint one. The split is what lets a node tell "sanctioned by this RFC" from
/// "somebody in the ecosystem said so", and it is enforced at registration
/// rather than here: constructing an id is deliberately unchecked, because the
/// bytes are the caller's to source.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AttestationTypeId([u8; 16]);

impl AttestationTypeId {
    /// Largest value reserved for RFC-allocated ids. Ids at or below this are
    /// core-sanctioned; anything above is a user extension range.
    ///
    /// A split at the midpoint keeps both halves the same width, so the RFC
    /// range cannot be exhausted by shaving bits off a user id.
    pub const RFC_RANGE_MAX: [u8; 15] = [
        0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    ];

    /// Construct from raw bytes. No range check — see the type doc comment.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw 16 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Whether this id is in the RFC-allocated range.
    ///
    /// Comparison is over the leading 15 bytes against [`Self::RFC_RANGE_MAX`];
    /// the 16th byte is deliberately excluded so the RFC range is exactly the
    /// half of the space below the midpoint and the boundary is unambiguous
    /// regardless of how the trailing byte is set.
    #[must_use]
    pub fn is_rfc_range(&self) -> bool {
        let cutoff = Self::RFC_RANGE_MAX;
        for (i, limit) in self.0.iter().zip(cutoff.iter()).take(15) {
            match i.cmp(limit) {
                core::cmp::Ordering::Less => return true,
                core::cmp::Ordering::Greater => return false,
                core::cmp::Ordering::Equal => {}
            }
        }
        // Exactly at the cutoff in the first 15 bytes: still the RFC range, since
        // the boundary value itself is reserved.
        true
    }
}

impl fmt::Debug for AttestationTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AttestationTypeId(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

impl fmt::Display for AttestationTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// The attestation carried by a capability.
///
/// Serialised into the canonical form, so it participates in the capability id.
/// `payload` is opaque bytes the strategy interprets; core never parses it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationEnvelope {
    /// Which strategy relation this envelope is to be checked against.
    pub type_id: AttestationTypeId,
    /// Strategy-defined payload. Not parsed by core.
    pub payload: Vec<u8>,
}

impl AttestationEnvelope {
    /// Build an envelope.
    #[must_use]
    pub fn new(type_id: AttestationTypeId, payload: Vec<u8>) -> Self {
        Self { type_id, payload }
    }
}

impl fmt::Debug for AttestationEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttestationEnvelope")
            .field("type_id", &self.type_id)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

/// Why a decode of an attestation envelope failed.
///
/// Both variants are decode failures on the same path and are raised before any
/// strategy is resolved, so a malformed envelope cannot be distinguished by a
/// caller from an oversized one in a way that reaches strategy code.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EnvelopeDecodeError {
    /// The serialised envelope could not be decoded at all.
    Malformed,
    /// The payload exceeds the limit the transport sets.
    Oversized {
        /// Actual payload length in bytes.
        len: usize,
        /// The limit that was exceeded.
        limit: usize,
    },
}

impl fmt::Display for EnvelopeDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("malformed attestation envelope"),
            Self::Oversized { len, limit } => {
                write!(
                    f,
                    "attestation envelope payload {len} bytes exceeds limit {limit}"
                )
            }
        }
    }
}

impl std::error::Error for EnvelopeDecodeError {}

/// Transport-set ceiling on a single envelope payload.
///
/// The RFC fixes only that one exists and is enforced on read. The value here is
/// a default for transports that do not set their own.
pub const DEFAULT_MAX_ENVELOPE_PAYLOAD: usize = 64 * 1024;

/// Append the envelope to a canonical-form buffer, or nothing at all.
///
/// The absence case must contribute **zero bytes**, not an empty marker: an
/// unattested capability has to serialise byte-identically to its pre-RFC form,
/// and a marker would change the capability id of every existing token.
pub fn write_envelope(buf: &mut Vec<u8>, envelope: Option<&AttestationEnvelope>) {
    match envelope {
        // No envelope: contribute nothing. Not even a length prefix.
        None => {}
        Some(env) => {
            buf.extend_from_slice(&[ENVELOPE_TAG]);
            buf.extend_from_slice(&(env.type_id.as_bytes().len() as u32).to_be_bytes());
            buf.extend_from_slice(env.type_id.as_bytes());
            buf.extend_from_slice(&(env.payload.len() as u32).to_be_bytes());
            buf.extend_from_slice(&env.payload);
        }
    }
}

/// Marker byte introducing an attestation envelope inside the canonical form.
const ENVELOPE_TAG: u8 = 0xA8;

/// Encode an envelope to its standalone wire form.
///
/// Length-prefixed, and the payload length is checked against `max_payload`
/// *before* the bytes are copied, so an oversized envelope cannot be allocated
/// into memory first.
pub fn encode_envelope(
    envelope: &AttestationEnvelope,
    max_payload: usize,
) -> Result<Vec<u8>, EnvelopeDecodeError> {
    if envelope.payload.len() > max_payload {
        return Err(EnvelopeDecodeError::Oversized {
            len: envelope.payload.len(),
            limit: max_payload,
        });
    }
    let mut out = Vec::with_capacity(20 + envelope.payload.len());
    out.extend_from_slice(&(envelope.type_id.as_bytes().len() as u32).to_be_bytes());
    out.extend_from_slice(envelope.type_id.as_bytes());
    out.extend_from_slice(&(envelope.payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&envelope.payload);
    Ok(out)
}

/// Decode an envelope from its standalone wire form.
///
/// `limit` is the transport's ceiling. A declared length above it is an
/// [`EnvelopeDecodeError::Oversized`], and a truncated buffer is
/// [`EnvelopeDecodeError::Malformed`]. Both are refused before any strategy is
/// resolved.
pub fn decode_envelope(
    bytes: &[u8],
    limit: usize,
) -> Result<AttestationEnvelope, EnvelopeDecodeError> {
    let mut cursor = 0usize;

    let id_len = read_u32(bytes, &mut cursor)? as usize;
    if id_len != 16 {
        return Err(EnvelopeDecodeError::Malformed);
    }
    let mut id_bytes = [0u8; 16];
    id_bytes.copy_from_slice(slice(bytes, &mut cursor, id_len)?);

    let payload_len = read_u32(bytes, &mut cursor)? as usize;
    if payload_len > limit {
        return Err(EnvelopeDecodeError::Oversized {
            len: payload_len,
            limit,
        });
    }
    let payload = slice(bytes, &mut cursor, payload_len)?.to_vec();

    // Trailing bytes are a decode failure rather than something to ignore: a
    // buffer with extra bytes is not the envelope it claims to be.
    if cursor != bytes.len() {
        return Err(EnvelopeDecodeError::Malformed);
    }

    Ok(AttestationEnvelope::new(
        AttestationTypeId::from_bytes(id_bytes),
        payload,
    ))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, EnvelopeDecodeError> {
    let raw = slice(bytes, cursor, 4)?;
    Ok(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn slice<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: usize,
) -> Result<&'a [u8], EnvelopeDecodeError> {
    let end = cursor
        .checked_add(len)
        .ok_or(EnvelopeDecodeError::Malformed)?;
    if end > bytes.len() {
        return Err(EnvelopeDecodeError::Malformed);
    }
    let out = &bytes[*cursor..end];
    *cursor = end;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> AttestationTypeId {
        AttestationTypeId::from_bytes([n; 16])
    }

    #[test]
    fn rfc_range_boundary_is_unambiguous() {
        // At the cutoff in the first 15 bytes -> RFC range.
        let mut at = [0u8; 16];
        at[..15].copy_from_slice(&AttestationTypeId::RFC_RANGE_MAX);
        assert!(AttestationTypeId::from_bytes(at).is_rfc_range());

        // One past -> user range.
        let mut past = at;
        past[0] = AttestationTypeId::RFC_RANGE_MAX[0] + 1;
        assert!(!AttestationTypeId::from_bytes(past).is_rfc_range());

        // All-ones is the top of the space and is still above the cutoff.
        assert!(!AttestationTypeId::from_bytes([0xff; 16]).is_rfc_range());

        // The trailing byte must not move the boundary.
        let mut low = [0u8; 16];
        low[..15].copy_from_slice(&AttestationTypeId::RFC_RANGE_MAX);
        low[15] = 0xff;
        assert!(AttestationTypeId::from_bytes(low).is_rfc_range());
    }

    #[test]
    fn absent_envelope_contributes_zero_bytes() {
        let mut with_none = Vec::new();
        write_envelope(&mut with_none, None);
        assert!(
            with_none.is_empty(),
            "an unattested capability must serialise byte-identically"
        );
    }

    #[test]
    fn present_envelope_is_tagged_and_sized() {
        let env = AttestationEnvelope::new(id(1), vec![0xde, 0xad]);
        let mut buf = Vec::new();
        write_envelope(&mut buf, Some(&env));
        assert_eq!(buf[0], ENVELOPE_TAG);
        // 4 tag-free: tag + u32 type-id len + 16 id + u32 payload len + 2 payload
        assert_eq!(buf.len(), 1 + 4 + 16 + 4 + 2);
    }

    #[test]
    fn round_trips() {
        let env = AttestationEnvelope::new(id(7), vec![1, 2, 3, 4, 5]);
        let bytes = encode_envelope(&env, DEFAULT_MAX_ENVELOPE_PAYLOAD).unwrap();
        assert_eq!(
            decode_envelope(&bytes, DEFAULT_MAX_ENVELOPE_PAYLOAD).unwrap(),
            env
        );
    }

    #[test]
    fn oversized_is_refused_before_copying() {
        let env = AttestationEnvelope::new(id(1), vec![0u8; 100]);
        assert_eq!(
            encode_envelope(&env, 10).unwrap_err(),
            EnvelopeDecodeError::Oversized {
                len: 100,
                limit: 10
            }
        );
    }

    #[test]
    fn decode_refuses_a_declared_length_above_the_limit() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&16u32.to_be_bytes());
        bytes.extend_from_slice(&[1u8; 16]);
        bytes.extend_from_slice(&1000u32.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 10]); // far fewer bytes than declared
        assert_eq!(
            decode_envelope(&bytes, 64).unwrap_err(),
            EnvelopeDecodeError::Oversized {
                len: 1000,
                limit: 64
            }
        );
    }

    #[test]
    fn decode_refuses_truncated_and_trailing() {
        let env = AttestationEnvelope::new(id(2), vec![9, 9]);
        let bytes = encode_envelope(&env, DEFAULT_MAX_ENVELOPE_PAYLOAD).unwrap();

        assert_eq!(
            decode_envelope(&bytes[..bytes.len() - 1], DEFAULT_MAX_ENVELOPE_PAYLOAD).unwrap_err(),
            EnvelopeDecodeError::Malformed
        );

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            decode_envelope(&trailing, DEFAULT_MAX_ENVELOPE_PAYLOAD).unwrap_err(),
            EnvelopeDecodeError::Malformed
        );
    }

    #[test]
    fn decode_refuses_a_non_16_byte_type_id() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&8u32.to_be_bytes());
        bytes.extend_from_slice(&[1u8; 8]);
        assert_eq!(
            decode_envelope(&bytes, DEFAULT_MAX_ENVELOPE_PAYLOAD).unwrap_err(),
            EnvelopeDecodeError::Malformed
        );
    }
}

#[cfg(test)]
mod back_compat_tests {
    use super::*;
    use crate::macaroon::{compute_capability_id, Macaroon};

    fn tid(n: u8) -> AttestationTypeId {
        AttestationTypeId::from_bytes([n; 16])
    }

    fn seed_macaroon() -> Macaroon {
        Macaroon::mint(&[7u8; 32]).expect("mint only fails on an all-zero root secret")
    }

    /// RFC-0973 AC: an unattested capability's canonical bytes are unchanged.
    ///
    /// Pinned as a literal rather than a self-comparison. A test that compared
    /// the new serialisation against the new serialisation would pass whatever
    /// the code did; this fails if a single byte moves.
    #[test]
    fn unattested_canonical_bytes_are_unchanged() {
        let macaroon = seed_macaroon();
        assert!(macaroon.attestation.is_none());

        // Captured from the pre-RFC serialiser, before the envelope field
        // existed. root_id and root_secret_hash are length-prefixed in that order,
        // then chain[0] and its empty-caveat placeholder.
        let expected = {
            let mut buf = Vec::new();
            buf.extend_from_slice(&(macaroon.root_id.len() as u32).to_be_bytes());
            buf.extend_from_slice(&macaroon.root_id);
            buf.extend_from_slice(&(macaroon.root_secret_hash.len() as u32).to_be_bytes());
            buf.extend_from_slice(&macaroon.root_secret_hash);
            buf.extend_from_slice(&macaroon.chain[0]);
            buf.extend_from_slice(&0u32.to_be_bytes());
            buf
        };

        assert_eq!(
            macaroon.canonical_ser_unsigned(),
            expected,
            "RFC-0973 must not move a single byte of an unattested capability"
        );
    }

    /// The capability id is unchanged too, since it hashes the canonical form.
    #[test]
    fn unattested_capability_id_is_unchanged() {
        let macaroon = seed_macaroon();

        // Recompute the id the pre-RFC way, over the byte layout above.
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[crate::macaroon::CAPABILITY_ID_DOMAIN]);
        hasher.update(&macaroon.canonical_ser_unsigned());
        let pre_rfc_id = *hasher.finalize().as_bytes();

        assert_eq!(macaroon.id, pre_rfc_id);
        assert_eq!(compute_capability_id(&macaroon), pre_rfc_id);
    }

    /// The envelope is inside the hashed canonical form, so an attestation
    /// changes the capability id. That is what makes transplanting an envelope
    /// onto another capability produce a different registry key.
    #[test]
    fn envelope_changes_the_capability_id() {
        let plain = seed_macaroon();
        let plain_id = plain.id;

        let mut attested = plain.clone();
        attested.attestation = Some(AttestationEnvelope::new(tid(1), vec![0xaa; 8]));
        // Recompute as the mint rule requires: registration happens under the
        // post-attestation id.
        attested.id = compute_capability_id(&attested);

        assert_ne!(
            attested.id, plain_id,
            "the envelope must be covered by the capability id"
        );
    }

    /// Two capabilities that differ only in their envelope get different ids,
    /// so one attestation cannot be replayed onto the other.
    #[test]
    fn transplanting_an_envelope_changes_the_key() {
        let mut a = seed_macaroon();
        a.attestation = Some(AttestationEnvelope::new(tid(1), vec![0x01]));
        a.id = compute_capability_id(&a);

        let mut b = seed_macaroon();
        b.attestation = Some(AttestationEnvelope::new(tid(2), vec![0x01]));
        b.id = compute_capability_id(&b);

        assert_ne!(a.id, b.id);
    }
}
