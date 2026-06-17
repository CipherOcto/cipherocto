//! DC-Initiated Group Creation & Invite envelopes — RFC-0850p-d
//!
//! Implements the 7 envelope types from RFC-0850p-d:
//!
//! - `CreateGroupEnvelope` (subtype `b"CGRO"`)
//! - `CreateGroupAckEnvelope` (subtype `b"CGAC"`; R16 R2-H1 fix —
//!   struct was missing from RFC-0850p-d)
//! - `CreateGroupDoneEnvelope` (subtype `b"CGDA"`)
//! - `CreateGroupFailEnvelope` (subtype `b"CGFA"`)
//! - `InviteEnvelope` (subtype `b"INVT"`)
//! - `UnbindAllEnvelope` (subtype `b"UALL"`)
//! - `UnbindAllAckEnvelope` (subtype `b"UAAC"`; R16 R2 fix — struct
//!   was missing from RFC-0850p-d)
//!
//! All envelopes use the canonical 10-byte header (`b"DOT1" || b"<TAG>" || 0x0001`)
//! and the DCS-style canonical serialization defined in `super::binding`.
//!
//! See mission `missions/claimed/0850p-d-dc-initiated-group-creation.md`
//! for the full requirements.

use blake3;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::binding::{header, write_bytes, write_string, GroupVisibility};
use super::error::DotError;

/// 4-byte ASCII subtype tags for 0850p-d envelopes.
pub mod tag {
    /// `CreateGroupEnvelope` — DC requests creation of a new transport group.
    pub const CREATE_GROUP: [u8; 4] = *b"CGRO";
    /// `CreateGroupAckEnvelope` — witness confirmation that it has seen
    /// the CGROUP and is reserving the `domain_id`.
    pub const CREATE_GROUP_ACK: [u8; 4] = *b"CGAC";
    /// `CreateGroupDoneEnvelope` — DC has successfully created the group
    /// on the platform; carries the `group_jid`.
    pub const CREATE_GROUP_DONE: [u8; 4] = *b"CGDA";
    /// `CreateGroupFailEnvelope` — DC failed to create the group on the
    /// platform; carries a `reason_code` and the platform-side error.
    pub const CREATE_GROUP_FAIL: [u8; 4] = *b"CGFA";
    /// `InviteEnvelope` — DC invites a node to join the group.
    pub const INVITE: [u8; 4] = *b"INVT";
    /// `UnbindAllEnvelope` — DC requests all members to leave the group.
    pub const UNBIND_ALL: [u8; 4] = *b"UALL";
    /// `UnbindAllAckEnvelope` — witness confirmation of the UNBIND_ALL.
    pub const UNBIND_ALL_ACK: [u8; 4] = *b"UAAC";
}

// -----------------------------------------------------------------------------
// CreateGroupEnvelope
// -----------------------------------------------------------------------------

/// `CreateGroupEnvelope` — DC requests creation of a new transport group.
///
/// The DC emits this envelope; the local platform adapter creates the
/// group, then emits `CreateGroupDoneEnvelope` with the `group_jid`.
/// On failure, `CreateGroupFailEnvelope` is emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateGroupEnvelope {
    /// Domain identifier (32-byte BLAKE3-256).
    pub domain_id: [u8; 32],
    /// Mission identifier.
    pub mission_id: [u8; 32],
    /// Target platform string (e.g., `"whatsapp"`).
    pub platform: String,
    /// Platform-agnostic group metadata (JSON-serialized UTF-8).
    pub proposed_group_metadata: Vec<u8>,
    /// Number of invitees the DC intends to invite.
    pub initial_invite_count: u32,
    /// Public key of the DC.
    pub dc_id: [u8; 32],
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// Current epoch.
    pub current_epoch: u64,
    /// DC coordinator term identifier (used to disambiguate DC rotations).
    pub coordinator_term_id: u64,
    /// Group visibility (R16 R1-M2 fix).
    pub group_visibility: GroupVisibility,
    /// `BLAKE3-256(header || body)`.
    pub cgroup_hash: [u8; 32],
    /// Ed25519 signature over `cgroup_hash`.
    pub signature: [u8; 64],
}

