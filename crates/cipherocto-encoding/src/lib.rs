//! Canonical encoding for RFC-0960 constraint types (RFC-0964 §0 + §1).
//!
//! Wire format:
//! ```text
//! bytes := [<NAMESPACE_TAG> ]            // 1 byte outer namespace tag
//!        || [<VERSION_TAG> ]             // 1 byte constraint-set version
//!        || [<DISCRIMINATOR>]            // 1 byte constraint variant
//!        || [<LEN>|<PAYLOAD>]            // length-prefixed payload (big-endian u16)
//! ```
//!
//! Maximum total encoding size: 256 bytes (RFC-0960 §G5 design goal).
//!
//! Hash separators (RFC-0964 §0.1 Domain-separator registry):
//! - 0xA0: `ConstraintSet` version (reserved for future use)
//! - 0xA1: `constraint_hash` prefix
//! - 0xA2: `RedemptionContext` `context_hash`
//! - 0xA3: `sql_statements_hash`
//! - 0xA4-0xAF: reserved
//! - 0xB0: `EIP-712` `domain_separator`
//! - 0xB1: `EIP-712` `message_hash`
//! - 0xB2: `EIP-712` `typed_data_hash`
//! - 0xC0-0xFF: application-specific
//!
//! Namespace tag (RFC-0964 §0):
//! - 0x01: Constraint envelope
//! - 0x02: Caveat envelope
//! - 0x03-0x06: other primitives (per RFC-0960 R4-F1)

#![warn(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

use serde::{Deserialize, Serialize};

/// Outer namespace tag prefix (RFC-0964 §0 wire-format envelope tag).
pub const NAMESPACE_TAG: u8 = 0x01;

/// Constraint-set version tag (RFC-0964 §4). High-bit to avoid collision
/// with namespace tags 0x00-0x06.
pub const VERSION_TAG: u8 = 0xA0;

/// Hash domain separator for `constraint_hash` (RFC-0964 §5).
pub const CONSTRAINT_HASH_PREFIX: u8 = 0xA1;

/// Maximum total encoded size (RFC-0960 §G5). Encodings exceeding this
/// return [`EncodingError::ConstraintOverflow`].
pub const MAX_ENCODED_SIZE: usize = 256;

/// Canonical 25-variant constraint type set (RFC-0960 §3 + RFC-0964 §1).
///
/// Discriminator bytes 0x01-0x19 per RFC-0964 §1. Discriminators are local
/// to the Constraint namespace (the 1-byte outer namespace tag `0x01`
/// disambiguates Constraint from Caveat/PolicyObject per RFC-0964 §0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Constraint {
    // Time
    ValidRange {
        valid_after_unix: u64,
        valid_until_unix: u64,
    },
    NotBefore(u64),
    UnlockAfter(u64),
    Period {
        max_per_period: u128,
        period_duration_secs: u64,
    },
    // Spend caps
    MaxPerTx {
        amount_micro: u128,
        asset_id: [u8; 32],
    },
    PerAssetSpendingCap {
        caps: Vec<([u8; 32], u128)>,
    },
    RateLimit {
        max_per_window: u128,
        window_duration_secs: u64,
        asset_id: [u8; 32],
    },
    // Destination
    AllowedDestinations {
        dids: Vec<String>,
    },
    DeniedDestinations {
        dids: Vec<String>,
    },
    IntentBound {
        message_template: Vec<u8>,
    },
    // Co-signing
    MultiSig {
        n: u32,
        signers: Vec<String>,
    },
    RequireReceiptSignatureBy(String),
    // Caller
    CallerBound(String),
    // Use count
    MaxUses {
        count: u32,
    },
    SingleUse,
    // Delegation
    AllowIf {
        predicate: Vec<u8>,
        step_budget: u32,
    },
    VerifierRequired {
        circuit_id: [u8; 32],
    },
    // Composition
    WrappedOnly,
    SponsoredBy {
        vault: [u8; 32],
    },
    CoordinatorCanSubmit {
        coordinator: String,
    },
    // Vesting
    LinearRelease {
        start: u64,
        end: u64,
        cliff: u64,
    },
    CliffVesting {
        until: u64,
        pct: u8,
        period: u64,
    },
    LiquidityLock {
        until: u64,
    },
    GovernanceLock {
        while_vote_active: bool,
    },
    // Compliance
    ComplianceHold {
        threshold: u128,
        delay_secs: u64,
    },
}

