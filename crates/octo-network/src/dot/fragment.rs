//! DOT Envelope Fragmentation — RFC-0850 §9
//!
//! Splits large envelopes into platform-appropriate fragments for
//! transport-constrained channels (IRC: 512B, LoRa: 256B, Telegram: 4KB).
//!
//! Fragments are self-describing: each carries envelope_id, fragment_index,
//! fragment_total, and envelope_hash for integrity verification.
//! Reassembly is deterministic — given identical fragment sets, all nodes
//! produce identical envelope bytes (ordered by fragment_index).

use std::collections::BTreeMap;
use std::time::Duration;

/// Platform payload size limits (total bytes including headers)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformLimit {
    /// IRC: 512 bytes total message
    Irc,
    /// LoRa: 256 bytes
    Lora,
    /// Telegram: 4096 bytes
    Telegram,
    /// Custom limit in bytes
    Custom(usize),
}

impl PlatformLimit {
    /// Maximum total message size in bytes for this platform.
    pub fn max_bytes(&self) -> usize {
        match self {
            PlatformLimit::Irc => 512,
            PlatformLimit::Lora => 256,
            PlatformLimit::Telegram => 4096,
            PlatformLimit::Custom(n) => *n,
        }
    }
}

/// Self-describing fragment header (42 bytes).
///
/// Every fragment carries enough metadata for deterministic reassembly
/// without external coordination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeFragment {
    /// BLAKE3-256 of the original envelope (for integrity verification)
    pub envelope_hash: [u8; 32],
    /// Fragment index (0-based)
    pub fragment_index: u16,
    /// Total number of fragments
    pub fragment_total: u16,
    /// Fragment payload (variable length)
    pub payload: Vec<u8>,
}

/// Fragment header size: envelope_hash(32) + index(2) + total(2) = 36 bytes
pub const FRAGMENT_HEADER_BYTES: usize = 36;

/// Errors during fragmentation or reassembly
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentError {
    /// Payload exceeds platform limit after header subtraction
    PayloadTooLarge {
        payload_size: usize,
        max_allowed: usize,
    },
    /// Fragment index out of range (>= fragment_total)
    IndexOutOfRange { index: u16, total: u16 },
    /// Fragment total is zero
    ZeroFragmentTotal,
    /// Envelope hash mismatch during reassembly
    IntegrityMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// Reassembly timed out with incomplete fragments
    ReassemblyTimeout { received: usize, total: usize },
    /// Reassembled payload hash doesn't match envelope_hash
    PayloadHashMismatch {
        envelope_hash: [u8; 32],
        computed_hash: [u8; 32],
    },
}

/// Fragment an envelope payload into platform-appropriate pieces.
///
/// Each fragment's payload size = platform_limit - FRAGMENT_HEADER_BYTES.
/// The last fragment may be smaller. All fragment payloads are deterministic
/// (byte slices of the original payload).
///
/// Returns an error if `platform_limit <= FRAGMENT_HEADER_BYTES`.
pub fn fragment_envelope(
    envelope_hash: [u8; 32],
    payload: &[u8],
    platform: PlatformLimit,
) -> Result<Vec<EnvelopeFragment>, FragmentError> {
    let max_total = platform.max_bytes();
    if max_total <= FRAGMENT_HEADER_BYTES {
        return Err(FragmentError::PayloadTooLarge {
            payload_size: payload.len(),
            max_allowed: 0,
        });
    }
    let max_payload = max_total - FRAGMENT_HEADER_BYTES;

    // Calculate total fragments (ceiling division)
    let fragment_total = if payload.is_empty() {
        1u16
    } else {
        ((payload.len() + max_payload - 1) / max_payload) as u16
    };

    if fragment_total == 0 {
        return Err(FragmentError::ZeroFragmentTotal);
    }

    let mut fragments = Vec::with_capacity(fragment_total as usize);
    if payload.is_empty() {
        fragments.push(EnvelopeFragment {
            envelope_hash,
            fragment_index: 0,
            fragment_total,
            payload: Vec::new(),
        });
    } else {
        for (i, chunk) in payload.chunks(max_payload).enumerate() {
            fragments.push(EnvelopeFragment {
                envelope_hash,
                fragment_index: i as u16,
                fragment_total,
                payload: chunk.to_vec(),
            });
        }
    }

    Ok(fragments)
}

