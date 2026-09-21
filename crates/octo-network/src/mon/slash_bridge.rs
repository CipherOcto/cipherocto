//! `SlashBridge` trait + payload types (RFC-0011-h §Substrate-Additions row G9 + RFC-0011-o).
//!
//! Bridges local `SlashStore` to external reputation substrates via the
//! per-extension crate pattern (Layer B trait; concrete per-transport impl
//! crates in Layer D, OUT OF SCOPE per RFC-0011-h).
//!
//! Per [[cipherocto-design-principles]] §User extensibility, the trait is the
//! extension surface; concrete impls register at runtime. CLI consumes via
//! runtime registry lookup identical to RFC-0863 `NetworkSender` pattern.
//!
//! ## Determinism
//!
//! `BridgedSlash::bridge_metadata` uses `BTreeMap` (NOT `HashMap`) for
//! deterministic iteration per RFC-0011-h §Output Envelope determinism pattern.
//!
//! ## Errors
//!
//! `BridgeError` is `#[non_exhaustive]` so per-extension impl crates can add
//! variant cases without breaking the layer-A frozen contract.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Trait bridging local slash store to external reputation substrates.
///
/// Per RFC-0011-h §Substrate-Additions row G9 + RFC-0011-o §Substrate Mapping
/// Table. Per-extension crate pattern: trait in Layer B; concrete per-transport
/// impl crates (substrate-ext-bridge-*) in Layer D, OUT OF SCOPE.
pub trait SlashBridge: Send + Sync {
    /// List all bridged slashes currently held by the local trait implementation.
    ///
    /// Infallible; returns owned `Vec<BridgedSlash>`. CLI translates 1:1 to
    /// `NetworkSlashBridgeListOutput` envelope per RFC-0011-o §Output Envelope.
    fn list(&self) -> Vec<BridgedSlash>;

    /// Propagate a slash envelope to its external destination.
    ///
    /// Idempotent on `slash_envelope_id` per RFC-0855 §8.4 External Reputation
    /// Bridge; double-propagate yields same `BridgeReceipt`.
    ///
    /// CLI translates fallible result via RFC-0011-o §Error Handling:
    /// every `BridgeError` variant maps to exit slot 89 with distinct message.
    fn propagate_to(&self, slash_envelope_id: [u8; 32]) -> Result<BridgeReceipt, BridgeError>;
}

/// A slash that has been bridged (or is pending bridging) to an external
/// reputation substrate. Per RFC-0011-o §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgedSlash {
    /// 32-byte canonical slash envelope identifier (RFC-0855p-b §Wire Format).
    pub slash_envelope_id: [u8; 32],

    /// Extension-defined bridge metadata (transport, peer URI, etc).
    ///
    /// `BTreeMap` for determinism per RFC-0011-h §Output Envelope determinism
    /// pattern. CLI serialization iterates this map; `HashMap` would yield
    /// non-canonical JSON output across runs.
    pub bridge_metadata: BTreeMap<String, String>,

    /// Epoch at which the slash was bridged (or queued for bridging).
    pub bridged_at_epoch: u64,
}

/// Receipt returned by `SlashBridge::propagate_to` on success. Per RFC-0011-o
/// §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeReceipt {
    /// 32-byte canonical slash envelope identifier (mirrors input).
    pub slash_envelope_id: [u8; 32],

    /// Opaque per-extension destination bytes (transport-specific).
    ///
    /// Not echoed in error envelopes per RFC-0011-h §Security Considerations
    /// redaction invariant; only present on success path.
    pub propagated_to: Vec<u8>,

    /// Epoch at which propagation completed.
    pub propagated_at_epoch: u64,
}

