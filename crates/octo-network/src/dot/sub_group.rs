//! Sub-Domain / Sub-Group Nesting — RFC-0855p-d
//!
//! Implements the `CreateSubGroupEnvelope` (subtype tag `b"CGSB"`) and
//! `SubGroupExtension` types. Sub-groups are bound to sub-`domain_id`s
//! derived as `sub_domain_id = BLAKE3(parent_domain_id || sub_label)`
//! and inherit the parent's mission and DC, but have their own membership
//! and binding.
//!
//! See RFC-0855p-d §"Envelope Type Extension" and
//! `missions/claimed/0855p-d-subgroup-nesting.md` Phase 1.
//!
//! ## Canonical 10-byte header
//!
//! `CreateSubGroupEnvelope` uses the standard 10-byte header per
//! RFC-0850p-c §A: `envelope_type = b"DOT1"`, `envelope_subtype = b"CGSB"`,
//! `version = u16 // 0x0001`. The body is serialized in field-declaration
//! order, with fixed-size integers big-endian, byte arrays verbatim, and
//! `String`/`Vec<u8>` length-prefixed by a big-endian `u32` count.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

use super::binding::{header, write_string, ENVELOPE_TYPE, ENVELOPE_VERSION};
use super::error::DotError;

/// Subtype tag for `CreateSubGroupEnvelope` (R16 R1-H2 fix; was previously
/// described as adding `sub_group_extension: Option<SubGroupExtension>`
/// to the base `CreateGroupEnvelope`, but the base CGROUP envelope has no
/// such field — the fix is a new envelope variant).
pub const SUBGROUP_TAG: [u8; 4] = *b"CGSB";

/// Maximum length of a `sub_label` (UTF-8 bytes). The RFC mandates no `/`
/// characters to enable URL-style addressing.
pub const MAX_SUB_LABEL_LEN: usize = 256;

/// Error type for `CreateSubGroupEnvelope` validation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SubGroupError {
    /// The sub_label is empty.
    #[error("sub_label is empty")]
    EmptyLabel,
    /// The sub_label exceeds `MAX_SUB_LABEL_LEN` bytes.
    #[error("sub_label exceeds {max} bytes (got {got})")]
    LabelTooLong {
        /// Maximum allowed length.
        max: usize,
        /// Actual length.
        got: usize,
    },
    /// The sub_label contains a `/` character (forbidden per RFC-0855p-d F-7,
    /// R16 R1-L2 fix — was SHOULD, now MUST).
    #[error("sub_label contains '/' at byte offset {offset} (forbidden per RFC-0855p-d F-7)")]
    SlashInLabel {
        /// Byte offset of the offending `/`.
        offset: usize,
    },
    /// Sub-domain id derivation mismatch.
    #[error("sub_domain_id mismatch (computed {:02x?}, stored {:02x?})", &computed[..8], &stored[..8])]
    SubDomainIdMismatch {
        /// Computed id.
        computed: [u8; 32],
        /// Stored id.
        stored: [u8; 32],
    },
    /// Envelope header mismatch (wrong envelope_type or envelope_subtype).
    #[error("envelope header mismatch (expected DOT1/{tag:?}, got {got_type:?}/{got_subtype:?})")]
    HeaderMismatch {
        /// Expected subtype tag.
        tag: [u8; 4],
        /// Actual envelope_type.
        got_type: [u8; 4],
        /// Actual envelope_subtype.
        got_subtype: [u8; 4],
    },
}

/// Optional fields added to `CreateSubGroupEnvelope` for sub-groups.
///
/// See RFC-0855p-d §"Envelope Type Extension".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubGroupExtension {
    /// The parent domain's identifier.
    pub parent_domain_id: [u8; 32],
    /// UTF-8 string label for the sub-group; MUST NOT contain `/`
    /// (R16 R1-L2 fix). Max 256 bytes.
    pub sub_label: String,
    /// The sub-DC's peer_id. `None` means the parent DC is the implicit DC
    /// for this sub-domain.
    pub sub_dc_id: Option<[u8; 32]>,
    /// Signed delegation envelope from the parent DC granting sub-DC
    /// authority to `sub_dc_id`. Format TBD (F-1 in RFC-0855p-d).
    pub delegation_proof: Option<Vec<u8>>,
}