/// Reassemble fragments into the original payload.
///
/// Fragments are collected into a BTreeMap keyed by fragment_index
/// (deterministic ordering). The first fragment determines the expected
/// fragment_total. All fragments must have matching envelope_hash.
///
/// On success, returns the concatenated payload bytes.
pub fn reassemble_fragments(fragments: &[EnvelopeFragment]) -> Result<Vec<u8>, FragmentError> {
    if fragments.is_empty() {
        return Err(FragmentError::ZeroFragmentTotal);
    }

    let total = fragments[0].fragment_total;
    if total == 0 {
        return Err(FragmentError::ZeroFragmentTotal);
    }

    let envelope_hash = fragments[0].envelope_hash;

    // Validate all fragments
    for f in fragments {
        if f.fragment_index >= total {
            return Err(FragmentError::IndexOutOfRange {
                index: f.fragment_index,
                total,
            });
        }
        if f.envelope_hash != envelope_hash {
            return Err(FragmentError::IntegrityMismatch {
                expected: envelope_hash,
                actual: f.envelope_hash,
            });
        }
    }

    // Check completeness
    if fragments.len() != total as usize {
        return Err(FragmentError::ReassemblyTimeout {
            received: fragments.len(),
            total: total as usize,
        });
    }

    // Deterministic reassembly: BTreeMap orders by fragment_index
    let mut map: BTreeMap<u16, &[u8]> = BTreeMap::new();
    for f in fragments {
        map.insert(f.fragment_index, &f.payload);
    }

    // Concatenate in order
    let mut result = Vec::new();
    for (_idx, chunk) in &map {
        result.extend_from_slice(chunk);
    }

    // Verify integrity: payload hash must match envelope_hash
    let computed = blake3::hash(&result);
    if *computed.as_bytes() != envelope_hash {
        return Err(FragmentError::PayloadHashMismatch {
            envelope_hash,
            computed_hash: *computed.as_bytes(),
        });
    }

    Ok(result)
}

/// Check if a set of fragments is complete (all indices present).
pub fn fragments_complete(fragments: &[EnvelopeFragment]) -> bool {
    if fragments.is_empty() {
        return false;
    }
    let total = fragments[0].fragment_total as usize;
    if fragments.len() != total {
        return false;
    }
    let mut seen = vec![false; total];
    for f in fragments {
        if (f.fragment_index as usize) < total {
            seen[f.fragment_index as usize] = true;
        }
    }
    seen.iter().all(|&b| b)
}

/// Track partial reassembly state with timeout support.
pub struct ReassemblyState {
    /// Collected fragments by index
    pub fragments: BTreeMap<u16, EnvelopeFragment>,
    /// Expected total fragments
    pub fragment_total: u16,
    /// Timestamp of first fragment arrival (epoch seconds)
    pub started_at: u64,
}

impl ReassemblyState {
    /// Create new reassembly state from the first fragment.
    pub fn new(fragment: EnvelopeFragment, now: u64) -> Self {
        let fragment_total = fragment.fragment_total;
        let mut fragments = BTreeMap::new();
        fragments.insert(fragment.fragment_index, fragment);
        Self {
            fragments,
            fragment_total,
            started_at: now,
        }
    }

    /// Add a fragment to the reassembly state.
    /// Returns Ok(true) if reassembly is complete, Ok(false) if more fragments needed.
    pub fn add_fragment(&mut self, fragment: EnvelopeFragment) -> Result<bool, FragmentError> {
        if fragment.fragment_total != self.fragment_total {
            return Err(FragmentError::IndexOutOfRange {
                index: fragment.fragment_index,
                total: self.fragment_total,
            });
        }
        if fragment.fragment_index >= self.fragment_total {
            return Err(FragmentError::IndexOutOfRange {
                index: fragment.fragment_index,
                total: self.fragment_total,
            });
        }
        if fragment.envelope_hash != self.fragments[&0].envelope_hash {
            return Err(FragmentError::IntegrityMismatch {
                expected: self.fragments[&0].envelope_hash,
                actual: fragment.envelope_hash,
            });
        }
        self.fragments.insert(fragment.fragment_index, fragment);
        Ok(self.fragments.len() == self.fragment_total as usize)
    }

    /// Check if reassembly has timed out.
    pub fn is_expired(&self, now: u64, timeout: Duration) -> bool {
        now.saturating_sub(self.started_at) >= timeout.as_secs()
    }

    /// Attempt to finalize reassembly. Returns the payload if complete.
    pub fn finalize(&self) -> Result<Vec<u8>, FragmentError> {
        let frags: Vec<EnvelopeFragment> = self.fragments.values().cloned().collect();
        reassemble_fragments(&frags)
    }

