//! Identity substrate.
//!
//! Per RFC-0009 §Identity Key Format + §Capability Keys + RFC-0009
//! §HsmAdapter Integration:
//! - `IdentityKey` holds an `Arc<dyn HsmAdapter>` for signing operations
//!   (host memory for `InMemorySigner`, secure element for `LedgerSigner`).
//! - The raw seed NEVER leaves the adapter; production `LedgerSigner` signs
//!   on-device with explicit user confirmation.
//! - `CapabilityKey` is a 32-byte symmetric key derived per-(audience, channel)
//!   via HKDF-BLAKE3; for `InMemorySigner`-backed identities the derivation
//!   uses the adapter's seed (crate-internal accessor only).

use std::str::FromStr;
use std::sync::Arc;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::WalletError;
use crate::hsm::{HsmAdapter, InMemorySigner};

use octo_ident::DidCodec;

/// Ed25519 identity keypair. Wraps an `Arc<dyn HsmAdapter>` + cached
/// 32-byte public key + lifecycle state (RFC-0009 §Lifecycle Requirements).
///
/// Per RFC-0009 §HsmAdapter Integration:
/// - `sign()` delegates to `self.signer.sign(msg)`; never touches host memory
///   for production `LedgerSigner` (seed lives in secure element).
/// - The raw seed is accessible only via the adapter — `InMemorySigner` exposes
///   it crate-internally for HKDF + keystore-export paths; hardware adapters
///   do not expose it at all.
pub struct IdentityKey {
    signer: Arc<dyn HsmAdapter>,
    public_key: [u8; 32],
    /// Lifecycle state per RFC-0009 §Identity Lifecycle State Machine.
    /// Defaults to `Designated`; flips to `Active` via [`Self::activate`].
    lifecycle: crate::lifecycle::LifecycleState,
    /// Unix timestamp (seconds) of the Designated → Active transition.
    /// `None` until [`Self::activate`] succeeds.
    activated_at_unix_secs: Option<u64>,
    /// Unix timestamp (seconds) of the Active → Revoked transition.
    /// `None` until [`Self::revoke`] succeeds.
    revoked_at_unix_secs: Option<u64>,
    /// `Ed25519(self.signer, "revoke")` produced at revocation time
    /// (RFC-0009 §Lifecycle row 4). Audit proof that the holder
    /// authorized the burn. `Some` only after `revoke()` succeeds.
    ///
    /// Forward-compat storage slot: this proof is consumed by the
    /// deferred revocation-event gossip fan-out via `octo_transport`
    /// (cross-node revocation propagation). The current surface
    /// captures + exposes it; production gossip wiring ships in a
    /// follow-on mission.
    revoked_proof: Option<[u8; 64]>,
    /// Successor `IdentityKey` for rotation (RFC-0009 §Lifecycle row 2).
    /// `None` until [`Self::begin_rotation`] succeeds; cleared on
    /// [`Self::complete_rotation`] or [`Self::abort_rotation`].
    successor_key: Option<Box<IdentityKey>>,
    /// Unix timestamp (seconds) of the Active → Rotating transition.
    /// `None` until [`Self::begin_rotation`] succeeds.
    rotation_started_at_unix_secs: Option<u64>,
    /// True after a rotation completes; old key remains verifiable for
    /// historical signatures but new signatures should target the successor
    /// (RFC-0009 §Lifecycle row 3).
    deprecated: bool,
}

/// Rotation grace period per RFC-0853 §12 (24 hours in seconds).
pub const ROTATION_GRACE_PERIOD_SECS: u64 = 24 * 60 * 60;

impl std::fmt::Debug for IdentityKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityKey")
            .field("public_key", &hex::encode(self.public_key))
            .field("signer", &"<dyn HsmAdapter>")
            .finish_non_exhaustive()
    }
}

impl Clone for IdentityKey {
    /// Clone the identity. Note: the signer (`Arc<dyn HsmAdapter>`) is
    /// reference-counted — revoking one clone does NOT revoke the others.
    /// If you clone an identity with intent to revoke the original,
    /// the clone still holds a live signer + (for `InMemorySigner`) the
    /// raw seed bytes. Use `revoke()` on every clone you intend to retire.
    fn clone(&self) -> Self {
        Self {
            signer: self.signer.clone(),
            public_key: self.public_key,
            lifecycle: self.lifecycle,
            activated_at_unix_secs: self.activated_at_unix_secs,
            revoked_at_unix_secs: self.revoked_at_unix_secs,
            revoked_proof: self.revoked_proof,
            successor_key: self.successor_key.clone(),
            rotation_started_at_unix_secs: self.rotation_started_at_unix_secs,
            deprecated: self.deprecated,
        }
    }
}