impl SubGroupExtension {
    /// Construct a new `SubGroupExtension` with `sub_dc_id = None` and
    /// `delegation_proof = None`.
    pub fn new(parent_domain_id: [u8; 32], sub_label: String) -> Self {
        Self {
            parent_domain_id,
            sub_label,
            sub_dc_id: None,
            delegation_proof: None,
        }
    }

    /// Compute the derived `sub_domain_id`:
    /// `BLAKE3-256(parent_domain_id || sub_label)`.
    pub fn derive_sub_domain_id(&self) -> [u8; 32] {
        derive_sub_domain_id(&self.parent_domain_id, &self.sub_label)
    }

    /// Validate the sub_label per RFC-0855p-d F-7:
    /// - non-empty
    /// - length <= `MAX_SUB_LABEL_LEN`
    /// - contains no `/` characters
    pub fn validate(&self) -> Result<(), SubGroupError> {
        validate_sub_label(&self.sub_label)
    }
}

/// Derive the `sub_domain_id` from a parent domain id and a sub_label.
///
/// `sub_domain_id = BLAKE3-256(parent_domain_id || sub_label)`.
pub fn derive_sub_domain_id(parent_domain_id: &[u8; 32], sub_label: &str) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + sub_label.len());
    buf.extend_from_slice(parent_domain_id);
    buf.extend_from_slice(sub_label.as_bytes());
    *blake3::hash(&buf).as_bytes()
}

/// Validate a `sub_label` per RFC-0855p-d F-7 (non-empty, length <= 256,
/// no `/` characters).
pub fn validate_sub_label(sub_label: &str) -> Result<(), SubGroupError> {
    if sub_label.is_empty() {
        return Err(SubGroupError::EmptyLabel);
    }
    if sub_label.len() > MAX_SUB_LABEL_LEN {
        return Err(SubGroupError::LabelTooLong {
            max: MAX_SUB_LABEL_LEN,
            got: sub_label.len(),
        });
    }
    if let Some(offset) = sub_label.find('/') {
        return Err(SubGroupError::SlashInLabel { offset });
    }
    Ok(())
}

/// `CreateSubGroupEnvelope` (DOT/1/CGSB).
///
/// Sub-groups are bound to a derived `sub_domain_id` and inherit the parent's
/// mission and DC. The `SubGroupExtension` carries the linkage to the parent
/// domain. R16 R1-H2 fix: this is a NEW envelope variant (not an extension to
/// the base `CreateGroupEnvelope`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSubGroupEnvelope {
    /// `b"DOT1"`.
    pub envelope_type: [u8; 4],
    /// `b"CGSB"`.
    pub envelope_subtype: [u8; 4],
    /// `0x0001` (canonical version).
    pub version: u16,
    /// The derived sub-domain id: `BLAKE3(parent_domain_id || sub_label)`.
    pub sub_domain_id: [u8; 32],
    /// Mission id (inherited from parent).
    pub mission_id: [u8; 32],
    /// Platform string (e.g., `"whatsapp"`).
    pub platform: String,
    /// Proposed physical group identifier on the platform (e.g., a
    /// WhatsApp group JID). May be empty if the platform assigns it.
    pub proposed_group_jid: String,
    /// Initial invite count (number of `InviteEnvelope`s the DC plans to
    /// emit).
    ///
    /// R17 R1-MEDIUM-5 fix: was `u16`, now `u32` to match
    /// `CreateGroupEnvelope.initial_invite_count` (which has always
    /// been `u32`). `u16` capped the sub-group at 65 535 invites,
    /// which is too low for large missions.
    pub initial_invite_count: u32,
    /// The sub-DC's peer_id, or the parent DC's peer_id if the extension
    /// does not delegate.
    pub dc_id: [u8; 32],
    /// Sub-group linkage to the parent domain.
    pub sub_group_extension: SubGroupExtension,
    /// 32-byte random nonce. R17 R1-HIGH-7 fix: was `[u8; 16]`, now
    /// 32 bytes for consistency with all other envelopes in the DOT
    /// protocol (BindEnvelope, HandoverEnvelopes, etc.).
    pub nonce: [u8; 32],
    /// Current epoch at CGROUP_SUB emission time.
    pub current_epoch: u64,
    /// The parent DC's term id (signs the envelope).
    pub coordinator_term_id: [u8; 32],
    /// `BLAKE3-256(header || body)`.
    pub sub_group_hash: [u8; 32],
    /// Ed25519 signature over `sub_group_hash`.
    pub signature: [u8; 64],
}

