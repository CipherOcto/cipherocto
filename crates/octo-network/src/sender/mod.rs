//! Network sender substrate (RFC-0863 General-Purpose Network Integration).
//!
//! Mission 0011-h-s-a-network-sender (RFC-0011-n Phase 6 G20 NEW
//! companion) — per-extension crate pattern (trait in Layer B,
//! per-transport impl crates in Layer D, registry lookup at runtime).
//!
//! ## Substrate surface
//!
//! - [`NetworkSender`] — trait; each transport (BLE, USB, TCP,
//!   QUIC, HID) ships its own Layer D impl crate.
//! - [`SendContext`] — wire-format + metadata for a single outbound
//!   payload (RFC-0863 §Network Sender L116 anchor).
//! - [`WireFormat`] — TCP-JSON / QUIC-Binary / BLE-CBOR / USB-Raw
//!   wire-format discriminator.
//! - [`NetworkSenderRegistry`] — in-process registry of
//!   `NetworkSender` impls indexed by transport tag.
//! - [`SendSummary`] — observability summary for `octo network status`.
//!
//! Per [[cipherocto-design-principles]] §User extensibility, the trait
//! lives in Layer B (`octo-network`); per-transport impls land in
//! separate Layer D crates. The registry is the only place that knows
//! about every transport.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Stable per-extension transport tag (e.g. "tcp-json",
/// "quic-binary", "ble-cbor"). Used by the registry to route
/// payloads to the correct impl.
pub type TransportTag = &'static str;

/// `SendContext` carries the wire-format + metadata for a single
/// outbound payload (RFC-0863 §Network Sender L116 anchor).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendContext {
    pub destination: [u8; 32],
    pub payload_kind: String,
    pub wire_format: WireFormat,
    pub attempts: u32,
}

/// Wire format discriminator per RFC-0863 §Wire Format.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WireFormat {
    /// TCP transport + JSON payload encoding.
    TcpJson,
    /// QUIC transport + binary payload encoding.
    QuicBinary,
    /// BLE transport + CBOR payload encoding.
    BleCbor,
    /// USB transport + raw byte payload encoding.
    UsbRaw,
}

/// `NetworkSender` trait per RFC-0863 §Network Sender trait anchor
/// at line 104. Per-extension crate pattern: each transport (BLE,
/// USB, TCP, QUIC, HID) ships in its own Layer D crate implementing
/// this trait. Core `octo-network` Layer B owns only the trait +
/// `SendContext` types.
pub trait NetworkSender: Send + Sync {
    /// Send a payload with the given `SendContext`. Returns the
    /// number of bytes actually transmitted on success.
    fn send(&self, ctx: &SendContext, payload: &[u8]) -> Result<usize, NetworkSendError>;

    /// Stable per-extension transport tag (e.g. "tcp-json",
    /// "quic-binary", "ble-cbor"). Used by the registry to route
    /// payloads to the correct impl.
    fn transport_tag(&self) -> TransportTag;

    /// Substrate-faithful observability helper (operator-side
    /// `octo network status` surface).
    fn last_send_summary(&self) -> Option<SendSummary>;
}

/// Substrate-faithful observability summary for `octo network status`
/// (RFC-0011-n Phase 6 G20 NEW companion).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendSummary {
    pub transport_tag: String,
    pub bytes_sent: u64,
    pub attempts: u64,
}

/// Substrate-faithful error surface for `NetworkSender::send`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkSendError {
    /// Destination is unreachable.
    Unreachable,
    /// Destination refused the connection.
    Refused,
    /// Payload size exceeds the transport's maximum.
    PayloadTooLarge { size: usize, max: usize },
    /// Wire format mismatch between context and transport.
    WireFormatMismatch {
        expected: WireFormat,
        got: WireFormat,
    },
    /// Internal error (substrate-faithful escape hatch for
    /// per-transport impls).
    Internal(String),
}

impl std::fmt::Display for NetworkSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable => write!(f, "network destination unreachable"),
            Self::Refused => write!(f, "network destination refused"),
            Self::PayloadTooLarge { size, max } => {
                write!(f, "payload size {size} exceeds max {max}")
            }
            Self::WireFormatMismatch { expected, got } => {
                write!(
                    f,
                    "wire format mismatch: expected {expected:?}, got {got:?}"
                )
            }
            Self::Internal(s) => write!(f, "network send internal error: {s}"),
        }
    }
}

impl std::error::Error for NetworkSendError {}

/// In-process registry of `NetworkSender` impls indexed by
/// transport tag. Substrate-faithful to the per-extension
/// registry pattern per [[cipherocto-design-principles]] §User
/// extensibility.
#[derive(Default)]
pub struct NetworkSenderRegistry {
    senders: BTreeMap<TransportTag, Arc<dyn NetworkSender>>,
}