impl CreateGroupEnvelope {
    /// Compute `cgroup_hash`.
    pub fn compute_cgroup_hash(&self) -> [u8; 32] {
        *blake3::hash(&self.body_bytes()).as_bytes()
    }

    /// Sign the envelope in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.cgroup_hash = self.compute_cgroup_hash();
        self.signature = key.sign(&self.cgroup_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_cgroup_hash();
        if computed != self.cgroup_hash {
            return Err(DotError::Serialization(
                "CreateGroupEnvelope: cgroup_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.cgroup_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.cgroup_hash,
            })?;
        Ok(())
    }

    /// Serialize the canonical bytes: 10-byte header followed by the body.
    ///
    /// R17 R1-LOW-9 fix: the previous name `body_bytes` was misleading
    /// because the function included the canonical header (unlike
    /// `BindEnvelope::body_bytes`, which does NOT include the header).
    /// The function continues to be named `body_bytes` for backwards
    /// compatibility, but the doc-comment now makes the header
    /// inclusion explicit. The hash (`compute_cgroup_hash`) is
    /// unchanged — it always hashed the full canonical serialization.
    pub fn body_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&header(tag::CREATE_GROUP));
        buf.extend_from_slice(&self.domain_id);
        buf.extend_from_slice(&self.mission_id);
        write_string(&mut buf, &self.platform);
        write_bytes(&mut buf, &self.proposed_group_metadata);
        buf.extend_from_slice(&self.initial_invite_count.to_be_bytes());
        buf.extend_from_slice(&self.dc_id);
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.coordinator_term_id.to_be_bytes());
        buf.push(self.group_visibility.as_byte());
        buf
    }
}

// -----------------------------------------------------------------------------
// CreateGroupAckEnvelope
// -----------------------------------------------------------------------------

/// `CreateGroupAckEnvelope` — witness confirmation of a `CreateGroupEnvelope`.
///
/// (R16 R2-H1 fix: this struct was listed in the envelope type table but
/// had no struct definition in the original RFC-0850p-d v1.0.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateGroupAckEnvelope {
    /// Domain identifier being reserved.
    pub domain_id: [u8; 32],
    /// The `cgroup_hash` of the `CreateGroupEnvelope` being acknowledged.
    pub cgroup_hash: [u8; 32],
    /// Public key of the witness.
    pub witness_id: [u8; 32],
    /// Epoch at which the witness observed the CGROUP.
    pub witness_epoch: u64,
    /// `BLAKE3-256(header || domain_id || cgroup_hash || witness_id || witness_epoch)`.
    pub ack_hash: [u8; 32],
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// Ed25519 signature over `ack_hash`.
    pub signature: [u8; 64],
}