impl CreateSubGroupEnvelope {
    /// Construct a new `CreateSubGroupEnvelope` with the canonical header
    /// fields populated. The caller fills in the rest of the fields and then
    /// calls `sign(...)` before transmitting.
    pub fn new(
        mission_id: [u8; 32],
        sub_group_extension: SubGroupExtension,
        dc_id: [u8; 32],
        current_epoch: u64,
        coordinator_term_id: [u8; 32],
    ) -> Self {
        let sub_domain_id = sub_group_extension.derive_sub_domain_id();
        Self {
            envelope_type: ENVELOPE_TYPE,
            envelope_subtype: SUBGROUP_TAG,
            version: ENVELOPE_VERSION,
            sub_domain_id,
            mission_id,
            platform: String::new(),
            proposed_group_jid: String::new(),
            initial_invite_count: 0,
            dc_id,
            sub_group_extension,
            nonce: [0u8; 32],
            current_epoch,
            coordinator_term_id,
            sub_group_hash: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    /// Validate header + sub_label + sub_domain_id consistency.
    pub fn validate(&self) -> Result<(), SubGroupError> {
        if self.envelope_type != ENVELOPE_TYPE || self.envelope_subtype != SUBGROUP_TAG {
            return Err(SubGroupError::HeaderMismatch {
                tag: SUBGROUP_TAG,
                got_type: self.envelope_type,
                got_subtype: self.envelope_subtype,
            });
        }
        self.sub_group_extension.validate()?;
        let computed = self.sub_group_extension.derive_sub_domain_id();
        if computed != self.sub_domain_id {
            return Err(SubGroupError::SubDomainIdMismatch {
                computed,
                stored: self.sub_domain_id,
            });
        }
        Ok(())
    }

    /// Serialize the body (everything after the 10-byte header) to bytes,
    /// in field-declaration order.
    pub fn body_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(512);
        buf.extend_from_slice(&self.sub_domain_id);
        buf.extend_from_slice(&self.mission_id);
        write_string(&mut buf, &self.platform);
        write_string(&mut buf, &self.proposed_group_jid);
        buf.extend_from_slice(&self.initial_invite_count.to_be_bytes());
        buf.extend_from_slice(&self.dc_id);
        // SubGroupExtension: parent_domain_id, sub_label, sub_dc_id (option tag),
        // delegation_proof (option tag).
        buf.extend_from_slice(&self.sub_group_extension.parent_domain_id);
        write_string(&mut buf, &self.sub_group_extension.sub_label);
        match &self.sub_group_extension.sub_dc_id {
            Some(id) => {
                buf.push(1);
                buf.extend_from_slice(id);
            }
            None => buf.push(0),
        }
        match &self.sub_group_extension.delegation_proof {
            Some(proof) => {
                buf.push(1);
                buf.extend_from_slice(&(proof.len() as u32).to_be_bytes());
                buf.extend_from_slice(proof);
            }
            None => buf.push(0),
        }
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.coordinator_term_id);
        buf
    }

    /// Compute `sub_group_hash = BLAKE3-256(header || body)`.
    pub fn compute_sub_group_hash(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(10 + 512);
        buf.extend_from_slice(&header(SUBGROUP_TAG));
        buf.extend_from_slice(&self.body_bytes());
        *blake3::hash(&buf).as_bytes()
    }