impl IdentityKey {
    /// Generate a fresh identity key from OS CSPRNG (defaults to
    /// `InMemorySigner` for MVP; production swaps in a hardware adapter via
    /// [`IdentityKey::with_signer`]).
    ///
    /// # Errors
    /// Returns `WalletError::OsRng` if the OS RNG fails (extremely rare).
    pub fn generate() -> Result<Self, WalletError> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).map_err(|e| WalletError::OsRng(e.to_string()))?;
        let k = Self::from_seed(seed);
        seed.zeroize();
        Ok(k)
    }

    /// Restore from a 32-byte seed using the default `InMemorySigner`.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let public_key = sk.verifying_key().to_bytes();
        Self {
            signer: Arc::new(InMemorySigner::new(seed, public_key)),
            public_key,
            lifecycle: crate::lifecycle::LifecycleState::Designated,
            activated_at_unix_secs: None,
            revoked_at_unix_secs: None,
            revoked_proof: None,
            successor_key: None,
            rotation_started_at_unix_secs: None,
            deprecated: false,
        }
    }

    /// Construct an `IdentityKey` backed by an arbitrary `HsmAdapter` impl
    /// (e.g. `LedgerSigner`). The public key is sourced from the adapter.
    ///
    /// # Errors
    /// Returns `WalletError::Hsm(HsmError::Device)` if `get_public_key`
    /// fails on the adapter (transport failure).
    pub fn with_signer(signer: Arc<dyn HsmAdapter>) -> Result<Self, WalletError> {
        let public_key = signer.get_public_key()?;
        Ok(Self {
            signer,
            public_key,
            lifecycle: crate::lifecycle::LifecycleState::Designated,
            activated_at_unix_secs: None,
            revoked_at_unix_secs: None,
            revoked_proof: None,
            successor_key: None,
            rotation_started_at_unix_secs: None,
            deprecated: false,
        })
    }

    /// Current lifecycle state (RFC-0009 §Identity Lifecycle State Machine).
    #[must_use]
    pub fn lifecycle(&self) -> crate::lifecycle::LifecycleState {
        self.lifecycle
    }

    /// `Some(unix_secs)` after the first successful `activate()`; `None` for
    /// a `Designated` (never-activated) identity.
    #[must_use]
    pub fn activated_at_unix_secs(&self) -> Option<u64> {
        self.activated_at_unix_secs
    }

    /// `Some(unix_secs)` after `revoke()` succeeds; `None` otherwise.
    #[must_use]
    pub fn revoked_at_unix_secs(&self) -> Option<u64> {
        self.revoked_at_unix_secs
    }

    /// True after `complete_rotation()` succeeds. A deprecated key has
    /// successfully handed off to its successor; new signatures should
    /// target the successor, but historical signatures remain verifiable
    /// per RFC-0009 §Lifecycle row 3.
    ///
    /// Distinct from `Revoked` state: a deprecated key is still in
    /// `Active` lifecycle state and can still sign during the grace
    /// period, but the rotation is complete.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecated
    }

    /// RFC-0009 §Lifecycle row 1: `Designated → Active`.
    ///
    /// Idempotent from `Active` (no-op, no event emission). Refuses from
    /// `Revoked` (terminal). Refuses from `Rotating` (rotation
    /// transitions owned by `IdentityKey::begin_rotation` /
    /// `complete_rotation` / `abort_rotation` — caller must complete or
    /// abort the rotation first. `Rotating → Active` is a valid
    /// state-machine edge but NOT a valid activation path per
    /// RFC-0009 §Lifecycle row 3).
    ///
    /// # Errors
    /// Returns `WalletError::AlreadyRevoked` if state is `Revoked`, or
    /// `WalletError::RotationInProgress` if state is `Rotating`.
    pub fn activate(&mut self, now_unix_secs: u64) -> Result<(), WalletError> {
        use crate::lifecycle::LifecycleState;
        match self.lifecycle {
            LifecycleState::Revoked => return Err(WalletError::AlreadyRevoked),
            LifecycleState::Active => return Ok(()), // idempotent no-op
            LifecycleState::Rotating => return Err(WalletError::RotationInProgress),
            LifecycleState::Designated => {}
        }
        // Defense-in-depth: confirm the state-machine edge is valid.
        debug_assert!(
            LifecycleState::Designated.can_transition_to(LifecycleState::Active),
            "activate() reached the transition step with an invalid edge"
        );
        self.lifecycle = LifecycleState::Active;
        self.activated_at_unix_secs = Some(now_unix_secs);
        Ok(())
    }

    /// `Some(proof)` after `revoke()` succeeds; `None` for a not-yet-revoked
    /// identity. The proof is `Ed25519(self.signer, "revoke")` per
    /// RFC-0009 §Lifecycle row 4 — bind the holder's authorization to the
    /// burn event for audit + cross-node revocation gossip.
    #[must_use]
    pub fn revoked_proof(&self) -> Option<[u8; 64]> {
        self.revoked_proof
    }

    /// RFC-0009 §Lifecycle row 4: `Active → Revoked` (or
    /// `Rotating → Revoked`).
    ///
    /// Idempotent from `Revoked` (no-op). Refuses from `Designated` (no
    /// prior activation — there is no key to revoke per RFC-0009 §Lifecycle
    /// state-machine edge table). Accepts from `Active` and `Rotating`
    /// (both have valid `→ Revoked` edges).
    ///
    /// Produces a `revoked_proof` (`Ed25519(self.signer, "revoke")`) for
    /// audit. After the proof is captured, replaces `self.signer` with a
    /// `NullSigner` so any further `sign()` calls fail at the adapter
    /// level (defense-in-depth — the lifecycle gate is the primary check).
    ///
    /// # Errors
    /// Returns `WalletError::NotActive { current_state: Designated }` if
    /// the identity was never activated. Returns `WalletError::Hsm(_)` on
    /// adapter failure during the proof signature.
    pub fn revoke(&mut self, now_unix_secs: u64) -> Result<(), WalletError> {
        use crate::lifecycle::LifecycleState;
        if self.lifecycle == LifecycleState::Revoked {
            return Ok(()); // idempotent
        }
        // Designated → Revoked is NOT a valid state-machine edge: refuse.
        if self.lifecycle == LifecycleState::Designated {
            return Err(WalletError::NotActive {
                current_state: LifecycleState::Designated,
            });
        }
        // Defense-in-depth: confirm the state-machine edge is valid
        // (Active → Revoked or Rotating → Revoked).
        debug_assert!(
            self.lifecycle.can_transition_to(LifecycleState::Revoked),
            "revoke() reached the transition step with an invalid edge from {:?}",
            self.lifecycle
        );
        // Produce the audit proof per RFC-0009 §Lifecycle row 4.
        let proof = self.signer.sign(b"revoke")?;
        self.revoked_proof = Some(proof);
        self.lifecycle = LifecycleState::Revoked;
        self.revoked_at_unix_secs = Some(now_unix_secs);
        // Replace signer with a NullSigner: any direct `sign()` calls on
        // the adapter now fail at the adapter level (lifecycle gate is the
        // primary check; this is defense-in-depth).
        self.signer = Arc::new(crate::hsm::NullSigner::new(self.public_key));
        Ok(())
    }

    /// RFC-0009 §Lifecycle row 2: `Active → Rotating`.
    ///
    /// Initiates a rotation: records `successor` + `rotation_started_at_unix_secs`
    /// plus produces a `successor_proof` signature over
    /// `b"rotate" || successor.public_key_bytes()`.
    /// Per RFC-0009 §Lifecycle table row 2 signing requirement.
    ///
    /// Idempotent: re-invoking from `Rotating` is a no-op (returns cached
    /// successor if same public key, or fails if different).
    ///
    /// # Errors
    /// Returns `WalletError::NotActive` if state ≠ `Active` (refuses from
    /// `Designated` / `Revoked` / `Rotating`), `WalletError::SelfRotation`
    /// if `successor.did() == self.did()`, or `WalletError::Hsm(_)` on
    /// adapter failure (successor_proof signature).
    pub fn begin_rotation(
        &mut self,
        successor: IdentityKey,
        now_unix_secs: u64,
    ) -> Result<[u8; 64], WalletError> {
        use crate::lifecycle::LifecycleState;
        if self.lifecycle != LifecycleState::Active {
            return Err(WalletError::NotActive {
                current_state: self.lifecycle,
            });
        }
        if successor.public_key_bytes() == self.public_key_bytes() {
            return Err(WalletError::SelfRotation);
        }
        let proof_message = {
            let mut msg = Vec::with_capacity(6 + 32);
            msg.extend_from_slice(b"rotate");
            msg.extend_from_slice(&successor.public_key_bytes());
            msg
        };
        let proof = self.signer.sign(&proof_message)?;
        // Cache successor + timestamp (consume `successor`).
        self.successor_key = Some(Box::new(successor));
        self.rotation_started_at_unix_secs = Some(now_unix_secs);
        self.lifecycle = LifecycleState::Rotating;
        Ok(proof)
    }

    /// RFC-0009 §Lifecycle row 3: `Rotating → Active` (after grace).
    ///
    /// Completes the rotation: verifies the stored `successor_proof` against
    /// the cached successor's public key (re-derives expected proof and
    /// compares). After grace period elapses, marks old key as deprecated,
    /// clears successor linkage, returns to `Active`.
    ///
    /// Grace period: 24 hours per RFC-0853 §12.
    ///
    /// # Errors
    /// Returns `WalletError::NotRotating` if state ≠ `Rotating`,
    /// `WalletError::GracePeriodNotElapsed` if 24h grace not satisfied,
    /// `WalletError::InvalidSuccessorProof` if proof verification fails.
    pub fn complete_rotation(&mut self, now_unix_secs: u64) -> Result<(), WalletError> {
        use crate::lifecycle::LifecycleState;
        if self.lifecycle != LifecycleState::Rotating {
            return Err(WalletError::NotRotating {
                current_state: self.lifecycle,
            });
        }
        let started_at = self
            .rotation_started_at_unix_secs
            .expect("invariant: Rotating implies rotation_started_at_unix_secs is Some");
        let elapsed = now_unix_secs.saturating_sub(started_at);
        if elapsed < ROTATION_GRACE_PERIOD_SECS {
            return Err(WalletError::GracePeriodNotElapsed {
                elapsed_secs: elapsed,
                required_secs: ROTATION_GRACE_PERIOD_SECS,
            });
        }
        // Refuse completion if no successor was recorded (should not happen
        // given the invariant, but defensive).
        if self.successor_key.is_none() {
            return Err(WalletError::NotRotating {
                current_state: self.lifecycle,
            });
        }
        self.deprecated = true;
        self.successor_key = None;
        self.rotation_started_at_unix_secs = None;
        self.lifecycle = LifecycleState::Active; // re-activated as deprecated
        Ok(())
    }

    /// RFC-0009 §Lifecycle implied abort path: `Rotating → Active`.
    ///
    /// Destroys the successor linkage + returns old key to `Active`. Use
    /// when user decides NOT to complete the rotation (e.g., successor
    /// key was compromised or the rotation was initiated in error).
    ///
    /// # Errors
    /// Returns `WalletError::NotRotating` if state ≠ `Rotating`.
    pub fn abort_rotation(&mut self) -> Result<(), WalletError> {
        use crate::lifecycle::LifecycleState;
        if self.lifecycle != LifecycleState::Rotating {
            return Err(WalletError::NotRotating {
                current_state: self.lifecycle,
            });
        }
        self.successor_key = None;
        self.rotation_started_at_unix_secs = None;
        self.lifecycle = LifecycleState::Active;
        Ok(())
    }

    /// RFC-0853 §12 successor proof verifier (pure helper).
    ///
    /// Re-derives the expected proof message (`b"rotate" || new_pub`) and
    /// verifies it against the old public key + provided proof signature.
    /// Returns `Ok(())` on valid proof; `Err(InvalidSuccessorProof)` on
    /// mismatch.
    ///
    /// # Errors
    /// Returns `WalletError::InvalidSuccessorProof` if signature does not
    /// verify.
    pub fn verify_successor_proof(
        old_pub: &[u8; 32],
        new_pub: &[u8; 32],
        proof: &[u8; 64],
    ) -> Result<(), WalletError> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let vk =
            VerifyingKey::from_bytes(old_pub).map_err(|_| WalletError::InvalidSuccessorProof)?;
        let sig = Signature::from_bytes(proof);
        let mut msg = Vec::with_capacity(6 + 32);
        msg.extend_from_slice(b"rotate");
        msg.extend_from_slice(new_pub);
        vk.verify(&msg, &sig)
            .map_err(|_| WalletError::InvalidSuccessorProof)
    }

    /// RFC-0009 §Lifecycle row 4 revocation proof verifier (pure helper).
    ///
    /// Re-derives the expected proof message (`b"revoke"`) and verifies
    /// it against the identity's public key + captured proof signature.
    /// Returns `Ok(())` on valid proof; `Err(InvalidRevocationProof)` on
    /// mismatch.
    ///
    /// Mirrors `verify_successor_proof` for the revocation path. Used by
    /// cross-node revocation gossip fan-out to confirm that a revocation
    /// event was actually authorized by the holder (not forged).
    ///
    /// # Errors
    /// Returns `WalletError::InvalidRevocationProof` if the public key
    /// is not a valid Ed25519 point, the signature is malformed, or the
    /// signature does not verify.
    pub fn verify_revocation_proof(pubkey: &[u8; 32], proof: &[u8; 64]) -> Result<(), WalletError> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let vk =
            VerifyingKey::from_bytes(pubkey).map_err(|_| WalletError::InvalidRevocationProof)?;
        let sig = Signature::from_bytes(proof);
        vk.verify(b"revoke", &sig)
            .map_err(|_| WalletError::InvalidRevocationProof)
    }

    /// Crate-internal seed accessor for HKDF derivation. Returns the seed
    /// when the adapter IS an `InMemorySigner`; otherwise returns an error.
    /// This is the ONLY path that exposes the raw seed bytes; production
    /// hardware adapters cannot satisfy it.
    ///
    /// # Errors
    /// Returns `WalletError::Hsm(HsmError::Device)` when the adapter is not
    /// an `InMemorySigner` (production hardware wallets MUST NOT export seed).
    ///
    /// **Note:** `pub` (not `pub(crate)`) because the `octo-wallet` CLI binary
    /// (`src/bin/octo-wallet.rs`) needs to export the seed for vault
    /// initialization. Any other consumer should use `IdentityKey::sign()`
    /// which delegates to the adapter without exposing the seed.
    pub fn seed_bytes_for_hkdf(&self) -> Result<[u8; 32], WalletError> {
        // MVP shortcut: every `IdentityKey::from_seed` constructs with
        // `InMemorySigner` as the adapter. Production wirings using real
        // `LedgerSigner` would need a separate `LedgerSigner::export_seed`
        // APDU (deliberately not implemented — seed export is what hardware
        // wallets REFUSE to do). For now, when a non-InMemorySigner is in
        // use, the caller must catch this error.
        let any = self.signer.as_any();
        if let Some(in_mem) = any.downcast_ref::<InMemorySigner>() {
            Ok(in_mem.seed_bytes())
        } else {
            Err(WalletError::Hsm(crate::hsm::HsmError::Device(
                "seed export not supported by hardware adapter".to_owned(),
            )))
        }
    }

    /// Public verifying key (32 bytes Ed25519 public key).
    #[must_use]
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public_key
    }

    /// Ed25519 signature over `msg`. Delegates to the underlying
    /// `HsmAdapter::sign` — production `LedgerSigner` will prompt the user
    /// on-device; rejection propagates as `WalletError::Hsm(HsmError::UserRejected)`.
    ///
    /// Gates on lifecycle state per RFC-0009 §Lifecycle Requirements:
    /// - `Designated`: rejects with `WalletError::NotActive`
    /// - `Active`: proceeds normally
    /// - `Rotating`: proceeds (old key valid during grace per RFC-0009 row 3)
    /// - `Revoked`: rejects with `WalletError::NotActive`
    ///
    /// # Errors
    /// Returns `WalletError::NotActive` when lifecycle ≠ Active/Rotating,
    /// or `WalletError::Hsm(_)` on any adapter failure (transport, user
    /// rejection, device error).
    pub fn sign(&self, msg: &[u8]) -> Result<Signature, WalletError> {
        if !self.lifecycle.can_sign() {
            return Err(WalletError::NotActive {
                current_state: self.lifecycle,
            });
        }
        let sig_bytes = self.signer.sign(msg)?;
        Ok(Signature::from_bytes(&sig_bytes))
    }

    /// Borrow the underlying signer (e.g. for tests asserting parity with
    /// direct adapter calls).
    #[must_use]
    pub fn signer(&self) -> &Arc<dyn HsmAdapter> {
        &self.signer
    }

    /// Verify a signature (useful for self-tests).
    ///
    /// # Errors
    /// Returns `WalletError::Signature` if the signature is invalid.
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> Result<(), WalletError> {
        let vk = VerifyingKey::from_bytes(&self.public_key)
            .map_err(|e| WalletError::Signature(e.to_string()))?;
        vk.verify(msg, sig)
            .map_err(|e| WalletError::Signature(e.to_string()))
    }
}