impl CreateGroupAckEnvelope {
    /// Compute `ack_hash`.
    ///
    /// Per RFC-0850p-d §C, the nonce is INCLUDED in the canonical hash.
    /// R17 R1-CRITICAL-1 fix (was missing from the hash, allowing replay
    /// with attacker-chosen nonces).
    pub fn compute_ack_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(10 + 32 + 32 + 32 + 8 + 32);
        buf.extend_from_slice(&header(tag::CREATE_GROUP_ACK));
        buf.extend_from_slice(&self.domain_id);
        buf.extend_from_slice(&self.cgroup_hash);
        buf.extend_from_slice(&self.witness_id);
        buf.extend_from_slice(&self.witness_epoch.to_be_bytes());
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.ack_hash = self.compute_ack_hash();
        self.signature = key.sign(&self.ack_hash).to_bytes();
    }

    /// Verify against the witness's public key.
    pub fn verify(&self, witness_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_ack_hash();
        if computed != self.ack_hash {
            return Err(DotError::Serialization(
                "CreateGroupAckEnvelope: ack_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        witness_pubkey
            .verify(&self.ack_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.ack_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// CreateGroupDoneEnvelope
// -----------------------------------------------------------------------------

/// `CreateGroupDoneEnvelope` — DC has successfully created the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateGroupDoneEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// The platform-specific group identifier assigned by the platform.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// The `cgroup_hash` of the `CreateGroupEnvelope` being confirmed.
    pub cgroup_hash: [u8; 32],
    /// The `nonce` of the `CreateGroupEnvelope` (proves correlation).
    pub nonce: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub done_hash: [u8; 32],
    /// Ed25519 signature over `done_hash`.
    pub signature: [u8; 64],
}

impl CreateGroupDoneEnvelope {
    /// Compute `done_hash`.
    pub fn compute_done_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&header(tag::CREATE_GROUP_DONE));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.cgroup_hash);
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.done_hash = self.compute_done_hash();
        self.signature = key.sign(&self.done_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_done_hash();
        if computed != self.done_hash {
            return Err(DotError::Serialization(
                "CreateGroupDoneEnvelope: done_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.done_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.done_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// CreateGroupFailEnvelope
// -----------------------------------------------------------------------------

/// `CreateGroupFailEnvelope` — DC failed to create the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateGroupFailEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Target platform.
    pub platform: String,
    /// The `cgroup_hash` of the `CreateGroupEnvelope` being reported on.
    pub cgroup_hash: [u8; 32],
    /// Reason code (`CreateGroupFailed` = 0x000E, `CgGroupSpam` = 0x000F, etc.).
    pub reason_code: u16,
    /// Platform-side error message (UTF-8).
    pub platform_error: String,
    /// `BLAKE3-256(header || body)`.
    pub fail_hash: [u8; 32],
    /// Ed25519 signature over `fail_hash`.
    pub signature: [u8; 64],
}

impl CreateGroupFailEnvelope {
    /// Compute `fail_hash`.
    pub fn compute_fail_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&header(tag::CREATE_GROUP_FAIL));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.cgroup_hash);
        buf.extend_from_slice(&self.reason_code.to_be_bytes());
        write_string(&mut buf, &self.platform_error);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.fail_hash = self.compute_fail_hash();
        self.signature = key.sign(&self.fail_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_fail_hash();
        if computed != self.fail_hash {
            return Err(DotError::Serialization(
                "CreateGroupFailEnvelope: fail_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.fail_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.fail_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// InviteEnvelope
// -----------------------------------------------------------------------------

/// `InviteEnvelope` — DC invites a node to join the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// The platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Public key of the invitee.
    pub invitee_pubkey: [u8; 32],
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// `BLAKE3-256(domain_id || mission_id || invitee_pubkey || nonce)`.
    /// The invitee uses this token to authenticate the join.
    pub invite_token: [u8; 32],
    /// Mission identifier (folded into the invite_token derivation).
    pub mission_id: [u8; 32],
    /// Epoch at which the invite was emitted.
    pub current_epoch: u64,
    /// Expiry epoch (the invite is invalid after this epoch).
    pub expires_at_epoch: u64,
    /// `BLAKE3-256(header || body)`.
    pub invite_hash: [u8; 32],
    /// Ed25519 signature over `invite_hash`.
    pub signature: [u8; 64],
}

impl InviteEnvelope {
    /// Compute `invite_token`.
    pub fn compute_invite_token(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(32 + 32 + 32 + 32);
        buf.extend_from_slice(&self.domain_id);
        buf.extend_from_slice(&self.mission_id);
        buf.extend_from_slice(&self.invitee_pubkey);
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Compute `invite_hash`.
    pub fn compute_invite_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&header(tag::INVITE));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.extend_from_slice(&self.invitee_pubkey);
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.invite_token);
        buf.extend_from_slice(&self.mission_id);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.expires_at_epoch.to_be_bytes());
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place. Recomputes `invite_token` and `invite_hash`.
    pub fn sign(&mut self, key: &SigningKey) {
        self.invite_token = self.compute_invite_token();
        self.invite_hash = self.compute_invite_hash();
        self.signature = key.sign(&self.invite_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed_token = self.compute_invite_token();
        if computed_token != self.invite_token {
            return Err(DotError::Serialization(
                "InviteEnvelope: invite_token mismatch".into(),
            ));
        }
        let computed = self.compute_invite_hash();
        if computed != self.invite_hash {
            return Err(DotError::Serialization(
                "InviteEnvelope: invite_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.invite_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.invite_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// UnbindAllEnvelope
// -----------------------------------------------------------------------------

/// Reason for an `UnbindAllEnvelope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum UnbindReason {
    /// Scheduled decommission (per-mission policy).
    Scheduled = 0x00,
    /// Multiple nodes kicked; the group is unsafe to continue.
    MassKick = 0x01,
    /// Mission terminated by the MissionCoordinator.
    MissionTerminated = 0x02,
    /// Coordinator resignation (DC handed over but no successor took
    /// the group).
    CoordinatorResign = 0x03,
    /// Compliance / safety review required immediate shutdown.
    SafetyShutdown = 0x04,
}

impl UnbindReason {
    /// Construct from wire byte.
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::Scheduled),
            0x01 => Some(Self::MassKick),
            0x02 => Some(Self::MissionTerminated),
            0x03 => Some(Self::CoordinatorResign),
            0x04 => Some(Self::SafetyShutdown),
            _ => None,
        }
    }

    /// Returns the wire byte.
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

/// `UnbindAllEnvelope` — DC requests all members to leave the group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnbindAllEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// Platform-specific group identifier.
    pub group_jid: String,
    /// Platform string.
    pub platform: String,
    /// Reason for the UNBIND_ALL.
    pub reason: UnbindReason,
    /// The `binding_hash` of the binding being dissolved.
    pub binding_hash: [u8; 32],
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// Current epoch.
    pub current_epoch: u64,
    /// DC coordinator term.
    pub coordinator_term_id: u64,
    /// `BLAKE3-256(header || body)`.
    pub unbind_hash: [u8; 32],
    /// Ed25519 signature over `unbind_hash`.
    pub signature: [u8; 64],
}

impl UnbindAllEnvelope {
    /// Compute `unbind_hash`.
    pub fn compute_unbind_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&header(tag::UNBIND_ALL));
        buf.extend_from_slice(&self.domain_id);
        write_string(&mut buf, &self.group_jid);
        write_string(&mut buf, &self.platform);
        buf.push(self.reason.as_byte());
        buf.extend_from_slice(&self.binding_hash);
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.coordinator_term_id.to_be_bytes());
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.unbind_hash = self.compute_unbind_hash();
        self.signature = key.sign(&self.unbind_hash).to_bytes();
    }

    /// Verify against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_unbind_hash();
        if computed != self.unbind_hash {
            return Err(DotError::Serialization(
                "UnbindAllEnvelope: unbind_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.unbind_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.unbind_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// UnbindAllAckEnvelope
// -----------------------------------------------------------------------------

/// `UnbindAllAckEnvelope` — witness confirmation of an `UnbindAllEnvelope`.
///
/// (R16 R2 fix: this struct was listed in the envelope type table but
/// had no struct definition in the original RFC-0850p-d v1.0.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnbindAllAckEnvelope {
    /// Domain identifier.
    pub domain_id: [u8; 32],
    /// The `unbind_hash` of the `UnbindAllEnvelope` being acknowledged.
    pub unbind_hash: [u8; 32],
    /// Public key of the witness.
    pub witness_id: [u8; 32],
    /// Epoch at which the witness saw the UNBIND_ALL.
    pub witness_epoch: u64,
    /// `BLAKE3-256(header || domain_id || unbind_hash || witness_id || witness_epoch)`.
    pub ack_hash: [u8; 32],
    /// 32-byte random nonce.
    pub nonce: [u8; 32],
    /// Ed25519 signature over `ack_hash`.
    pub signature: [u8; 64],
}

impl UnbindAllAckEnvelope {
    /// Compute `ack_hash`.
    ///
    /// Per RFC-0850p-d §F, the nonce is INCLUDED in the canonical hash.
    /// R17 R1-CRITICAL-1 fix (was missing from the hash, allowing replay
    /// with attacker-chosen nonces).
    pub fn compute_ack_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(10 + 32 + 32 + 32 + 8 + 32);
        buf.extend_from_slice(&header(tag::UNBIND_ALL_ACK));
        buf.extend_from_slice(&self.domain_id);
        buf.extend_from_slice(&self.unbind_hash);
        buf.extend_from_slice(&self.witness_id);
        buf.extend_from_slice(&self.witness_epoch.to_be_bytes());
        buf.extend_from_slice(&self.nonce);
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign in place.
    pub fn sign(&mut self, key: &SigningKey) {
        self.ack_hash = self.compute_ack_hash();
        self.signature = key.sign(&self.ack_hash).to_bytes();
    }

    /// Verify against the witness's public key.
    pub fn verify(&self, witness_pubkey: &VerifyingKey) -> Result<(), DotError> {
        let computed = self.compute_ack_hash();
        if computed != self.ack_hash {
            return Err(DotError::Serialization(
                "UnbindAllAckEnvelope: ack_hash mismatch".into(),
            ));
        }
        let sig = Signature::from_bytes(&self.signature);
        witness_pubkey
            .verify(&self.ack_hash, &sig)
            .map_err(|_| DotError::InvalidSignature {
                envelope_id: self.ack_hash,
            })?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Serialization helpers
// -----------------------------------------------------------------------------
//
// R17 R1-MEDIUM-3 fix: `write_string` and `write_bytes` used to be
// defined here in duplicate (already defined in `super::binding`).
// They are now imported from `binding` so there is exactly one
// canonical implementation across the DOT protocol.

// R17 R1-MEDIUM-4 fix: removed the dead-code suppression hacks for
// the `_PARENT_ENVELOPE_TYPE`, `_PARENT_ENVELOPE_VERSION`, and
// `_UNUSED_UNBIND_AUTHORITY_REEXPORT` re-exports — these existed
// only to silence `dead_code` warnings, but the silence was hiding
// the fact that the re-exports had no consumer. The re-exports are
// also gone; any downstream code that needed them should import
// directly from `super::binding`.

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dc_key() -> SigningKey {
        SigningKey::from_bytes(&[1u8; 32])
    }

    fn test_witness_key() -> SigningKey {
        SigningKey::from_bytes(&[2u8; 32])
    }

    fn test_founder_key() -> SigningKey {
        SigningKey::from_bytes(&[3u8; 32])
    }

    #[test]
    fn cgroup_sign_verify_roundtrip() {
        let key = test_dc_key();
        let mut env = CreateGroupEnvelope {
            domain_id: [1u8; 32],
            mission_id: [2u8; 32],
            platform: "whatsapp".into(),
            proposed_group_metadata: b"{}".to_vec(),
            initial_invite_count: 5,
            dc_id: key.verifying_key().to_bytes(),
            nonce: [3u8; 32],
            current_epoch: 100,
            coordinator_term_id: 1,
            group_visibility: GroupVisibility::Private,
            cgroup_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn cgroup_ack_sign_verify_roundtrip() {
        let wkey = test_witness_key();
        let mut ack = CreateGroupAckEnvelope {
            domain_id: [1u8; 32],
            cgroup_hash: [4u8; 32],
            witness_id: wkey.verifying_key().to_bytes(),
            witness_epoch: 101,
            ack_hash: [0u8; 32],
            nonce: [5u8; 32],
            signature: [0u8; 64],
        };
        ack.sign(&wkey);
        assert!(ack.verify(&wkey.verifying_key()).is_ok());
    }

    // R17 R1-CRITICAL-1 regression test: changing the nonce must change
    // the hash so an attacker cannot swap a stored envelope's nonce to
    // bypass replay protection.
    #[test]
    fn cgroup_ack_nonce_changes_hash() {
        let wkey = test_witness_key();
        let mut ack = CreateGroupAckEnvelope {
            domain_id: [1u8; 32],
            cgroup_hash: [4u8; 32],
            witness_id: wkey.verifying_key().to_bytes(),
            witness_epoch: 101,
            ack_hash: [0u8; 32],
            nonce: [5u8; 32],
            signature: [0u8; 64],
        };
        ack.sign(&wkey);
        let original_hash = ack.ack_hash;
        ack.nonce = [9u8; 32];
        ack.sign(&wkey);
        assert_ne!(ack.ack_hash, original_hash);
    }

    #[test]
    fn cgroup_done_sign_verify_roundtrip() {
        let key = test_dc_key();
        let mut env = CreateGroupDoneEnvelope {
            domain_id: [1u8; 32],
            group_jid: "120363012345678@g.us".into(),
            platform: "whatsapp".into(),
            cgroup_hash: [6u8; 32],
            nonce: [7u8; 32],
            done_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn cgroup_fail_sign_verify_roundtrip() {
        let key = test_dc_key();
        let mut env = CreateGroupFailEnvelope {
            domain_id: [1u8; 32],
            platform: "matrix".into(),
            cgroup_hash: [8u8; 32],
            reason_code: 0x000E, // CreateGroupFailed
            platform_error: "rate limited".into(),
            fail_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn invite_sign_verify_roundtrip() {
        let key = test_dc_key();
        let invitee = test_founder_key().verifying_key().to_bytes();
        let mut env = InviteEnvelope {
            domain_id: [1u8; 32],
            group_jid: "120363012345678@g.us".into(),
            platform: "whatsapp".into(),
            invitee_pubkey: invitee,
            nonce: [9u8; 32],
            invite_token: [0u8; 32],
            mission_id: [2u8; 32],
            current_epoch: 100,
            expires_at_epoch: 200,
            invite_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        // Verify the invite_token was computed deterministically.
        let expected_token = env.compute_invite_token();
        assert_eq!(env.invite_token, expected_token);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn invite_mutation_rejected() {
        let key = test_dc_key();
        let invitee = test_founder_key().verifying_key().to_bytes();
        let mut env = InviteEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            invitee_pubkey: invitee,
            nonce: [9u8; 32],
            invite_token: [0u8; 32],
            mission_id: [2u8; 32],
            current_epoch: 100,
            expires_at_epoch: 200,
            invite_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        // Tamper with the expiry
        env.expires_at_epoch = u64::MAX;
        // Verify must reject.
        assert!(env.verify(&key.verifying_key()).is_err());
    }

    #[test]
    fn unbind_all_sign_verify_roundtrip() {
        let key = test_dc_key();
        let mut env = UnbindAllEnvelope {
            domain_id: [1u8; 32],
            group_jid: "g1@g.us".into(),
            platform: "whatsapp".into(),
            reason: UnbindReason::Scheduled,
            binding_hash: [10u8; 32],
            nonce: [11u8; 32],
            current_epoch: 100,
            coordinator_term_id: 1,
            unbind_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        env.sign(&key);
        assert!(env.verify(&key.verifying_key()).is_ok());
    }

    #[test]
    fn unbind_all_ack_sign_verify_roundtrip() {
        let wkey = test_witness_key();
        let mut ack = UnbindAllAckEnvelope {
            domain_id: [1u8; 32],
            unbind_hash: [12u8; 32],
            witness_id: wkey.verifying_key().to_bytes(),
            witness_epoch: 101,
            ack_hash: [0u8; 32],
            nonce: [13u8; 32],
            signature: [0u8; 64],
        };
        ack.sign(&wkey);
        assert!(ack.verify(&wkey.verifying_key()).is_ok());
    }

    // R17 R1-CRITICAL-1 regression test: changing the nonce must change
    // the hash so an attacker cannot swap a stored envelope's nonce to
    // bypass replay protection.
    #[test]
    fn unbind_all_ack_nonce_changes_hash() {
        let wkey = test_witness_key();
        let mut ack = UnbindAllAckEnvelope {
            domain_id: [1u8; 32],
            unbind_hash: [12u8; 32],
            witness_id: wkey.verifying_key().to_bytes(),
            witness_epoch: 101,
            ack_hash: [0u8; 32],
            nonce: [13u8; 32],
            signature: [0u8; 64],
        };
        ack.sign(&wkey);
        let original_hash = ack.ack_hash;
        ack.nonce = [99u8; 32];
        ack.sign(&wkey);
        assert_ne!(ack.ack_hash, original_hash);
    }

    #[test]
    fn unbind_reason_roundtrip() {
        for r in [
            UnbindReason::Scheduled,
            UnbindReason::MassKick,
            UnbindReason::MissionTerminated,
            UnbindReason::CoordinatorResign,
            UnbindReason::SafetyShutdown,
        ] {
            assert_eq!(UnbindReason::from_byte(r.as_byte()), Some(r));
        }
        assert_eq!(UnbindReason::from_byte(0xFF), None);
    }

    #[test]
    fn header_subtype_distinct() {
        // All 7 tags must be distinct.
        let tags = [
            tag::CREATE_GROUP,
            tag::CREATE_GROUP_ACK,
            tag::CREATE_GROUP_DONE,
            tag::CREATE_GROUP_FAIL,
            tag::INVITE,
            tag::UNBIND_ALL,
            tag::UNBIND_ALL_ACK,
        ];
        for (i, a) in tags.iter().enumerate() {
            for (j, b) in tags.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "tags {} and {} are equal", i, j);
                }
            }
        }
    }
}