impl NetworkSenderRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a `NetworkSender` impl by transport tag. Idempotent:
    /// re-registering the same tag overwrites the previous impl.
    pub fn register(&mut self, sender: Arc<dyn NetworkSender>) {
        self.senders.insert(sender.transport_tag(), sender);
    }

    /// Look up a `NetworkSender` by transport tag. Returns `None`
    /// if the tag is not registered.
    #[must_use]
    pub fn get(&self, tag: TransportTag) -> Option<Arc<dyn NetworkSender>> {
        self.senders.get(tag).cloned()
    }

    /// Iterate registered senders in deterministic (BTreeMap)
    /// order. Returns (transport_tag, Arc<dyn NetworkSender>) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (TransportTag, &Arc<dyn NetworkSender>)> {
        self.senders.iter().map(|(k, v)| (*k, v))
    }

    /// Number of registered senders.
    #[must_use]
    pub fn len(&self) -> usize {
        self.senders.len()
    }

    /// Whether the registry has no senders registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.senders.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test sender impl for the unit tests.
    struct TestSender {
        tag: TransportTag,
        last_summary: Option<SendSummary>,
    }

    impl TestSender {
        fn new(tag: TransportTag) -> Self {
            Self {
                tag,
                last_summary: None,
            }
        }
    }

    impl NetworkSender for TestSender {
        fn send(&self, _ctx: &SendContext, payload: &[u8]) -> Result<usize, NetworkSendError> {
            Ok(payload.len())
        }

        fn transport_tag(&self) -> TransportTag {
            self.tag
        }

        fn last_send_summary(&self) -> Option<SendSummary> {
            self.last_summary.clone()
        }
    }

    #[test]
    fn registry_new_is_empty() {
        let reg = NetworkSenderRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = NetworkSenderRegistry::default();
        assert!(reg.is_empty());
    }

    #[test]
    fn registry_register_and_get_hit() {
        let mut reg = NetworkSenderRegistry::new();
        let sender = Arc::new(TestSender::new("tcp-json"));
        reg.register(sender);
        assert_eq!(reg.len(), 1);
        let retrieved = reg.get("tcp-json");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().transport_tag(), "tcp-json");
    }

    #[test]
    fn registry_get_miss_returns_none() {
        let reg = NetworkSenderRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_iter_deterministic_order() {
        let mut reg = NetworkSenderRegistry::new();
        reg.register(Arc::new(TestSender::new("zzz")));
        reg.register(Arc::new(TestSender::new("aaa")));
        reg.register(Arc::new(TestSender::new("mmm")));
        let tags: Vec<TransportTag> = reg.iter().map(|(tag, _)| tag).collect();
        assert_eq!(tags, vec!["aaa", "mmm", "zzz"]);
    }

    #[test]
    fn registry_register_overwrites_idempotently() {
        let mut reg = NetworkSenderRegistry::new();
        let s1 = Arc::new(TestSender::new("tcp-json"));
        let s2 = Arc::new(TestSender::new("tcp-json"));
        reg.register(s1);
        reg.register(s2);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn send_context_clone_preserves_fields() {
        let ctx = SendContext {
            destination: [0xABu8; 32],
            payload_kind: "test-payload".to_string(),
            wire_format: WireFormat::TcpJson,
            attempts: 3,
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.destination, [0xABu8; 32]);
        assert_eq!(cloned.payload_kind, "test-payload");
        assert_eq!(cloned.wire_format, WireFormat::TcpJson);
        assert_eq!(cloned.attempts, 3);
    }

    #[test]
    fn wire_format_partial_eq() {
        assert_eq!(WireFormat::TcpJson, WireFormat::TcpJson);
        assert_ne!(WireFormat::TcpJson, WireFormat::QuicBinary);
        assert_ne!(WireFormat::BleCbor, WireFormat::UsbRaw);
    }

    #[test]
    fn send_summary_serde_round_trip() {
        let summary = SendSummary {
            transport_tag: "tcp-json".to_string(),
            bytes_sent: 1024,
            attempts: 5,
        };
        let json = serde_json::to_string(&summary).expect("serialize");
        let decoded: SendSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.transport_tag, "tcp-json");
        assert_eq!(decoded.bytes_sent, 1024);
        assert_eq!(decoded.attempts, 5);
    }

    #[test]
    fn network_send_error_display() {
        let err = NetworkSendError::PayloadTooLarge {
            size: 1000,
            max: 500,
        };
        assert_eq!(err.to_string(), "payload size 1000 exceeds max 500");
    }

    #[test]
    fn trait_object_round_trip() {
        let sender: Arc<dyn NetworkSender> = Arc::new(TestSender::new("quic-binary"));
        let ctx = SendContext {
            destination: [0x01u8; 32],
            payload_kind: "test".to_string(),
            wire_format: WireFormat::QuicBinary,
            attempts: 1,
        };
        let payload = b"hello world";
        let result = sender.send(&ctx, payload);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), payload.len());
    }
}
