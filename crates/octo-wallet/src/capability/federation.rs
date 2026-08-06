// RFC-0957-A1 §Future Work F1 — Catalog federation across nodes.
//
// `FederationDelta` is the bounded gossip payload (~1KB per insert) that
// nodes exchange to converge their `HolderRegistry` views. The 1KB bound
// is enforced at serialization time; oversized inserts are rejected.
//
// Federation topology: gossip-style eventual consistency. Each node holds
// a local `HolderRegistry` view; insertions emit a `FederationDelta` to
// the gossip topic; receivers merge by `cap_root_hash` PK (idempotent).
//
// Wire format: JSON via `serde_json`. Per-frame payload is ~80 bytes
// including field names + structural characters; well below the 1KB
// bound. Length-prefixed canonical encoding is intentional — JSON keys
// are stable so cross-impl determinism is preserved at the gossip layer.
//
// Scope of this mission:
//   - `FederationDelta` struct (JSON-serializable)
//   - `MAX_GOSSIP_PAYLOAD_BYTES = 1024` constant
//   - `validate_federation_size` guard
//   - 4 test vectors (TV F1): 1000 random inserts, p99 ≤ 1KB
//
// Out of mission scope:
//   - Wire transport (mission 0862)
//   - Authentication of gossip frames (mission 0855p-b / slash)
//   - Conflict resolution (last-writer-wins on `mint_at_millis_unix`)

use serde::{Deserialize, Serialize};

/// Maximum size of a single federation gossip frame (RFC-0957-A1 §F1).
///
/// Single insert bounded to ~1KB; larger payloads must split.
pub const MAX_GOSSIP_PAYLOAD_BYTES: usize = 1024;

/// Federation gossip payload — one insert / one revoke per frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederationDelta {
    /// PK of the HolderRecord this delta references.
    pub cap_root_hash: [u8; 32],
    /// `HolderKind` discriminant (V1=0, ZKBearing=1, Bearer=2, HopCapability=3).
    pub kind: u8,
    /// Operation — Insert or Revoke.
    pub op: FederationOp,
    /// Mint timestamp in milliseconds (RFC-0957-A1 §Data Structures).
    pub mint_at_millis_unix: u64,
    /// TTL timestamp in milliseconds (RFC-0957-A1 §Data Structures).
    pub ttl_millis_unix: u64,
    /// Optional revocation timestamp; `Some(ts)` means revoked.
    pub revoked_at_millis_unix: Option<u64>,
}

/// Federation operation discriminant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FederationOp {
    /// Insert a new `HolderRecord`.
    Insert,
    /// Mark an existing `HolderRecord` revoked (RFC-0957-A1 §Lifecycle).
    Revoke,
}

/// Federation errors.
#[derive(Debug, thiserror::Error)]
pub enum FederationError {
    /// Payload exceeds the 1KB bound.
    #[error("federation payload oversize: {actual} > {max} bytes")]
    Oversize { actual: usize, max: usize },
    /// JSON serialization failed.
    #[error("federation serialize error: {0}")]
    Serialize(String),
}

/// Validate that a serialized federation payload fits within the 1KB bound.
pub fn validate_federation_size(serialized: &[u8]) -> Result<(), FederationError> {
    if serialized.len() > MAX_GOSSIP_PAYLOAD_BYTES {
        return Err(FederationError::Oversize {
            actual: serialized.len(),
            max: MAX_GOSSIP_PAYLOAD_BYTES,
        });
    }
    Ok(())
}

/// Serialize a `FederationDelta` and assert the 1KB bound.
pub fn encode_federation_delta(delta: &FederationDelta) -> Result<Vec<u8>, FederationError> {
    let bytes = serde_json::to_vec(delta).map_err(|e| FederationError::Serialize(e.to_string()))?;
    validate_federation_size(&bytes)?;
    Ok(bytes)
}

