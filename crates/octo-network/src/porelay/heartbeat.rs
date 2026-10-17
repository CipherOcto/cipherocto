//! Gateway Heartbeat (RFC-0860 §3.2)

use serde::{Deserialize, Serialize};

mod serde_sig {
    use serde::{Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        sig.as_slice().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = serde_bytes::deserialize(d)?;
        v.try_into()
            .map_err(|_| serde::de::Error::custom("expected exactly 64 bytes for signature"))
    }
}

/// Gateway Heartbeat — proves gateway is online and responsive.
///
/// 7 fields per RFC-0860 §3.2.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct GatewayHeartbeat {
    /// Gateway sending the heartbeat
    pub gateway_id: [u8; 32],
    /// Monotonically increasing sequence
    pub sequence: u64,
    /// Number of active relay routes
    pub active_routes: u32,
    /// Load class (0-255, where 0 = idle, 255 = saturated)
    pub load_class: u8,
    /// Uptime class (0-255, where 0 = just started, 255 = maximum uptime)
    pub uptime_class: u8,
    /// Logical timestamp
    pub logical_timestamp: u64,
    /// Ed25519 signature over all above fields
    #[serde(with = "serde_sig")]
    pub signature: [u8; 64],
}

impl GatewayHeartbeat {
    /// Compute canonical bytes for signing (all fields except signature)
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 8 + 4 + 1 + 1 + 8);
        buf.extend_from_slice(&self.gateway_id);
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.active_routes.to_be_bytes());
        buf.push(self.load_class);
        buf.push(self.uptime_class);
        buf.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        buf
    }

    /// Verify the signature, dispatching on `algorithm` (RFC-0009 AC#11).
    ///
    /// The key arrives from the caller as bytes, so the caller is the only
    /// party that knows which algorithm they belong to — hence the parameter.
    /// An unregistered algorithm fails closed.
    pub fn verify_signature(&self, algorithm: &octo_ident::AlgorithmId, public_key: &[u8]) -> bool {
        octo_algo_ed25519::verify_dispatch(
            algorithm,
            public_key,
            &self.to_signing_bytes(),
            &self.signature,
        )
        .is_ok()
    }

    /// Full verification: signature + sequence monotonicity.
    pub fn verify_full(
        &self,
        algorithm: &octo_ident::AlgorithmId,
        public_key: &[u8],
        previous_sequence: u64,
    ) -> bool {
        self.sequence > previous_sequence && self.verify_signature(algorithm, public_key)
    }
}

/// Load classification for heartbeats
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum LoadClass {
    Idle = 0x00,
    Light = 0x40,
    Moderate = 0x80,
    Heavy = 0xC0,
    Saturated = 0xFF,
}

/// Uptime classification for heartbeats
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum UptimeClass {
    JustStarted = 0x00,
    Minutes = 0x40,
    Hours = 0x80,
    Days = 0xC0,
    Maximum = 0xFF,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;

    #[test]
    fn test_heartbeat_signing_bytes_size() {
        let hb = GatewayHeartbeat {
            gateway_id: [0u8; 32],
            sequence: 1,
            active_routes: 5,
            load_class: 0x80,
            uptime_class: 0xC0,
            logical_timestamp: 100,
            signature: [0u8; 64],
        };
        assert_eq!(hb.to_signing_bytes().len(), 54);
    }

    #[test]
    fn test_load_class_values() {
        assert_eq!(LoadClass::Idle as u8, 0x00);
        assert_eq!(LoadClass::Saturated as u8, 0xFF);
    }

    // ── AC#11: dispatch on the caller's declared algorithm ────────────────

    fn signed_heartbeat() -> (ed25519_dalek::SigningKey, GatewayHeartbeat) {
        let sk = ed25519_dalek::SigningKey::from_bytes(&[0x33u8; 32]);
        let mut hb = GatewayHeartbeat {
            gateway_id: [0x11u8; 32],
            sequence: 7,
            active_routes: 3,
            load_class: 0,
            uptime_class: 1,
            logical_timestamp: 1_700_000_000,
            signature: [0u8; 64],
        };
        let sig = sk.sign(&hb.to_signing_bytes());
        hb.signature = sig.to_bytes();
        (sk, hb)
    }

    /// The happy path with Ed25519 named explicitly.
    ///
    /// **Control:** this cannot distinguish dispatch from a hardcoded
    /// Ed25519 — see the unregistered-algorithm vector below, which can.
    #[test]
    fn heartbeat_dispatch_on_declared_algorithm_verifies() {
        let (sk, hb) = signed_heartbeat();
        let pk = sk.verifying_key().to_bytes();
        assert!(hb.verify_signature(&octo_algo_ed25519::ed25519_algorithm_id(), &pk));
    }

    /// The fail-closed half, and the only vector that proves the dispatch.
    ///
    /// A genuine Ed25519 signature, a valid key, correct sequence — the only
    /// thing wrong is that the caller named an algorithm this deployment has
    /// not registered. Before the change this was inexpressible: the method
    /// took a bare key and Ed25519 was the only thing it could mean.
    #[test]
    fn heartbeat_refuses_an_unregistered_algorithm() {
        let (sk, hb) = signed_heartbeat();
        let pk = sk.verifying_key().to_bytes();
        let bogus = octo_ident::AlgorithmId::derive("not-a-registered-heartbeat-algorithm");
        assert_ne!(bogus, octo_algo_ed25519::ed25519_algorithm_id());
        assert!(
            !hb.verify_signature(&bogus, &pk),
            "an unregistered algorithm must fail closed"
        );
    }

    /// Dispatch must not degrade the check into a lookup.
    #[test]
    fn heartbeat_still_refuses_a_bad_signature() {
        let (sk, hb) = signed_heartbeat();
        let other = ed25519_dalek::SigningKey::from_bytes(&[0x44u8; 32]);
        let mut bad = hb.clone();
        bad.signature = other.sign(&hb.to_signing_bytes()).to_bytes();
        assert!(!bad.verify_signature(
            &octo_algo_ed25519::ed25519_algorithm_id(),
            &sk.verifying_key().to_bytes()
        ));
    }

    #[test]
    fn test_uptime_class_values() {
        assert_eq!(UptimeClass::JustStarted as u8, 0x00);
        assert_eq!(UptimeClass::Maximum as u8, 0xFF);
    }
}
