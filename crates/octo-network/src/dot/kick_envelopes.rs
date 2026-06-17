//! Kick & Platform Membership Change Detection envelopes — RFC-0850p-e
//!
//! Implements the 5 envelope types from RFC-0850p-e:
//!
//! - `SelfKickedEnvelope` (subtype `b"SFCK"`)
//! - `KickDetectedEnvelope` (subtype `b"KFDT"`)
//! - `MemberRemovedEnvelope` (subtype `b"MREM"`)
//! - `RejoinRequestEnvelope` (subtype `b"RJRQ"`)
//! - `RejoinGrantEnvelope` (subtype `b"RJGT"`)
//!
//! Closes the CRITICAL E2E implicit spec IS-5.1: a kicked bot must
//! detect the removal within 5 epochs and emit `SELF_KICKED`; otherwise
//! the group becomes a zombie partition that the DC cannot REBIND.
//!
//! See mission `missions/claimed/0850p-e-kick-detection.md` for the full
//! requirements. Cross-references:
//! - `super::binding::PlatformLossEnvelope` (defined in 0850p-c-base;
//!   emitted by adapters on local kick detection)
//! - `super::binding::WitnessAssertion` (defined in 0850p-d; carried in
//!   `KickDetectedEnvelope.witness_assertion`)
//! - `super::group_registry::REJOIN_GRANT_TIMEOUT` (= 50 epochs)
//! - `super::slash::SlashCode::SelfKicked` and `::FalseWitness`

use blake3;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::binding::{header, WitnessAssertion};
use super::error::DotError;

/// 4-byte ASCII subtype tags for 0850p-e envelopes.
pub mod tag {
    /// `SelfKickedEnvelope` — bot self-reports it was removed from the
    /// group (no witness required).
    pub const SELF_KICKED: [u8; 4] = *b"SFCK";
    /// `KickDetectedEnvelope` — third-party claim (witness required).
    pub const KICK_DETECTED: [u8; 4] = *b"KFDT";
    /// `MemberRemovedEnvelope` — informational; another member was
    /// removed (not the local bot).
    pub const MEMBER_REMOVED: [u8; 4] = *b"MREM";
    /// `RejoinRequestEnvelope` — kicked node requests permission to
    /// rejoin the group.
    pub const REJOIN_REQUEST: [u8; 4] = *b"RJRQ";
    /// `RejoinGrantEnvelope` — DC grants the rejoin permission.
    pub const REJOIN_GRANT: [u8; 4] = *b"RJGT";
}

/// Platform kick event classification (RFC-0850p-e §"Per-Adapter Detection
/// Strategies").
///
/// This is the kick-detection-layer classification; the canonical adapter
/// event is `PlatformEvent::KickedFromGroup { group_jid, kick_epoch, kicker_participant_id }`
/// (per RFC-0855p-c §3), which the adapter maps to one of these
/// values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PlatformKickEvent {
    /// The bot was removed by an admin (e.g., "kicked" on WhatsApp,
    /// `membership: ban` on Matrix, `status: kicked` on Telegram).
    YouGotKicked = 0x00,
    /// The bot voluntarily left the group.
    YouLeft = 0x01,
    /// The group was dissolved by its founder.
    GroupDissolved = 0x02,
    /// The group disappeared from the platform (e.g., all members
    /// left, or the platform deleted it).
    GroupDisappeared = 0x03,
    /// The bot's session was lost (e.g., authentication token
    /// expired).
    SessionLost = 0x04,
    /// The kick-detection heartbeat timed out (50 epochs); used by
    /// the local-node fallback path.
    HeartbeatTimeout = 0x05,
}

impl PlatformKickEvent {
    /// Construct from wire byte.
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::YouGotKicked),
            0x01 => Some(Self::YouLeft),
            0x02 => Some(Self::GroupDissolved),
            0x03 => Some(Self::GroupDisappeared),
            0x04 => Some(Self::SessionLost),
            0x05 => Some(Self::HeartbeatTimeout),
            _ => None,
        }
    }

    /// Returns the wire byte.
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    /// Returns `true` if this event indicates the bot is no longer in
    /// the group (and therefore should transition to
    /// `UnboundQuarantined`).
    pub fn is_kick(self) -> bool {
        matches!(
            self,
            Self::YouGotKicked
                | Self::GroupDissolved
                | Self::GroupDisappeared
                | Self::SessionLost
                | Self::HeartbeatTimeout
        )
    }
}