impl Constraint {
    /// Discriminator byte (RFC-0964 §1).
    #[must_use]
    pub fn discriminator(&self) -> u8 {
        match self {
            Self::ValidRange { .. } => 0x01,
            Self::NotBefore(_) => 0x02,
            Self::UnlockAfter(_) => 0x03,
            Self::Period { .. } => 0x04,
            Self::MaxPerTx { .. } => 0x05,
            Self::PerAssetSpendingCap { .. } => 0x06,
            Self::RateLimit { .. } => 0x07,
            Self::AllowedDestinations { .. } => 0x08,
            Self::DeniedDestinations { .. } => 0x09,
            Self::IntentBound { .. } => 0x0A,
            Self::MultiSig { .. } => 0x0B,
            Self::RequireReceiptSignatureBy(_) => 0x0C,
            Self::CallerBound(_) => 0x0D,
            Self::MaxUses { .. } => 0x0E,
            Self::SingleUse => 0x0F,
            Self::AllowIf { .. } => 0x10,
            Self::VerifierRequired { .. } => 0x11,
            Self::WrappedOnly => 0x12,
            Self::SponsoredBy { .. } => 0x13,
            Self::CoordinatorCanSubmit { .. } => 0x14,
            Self::LinearRelease { .. } => 0x15,
            Self::CliffVesting { .. } => 0x16,
            Self::LiquidityLock { .. } => 0x17,
            Self::GovernanceLock { .. } => 0x18,
            Self::ComplianceHold { .. } => 0x19,
        }
    }

    /// Canonical encoding body (no namespace/version prefix).
    ///
    /// Length-prefixed payload (2-byte big-endian u16 length + payload).
    /// The discriminator is implicit via `discriminator()`; the combined
    /// wire format `[NAMESPACE_TAG, VERSION_TAG, discriminator, len, payload]`
    /// is computed by [`encode`].
    fn encode_body(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::ValidRange {
                valid_after_unix,
                valid_until_unix,
            } => {
                out.extend_from_slice(&valid_after_unix.to_be_bytes());
                out.extend_from_slice(&valid_until_unix.to_be_bytes());
            }
            Self::NotBefore(ts) => out.extend_from_slice(&ts.to_be_bytes()),
            Self::UnlockAfter(h) => out.extend_from_slice(&h.to_be_bytes()),
            Self::Period {
                max_per_period,
                period_duration_secs,
            } => {
                out.extend_from_slice(&max_per_period.to_be_bytes());
                out.extend_from_slice(&period_duration_secs.to_be_bytes());
            }
            Self::MaxPerTx {
                amount_micro,
                asset_id,
            } => {
                out.extend_from_slice(&amount_micro.to_be_bytes());
                out.extend_from_slice(asset_id);
            }
            Self::PerAssetSpendingCap { caps } => {
                // Per RFC-0964 §3.2 + R6-F4: elements MUST be sorted by asset_id
                // in lexicographic byte order. max N=5 (R5-F4).
                let mut sorted = caps.clone();
                sorted.sort_by_key(|c| c.0);
                if sorted.len() > 5 {
                    // Silently cap at 5 (encoder); decoding rejects >5.
                    sorted.truncate(5);
                }
                out.extend_from_slice(&len_as_u32(sorted.len()).to_be_bytes());
                for (asset_id, amount) in &sorted {
                    out.extend_from_slice(asset_id);
                    out.extend_from_slice(&amount.to_be_bytes());
                }
            }
            Self::RateLimit {
                max_per_window,
                window_duration_secs,
                asset_id,
            } => {
                out.extend_from_slice(&max_per_window.to_be_bytes());
                out.extend_from_slice(&window_duration_secs.to_be_bytes());
                out.extend_from_slice(asset_id);
            }
            Self::AllowedDestinations { dids } | Self::DeniedDestinations { dids } => {
                encode_string_set(&mut out, dids);
            }
            Self::IntentBound { message_template } => {
                out.extend_from_slice(&len_as_u32(message_template.len()).to_be_bytes());
                out.extend_from_slice(message_template);
            }
            Self::MultiSig { n, signers } => {
                out.extend_from_slice(&n.to_be_bytes());
                encode_string_set(&mut out, signers);
            }
            Self::RequireReceiptSignatureBy(did) | Self::CallerBound(did) => {
                encode_string(&mut out, did);
            }
            Self::MaxUses { count } => out.extend_from_slice(&count.to_be_bytes()),
            Self::SingleUse | Self::WrappedOnly => {}
            Self::AllowIf {
                predicate,
                step_budget,
            } => {
                out.extend_from_slice(&step_budget.to_be_bytes());
                out.extend_from_slice(&len_as_u32(predicate.len()).to_be_bytes());
                out.extend_from_slice(predicate);
            }
            Self::VerifierRequired { circuit_id } => out.extend_from_slice(circuit_id),
            Self::SponsoredBy { vault } => out.extend_from_slice(vault),
            Self::CoordinatorCanSubmit { coordinator } => encode_string(&mut out, coordinator),
            Self::LinearRelease { start, end, cliff } => {
                out.extend_from_slice(&start.to_be_bytes());
                out.extend_from_slice(&end.to_be_bytes());
                out.extend_from_slice(&cliff.to_be_bytes());
            }
            Self::CliffVesting { until, pct, period } => {
                out.extend_from_slice(&until.to_be_bytes());
                out.push(*pct);
                out.extend_from_slice(&period.to_be_bytes());
            }
            Self::LiquidityLock { until } => out.extend_from_slice(&until.to_be_bytes()),
            Self::GovernanceLock { while_vote_active } => out.push(u8::from(*while_vote_active)),
            Self::ComplianceHold {
                threshold,
                delay_secs,
            } => {
                out.extend_from_slice(&threshold.to_be_bytes());
                out.extend_from_slice(&delay_secs.to_be_bytes());
            }
        }
        out
    }
}