/// Errors returned by `SlashBridge::propagate_to`. All variants translate to
/// exit slot 89 `NetworkSubstrateUnavailable` per RFC-0011-o §Error Handling
/// + RFC-0011-h §Error Handling row 89.
///
/// `#[non_exhaustive]` so per-extension impl crates can add cases without
/// breaking the layer-A frozen contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BridgeError {
    /// Destination unreachable (transport-layer timeout or DNS failure).
    Unreachable,
    /// Bridge refused by external reputation substrate (e.g. rate limit).
    Refused,
    /// Payload exceeds transport-layer size limit.
    PayloadTooLarge,
    /// Wire format mismatch (e.g. peer speaks pre-canonical format).
    WireFormatMismatch,
    /// Internal bridge failure with opaque message.
    Internal(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeError::Unreachable => write!(f, "destination unreachable"),
            BridgeError::Refused => write!(f, "bridge refused"),
            BridgeError::PayloadTooLarge => write!(f, "payload too large"),
            BridgeError::WireFormatMismatch => write!(f, "wire format mismatch"),
            BridgeError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// Stub `SlashBridge` impl returning empty list + unreachable errors.
///
/// Per RFC-0011-h §Substrate-Faithfulness + Phase 12 R2.5 / Phase 13 R1.5
/// / Phase 14 R1.5 precedent: CLI handlers must invoke substrate
/// unconditionally. This stub provides a deterministic substrate surface
/// for the pre-Layer-D-impl deployment window. Per-extension concrete
/// impl crates (Layer D, OUT OF SCOPE) register their own impls at
/// runtime via the per-extension crate pattern, replacing this stub.
///
/// `list` returns an empty `Vec` (no bridged slashes). `propagate_to`
/// returns `BridgeError::Unreachable` for any input (no transport
/// available). Idempotency is preserved structurally: calling
/// `propagate_to` twice with the same `slash_envelope_id` returns the
/// same `Err(BridgeError::Unreachable)` both times.
#[derive(Clone, Debug, Default)]
pub struct EmptyBridge;

impl SlashBridge for EmptyBridge {
    fn list(&self) -> Vec<BridgedSlash> {
        Vec::new()
    }

    fn propagate_to(&self, _slash_envelope_id: [u8; 32]) -> Result<BridgeReceipt, BridgeError> {
        Err(BridgeError::Unreachable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory `SlashBridge` impl for testing. Per-extension crates provide
    /// their own impls in Layer D (OUT OF SCOPE for this module).
    struct InMemorySlashBridge {
        slashes: Vec<BridgedSlash>,
    }

    impl SlashBridge for InMemorySlashBridge {
        fn list(&self) -> Vec<BridgedSlash> {
            self.slashes.clone()
        }

        fn propagate_to(&self, slash_envelope_id: [u8; 32]) -> Result<BridgeReceipt, BridgeError> {
            if self
                .slashes
                .iter()
                .any(|s| s.slash_envelope_id == slash_envelope_id)
            {
                Ok(BridgeReceipt {
                    slash_envelope_id,
                    propagated_to: vec![0xAA, 0xBB],
                    propagated_at_epoch: 42,
                })
            } else {
                Err(BridgeError::Unreachable)
            }
        }
    }

    #[test]
    fn test_bridge_list_empty() {
        let bridge = InMemorySlashBridge { slashes: vec![] };
        assert_eq!(bridge.list().len(), 0);
    }

    #[test]
    fn test_bridge_list_populated() {
        let mut metadata = BTreeMap::new();
        metadata.insert("transport".to_string(), "libp2p".to_string());
        let slashes = vec![BridgedSlash {
            slash_envelope_id: [0x42; 32],
            bridge_metadata: metadata,
            bridged_at_epoch: 100,
        }];
        let bridge = InMemorySlashBridge { slashes };
        let listed = bridge.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].slash_envelope_id, [0x42; 32]);
        assert_eq!(listed[0].bridged_at_epoch, 100);
    }

    #[test]
    fn test_propagate_to_success() {
        let slashes = vec![BridgedSlash {
            slash_envelope_id: [0x42; 32],
            bridge_metadata: BTreeMap::new(),
            bridged_at_epoch: 100,
        }];
        let bridge = InMemorySlashBridge { slashes };
        let receipt = bridge.propagate_to([0x42; 32]).unwrap();
        assert_eq!(receipt.slash_envelope_id, [0x42; 32]);
        assert_eq!(receipt.propagated_to, vec![0xAA, 0xBB]);
        assert_eq!(receipt.propagated_at_epoch, 42);
    }

    #[test]
    fn test_propagate_to_unreachable() {
        let bridge = InMemorySlashBridge { slashes: vec![] };
        let result = bridge.propagate_to([0x99; 32]);
        assert!(matches!(result, Err(BridgeError::Unreachable)));
    }

    #[test]
    fn test_bridge_error_display() {
        assert_eq!(
            format!("{}", BridgeError::Unreachable),
            "destination unreachable"
        );
        assert_eq!(
            format!("{}", BridgeError::Internal("boom".to_string())),
            "internal error: boom"
        );
    }

    /// Idempotency contract (RFC-0011-o §Implicit Assumptions Audit
    /// row 6): `propagate_to` is idempotent on `slash_envelope_id`.
    /// `InMemorySlashBridge` is structurally idempotent (same input
    /// yields same receipt). Calling twice with the same id must
    /// return equal receipts.
    #[test]
    fn test_propagate_to_twice_with_same_id_yields_identical_receipt() {
        let slashes = vec![BridgedSlash {
            slash_envelope_id: [0x42; 32],
            bridge_metadata: BTreeMap::new(),
            bridged_at_epoch: 100,
        }];
        let bridge = InMemorySlashBridge { slashes };
        let r1 = bridge.propagate_to([0x42; 32]).unwrap();
        let r2 = bridge.propagate_to([0x42; 32]).unwrap();
        assert_eq!(r1, r2);
    }

    /// `EmptyBridge` stub (R1.5 fix): `list` returns empty `Vec`;
    /// `propagate_to` returns `Unreachable` for any input.
    #[test]
    fn test_empty_bridge_list_returns_empty() {
        let bridge = EmptyBridge;
        assert_eq!(bridge.list().len(), 0);
    }

    #[test]
    fn test_empty_bridge_propagate_returns_unreachable() {
        let bridge = EmptyBridge;
        let result = bridge.propagate_to([0x99; 32]);
        assert!(matches!(result, Err(BridgeError::Unreachable)));
    }

    /// `EmptyBridge::propagate_to` is structurally idempotent: same
    /// input returns same `Unreachable` error both times.
    #[test]
    fn test_empty_bridge_propagate_idempotent() {
        let bridge = EmptyBridge;
        let r1 = bridge.propagate_to([0x42; 32]);
        let r2 = bridge.propagate_to([0x42; 32]);
        assert_eq!(r1, r2);
        assert!(matches!(r1, Err(BridgeError::Unreachable)));
    }
}