// -----------------------------------------------------------------------------
// Kick-detection reason codes (out of slash reason code space)
// -----------------------------------------------------------------------------

/// Kick-detection reason code space (0xF0xx).
///
/// Per RFC-0850p-e §"Reason Codes for KICK_DETECTED" (R16 R1-M4 fix —
/// moved out of the slash reason code space 0x0001-0xFFFF into the
/// 0xF0xx kick-detection layer code space).
pub mod reason_code {
    /// Status could not be determined within `KICK_DETECTION_TIMEOUT = 50`
    /// epochs; transition to `UnboundQuarantined` on the assumption
    /// of a kick.
    pub const STATUS_TIMEOUT: u16 = 0xF001;
    /// A witness observed the kick via platform-side query.
    pub const WITNESS_OBSERVATION: u16 = 0xF002;
    /// The DC observed the kick via platform-side query.
    pub const DC_OBSERVATION: u16 = 0xF003;
}

// -----------------------------------------------------------------------------
// SelfKickedEnvelope
// -----------------------------------------------------------------------------

/// `SelfKickedEnvelope` — bot self-reports it was removed from the group.
///
/// No witness is required; the bot knows it was kicked because the
/// platform told it directly (via `GroupParticipantRemove` on WhatsApp,
/// `m.room.member` on Matrix, `Update.chat_member` on Telegram). On
/// emission, the local node transitions the binding to
/// `UnboundQuarantined` and moves it to the `unbound_quarantine` map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfKickedEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Platform kick event classification.
    pub platform_event: PlatformKickEvent,
    /// Epoch at which the kick was detected.
    pub detected_at_epoch: u64,
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub self_kicked_hash: [u8; 32],
    /// Ed25519 signature over `self_kicked_hash`.
    pub signature: [u8; 64],
}