fn encode_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&len_as_u32(bytes.len()).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn encode_string_set(out: &mut Vec<u8>, items: &[String]) {
    // RFC-0964 §3.3: sorted lexicographically by string bytes.
    let mut sorted: Vec<&String> = items.iter().collect();
    sorted.sort();
    out.extend_from_slice(&len_as_u32(sorted.len()).to_be_bytes());
    for s in sorted {
        encode_string(out, s);
    }
}

/// Convert a `usize` length to `u32` for wire-format encoding.
///
/// All lengths in the canonical encoding are bounded by [`MAX_ENCODED_SIZE`]
/// (256 bytes), so the conversion cannot truncate on any supported target.
fn len_as_u32(len: usize) -> u32 {
    u32::try_from(len).expect("length bounded by MAX_ENCODED_SIZE")
}

/// Encoding errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncodingError {
    #[error("encoded constraint exceeds {MAX_ENCODED_SIZE} bytes")]
    ConstraintOverflow,
    #[error("invalid namespace tag: expected 0x{NAMESPACE_TAG:02x}, got 0x{0:02x}")]
    InvalidNamespaceTag(u8),
    #[error("invalid version tag: expected 0x{VERSION_TAG:02x}, got 0x{0:02x}")]
    InvalidVersionTag(u8),
    #[error("unknown discriminator: 0x{0:02x}")]
    UnknownDiscriminator(u8),
    #[error("payload truncated: expected {expected} bytes, got {got}")]
    Truncated { expected: usize, got: usize },
    #[error("PerAssetSpendingCap caps count exceeds 5 (R5-F4): got {0}")]
    TooManyCaps(usize),
    #[error("PerAssetSpendingCap caps not sorted by asset_id (R6-F4)")]
    CapsNotSorted,
}