    /// Sign the envelope in place. Recomputes `sub_group_hash` and signs it.
    ///
    /// R17 R1-HIGH-6 fix: previously the `validate()` call used
    /// `.expect(...)`, which would panic the process if the sub_label
    /// was invalid. The function now returns `Result<(), SubGroupError>`
    /// so callers can handle the failure gracefully (e.g., return the
    /// error to the orchestrator, or surface it to a higher layer).
    /// The `expect`-on-panic was a denial-of-service vector — a single
    /// malformed envelope could crash the DC.
    pub fn sign(&mut self, key: &SigningKey) -> Result<(), SubGroupError> {
        // Validate first so we never sign an inconsistent envelope.
        self.sub_group_extension.validate()?;
        // Ensure sub_domain_id matches the derived value before signing.
        self.sub_domain_id = self.sub_group_extension.derive_sub_domain_id();
        self.sub_group_hash = self.compute_sub_group_hash();
        self.signature = key.sign(&self.sub_group_hash).to_bytes();
        Ok(())
    }

    /// Verify the signature against the DC's public key.
    pub fn verify(&self, dc_pubkey: &VerifyingKey) -> Result<(), DotError> {
        self.validate().map_err(|e| {
            DotError::Serialization(format!("CreateSubGroupEnvelope: invalid: {e}"))
        })?;
        let computed = self.compute_sub_group_hash();
        if computed != self.sub_group_hash {
            return Err(DotError::Serialization(format!(
                "CreateSubGroupEnvelope: sub_group_hash mismatch (computed {:02x?}, stored {:02x?})",
                &computed[..8],
                &self.sub_group_hash[..8]
            )));
        }
        let sig = Signature::from_bytes(&self.signature);
        dc_pubkey
            .verify(&self.sub_group_hash, &sig)
            .map_err(|_e| DotError::InvalidSignature {
                envelope_id: self.sub_group_hash,
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn make_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn make_ext() -> SubGroupExtension {
        SubGroupExtension::new([0xAAu8; 32], "legal-review".to_string())
    }

    #[test]
    fn sub_domain_id_is_blake3_of_parent_and_label() {
        let parent = [0x11u8; 32];
        let label = "comms-review";
        let id = derive_sub_domain_id(&parent, label);
        // Recompute manually: blake3(parent || label).
        let mut buf = Vec::new();
        buf.extend_from_slice(&parent);
        buf.extend_from_slice(label.as_bytes());
        assert_eq!(id, *blake3::hash(&buf).as_bytes());
    }

    #[test]
    fn derive_is_deterministic_for_same_input() {
        let a = derive_sub_domain_id(&[0xAAu8; 32], "x");
        let b = derive_sub_domain_id(&[0xAAu8; 32], "x");
        assert_eq!(a, b);
    }

    #[test]
    fn derive_differs_for_different_label() {
        let a = derive_sub_domain_id(&[0xAAu8; 32], "alpha");
        let b = derive_sub_domain_id(&[0xAAu8; 32], "beta");
        assert_ne!(a, b);
    }

    #[test]
    fn derive_differs_for_different_parent() {
        let a = derive_sub_domain_id(&[0xAAu8; 32], "x");
        let b = derive_sub_domain_id(&[0xBBu8; 32], "x");
        assert_ne!(a, b);
    }

    #[test]
    fn validate_sub_label_ok() {
        assert!(validate_sub_label("legal-review").is_ok());
        assert!(validate_sub_label("comms_review").is_ok());
        assert!(validate_sub_label("a").is_ok());
    }

    #[test]
    fn validate_sub_label_empty() {
        assert!(matches!(
            validate_sub_label(""),
            Err(SubGroupError::EmptyLabel)
        ));
    }

    #[test]
    fn validate_sub_label_too_long() {
        let s = "a".repeat(MAX_SUB_LABEL_LEN + 1);
        assert!(matches!(
            validate_sub_label(&s),
            Err(SubGroupError::LabelTooLong { .. })
        ));
    }

    #[test]
    fn validate_sub_label_rejects_slash() {
        assert!(matches!(
            validate_sub_label("legal/review"),
            Err(SubGroupError::SlashInLabel { offset: 5 })
        ));
        assert!(matches!(
            validate_sub_label("/leading"),
            Err(SubGroupError::SlashInLabel { offset: 0 })
        ));
        assert!(matches!(
            validate_sub_label("trailing/"),
            Err(SubGroupError::SlashInLabel { offset: 8 })
        ));
    }

    #[test]
    fn envelope_new_initializes_header() {
        let key = make_key(7);
        let pubkey = key.verifying_key();
        let mut env = CreateSubGroupEnvelope::new(
            [0xCCu8; 32],
            make_ext(),
            *pubkey.as_bytes(),
            42,
            [0xDDu8; 32],
        );
        env.platform = "whatsapp".to_string();
        env.proposed_group_jid = "120363@g.us".to_string();
        env.initial_invite_count = 3;
        env.nonce = [0xEEu8; 32];

        assert_eq!(env.envelope_type, *b"DOT1");
        assert_eq!(env.envelope_subtype, *b"CGSB");
        assert_eq!(env.version, 1);
        assert_eq!(env.current_epoch, 42);
        // sub_domain_id is derived.
        assert_eq!(
            env.sub_domain_id,
            derive_sub_domain_id(&[0xAAu8; 32], "legal-review"),
        );
    }

    #[test]
    fn envelope_validate_catches_sub_domain_id_mismatch() {
        let key = make_key(7);
        let pubkey = key.verifying_key();
        let mut env = CreateSubGroupEnvelope::new(
            [0xCCu8; 32],
            make_ext(),
            *pubkey.as_bytes(),
            42,
            [0xDDu8; 32],
        );
        env.platform = "whatsapp".to_string();
        env.proposed_group_jid = "120363@g.us".to_string();
        env.initial_invite_count = 3;
        env.nonce = [0xEEu8; 32];
        env.sign(&key).unwrap();
        // Tamper with sub_domain_id.
        env.sub_domain_id = [0xFFu8; 32];
        assert!(matches!(
            env.validate(),
            Err(SubGroupError::SubDomainIdMismatch { .. })
        ));
        // Verification also fails.
        assert!(env.verify(&pubkey).is_err());
    }

    #[test]
    fn envelope_sign_verify_round_trip() {
        let key = make_key(7);
        let pubkey = key.verifying_key();
        let mut env = CreateSubGroupEnvelope::new(
            [0xCCu8; 32],
            make_ext(),
            *pubkey.as_bytes(),
            42,
            [0xDDu8; 32],
        );
        env.platform = "whatsapp".to_string();
        env.proposed_group_jid = "120363@g.us".to_string();
        env.initial_invite_count = 3;
        env.nonce = [0xEEu8; 32];

        env.sign(&key).unwrap();
        assert!(env.verify(&pubkey).is_ok());
    }

    #[test]
    fn envelope_signature_failure_on_tamper() {
        let key = make_key(7);
        let pubkey = key.verifying_key();
        let mut env = CreateSubGroupEnvelope::new(
            [0xCCu8; 32],
            make_ext(),
            *pubkey.as_bytes(),
            42,
            [0xDDu8; 32],
        );
        env.platform = "whatsapp".to_string();
        env.proposed_group_jid = "120363@g.us".to_string();
        env.initial_invite_count = 3;
        env.nonce = [0xEEu8; 32];
        env.sign(&key).unwrap();
        // Tamper with proposed_group_jid after signing.
        env.proposed_group_jid = "tampered@g.us".to_string();
        assert!(env.verify(&pubkey).is_err());
    }

    #[test]
    fn envelope_signature_failure_on_wrong_key() {
        let key = make_key(7);
        let other = make_key(8);
        let mut env = CreateSubGroupEnvelope::new(
            [0xCCu8; 32],
            make_ext(),
            *key.verifying_key().as_bytes(),
            42,
            [0xDDu8; 32],
        );
        env.platform = "whatsapp".to_string();
        env.proposed_group_jid = "120363@g.us".to_string();
        env.initial_invite_count = 3;
        env.nonce = [0xEEu8; 32];
        env.sign(&key).unwrap();
        assert!(env.verify(&other.verifying_key()).is_err());
    }

    #[test]
    fn envelope_with_sub_dc_and_delegation() {
        let key = make_key(7);
        let pubkey = key.verifying_key();
        let mut ext = make_ext();
        ext.sub_dc_id = Some([0x42u8; 32]);
        ext.delegation_proof = Some(vec![0x01, 0x02, 0x03, 0x04]);
        let mut env =
            CreateSubGroupEnvelope::new([0xCCu8; 32], ext, *pubkey.as_bytes(), 100, [0xDDu8; 32]);
        env.platform = "matrix".to_string();
        env.proposed_group_jid = "!room:matrix.org".to_string();
        env.initial_invite_count = 5;
        env.nonce = [0xAAu8; 32];
        env.sign(&key).unwrap();
        assert!(env.verify(&pubkey).is_ok());
    }

    #[test]
    fn envelope_header_mismatch_rejected() {
        let key = make_key(7);
        let pubkey = key.verifying_key();
        let mut env = CreateSubGroupEnvelope::new(
            [0xCCu8; 32],
            make_ext(),
            *pubkey.as_bytes(),
            42,
            [0xDDu8; 32],
        );
        env.platform = "whatsapp".to_string();
        env.proposed_group_jid = "120363@g.us".to_string();
        env.initial_invite_count = 3;
        env.nonce = [0xEEu8; 32];
        env.sign(&key).unwrap();
        // Tamper with envelope_subtype.
        env.envelope_subtype = *b"BIND";
        assert!(matches!(
            env.validate(),
            Err(SubGroupError::HeaderMismatch { .. })
        ));
    }

    #[test]
    fn canonical_10_byte_header() {
        let h = header(SUBGROUP_TAG);
        assert_eq!(&h[0..4], b"DOT1");
        assert_eq!(&h[4..8], b"CGSB");
        assert_eq!(u16::from_be_bytes([h[8], h[9]]), 0x0001);
    }

    #[test]
    fn sign_returns_err_for_invalid_sub_label() {
        // R17 R1-HIGH-6 regression: sign() used to `.expect(...)` on
        // the validate() result, panicking the process if the
        // sub_label was invalid. It now returns Err, letting the
        // caller surface a proper error.
        use ed25519_dalek::SigningKey;
        let key = SigningKey::from_bytes(&[9u8; 32]);
        let mut env = CreateSubGroupEnvelope {
            envelope_type: *b"DOT1",
            envelope_subtype: *b"CGSB",
            version: 0x0001,
            sub_domain_id: [0u8; 32],
            mission_id: [1u8; 32],
            platform: "whatsapp".into(),
            proposed_group_jid: "120363@g.us".into(),
            initial_invite_count: 1,
            dc_id: [0u8; 32],
            sub_group_extension: SubGroupExtension {
                parent_domain_id: [1u8; 32],
                sub_label: "bad/label".into(), // slash → invalid
                sub_dc_id: None,
                delegation_proof: None,
            },
            nonce: [0u8; 32],
            current_epoch: 0,
            coordinator_term_id: [0u8; 32],
            sub_group_hash: [0u8; 32],
            signature: [0u8; 64],
        };
        let result = env.sign(&key);
        assert!(matches!(result, Err(SubGroupError::SlashInLabel { .. })));
    }

    #[test]
    fn sub_label_max_boundary_ok() {
        // Exactly 256 bytes: OK.
        let s = "a".repeat(MAX_SUB_LABEL_LEN);
        assert!(validate_sub_label(&s).is_ok());
        let id = derive_sub_domain_id(&[0xAAu8; 32], &s);
        assert_ne!(id, [0u8; 32]);
    }

    #[test]
    fn derive_id_changes_with_label_size() {
        let a = derive_sub_domain_id(&[0xAAu8; 32], "x");
        let b = derive_sub_domain_id(&[0xAAu8; 32], "xx");
        assert_ne!(a, b);
    }
}