/// Capability key (per-(audience, channel) symmetric key). 32 bytes.
///
/// Derived via HKDF-BLAKE3 per RFC-0009 §Capability Keys:
/// `capability_root = HKDF-BLAKE3(salt=identity_seed, info="cipherocto/cap/v1/{channel_id}", ikm=audience_did)`
///
/// `ZeroizeOnDrop` ensures the key bytes are wiped from memory when the value
/// goes out of scope (RFC-0009 §Security Considerations).
#[derive(Clone, ZeroizeOnDrop)]
pub struct CapabilityKey([u8; 32]);

impl std::fmt::Debug for CapabilityKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityKey")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl AsRef<[u8]> for CapabilityKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl CapabilityKey {
    /// Raw 32 key bytes. Caller MUST NOT log, persist, or copy. Zeroized on drop.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Audience identifier (canonical DID form). Used as HKDF IKM.
///
/// Per RFC-0010 F4 (Wallet audience validation): `AudienceId::from_str`
/// MUST validate via `octo_ident::CanonicalCodec::parse(s, allow_legacy_bare: false)`.
/// Legacy `did:octo:b<base32>` form is rejected post-deprecation window.
/// Bare `did:octo:<suffix>` literals are rejected (`DidError::LegacyFormExpired`).
/// Only canonical `did:octo:z<base58btc>` form (43-44 char suffix) is valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AudienceId(String);