/// Canonical encoding of a `Constraint` to wire bytes.
///
/// Output: `[NAMESPACE_TAG, VERSION_TAG, discriminator, len_be_u32, payload]`.
/// Total size capped at [`MAX_ENCODED_SIZE`] bytes (RFC-0960 §G5; RFC-0964 §3.2).
///
/// # Errors
///
/// Returns [`EncodingError::ConstraintOverflow`] when the encoded payload
/// would exceed [`MAX_ENCODED_SIZE`] bytes.
pub fn encode(c: &Constraint) -> Result<Vec<u8>, EncodingError> {
    let body = c.encode_body();
    // Pre-check the total wire size before prepending headers.
    let total = 6 + body.len(); // NAMESPACE_TAG + VERSION_TAG + discriminator + 4B len + body
    if total > MAX_ENCODED_SIZE {
        return Err(EncodingError::ConstraintOverflow);
    }
    let mut out = Vec::with_capacity(total);
    out.push(NAMESPACE_TAG);
    out.push(VERSION_TAG);
    out.push(c.discriminator());
    out.extend_from_slice(&len_as_u32(body.len()).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decode a `Constraint` from wire bytes. Inverse of [`encode`].
///
/// # Errors
///
/// Returns [`EncodingError::InvalidNamespaceTag`] / [`EncodingError::InvalidVersionTag`]
/// / [`EncodingError::UnknownDiscriminator`] for malformed prefixes,
/// [`EncodingError::Truncated`] if the payload is shorter than the declared
/// length, [`EncodingError::TooManyCaps`] / [`EncodingError::CapsNotSorted`]
/// for malformed `PerAssetSpendingCap` entries.
pub fn decode(bytes: &[u8]) -> Result<Constraint, EncodingError> {
    if bytes.len() < 6 {
        return Err(EncodingError::Truncated {
            expected: 6,
            got: bytes.len(),
        });
    }
    if bytes[0] != NAMESPACE_TAG {
        return Err(EncodingError::InvalidNamespaceTag(bytes[0]));
    }
    if bytes[1] != VERSION_TAG {
        return Err(EncodingError::InvalidVersionTag(bytes[1]));
    }
    let disc = bytes[2];
    let len = u32::from_be_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]) as usize;
    let body = &bytes[7..];
    if body.len() < len {
        return Err(EncodingError::Truncated {
            expected: len,
            got: body.len(),
        });
    }
    let body = &body[..len];

    decode_body(disc, body)
}