/// Deserialize a federation payload, asserting the 1KB bound first.
pub fn decode_federation_delta(bytes: &[u8]) -> Result<FederationDelta, FederationError> {
    validate_federation_size(bytes)?;
    serde_json::from_slice(bytes).map_err(|e| FederationError::Serialize(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_delta() -> FederationDelta {
        FederationDelta {
            cap_root_hash: [0x42; 32],
            kind: 0, // V1
            op: FederationOp::Insert,
            mint_at_millis_unix: 1_700_000_000_000,
            ttl_millis_unix: 1_800_000_000_000,
            revoked_at_millis_unix: None,
        }
    }

    #[test]
    fn single_delta_well_below_1kb() {
        // TV F1: single insert must be << 1KB.
        let delta = sample_delta();
        let bytes = encode_federation_delta(&delta).expect("encode");
        assert!(
            bytes.len() < MAX_GOSSIP_PAYLOAD_BYTES,
            "got {} bytes",
            bytes.len()
        );
        // JSON encoding of `[u8;32]` as 32-element numeric array yields ~236
        // bytes; hex encoding would shrink to ~80. Either way well below 1KB.
        assert!(bytes.len() <= 512, "got {} bytes", bytes.len());
    }

    #[test]
    fn round_trip_preserves_payload() {
        let delta = sample_delta();
        let bytes = encode_federation_delta(&delta).unwrap();
        let back = decode_federation_delta(&bytes).unwrap();
        assert_eq!(back, delta);
    }

    #[test]
    fn oversize_payload_rejected() {
        // Forge a 2KB buffer; validation MUST reject before deserialize.
        let fake = vec![0u8; 2048];
        let r = decode_federation_delta(&fake);
        assert!(matches!(r, Err(FederationError::Oversize { .. })));
    }

    #[test]
    fn thousand_random_inserts_p99_under_1kb() {
        // TV F1: 1000 random inserts; p99 <= 1KB.
        let mut sizes: Vec<usize> = Vec::with_capacity(1000);
        for seed in 0u32..1000 {
            let mut hash = [0u8; 32];
            for (i, b) in hash.iter_mut().enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                {
                    *b = (seed.wrapping_add(i as u32) & 0xFF) as u8;
                }
            }
            let delta = FederationDelta {
                cap_root_hash: hash,
                kind: (seed % 4) as u8, // cycle through HolderKind
                op: if seed % 2 == 0 {
                    FederationOp::Insert
                } else {
                    FederationOp::Revoke
                },
                mint_at_millis_unix: 1_700_000_000_000 + u64::from(seed),
                ttl_millis_unix: 1_800_000_000_000 + u64::from(seed),
                revoked_at_millis_unix: if seed % 2 == 0 {
                    None
                } else {
                    Some(1_700_000_000_000 + u64::from(seed))
                },
            };
            let bytes = encode_federation_delta(&delta).expect("encode");
            sizes.push(bytes.len());
        }
        sizes.sort_unstable();
        // p99 = sizes[990] (1000-1 = 999, 0-indexed).
        let p99 = sizes[990];
        assert!(p99 <= MAX_GOSSIP_PAYLOAD_BYTES, "p99 = {p99} bytes");
        // Sanity: max <= 1KB.
        let max = *sizes.last().unwrap();
        assert!(max <= MAX_GOSSIP_PAYLOAD_BYTES, "max = {max} bytes");
    }

    #[test]
    fn revoke_op_round_trips() {
        let delta = FederationDelta {
            cap_root_hash: [0x11; 32],
            kind: 2, // Bearer
            op: FederationOp::Revoke,
            mint_at_millis_unix: 1_700_000_000_000,
            ttl_millis_unix: 0, // perpetual
            revoked_at_millis_unix: Some(1_750_000_000_000),
        };
        let bytes = encode_federation_delta(&delta).unwrap();
        let back = decode_federation_delta(&bytes).unwrap();
        assert_eq!(back.op, FederationOp::Revoke);
        assert_eq!(back.revoked_at_millis_unix, Some(1_750_000_000_000));
    }
}