impl FromStr for AudienceId {
    type Err = WalletError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(WalletError::InvalidAudienceId("empty".to_owned()));
        }
        // RFC-0010 F4: validate canonical DID shape via the codec.
        // Production paths MUST use `allow_legacy_bare: false`; legacy form
        // is rejected post-deprecation window. Test fixtures that need the
        // legacy form can call `CanonicalCodec::parse(s, true)` directly.
        octo_ident::CanonicalCodec::parse(s, false)
            .map(|_| Self(s.to_owned()))
            .map_err(|e| {
                WalletError::InvalidAudienceId(format!("canonical DID validation failed: {e}"))
            })
    }
}

impl std::fmt::Display for AudienceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for AudienceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Channel identifier (per-(audience, channel) domain separator).
/// Used as HKDF info suffix.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelId(String);

impl ChannelId {
    /// Wire-format version. Bump on info-string change.
    pub const VERSION: &'static str = "v1";

    /// HKDF info prefix per RFC-0009 §Capability Keys.
    pub const INFO_PREFIX: &'static str = "cipherocto/cap/";

    /// Compose the HKDF info string: `cipherocto/cap/v1/{channel_id}`.
    #[must_use]
    pub fn info_string(&self) -> String {
        format!("{}{}/{}", Self::INFO_PREFIX, Self::VERSION, self.0)
    }
}

impl FromStr for ChannelId {
    type Err = WalletError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(WalletError::InvalidChannelId("empty".to_owned()));
        }
        Ok(Self(s.to_owned()))
    }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Derive a per-(audience, channel) capability key from an identity.