    /// Number of fragments collected so far.
    pub fn received_count(&self) -> usize {
        self.fragments.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_payload(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    #[test]
    fn test_fragment_reassemble_roundtrip_irc() {
        let payload = test_payload(1000);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Irc).unwrap();
        // IRC: 512 - 36 = 476 bytes per fragment → ceil(1000/476) = 3 fragments
        assert_eq!(fragments.len(), 3);
        assert_eq!(fragments[0].fragment_index, 0);
        assert_eq!(fragments[0].fragment_total, 3);
        assert_eq!(fragments[1].fragment_index, 1);
        assert_eq!(fragments[2].fragment_index, 2);

        let reassembled = reassemble_fragments(&fragments).unwrap();
        assert_eq!(reassembled, payload);
    }

    #[test]
    fn test_fragment_reassemble_roundtrip_lora() {
        let payload = test_payload(500);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Lora).unwrap();
        // LoRa: 256 - 36 = 220 bytes per fragment → ceil(500/220) = 3
        assert_eq!(fragments.len(), 3);
        let reassembled = reassemble_fragments(&fragments).unwrap();
        assert_eq!(reassembled, payload);
    }

    #[test]
    fn test_fragment_single_fragment() {
        let payload = b"small payload".to_vec();
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments =
            fragment_envelope(envelope_hash, &payload, PlatformLimit::Telegram).unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].fragment_total, 1);
        let reassembled = reassemble_fragments(&fragments).unwrap();
        assert_eq!(reassembled, payload);
    }

    #[test]
    fn test_fragment_empty_payload() {
        let payload = b"".to_vec();
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments =
            fragment_envelope(envelope_hash, &payload, PlatformLimit::Telegram).unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].payload.len(), 0);
    }

    #[test]
    fn test_reassemble_incomplete_fails() {
        let payload = test_payload(1000);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Irc).unwrap();
        // Only pass first 2 of 3 fragments
        let incomplete = &fragments[0..2];
        let result = reassemble_fragments(incomplete);
        assert!(matches!(
            result,
            Err(FragmentError::ReassemblyTimeout {
                received: 2,
                total: 3
            })
        ));
    }

    #[test]
    fn test_reassemble_integrity_mismatch() {
        let payload = test_payload(1000);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let mut fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Irc).unwrap();
        // Corrupt one fragment's envelope_hash
        fragments[1].envelope_hash = [0xFFu8; 32];
        let result = reassemble_fragments(&fragments);
        assert!(matches!(
            result,
            Err(FragmentError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn test_reassemble_out_of_order() {
        let payload = test_payload(1000);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let mut fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Irc).unwrap();
        // Reverse order
        fragments.reverse();
        // Reassembly should still work (BTreeMap sorts by index)
        let reassembled = reassemble_fragments(&fragments).unwrap();
        assert_eq!(reassembled, payload);
    }

    #[test]
    fn test_reassembly_state_timeout() {
        let payload = test_payload(1000);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Irc).unwrap();
        let mut state = ReassemblyState::new(fragments[0].clone(), 1000);
        state.add_fragment(fragments[1].clone()).unwrap();
        // After 3601 seconds, should be expired with 1-hour timeout
        assert!(state.is_expired(4601, Duration::from_secs(3600)));
        // Before timeout
        assert!(!state.is_expired(4599, Duration::from_secs(3600)));
    }

    #[test]
    fn test_reassembly_state_complete() {
        let payload = test_payload(1000);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Irc).unwrap();
        let mut state = ReassemblyState::new(fragments[0].clone(), 1000);
        assert!(!state.add_fragment(fragments[1].clone()).unwrap());
        assert!(state.add_fragment(fragments[2].clone()).unwrap());
        let result = state.finalize().unwrap();
        assert_eq!(result, payload);
    }

    #[test]
    fn test_fragments_complete() {
        let payload = test_payload(1000);
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        let fragments = fragment_envelope(envelope_hash, &payload, PlatformLimit::Irc).unwrap();
        assert!(!fragments_complete(&fragments[0..2]));
        assert!(fragments_complete(&fragments));
    }

    #[test]
    fn test_platform_limit_too_small() {
        let payload = b"test".to_vec();
        let envelope_hash = *blake3::hash(&payload).as_bytes();
        // Custom 10 bytes is smaller than FRAGMENT_HEADER_BYTES (36)
        let result = fragment_envelope(envelope_hash, &payload, PlatformLimit::Custom(10));
        assert!(matches!(
            result,
            Err(FragmentError::PayloadTooLarge { max_allowed: 0, .. })
        ));
    }
}