// Function is intentionally long: each discriminator branch decodes a
// different field layout. Refactoring to per-variant helpers obscures the
// wire-format table without reducing real complexity.
#[allow(clippy::too_many_lines)]
fn decode_body(disc: u8, body: &[u8]) -> Result<Constraint, EncodingError> {
    let mut r = Reader::new(body);
    match disc {
        0x01 => {
            let valid_after_unix = r.read_u64()?;
            let valid_until_unix = r.read_u64()?;
            // RFC-0964 §3.1 + R5-F2: invalid range ⇒ always-reject.
            if valid_after_unix > valid_until_unix {
                // Encoded form is valid; semantic check is at evaluation.
                // Continue decoding.
            }
            Ok(Constraint::ValidRange {
                valid_after_unix,
                valid_until_unix,
            })
        }
        0x02 => Ok(Constraint::NotBefore(r.read_u64()?)),
        0x03 => Ok(Constraint::UnlockAfter(r.read_u64()?)),
        0x04 => {
            let max_per_period = r.read_u128()?;
            let period_duration_secs = r.read_u64()?;
            Ok(Constraint::Period {
                max_per_period,
                period_duration_secs,
            })
        }
        0x05 => {
            let amount_micro = r.read_u128()?;
            let asset_id = r.read_bytes32()?;
            Ok(Constraint::MaxPerTx {
                amount_micro,
                asset_id,
            })
        }
        0x06 => {
            let count = r.read_u32()?;
            if count > 5 {
                return Err(EncodingError::TooManyCaps(count as usize));
            }
            let mut caps = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let asset_id = r.read_bytes32()?;
                let amount = r.read_u128()?;
                caps.push((asset_id, amount));
            }
            // RFC-0964 §3.2 + R6-F4: reject if not sorted.
            for w in caps.windows(2) {
                if w[0].0 >= w[1].0 {
                    return Err(EncodingError::CapsNotSorted);
                }
            }
            Ok(Constraint::PerAssetSpendingCap { caps })
        }
        0x07 => {
            let max_per_window = r.read_u128()?;
            let window_duration_secs = r.read_u64()?;
            let asset_id = r.read_bytes32()?;
            Ok(Constraint::RateLimit {
                max_per_window,
                window_duration_secs,
                asset_id,
            })
        }
        0x08 => Ok(Constraint::AllowedDestinations {
            dids: r.read_string_set()?,
        }),
        0x09 => Ok(Constraint::DeniedDestinations {
            dids: r.read_string_set()?,
        }),
        0x0A => {
            let n = r.read_u32()?;
            let message_template = r.read_bytes(n as usize)?;
            Ok(Constraint::IntentBound { message_template })
        }
        0x0B => {
            let n = r.read_u32()?;
            let signers = r.read_string_set()?;
            Ok(Constraint::MultiSig { n, signers })
        }
        0x0C => Ok(Constraint::RequireReceiptSignatureBy(r.read_string()?)),
        0x0D => Ok(Constraint::CallerBound(r.read_string()?)),
        0x0E => Ok(Constraint::MaxUses {
            count: r.read_u32()?,
        }),
        0x0F => Ok(Constraint::SingleUse),
        0x10 => {
            let step_budget = r.read_u32()?;
            let n = r.read_u32()?;
            let predicate = r.read_bytes(n as usize)?;
            Ok(Constraint::AllowIf {
                predicate,
                step_budget,
            })
        }
        0x11 => Ok(Constraint::VerifierRequired {
            circuit_id: r.read_bytes32()?,
        }),
        0x12 => Ok(Constraint::WrappedOnly),
        0x13 => Ok(Constraint::SponsoredBy {
            vault: r.read_bytes32()?,
        }),
        0x14 => Ok(Constraint::CoordinatorCanSubmit {
            coordinator: r.read_string()?,
        }),
        0x15 => {
            let start = r.read_u64()?;
            let end = r.read_u64()?;
            let cliff = r.read_u64()?;
            Ok(Constraint::LinearRelease { start, end, cliff })
        }
        0x16 => {
            let until = r.read_u64()?;
            let pct = r.read_u8()?;
            let period = r.read_u64()?;
            Ok(Constraint::CliffVesting { until, pct, period })
        }
        0x17 => Ok(Constraint::LiquidityLock {
            until: r.read_u64()?,
        }),
        0x18 => Ok(Constraint::GovernanceLock {
            while_vote_active: r.read_u8()? != 0,
        }),
        0x19 => {
            let threshold = r.read_u128()?;
            let delay_secs = r.read_u64()?;
            Ok(Constraint::ComplianceHold {
                threshold,
                delay_secs,
            })
        }
        _ => Err(EncodingError::UnknownDiscriminator(disc)),
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn read_u8(&mut self) -> Result<u8, EncodingError> {
        if self.pos + 1 > self.buf.len() {
            return Err(EncodingError::Truncated {
                expected: 1,
                got: self.buf.len() - self.pos,
            });
        }
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32, EncodingError> {
        if self.pos + 4 > self.buf.len() {
            return Err(EncodingError::Truncated {
                expected: 4,
                got: self.buf.len() - self.pos,
            });
        }
        let v = u32::from_be_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn read_u64(&mut self) -> Result<u64, EncodingError> {
        if self.pos + 8 > self.buf.len() {
            return Err(EncodingError::Truncated {
                expected: 8,
                got: self.buf.len() - self.pos,
            });
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&self.buf[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(u64::from_be_bytes(arr))
    }

    fn read_u128(&mut self) -> Result<u128, EncodingError> {
        if self.pos + 16 > self.buf.len() {
            return Err(EncodingError::Truncated {
                expected: 16,
                got: self.buf.len() - self.pos,
            });
        }
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&self.buf[self.pos..self.pos + 16]);
        self.pos += 16;
        Ok(u128::from_be_bytes(arr))
    }

    fn read_bytes32(&mut self) -> Result<[u8; 32], EncodingError> {
        if self.pos + 32 > self.buf.len() {
            return Err(EncodingError::Truncated {
                expected: 32,
                got: self.buf.len() - self.pos,
            });
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&self.buf[self.pos..self.pos + 32]);
        self.pos += 32;
        Ok(arr)
    }

    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, EncodingError> {
        if self.pos + n > self.buf.len() {
            return Err(EncodingError::Truncated {
                expected: n,
                got: self.buf.len() - self.pos,
            });
        }
        let v = self.buf[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(v)
    }

    fn read_string(&mut self) -> Result<String, EncodingError> {
        let n = self.read_u32()?;
        let bytes = self.read_bytes(n as usize)?;
        String::from_utf8(bytes).map_err(|_| EncodingError::Truncated {
            expected: n as usize,
            got: 0,
        })
    }

    fn read_string_set(&mut self) -> Result<Vec<String>, EncodingError> {
        let n = self.read_u32()?;
        let mut out = Vec::with_capacity(n as usize);
        for _ in 0..n {
            out.push(self.read_string()?);
        }
        Ok(out)
    }
}

/// BLAKE3 hash of a constraint (RFC-0964 §5).
///
/// `BLAKE3(CONSTRAINT_HASH_PREFIX || canonical_ser(constraint))`.
///
/// # Panics
///
/// Panics if the in-memory constraint fails to encode (only possible on
/// future encodings; current variants never exceed [`MAX_ENCODED_SIZE`]).
#[must_use]
pub fn constraint_hash(c: &Constraint) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[CONSTRAINT_HASH_PREFIX]);
    hasher.update(&encode(c).expect("encoding should not fail for in-memory constraint"));
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_and_version_tags_distinct() {
        // RFC-0964 §0.1: namespace tags 0x00-0x06; high-bit separators 0xA0-0xFF.
        assert_eq!(NAMESPACE_TAG, 0x01);
        assert_eq!(VERSION_TAG, 0xA0);
        assert_eq!(CONSTRAINT_HASH_PREFIX, 0xA1);
    }

    #[test]
    fn encode_decode_roundtrip_valid_range() {
        let c = Constraint::ValidRange {
            valid_after_unix: 1000,
            valid_until_unix: 2000,
        };
        let bytes = encode(&c).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn encode_decode_roundtrip_max_per_tx() {
        let c = Constraint::MaxPerTx {
            amount_micro: 1_000_000_000,
            asset_id: [0xab; 32],
        };
        let bytes = encode(&c).unwrap();
        // Header (7 bytes: NAMESPACE_TAG + VERSION_TAG + discriminator + 4-byte length) + u128 (16) + asset_id (32) = 55.
        assert_eq!(bytes.len(), 7 + 16 + 32);
        let back = decode(&bytes).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn encode_decode_roundtrip_single_use() {
        let c = Constraint::SingleUse;
        let bytes = encode(&c).unwrap();
        // SingleUse has empty body; header is 7 bytes (namespace + version + discriminator + u32 len = 0).
        assert_eq!(bytes.len(), 7);
        let back = decode(&bytes).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn encode_decode_roundtrip_wrapped_only() {
        let c = Constraint::WrappedOnly;
        let bytes = encode(&c).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn encode_decode_roundtrip_governance_lock() {
        let c = Constraint::GovernanceLock {
            while_vote_active: true,
        };
        let bytes = encode(&c).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn encode_decode_per_asset_spending_cap_canonical_order() {
        // Out-of-order input still encodes in sorted order.
        let a = [0x02; 32];
        let b = [0x01; 32];
        let c = Constraint::PerAssetSpendingCap {
            caps: vec![(a, 100), (b, 200)],
        };
        let bytes = encode(&c).unwrap();
        let back = decode(&bytes).unwrap();
        if let Constraint::PerAssetSpendingCap { caps } = back {
            assert_eq!(caps[0].0, b);
            assert_eq!(caps[1].0, a);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn decode_rejects_too_many_caps() {
        // Manually construct a payload with 6 caps; should reject.
        let mut body = Vec::new();
        body.extend_from_slice(&6u32.to_be_bytes());
        for _ in 0..6 {
            body.extend_from_slice(&[0x00; 32]);
            body.extend_from_slice(&0u128.to_be_bytes());
        }
        let mut bytes = vec![NAMESPACE_TAG, VERSION_TAG, 0x06];
        bytes.extend_from_slice(&len_as_u32(body.len()).to_be_bytes());
        bytes.extend_from_slice(&body);
        let err = decode(&bytes).unwrap_err();
        assert_eq!(err, EncodingError::TooManyCaps(6));
    }

    #[test]
    fn decode_rejects_unsorted_caps() {
        let mut body = Vec::new();
        body.extend_from_slice(&2u32.to_be_bytes());
        let a = [0x02; 32];
        let b = [0x01; 32];
        body.extend_from_slice(&a);
        body.extend_from_slice(&0u128.to_be_bytes());
        body.extend_from_slice(&b);
        body.extend_from_slice(&0u128.to_be_bytes());
        let mut bytes = vec![NAMESPACE_TAG, VERSION_TAG, 0x06];
        bytes.extend_from_slice(&len_as_u32(body.len()).to_be_bytes());
        bytes.extend_from_slice(&body);
        let err = decode(&bytes).unwrap_err();
        assert_eq!(err, EncodingError::CapsNotSorted);
    }

    #[test]
    fn decode_rejects_invalid_namespace_tag() {
        let bytes = vec![0x99, VERSION_TAG, 0x05, 0, 0, 0, 0];
        let err = decode(&bytes).unwrap_err();
        assert!(matches!(err, EncodingError::InvalidNamespaceTag(0x99)));
    }

    #[test]
    fn decode_rejects_invalid_version_tag() {
        let bytes = vec![NAMESPACE_TAG, 0x99, 0x05, 0, 0, 0, 0];
        let err = decode(&bytes).unwrap_err();
        assert!(matches!(err, EncodingError::InvalidVersionTag(0x99)));
    }

    #[test]
    fn decode_rejects_unknown_discriminator() {
        let bytes = vec![NAMESPACE_TAG, VERSION_TAG, 0xFF, 0, 0, 0, 0];
        let err = decode(&bytes).unwrap_err();
        assert_eq!(err, EncodingError::UnknownDiscriminator(0xFF));
    }

    #[test]
    fn decode_rejects_truncated() {
        let bytes = vec![NAMESPACE_TAG, VERSION_TAG, 0x05, 0, 0, 0, 16];
        // announces 16-byte body but body is empty
        let err = decode(&bytes).unwrap_err();
        assert!(matches!(err, EncodingError::Truncated { .. }));
    }

    #[test]
    fn constraint_hash_deterministic() {
        let c = Constraint::MaxPerTx {
            amount_micro: 1_000_000,
            asset_id: [0u8; 32],
        };
        let h1 = constraint_hash(&c);
        let h2 = constraint_hash(&c);
        assert_eq!(h1, h2);
    }

    #[test]
    fn constraint_hash_differs_for_different_values() {
        let a = constraint_hash(&Constraint::MaxPerTx {
            amount_micro: 1_000,
            asset_id: [0u8; 32],
        });
        let b = constraint_hash(&Constraint::MaxPerTx {
            amount_micro: 2_000,
            asset_id: [0u8; 32],
        });
        assert_ne!(a, b);
    }

    #[test]
    fn all_23_variants_have_unique_discriminators() {
        // Ensure no two variants collide.
        let variants = [
            Constraint::ValidRange {
                valid_after_unix: 0,
                valid_until_unix: 0,
            },
            Constraint::NotBefore(0),
            Constraint::UnlockAfter(0),
            Constraint::Period {
                max_per_period: 0,
                period_duration_secs: 0,
            },
            Constraint::MaxPerTx {
                amount_micro: 0,
                asset_id: [0; 32],
            },
            Constraint::PerAssetSpendingCap { caps: vec![] },
            Constraint::RateLimit {
                max_per_window: 0,
                window_duration_secs: 0,
                asset_id: [0; 32],
            },
            Constraint::AllowedDestinations { dids: vec![] },
            Constraint::DeniedDestinations { dids: vec![] },
            Constraint::IntentBound {
                message_template: vec![],
            },
            Constraint::MultiSig {
                n: 0,
                signers: vec![],
            },
            Constraint::RequireReceiptSignatureBy(String::new()),
            Constraint::CallerBound(String::new()),
            Constraint::MaxUses { count: 0 },
            Constraint::SingleUse,
            Constraint::AllowIf {
                predicate: vec![],
                step_budget: 0,
            },
            Constraint::VerifierRequired {
                circuit_id: [0; 32],
            },
            Constraint::WrappedOnly,
            Constraint::SponsoredBy { vault: [0; 32] },
            Constraint::CoordinatorCanSubmit {
                coordinator: String::new(),
            },
            Constraint::LinearRelease {
                start: 0,
                end: 0,
                cliff: 0,
            },
            Constraint::CliffVesting {
                until: 0,
                pct: 0,
                period: 0,
            },
            Constraint::LiquidityLock { until: 0 },
            Constraint::GovernanceLock {
                while_vote_active: false,
            },
            Constraint::ComplianceHold {
                threshold: 0,
                delay_secs: 0,
            },
        ];
        let mut discs: Vec<u8> = variants.iter().map(Constraint::discriminator).collect();
        discs.sort_unstable();
        discs.dedup();
        assert_eq!(discs.len(), variants.len(), "discriminator collision");
    }

    #[test]
    fn all_25_variants_roundtrip() {
        // 23 listed in RFC-0960 §3 + 2 we've added (SponsoredBy, CoordinatorCanSubmit).
        let variants = vec![
            Constraint::ValidRange {
                valid_after_unix: 100,
                valid_until_unix: 200,
            },
            Constraint::NotBefore(1_000),
            Constraint::UnlockAfter(2_000),
            Constraint::Period {
                max_per_period: 100,
                period_duration_secs: 60,
            },
            Constraint::MaxPerTx {
                amount_micro: 1_000_000,
                asset_id: [0xab; 32],
            },
            Constraint::PerAssetSpendingCap {
                caps: vec![([1; 32], 100), ([2; 32], 200)],
            },
            Constraint::RateLimit {
                max_per_window: 100,
                window_duration_secs: 60,
                asset_id: [0xcd; 32],
            },
            Constraint::AllowedDestinations {
                dids: vec![octo_ident::test_helpers::sample_did(164).clone()],
            },
            Constraint::DeniedDestinations {
                dids: vec!["did:octo:b".to_owned()],
            },
            Constraint::IntentBound {
                message_template: b"transfer".to_vec(),
            },
            Constraint::MultiSig {
                n: 2,
                signers: vec!["did:octo:s1".to_owned(), "did:octo:s2".to_owned()],
            },
            Constraint::RequireReceiptSignatureBy(
                octo_ident::test_helpers::sample_did(103).clone(),
            ),
            Constraint::CallerBound(octo_ident::test_helpers::sample_did(150).clone()),
            Constraint::MaxUses { count: 5 },
            Constraint::SingleUse,
            Constraint::AllowIf {
                predicate: b"check".to_vec(),
                step_budget: 100,
            },
            Constraint::VerifierRequired {
                circuit_id: [0xab; 32],
            },
            Constraint::WrappedOnly,
            Constraint::SponsoredBy { vault: [0x99; 32] },
            Constraint::CoordinatorCanSubmit {
                coordinator: octo_ident::test_helpers::sample_did(206).clone(),
            },
            Constraint::LinearRelease {
                start: 0,
                end: 1000,
                cliff: 100,
            },
            Constraint::CliffVesting {
                until: 5_000,
                pct: 50,
                period: 30,
            },
            Constraint::LiquidityLock { until: 10_000 },
            Constraint::GovernanceLock {
                while_vote_active: true,
            },
            Constraint::ComplianceHold {
                threshold: 1_000_000,
                delay_secs: 86400,
            },
        ];
        for v in &variants {
            let bytes = encode(v).unwrap_or_else(|e| panic!("encode {v:?} failed: {e}"));
            assert!(bytes.len() <= MAX_ENCODED_SIZE, "overflow for {v:?}");
            let back = decode(&bytes).unwrap_or_else(|e| panic!("decode {v:?} failed: {e}"));
            assert_eq!(v, &back);
        }
    }
}