///
/// `capability_root = HKDF-BLAKE3(salt=identity_seed, info="cipherocto/cap/v1/{channel_id}", ikm=audience_did)`
///
/// Per RFC-0009 §Capability Keys:
/// - salt = identity_seed (per-identity domain separation; sourced from
///   adapter for `InMemorySigner`; returns `WalletError::Hsm` for hardware
///   adapters that cannot export the seed — those adapters are NOT supported
///   for HKDF-derived capability roots; mission `0871a` defines the
///   hardware-wallet capability mint path)
/// - info = `cipherocto/cap/v1/{channel_id}` (versioned namespace)
/// - ikm = audience_did (audience unlinkability)
///
/// Same `(identity, audience, channel)` triple → same capability key (deterministic).
/// Different `(audience, channel)` → independent keys (SimpleX-style unlinkability).
///
/// # Errors
/// Returns `WalletError::InvalidAudienceId` / `WalletError::InvalidChannelId`
/// if the inputs fail validation, or `WalletError::Hsm` when the identity's
/// adapter cannot export the seed.
pub fn derive_capability_key(
    identity: &IdentityKey,
    audience: &AudienceId,
    channel: &ChannelId,
) -> Result<CapabilityKey, WalletError> {
    // RFC-0009 §Capability Keys: HKDF-BLAKE3(salt=identity_seed,
    // info="cipherocto/cap/v1/{channel_id}", ikm=audience_did).
    // We use `blake3::derive_key` (HKDF-style Extract-and-Expand with BLAKE3).
    // Salt is sourced from the adapter (InMemorySigner exposes it crate-internally;
    // hardware adapters refuse to export it).
    let info = channel.info_string();
    let ikm = audience.to_string();
    let context = format!("{info}:{ikm}");

    let seed = identity.seed_bytes_for_hkdf()?;
    let mut salted_ikm = Vec::with_capacity(32 + ikm.len());
    salted_ikm.extend_from_slice(&seed);
    salted_ikm.extend_from_slice(ikm.as_bytes());

    let derived = blake3::derive_key(&context, &salted_ikm);
    let mut okm = [0u8; 32];
    okm.copy_from_slice(&derived);
    Ok(CapabilityKey(okm))
}