impl SelfKickedEnvelope {
    /// Compute `self_kicked_hash`.
    pub fn compute_self_kicked_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&header(tag::SELF_KICKED));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.push(self.platform_event.as_byte());
        buf.extend_from_slice(&self.detected_at_epoch.to_be_bytes());
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.self_kicked_hash = self.compute_self_kicked_hash();
        self.signature = key.sign(&self.self_kicked_hash).to_bytes();
    }

    /// Verify against the local node's public key.
    pub fn verify(&self, local_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_self_kicked_hash();
        if computed != self.self_kicked_hash {
            return Err(DotError::Serialization(
                "SelfKickedEnvelope: self_kicked_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        local_pubkey
            .verify(&self.self_kicked_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.self_kicked_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// KickDetectedEnvelope
// -----------------------------------------------------------------------------

/// `KickDetectedEnvelope` — witness or DC claim that a node was kicked.
///
/// The witness assertion (RFC-0850p-d §D) is required. On receipt, the
/// DC validates the assertion and, if valid, transitions the binding
/// to `UnboundQuarantined`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KickDetectedEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Public key of the kicked node.
    pub kicked_node_id: [u8; 32],
    /// Reason code (kick-detection layer; e.g., `STATUS_TIMEOUT`).
    pub reason_code: u16,
    /// Epoch at which the kick was detected.
    pub detected_at_epoch: u64,
    /// Witness assertion proving the kick.
    pub witness_assertion: WitnessAssertion,
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub kick_detected_hash: [u8; 32],
    /// Ed25519 signature over `kick_detected_hash`.
    pub signature: [u8; 64],
}

impl KickDetectedEnvelope {
    /// Compute `kick_detected_hash`.
    pub fn compute_kick_detected_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&header(tag::KICK_DETECTED));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.kicked_node_id);
        buf.extend_from_slice(&self.reason_code.to_be_bytes());
        buf.extend_from_slice(&self.detected_at_epoch.to_be_bytes());
        // Fold the witness assertion's hash into the body.
        buf.extend_from_slice(&self.witness_assertion.assertion_hash);
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.kick_detected_hash = self.compute_kick_detected_hash();
        self.signature = key.sign(&self.kick_detected_hash).to_bytes();
    }

    /// Verify against the witness/DC's public key.
    pub fn verify(&self, issuer_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_kick_detected_hash();
        if computed != self.kick_detected_hash {
            return Err(DotError::Serialization(
                "KickDetectedEnvelope: kick_detected_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        issuer_pubkey
            .verify(&self.kick_detected_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.kick_detected_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// MemberRemovedEnvelope
// -----------------------------------------------------------------------------

/// `MemberRemovedEnvelope` — informational; another member (not the
/// local bot) was removed.
///
/// The DC MAY emit this envelope to inform other nodes of a non-local
/// kick. It does NOT trigger REBIND.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRemovedEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Public key of the removed member.
    pub removed_member_id: [u8; 32],
    /// Epoch at which the removal was observed.
    pub observed_at_epoch: u64,
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub member_removed_hash: [u8; 32],
    /// Ed25519 signature over `member_removed_hash`.
    pub signature: [u8; 64],
}

impl MemberRemovedEnvelope {
    /// Compute `member_removed_hash`.
    pub fn compute_member_removed_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&header(tag::MEMBER_REMOVED));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.removed_member_id);
        buf.extend_from_slice(&self.observed_at_epoch.to_be_bytes());
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.member_removed_hash = self.compute_member_removed_hash();
        self.signature = key.sign(&self.member_removed_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_member_removed_hash();
        if computed != self.member_removed_hash {
            return Err(DotError::Serialization(
                "MemberRemovedEnvelope: member_removed_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.member_removed_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.member_removed_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// RejoinRequestEnvelope
// -----------------------------------------------------------------------------

/// `RejoinRequestEnvelope` — kicked node requests permission to rejoin
/// the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejoinRequestEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Public key of the requesting node.
    pub requester_id: [u8; 32],
    /// Current epoch.
    pub current_epoch: u64,
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub request_hash: [u8; 32],
    /// Ed25519 signature over `request_hash`.
    pub signature: [u8; 64],
}

impl RejoinRequestEnvelope {
    /// Compute `request_hash`.
    pub fn compute_request_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&header(tag::REJOIN_REQUEST));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.requester_id);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.request_hash = self.compute_request_hash();
        self.signature = key.sign(&self.request_hash).to_bytes();
    }

    /// Verify against the requester's public key.
    pub fn verify(&self, requester_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_request_hash();
        if computed != self.request_hash {
            return Err(DotError::Serialization(
                "RejoinRequestEnvelope: request_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        requester_pubkey
            .verify(&self.request_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.request_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// RejoinGrantEnvelope
// -----------------------------------------------------------------------------

/// `RejoinGrantEnvelope` — DC grants the rejoin permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejoinGrantEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Public key of the requester being granted.
    pub requester_id: [u8; 32],
    /// The `request_hash` of the `RejoinRequestEnvelope` being
    /// granted.
    pub request_hash: [u8; 32],
    /// A fresh invite token the requester can use to authenticate
    /// the platform-side re-join.
    pub fresh_invite_token: [u8; 32],
    /// Epoch at which the grant was issued.
    pub granted_at_epoch: u64,
    /// Expiry epoch (the grant is invalid after this epoch).
    pub expires_at_epoch: u64,
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub grant_hash: [u8; 32],
    /// Ed25519 signature over `grant_hash`.
    pub signature: [u8; 64],
}

impl RejoinGrantEnvelope {
    /// Compute `grant_hash`.
    pub fn compute_grant_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&header(tag::REJOIN_GRANT));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.requester_id);
        buf.extend_from_slice(&self.request_hash);
        buf.extend_from_slice(&self.fresh_invite_token);
        buf.extend_from_slice(&self.granted_at_epoch.to_be_bytes());
        buf.extend_from_slice(&self.expires_at_epoch.to_be_bytes());
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.grant_hash = self.compute_grant_hash();
        self.signature = key.sign(&self.grant_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_grant_hash();
        if computed != self.grant_hash {
            return Err(DotError::Serialization(
                "RejoinGrantEnvelope: grant_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.grant_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.grant_hash,
            })?;
        Ok(())
    }

    /// `true` if this grant has expired (relative to `current_epoch`).
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        current_epoch >= self.expires_at_epoch
    }
}

// -----------------------------------------------------------------------------
// Serialization helpers
// -----------------------------------------------------------------------------

/// Write a length-prefixed string (u32 BE length, then UTF-8 bytes).
fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn make_assertion(witness: &SigningKey, subject: [u8; 32], epoch: u64) -> WitnessAssertion {
        let mut a = WitnessAssertion {
            subject_hash: subject,
            witness_id: witness.verifying_key().to_bytes(),
            witness_epoch: epoch,
            assertion_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        a.sign(witness);
        a
    }

    #[test]
    fn platform_kick_event_roundtrip() {
        for ev in [
            PlatformKickEvent::YouGotKicked,
            PlatformKickEvent::YouLeft,
            PlatformKickEvent::GroupDissolved,
            PlatformKickEvent::GroupDisappeared,
            PlatformKickEvent::SessionLost,
            PlatformKickEvent::HeartbeatTimeout,
        ] {
            assert_eq!(PlatformKickEvent::from_byte(ev.as_byte()), Some(ev));
        }
        assert_eq!(PlatformKickEvent::from_byte(0xFF), None);
    }

    #[test]
    fn is_kick_classification() {
        assert!(PlatformKickEvent::YouGotKicked.is_kick());
        assert!(!PlatformKickEvent::YouLeft.is_kick());
        assert!(PlatformKickEvent::GroupDissolved.is_kick());
        assert!(PlatformKickEvent::GroupDisappeared.is_kick());
        assert!(PlatformKickEvent::SessionLost.is_kick());
        assert!(PlatformKickEvent::HeartbeatTimeout.is_kick());
    }

    #[test]
    fn self_kicked_sign_verify() {
        let key = test_key(1);
        let mut env = SelfKickedEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            platform_event: PlatformKickEvent::YouGotKicked,
            detected_at_epoch: 100,
            nonce: [2u8; 32],
            self_kicked_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn self_kicked_mutation_rejected() {
        let key = test_key(1);
        let mut env = SelfKickedEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            platform_event: PlatformKickEvent::YouGotKicked,
            detected_at_epoch: 100,
            nonce: [2u8; 32],
            self_kicked_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        env.detected_at_epoch = 200;
        assert!(env.verify(&key.verifying_key()).is_err());
    }

    #[test]
    fn kick_detected_sign_verify() {
        let wkey = test_key(2);
        let assertion = make_assertion(&wkey, [5u8; 32], 100);
        let mut env = KickDetectedEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            kicked_node_id: [3u8; 32],
            reason_code: reason_code::WITNESS_OBSERVATION,
            detected_at_epoch: 100,
            witness_assertion: assertion,
            nonce: [4u8; 32],
            kick_detected_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&wkey);
        assert!(env.verify(&wkey.verifying_key()).is_ok());
    }

    #[test]
    fn member_removed_sign_verify() {
        let dckey = test_key(3);
        let mut env = MemberRemovedEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            removed_member_id: [5u8; 32],
            observed_at_epoch: 100,
            nonce: [6u8; 32],
            member_removed_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&dckey);
        assert!(env.verify(&dckey.verifying_key()).is_ok());
    }

    #[test]
    fn rejoin_request_sign_verify() {
        let key = test_key(4);
        let mut env = RejoinRequestEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            requester_id: [7u8; 32],
            current_epoch: 100,
            nonce: [8u8; 32],
            request_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn rejoin_grant_sign_verify() {
        let dckey = test_key(5);
        let mut env = RejoinGrantEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            requester_id: [7u8; 32],
            request_hash: [9u8; 32],
            fresh_invite_token: [10u8; 32],
            granted_at_epoch: 100,
            expires_at_epoch: 200,
            nonce: [11u8; 32],
            grant_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&dckey);
        assert!(env.verify(&dckey.verifying_key()).is_ok());
        assert!(!env.is_expired(150));
        assert!(env.is_expired(200));
        assert!(env.is_expired(250));
    }

    #[test]
    fn header_subtypes_distinct() {
        let tags = [
            tag::SELF_KICKED,
            tag::KICK_DETECTED,
            tag::MEMBER_REMOVED,
            tag::REJOIN_REQUEST,
            tag::REJOIN_GRANT,
        ];
        for (i, a) in tags.iter().enumerate() {
            for (j, b) in tags.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b);
                }
            }
        }
        // Verify the ASCII spelling.
        assert_eq!(&tag::SELF_KICKED, b"SFCK");
        assert_eq!(&tag::KICK_DETECTED, b"KFDT");
        assert_eq!(&tag::MEMBER_REMOVED, b"MREM");
        assert_eq!(&tag::REJOIN_REQUEST, b"RJRQ");
        assert_eq!(&tag::REJOIN_GRANT, b"RJGT");
    }
}