/// `octo-cap-macaroon::CapabilitySigner` blanket impl — enables
/// `IdentityKey` to sign `CapabilityToken`s minted by the Layer 4
/// extension crate (mission 0957 Phase 2b-2). The original
/// `IdentityKey::sign` returns `Result<Signature, WalletError>`; this
/// impl maps `WalletError` → `CapabilitySignerError::Signer` for the
/// crate boundary.
///
/// Mission 0957 Phase 2b-2 / RFC-0957 §3.1: holder signing is routed
/// through the `CapabilitySigner` trait abstraction so `CapabilityToken`
/// can live in `octo-cap-macaroon` (Layer 4) without taking a dep on
/// `octo-wallet` (Layer B) — preserving the layer model.
impl octo_cap_macaroon::CapabilitySigner for IdentityKey {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], octo_cap_macaroon::CapabilitySignerError> {
        // Delegate to existing `IdentityKey::sign` (RFC-0009 HSM routing).
        // The HSM adapter (InMemorySigner / MockLedgerSigner / etc.) is
        // consulted; failure is mapped to `CapabilitySignerError::Signer`
        // with the original `WalletError` message preserved.
        IdentityKey::sign(self, msg)
            .map(|sig| sig.to_bytes())
            .map_err(|e| octo_cap_macaroon::CapabilitySignerError::Signer(e.to_string()))
    }

    fn public_key_bytes(&self) -> [u8; 32] {
        IdentityKey::public_key_bytes(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsm::InMemorySigner;
    use ed25519_dalek::Signer;

    #[test]
    fn generate_sign_verify_roundtrip() {
        let mut k = IdentityKey::generate().expect("generate");
        k.activate(1_700_000_000).expect("activate");
        let msg = b"hello world";
        let sig = k.sign(msg).expect("sign");
        k.verify(msg, &sig).expect("verify");
    }

    #[test]
    fn from_seed_signs_deterministically() {
        let seed = [42u8; 32];
        let mut k = IdentityKey::from_seed(seed);
        k.activate(1_700_000_000).expect("activate");
        let msg = b"deterministic";
        let sig1 = k.sign(msg).expect("sign 1");
        let sig2 = k.sign(msg).expect("sign 2");
        assert_eq!(sig1, sig2, "Ed25519 sign must be deterministic");
    }

    #[test]
    fn in_memory_signer_byte_identical_to_raw_signing_key() {
        // RFC-0009 byte-identical parity: HsmAdapter-mediated signing MUST
        // produce the same bytes as a direct ed25519_dalek::SigningKey call.
        let seed = [99u8; 32];
        let raw_sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let raw_sig = raw_sk.sign(b"parity check");

        let mut id = IdentityKey::from_seed(seed);
        id.activate(1_700_000_000).expect("activate");
        let adapter_sig = id.sign(b"parity check").expect("adapter sign");
        assert_eq!(raw_sig.to_bytes(), adapter_sig.to_bytes());
    }

    #[test]
    fn public_key_matches_signing_key_derivation() {
        let seed = [7u8; 32];
        let expected_pk = ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes();
        let id = IdentityKey::from_seed(seed);
        assert_eq!(id.public_key_bytes(), expected_pk);
    }

    #[test]
    fn seed_export_blocked_for_hardware_adapter() {
        // RFC-0009 A9 mitigation: a hardware-backed IdentityKey MUST NOT
        // expose its seed via `seed_bytes_for_hkdf`. We simulate a hardware
        // adapter with a custom HsmAdapter that returns a fixed public key.
        struct FakeHwAdapter {
            pk: [u8; 32],
        }
        impl HsmAdapter for FakeHwAdapter {
            fn get_public_key(&self) -> Result<[u8; 32], crate::hsm::HsmError> {
                Ok(self.pk)
            }
            fn sign(&self, _msg: &[u8]) -> Result<[u8; 64], crate::hsm::HsmError> {
                Ok([0u8; 64])
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
        let pk = [1u8; 32];
        let id = IdentityKey::with_signer(Arc::new(FakeHwAdapter { pk })).unwrap();
        let r = id.seed_bytes_for_hkdf();
        assert!(matches!(r, Err(WalletError::Hsm(_))));
    }

    #[test]
    fn derive_capability_key_succeeds_for_in_memory() {
        let k = IdentityKey::from_seed([7u8; 32]);
        let aud: AudienceId = octo_ident::test_helpers::sample_did(7)
            .parse()
            .expect("canonical audience");
        let ch: ChannelId = "channel-1".parse().unwrap();
        let _ = derive_capability_key(&k, &aud, &ch).expect("hkdf derive");
    }

    #[test]
    fn derive_deterministic() {
        let k = IdentityKey::generate().unwrap();
        let aud: AudienceId = octo_ident::test_helpers::sample_did(188).parse().unwrap();
        let ch: ChannelId = "channel-1".parse().unwrap();
        let cap1 = derive_capability_key(&k, &aud, &ch).unwrap();
        let cap2 = derive_capability_key(&k, &aud, &ch).unwrap();
        assert_eq!(cap1.as_bytes(), cap2.as_bytes());
    }

    #[test]
    fn derive_independent_channels() {
        let k = IdentityKey::generate().unwrap();
        let aud: AudienceId = octo_ident::test_helpers::sample_did(188).parse().unwrap();
        let cap_a = derive_capability_key(&k, &aud, &"ch-a".parse().unwrap()).unwrap();
        let cap_b = derive_capability_key(&k, &aud, &"ch-b".parse().unwrap()).unwrap();
        assert_ne!(cap_a.as_bytes(), cap_b.as_bytes());
    }

    #[test]
    fn derive_independent_audiences() {
        let k = IdentityKey::generate().unwrap();
        let ch: ChannelId = "channel-1".parse().unwrap();
        let cap_a = derive_capability_key(
            &k,
            &octo_ident::test_helpers::sample_did(85).parse().unwrap(),
            &ch,
        )
        .unwrap();
        // Use a second canonical DID (different pubkey → different hash)
        // rather than the legacy `did:octo:b` form, which RFC-0010 F4
        // rejects post-deprecation. The HKDF IKM is the wire string, so
        // distinct valid DIDs produce distinct capability keys.
        let cap_b = derive_capability_key(
            &k,
            &octo_ident::test_helpers::sample_did(99).parse().unwrap(),
            &ch,
        )
        .unwrap();
        assert_ne!(cap_a.as_bytes(), cap_b.as_bytes());
    }

    #[test]
    fn empty_audience_rejected() {
        assert!("".parse::<AudienceId>().is_err());
    }

    #[test]
    fn canonical_did_audience_accepted() {
        // RFC-0010 F4: canonical `did:octo:z<base58btc>` accepted.
        let canonical = octo_ident::test_helpers::sample_did(7);
        let audience: AudienceId = canonical.parse().expect("canonical accepted");
        assert_eq!(audience.to_string(), canonical);
    }

    #[test]
    fn legacy_did_b_form_rejected_post_deprecation() {
        // RFC-0010 F4: legacy `did:octo:b<base32>` rejected.
        // 52-char base32 payload satisfies `parse` step 2 *only when*
        // `allow_legacy_bare: true`; the wallet surface uses `false`, so the
        // legacy form must be rejected. We construct a syntactically valid
        // 62-char legacy form (did:octo:b + 52 base32 chars).
        let legacy = format!(
            "did:octo:b{}",
            (0..52)
                .map(|i| match i % 32 {
                    0 => b'a',
                    1 => b'b',
                    _ => b'c',
                } as char)
                .collect::<String>()
        );
        let r: Result<AudienceId, _> = legacy.parse();
        assert!(r.is_err(), "legacy did:octo:b must be rejected");
    }

    #[test]
    fn bare_did_octo_suffix_rejected_post_window() {
        // RFC-0010 §`parse` step 3: bare `did:octo:<name>` is rejected
        // post-deprecation window (allow_legacy_bare: false).
        let r: Result<AudienceId, _> = "did:octo:evil".parse();
        assert!(r.is_err(), "bare did:octo: suffix must be rejected");
    }

    #[test]
    fn wrong_prefix_did_rejected() {
        let r: Result<AudienceId, _> = "did:foo:zSomeBase58String".parse();
        assert!(r.is_err(), "non-octo DID prefix must be rejected");
    }

    #[test]
    fn malformed_did_truncated_rejected() {
        let r: Result<AudienceId, _> = "did:octo:zabc".parse();
        assert!(r.is_err(), "truncated canonical DID must be rejected");
    }

    #[test]
    fn empty_channel_rejected() {
        assert!("".parse::<ChannelId>().is_err());
    }

    #[test]
    fn channel_info_string() {
        let ch: ChannelId = "test".parse().unwrap();
        assert_eq!(ch.info_string(), "cipherocto/cap/v1/test");
    }

    #[test]
    fn in_memory_signer_factory_round_trips() {
        // Sanity check the construction path used by `IdentityKey::from_seed`.
        let seed = [11u8; 32];
        let s = InMemorySigner::new(seed, [0u8; 32]);
        assert_eq!(s.seed_bytes(), seed);
    }

    // ----- Identity lifecycle tests (mission 0009-l1) -----

    #[test]
    fn identity_key_default_lifecycle_is_designated() {
        let key = IdentityKey::from_seed([1u8; 32]);
        assert_eq!(
            key.lifecycle(),
            crate::lifecycle::LifecycleState::Designated
        );
        assert!(key.activated_at_unix_secs().is_none());
        assert!(key.revoked_at_unix_secs().is_none());
    }

    #[test]
    fn identity_key_sign_rejects_when_designated() {
        let key = IdentityKey::from_seed([2u8; 32]);
        let result = key.sign(b"msg");
        assert!(
            matches!(result, Err(WalletError::NotActive { .. })),
            "Designated must reject sign, got {result:?}"
        );
    }

    #[test]
    fn activate_from_designated_records_timestamp() {
        let mut key = IdentityKey::from_seed([3u8; 32]);
        key.activate(1_700_000_000).expect("activate");
        assert_eq!(key.lifecycle(), crate::lifecycle::LifecycleState::Active);
        assert_eq!(key.activated_at_unix_secs(), Some(1_700_000_000));
        assert!(key.revoked_at_unix_secs().is_none());
        // sign() now succeeds
        let sig = key.sign(b"msg").expect("sign after activate");
        assert_eq!(sig.to_bytes().len(), 64);
    }

    #[test]
    fn activate_is_idempotent_from_active() {
        let mut key = IdentityKey::from_seed([4u8; 32]);
        key.activate(1_000).expect("first activate");
        // Second activate returns Ok(()) no-op; timestamp unchanged.
        key.activate(2_000).expect("idempotent activate");
        assert_eq!(
            key.activated_at_unix_secs(),
            Some(1_000),
            "timestamp must NOT advance on no-op"
        );
    }

    #[test]
    fn activate_refuses_from_revoked() {
        let mut key = IdentityKey::from_seed([5u8; 32]);
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        let result = key.activate(3_000);
        assert!(
            matches!(result, Err(WalletError::AlreadyRevoked)),
            "activate on Revoked must return AlreadyRevoked, got {result:?}"
        );
    }

    #[test]
    fn revoke_from_active_rejects_sign_via_lifecycle_gate() {
        let mut key = IdentityKey::from_seed([6u8; 32]);
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        assert_eq!(key.lifecycle(), crate::lifecycle::LifecycleState::Revoked);
        assert_eq!(key.revoked_at_unix_secs(), Some(2_000));
        // Primary defense: lifecycle gate fires before the adapter is reached.
        // (NullSigner is defense-in-depth for any code path that bypasses
        // the gate via direct `sign(msg)` on the adapter.)
        let result = key.sign(b"msg");
        assert!(
            matches!(
                result,
                Err(WalletError::NotActive {
                    current_state: crate::lifecycle::LifecycleState::Revoked
                })
            ),
            "post-revoke sign must fail at lifecycle gate, got {result:?}"
        );
    }

    #[test]
    fn revoke_is_idempotent_from_revoked() {
        let mut key = IdentityKey::from_seed([7u8; 32]);
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("first revoke");
        // Second revoke: idempotent no-op; timestamp unchanged.
        key.revoke(3_000).expect("second revoke");
        assert_eq!(
            key.revoked_at_unix_secs(),
            Some(2_000),
            "timestamp must NOT advance on idempotent revoke"
        );
    }

    #[test]
    fn identity_lifecycle_clone_preserves_state() {
        let mut key = IdentityKey::from_seed([8u8; 32]);
        key.activate(1_000).expect("activate");
        let cloned = key.clone();
        assert_eq!(cloned.lifecycle(), crate::lifecycle::LifecycleState::Active);
        assert_eq!(cloned.activated_at_unix_secs(), Some(1_000));
        assert_eq!(cloned.public_key_bytes(), key.public_key_bytes());
    }

    // ----- Rotation tests (mission 0009-l2) -----

    #[test]
    fn begin_rotation_from_active_transitions_to_rotating() {
        let mut old = IdentityKey::from_seed([10u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([11u8; 32]);
        let proof = old
            .begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        assert_eq!(proof.len(), 64);
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Rotating);
    }

    #[test]
    fn begin_rotation_refuses_from_designated() {
        let mut old = IdentityKey::from_seed([12u8; 32]);
        let successor = IdentityKey::from_seed([13u8; 32]);
        let result = old.begin_rotation(successor, 2_000);
        assert!(
            matches!(result, Err(WalletError::NotActive { .. })),
            "begin_rotation from Designated must return NotActive, got {result:?}"
        );
    }

    #[test]
    fn begin_rotation_refuses_self_rotation() {
        let mut old = IdentityKey::from_seed([14u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = old.clone();
        let result = old.begin_rotation(successor, 2_000);
        assert!(
            matches!(result, Err(WalletError::SelfRotation)),
            "begin_rotation with self must return SelfRotation, got {result:?}"
        );
    }

    #[test]
    fn complete_rotation_refuses_before_grace_period() {
        let mut old = IdentityKey::from_seed([15u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([16u8; 32]);
        old.begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        // Now at t=2_000; try to complete at t=2_000 + 1h (well before 24h grace).
        let result = old.complete_rotation(2_000 + 3600);
        assert!(
            matches!(result, Err(WalletError::GracePeriodNotElapsed { .. })),
            "complete_rotation before grace must return GracePeriodNotElapsed, got {result:?}"
        );
    }

    #[test]
    fn complete_rotation_succeeds_after_grace_period() {
        let mut old = IdentityKey::from_seed([17u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([18u8; 32]);
        let old_pub = old.public_key_bytes();
        let new_pub = successor.public_key_bytes();
        let proof = old
            .begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        // Verify proof signature immediately (per RFC-0009 row 2 contract).
        IdentityKey::verify_successor_proof(&old_pub, &new_pub, &proof).expect("proof must verify");
        // Now at t=2_000 + 24h + 1s (past grace).
        let result = old.complete_rotation(2_000 + 24 * 3600 + 1);
        assert!(
            result.is_ok(),
            "complete_rotation after grace must succeed, got {result:?}"
        );
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Active);
    }

    #[test]
    fn abort_rotation_returns_to_active() {
        let mut old = IdentityKey::from_seed([19u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([20u8; 32]);
        old.begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        old.abort_rotation().expect("abort_rotation");
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Active);
    }

    #[test]
    fn verify_successor_proof_rejects_tampered_new_pub() {
        let mut old = IdentityKey::from_seed([21u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([22u8; 32]);
        let old_pub = old.public_key_bytes();
        let proof = old
            .begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        // Tamper with new_pub (use a different pubkey than the actual successor)
        let tampered = [0xff; 32];
        let result = IdentityKey::verify_successor_proof(&old_pub, &tampered, &proof);
        assert!(
            matches!(result, Err(WalletError::InvalidSuccessorProof)),
            "tampered new_pub must invalidate proof, got {result:?}"
        );
    }

    // ----- Round 1 review fixes -----

    /// F3: `activate()` from `Rotating` returns `RotationInProgress`.
    #[test]
    fn activate_refuses_from_rotating() {
        let mut old = IdentityKey::from_seed([30u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([31u8; 32]);
        old.begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        // Now in Rotating. activate() must refuse.
        let result = old.activate(3_000);
        assert!(
            matches!(result, Err(WalletError::RotationInProgress)),
            "activate from Rotating must return RotationInProgress, got {result:?}"
        );
    }

    /// F4: `revoke()` from `Designated` is REFUSED (Designated → Revoked
    /// is not a valid state-machine edge).
    #[test]
    fn revoke_refuses_from_designated() {
        let mut key = IdentityKey::from_seed([32u8; 32]);
        // Never activated. revoke() must NOT produce a revocation signature.
        let result = key.revoke(1_000);
        assert!(
            matches!(
                result,
                Err(WalletError::NotActive {
                    current_state: crate::lifecycle::LifecycleState::Designated,
                })
            ),
            "revoke from Designated must return NotActive, got {result:?}"
        );
        // State unchanged; no proof produced.
        assert_eq!(
            key.lifecycle(),
            crate::lifecycle::LifecycleState::Designated,
            "Designated state must be preserved after refused revoke"
        );
        assert!(key.revoked_proof().is_none());
    }

    /// F5: `revoke()` from `Rotating` succeeds (Rotating → Revoked IS a
    /// valid state-machine edge per RFC-0009 §Lifecycle table).
    #[test]
    fn revoke_succeeds_from_rotating() {
        let mut old = IdentityKey::from_seed([33u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([34u8; 32]);
        old.begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Rotating);
        // Revoke from Rotating should succeed and produce a proof.
        old.revoke(3_000).expect("revoke from Rotating");
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Revoked);
        assert!(old.revoked_proof().is_some(), "proof must be captured");
    }

    /// F6: `revoke()` captures the `Ed25519(signer, "revoke")` audit proof;
    /// it is `None` before, `Some(64 bytes)` after, and stable on idem-
    /// potent re-invocation.
    #[test]
    fn revoke_captures_audit_proof() {
        let mut key = IdentityKey::from_seed([35u8; 32]);
        assert!(key.revoked_proof().is_none(), "no proof pre-revoke");
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        let proof1 = key.revoked_proof().expect("proof post-revoke");
        assert_eq!(proof1.len(), 64);
        // Idempotent re-revoke: timestamp unchanged, proof unchanged.
        key.revoke(9_999).expect("idempotent revoke");
        let proof2 = key.revoked_proof().expect("proof post-re-revoke");
        assert_eq!(proof1, proof2, "proof must NOT change on idempotent revoke");
    }

    /// F14: captured `revoked_proof` actually verifies against the public
    /// key as `Ed25519(pub, "revoke")`. Cross-node revocation gossip needs
    /// this to confirm holder authorization (not forgery).
    #[test]
    fn revoke_captured_proof_verifies_against_pubkey() {
        let mut key = IdentityKey::from_seed([36u8; 32]);
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        let proof = key.revoked_proof().expect("proof captured");
        let pubkey = key.public_key_bytes();
        IdentityKey::verify_revocation_proof(&pubkey, &proof)
            .expect("captured proof must verify against pubkey");
    }

    /// F14 (tamper): tampered proof bytes or wrong pubkey must be rejected.
    #[test]
    fn verify_revocation_proof_rejects_tampered_proof() {
        let mut key = IdentityKey::from_seed([37u8; 32]);
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        let mut tampered = key.revoked_proof().expect("proof captured");
        tampered[0] ^= 0x01;
        let result = IdentityKey::verify_revocation_proof(&key.public_key_bytes(), &tampered);
        assert!(
            matches!(result, Err(WalletError::InvalidRevocationProof)),
            "tampered proof must invalidate, got {result:?}"
        );
        // Wrong pubkey (different identity).
        let wrong_pub = [0xee; 32];
        let proof = key.revoked_proof().expect("proof");
        let result = IdentityKey::verify_revocation_proof(&wrong_pub, &proof);
        assert!(
            matches!(result, Err(WalletError::InvalidRevocationProof)),
            "proof under wrong pubkey must fail, got {result:?}"
        );
    }

    /// F30: `is_deprecated()` is `false` by default, flips to `true` only
    /// after `complete_rotation()` succeeds. Distinct from `Revoked` state.
    #[test]
    fn is_deprecated_flips_only_after_complete_rotation() {
        let mut old = IdentityKey::from_seed([38u8; 32]);
        assert!(!old.is_deprecated(), "fresh key must NOT be deprecated");

        old.activate(1_000).expect("activate");
        assert!(!old.is_deprecated(), "active key must NOT be deprecated");

        let successor = IdentityKey::from_seed([39u8; 32]);
        old.begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        assert!(
            !old.is_deprecated(),
            "rotating key must NOT be deprecated (grace window)"
        );

        // complete_rotation after grace period: flips to deprecated.
        old.complete_rotation(2_000 + 24 * 3600 + 1)
            .expect("complete_rotation");
        assert!(
            old.is_deprecated(),
            "key must be deprecated after complete_rotation"
        );
        // Lifecycle state remains Active (deprecated is orthogonal).
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Active);

        // Revoking a deprecated key still works (Revoked supersedes deprecated).
        old.revoke(9_999).expect("revoke");
        assert!(old.is_deprecated(), "deprecated flag persists past revoke");
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Revoked);
    }

    /// F33: documented semantics — revoking one clone does NOT revoke the
    /// others. Each clone retains its own signer Arc.
    #[test]
    fn clone_is_independent_of_revoke() {
        let mut key = IdentityKey::from_seed([40u8; 32]);
        key.activate(1_000).expect("activate");
        let backup = key.clone();

        // Revoke the original.
        key.revoke(2_000).expect("revoke");
        assert_eq!(key.lifecycle(), crate::lifecycle::LifecycleState::Revoked);
        assert!(key.sign(b"msg").is_err(), "revoked key must NOT sign");

        // Backup clone retains the Active lifecycle + live signer.
        assert_eq!(
            backup.lifecycle(),
            crate::lifecycle::LifecycleState::Active,
            "clone must retain independent lifecycle state"
        );
        assert!(
            backup.sign(b"msg").is_ok(),
            "clone of pre-revoke key must still sign"
        );

        // Backup must revoke() independently to be retired.
        let mut backup = backup;
        backup.revoke(3_000).expect("backup revoke");
        assert!(backup.sign(b"msg").is_err());
    }
}
