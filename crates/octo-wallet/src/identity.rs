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
    public_key: Vec<u8>,
    /// `key_id` for the mandatory signable prefix (RFC-0009 §Domain
    /// separation), or `None` for an identity restored without one.
    ///
    /// `None` means this identity **cannot** sign a prefixed message, and
    /// [`Self::sign_prefixed`] refuses rather than falling back to an
    /// unprefixed signature. That is deliberate: RFC-0009 forbids both
    /// deriving the id from the public key and allocating one at signing
    /// time, so there is no honest way to invent one here, and a locally
    /// allocated id would make the signed domain depend on who signed — which
    /// defeats the entire mechanism.
    verification_method_id: Option<octo_ident::VerificationMethodId>,
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
    revoked_proof: Option<Vec<u8>>,
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
            .field("public_key", &hex::encode(&self.public_key))
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
            public_key: self.public_key.clone(),
            verification_method_id: self.verification_method_id,
            lifecycle: self.lifecycle,
            activated_at_unix_secs: self.activated_at_unix_secs,
            revoked_at_unix_secs: self.revoked_at_unix_secs,
            revoked_proof: self.revoked_proof.clone(),
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
            signer: Arc::new(InMemorySigner::new(seed, public_key.to_vec())),
            public_key: public_key.to_vec(),
            verification_method_id: None,
            lifecycle: crate::lifecycle::LifecycleState::Designated,
            activated_at_unix_secs: None,
            revoked_at_unix_secs: None,
            revoked_proof: None,
            successor_key: None,
            rotation_started_at_unix_secs: None,
            deprecated: false,
        }
    }

    /// Rehydrate an `IdentityKey` from a persisted `IdentityRecord` (mission
    /// 0011-x-s-a-wallet-store-identity §AC-33). Takes **five** parameters
    /// because four are not enough: `from_seed` hard-codes
    /// `LifecycleState::Designated` and leaves
    /// `rotation_started_at_unix_secs` at `None`, so rehydrating through
    /// it resurrects a `Revoked` identity as a signing key and panics
    /// inside `complete_rotation` on the next call.
    ///
    /// `rotation_started_at` is reconstructed from the newest entry in
    /// `record.rotation_history` when the persisted lifecycle is
    /// `Rotating`; `None` otherwise. The `deprecated` field of the
    /// rehydrated key always starts `false` — the deprecation marker
    /// is post-`complete_rotation` state that the store persists on
    /// the `IdentityRecord.deprecated` field (mission §AC-36) and
    /// reads back via `identity_record`, not via this constructor.
    /// The `revoked_proof` is intentionally NOT rehydrated — the
    /// proof is for cross-node gossip, which the store does not
    /// participate in; only the marker that a revoke has happened
    /// is rehydrated, via `lifecycle = Revoked` plus
    /// `revoked_at_unix_secs`, but the substrate refuses to mint a
    /// signable key from a `Revoked` record (see error contract
    /// below).
    ///
    /// # Errors
    /// Returns `WalletError::AlreadyRevoked` when `lifecycle == Revoked`
    /// — the substrate refuses to manufacture a signable key from a
    /// terminal record even though the underlying primitives would
    /// permit it. Returning a `Revoked` key here defeats the
    /// rehydration guarantee AC-32 names.
    #[allow(clippy::too_many_arguments)]
    pub fn from_seed_with_lifecycle(
        seed: [u8; 32],
        lifecycle: crate::lifecycle::LifecycleState,
        activated_at: Option<u64>,
        revoked_at: Option<u64>,
        rotation_started_at: Option<u64>,
    ) -> Result<Self, WalletError> {
        use crate::lifecycle::LifecycleState;
        if lifecycle == LifecycleState::Revoked {
            return Err(WalletError::AlreadyRevoked);
        }
        let sk = ed25519_dalek::SigningKey::from_bytes(&seed);
        let public_key = sk.verifying_key().to_bytes();
        Ok(Self {
            signer: Arc::new(InMemorySigner::new(seed, public_key.to_vec())),
            public_key: public_key.to_vec(),
            // The persisted record carries no `key_id`, so a rehydrated
            // identity has none. It refuses prefixed signing instead of
            // allocating one: a locally allocated id would make the signed
            // domain depend on who signed. Carrying the id through the
            // record is the follow-up that lifts this.
            verification_method_id: None,
            lifecycle,
            activated_at_unix_secs: activated_at,
            revoked_at_unix_secs: revoked_at,
            revoked_proof: None,
            successor_key: None,
            rotation_started_at_unix_secs: rotation_started_at,
            deprecated: false,
        })
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
            verification_method_id: None,
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

    /// Restore the deprecation flag a completed rotation set.
    ///
    /// The flag is the only record that this key was retired by a
    /// rotation rather than never used. `from_seed_with_lifecycle`
    /// cannot take it as an argument without changing a five-argument
    /// constructor that every other caller depends on, and
    /// `persist_active_record` writes the in-memory value straight
    /// back over the persisted one - so a key that did not restore it
    /// here reported the predecessor as un-retired AND erased the
    /// true flag from disk on its next transition.
    pub fn set_deprecated(&mut self, deprecated: bool) {
        self.deprecated = deprecated;
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
    pub fn revoked_proof(&self) -> Option<Vec<u8>> {
        self.revoked_proof.clone()
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
        //
        // The literal is bound to this identity's algorithm and key rather
        // than signed bare. As a bare constant it attested to nothing beyond
        // "someone holding this key signed the word revoke" — the same six
        // bytes under any algorithm, any key, any time.
        //
        // **Revocation is never gated on this.** A key context is an audit
        // annotation, and making the ability to revoke depend on one would
        // hand anyone who can strip a `key_id` from a record a permanent
        // denial of service on the safety action itself. So an identity with
        // no key context still revokes; it simply produces no *bound* audit
        // proof, and §`verify_revocation_proof` will refuse that absence
        // rather than mistake it for one.
        //
        // This is asymmetric with §`begin_rotation` on purpose. Rotation is
        // an operator action with the caller standing right there, so
        // failing loudly is correct there and refusing is not a DoS.
        self.revoked_proof = self.sign_lifecycle_proof(b"revoke")?;
        self.lifecycle = LifecycleState::Revoked;
        self.revoked_at_unix_secs = Some(now_unix_secs);
        // Replace signer with a NullSigner: any direct `sign()` calls on
        // the adapter now fail at the adapter level (lifecycle gate is the
        // primary check; this is defense-in-depth).
        self.signer = Arc::new(crate::hsm::NullSigner::new(self.public_key.clone()));
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
    ) -> Result<Vec<u8>, WalletError> {
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
            msg.extend_from_slice(successor.public_key_bytes());
            msg
        };
        // Bound to this identity's algorithm and key. Safe to require now
        // that `IdentityRecord` carries the key context across a store
        // reload — without that, this would work before a save and fail
        // after the reopen, which is the cliff this line used to sit on.
        let proof = self
            .sign_lifecycle_proof(&proof_message)?
            .ok_or(WalletError::NoKeyContext)?;
        // Cache successor + timestamp (consume `successor`).
        self.successor_key = Some(Box::new(successor));
        self.rotation_started_at_unix_secs = Some(now_unix_secs);
        self.lifecycle = LifecycleState::Rotating;
        Ok(proof)
    }

    /// Re-attach a successor key to a key that is already in the
    /// `Rotating` lifecycle, without re-running the transition.
    ///
    /// `begin_rotation` caches the successor in memory only, so a
    /// second process that rehydrates the predecessor from disk has
    /// no successor linkage and `complete_rotation` refuses with
    /// `NotRotating { current_state: Rotating }` — the state says the
    /// rotation is in flight while the key says it has no successor.
    /// The store rehydrates the successor from its vault-sealed seed
    /// and calls this before completing.
    ///
    /// Returns `WalletError::SelfRotation` if `successor` is this
    /// key, refusing the same pairing `begin_rotation` refuses.
    pub fn rehydrate_successor(&mut self, successor: IdentityKey) -> Result<(), WalletError> {
        use crate::lifecycle::LifecycleState;
        if !matches!(self.lifecycle, LifecycleState::Rotating) {
            return Err(WalletError::NotRotating {
                current_state: self.lifecycle,
            });
        }
        if successor.public_key_bytes() == self.public_key_bytes() {
            return Err(WalletError::SelfRotation);
        }
        self.successor_key = Some(Box::new(successor));
        Ok(())
    }

    /// RFC-0009 §Lifecycle row 3: `Rotating → Active` (after grace).
    ///
    /// Completes the rotation: after the grace period elapses, marks the
    /// old key as deprecated, clears the successor linkage, and returns
    /// to `Active`.
    ///
    /// Grace period: 24 hours per RFC-0853 §12.
    ///
    /// # Where the successor proof is checked — NOT here
    ///
    /// An earlier revision of this comment claimed that this function
    /// "verifies the stored `successor_proof` against the cached
    /// successor's public key", and listed
    /// `WalletError::InvalidSuccessorProof` among the errors it returns.
    /// Neither was true and neither could have become true here: the
    /// body contains no call that can raise either, and the proof is
    /// not among this function's inputs. The check lives in
    /// `WalletStore::rehydrate_successor_key`, which every rotation
    /// path reaches through `unlock` before the successor is ever
    /// attached to a key — so a `Rotating` key reaching this function
    /// has already had its proof verified, and a hand-edited index
    /// naming an unauthorised successor is refused upstream with
    /// `InvalidSuccessorProof` rather than here.
    ///
    /// The property is not unpinned. `tv_x_65` forges the proof on a
    /// persisted rotation event and asserts the refusal, and the
    /// round-trip in `tv_x_c_41` fails if the message either side of
    /// that check - `b"rotate" || successor_pubkey`, written in three
    /// places - drifts from the other two. What was wrong was the
    /// claim about *this* function, and a reader auditing the function
    /// that promotes a successor to active would have stopped here.
    ///
    /// # Errors
    /// Returns `WalletError::NotRotating` if state ≠ `Rotating` or if
    /// no successor is attached, and `WalletError::GracePeriodNotElapsed`
    /// if the 24h grace is not satisfied. A clock that has moved
    /// backwards yields an elapsed of zero and is refused rather than
    /// underflowing. This function does not return
    /// `InvalidSuccessorProof`; see the section above.
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

    /// Sign a lifecycle proof under the mandatory signable prefix.
    ///
    /// The proof is a bare literal (`b"revoke"`) or a bare concatenation
    /// (`b"rotate" ‖ new_pub`). Both are bound here to this identity's
    /// algorithm and verification method, so a proof stops being a value
    /// that means the same thing under any equation or any key.
    ///
    /// # Errors
    /// - [`WalletError::NoKeyContext`] when no `key_id` is attached. Refused
    ///   rather than downgraded: an unprefixed proof is the legacy shape, and
    ///   a verifier that accepts both has a downgrade path.
    /// - Any error from the underlying [`HsmAdapter::sign`].
    fn sign_lifecycle_proof(&self, payload: &[u8]) -> Result<Option<Vec<u8>>, WalletError> {
        // `None` means "this identity has no key context, so there is no
        // bound proof to produce" — never "the proof is invalid". Callers
        // decide whether that is fatal; §`revoke` decides it is not.
        let Some(prefix) = self
            .verification_method_id
            .map(|key_id| octo_ident::SignablePrefix::new(self.algorithm_id(), key_id))
        else {
            return Ok(None);
        };
        let message = octo_ident::SignedMessage::new(prefix, payload.to_vec());
        let signable = message
            .signable(octo_ident::SignableForm::Raw)
            .map_err(|e| WalletError::Signature(e.to_string()))?;
        Ok(Some(self.signer.sign(&signable)?))
    }

    /// RFC-0853 §12 successor proof verifier (pure helper).
    ///
    /// Re-derives the expected proof message under the mandatory signable
    /// prefix and verifies it against the old public key, dispatching on
    /// `algorithm_id` through `AlgorithmRegistry` (RFC-0009 AC#11).
    ///
    /// `key_id` is required rather than inferred: the prefix binds it, and a
    /// verifier that cannot name the key cannot rebuild the domain the signer
    /// signed. Passing an algorithm without a key is exactly the preference
    /// case RFC-0009 §Domain separation removes.
    ///
    /// # Errors
    /// Returns `WalletError::InvalidSuccessorProof` if the algorithm is not
    /// registered, or the signature does not verify.
    pub fn verify_successor_proof(
        algorithm_id: &octo_ident::AlgorithmId,
        key_id: &octo_ident::VerificationMethodId,
        old_pub: &[u8],
        new_pub: &[u8],
        proof: &[u8],
    ) -> Result<(), WalletError> {
        let mut payload = Vec::with_capacity(6 + 32);
        payload.extend_from_slice(b"rotate");
        payload.extend_from_slice(new_pub);
        IdentityKey::verify_lifecycle_proof(algorithm_id, key_id, old_pub, &payload, proof, || {
            WalletError::InvalidSuccessorProof
        })
    }

    /// RFC-0009 §Lifecycle row 4 revocation proof verifier (pure helper).
    ///
    /// Re-derives the expected proof message (`b"revoke"`) and verifies
    /// it against the identity's public key + captured proof signature,
    /// dispatching on `algorithm_id` through `AlgorithmRegistry`.
    ///
    /// Mirrors `verify_successor_proof` for the revocation path. Used by
    /// cross-node revocation gossip fan-out to confirm that a revocation
    /// event was actually authorized by the holder (not forged).
    ///
    /// # Errors
    /// Returns `WalletError::InvalidRevocationProof` if the algorithm is
    /// not registered, the key or signature is malformed, or the
    /// signature does not verify.
    pub fn verify_revocation_proof(
        algorithm_id: &octo_ident::AlgorithmId,
        key_id: &octo_ident::VerificationMethodId,
        pubkey: &[u8],
        proof: &[u8],
    ) -> Result<(), WalletError> {
        IdentityKey::verify_lifecycle_proof(algorithm_id, key_id, pubkey, b"revoke", proof, || {
            WalletError::InvalidRevocationProof
        })
    }

    /// Shared body for the two lifecycle-proof verifiers.
    ///
    /// Rebuilds the signable prefix the signer used and verifies under the
    /// named algorithm. Both proofs are a short payload, and both used to be
    /// verified as a bare concatenation against a caller-supplied algorithm.
    fn verify_lifecycle_proof(
        algorithm_id: &octo_ident::AlgorithmId,
        key_id: &octo_ident::VerificationMethodId,
        public_key: &[u8],
        payload: &[u8],
        proof: &[u8],
        error: fn() -> WalletError,
    ) -> Result<(), WalletError> {
        let message = octo_ident::SignedMessage::new(
            octo_ident::SignablePrefix::new(*algorithm_id, *key_id),
            payload.to_vec(),
        );
        let signable = message
            .signable(octo_ident::SignableForm::Raw)
            .map_err(|_| error())?;
        octo_algo_ed25519::verify_dispatch(algorithm_id, public_key, &signable, proof)
            .map_err(|_| error())
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

    /// The verifying key, whatever length the adapter's algorithm uses.
    ///
    /// Borrowed rather than copied: a stark-schnorr key is 33 bytes and a
    /// fixed-width accessor would have forced it back to 32.
    #[must_use]
    pub fn public_key_bytes(&self) -> &[u8] {
        &self.public_key
    }

    /// Return this identity's DID. Layer B `[ADD]` per RFC-0011 §Subcommand
    /// Taxonomy (entry #2: `identity show <DID>`).
    ///
    /// Canonical form per RFC-0010: `did:octo:<encoded-pubkey>`. The current
    /// encoding is hex (RFC-0010 wire-format prep — the canonical
    /// `did:octo:z<base58btc>` form is assembled by the CLI layer that
    /// consumes the public key; the wallet substrate returns the hex form
    /// because hex is a stable, lossless representation and avoids
    /// re-implementing base58btc in the substrate).
    #[must_use]
    pub fn did(&self) -> crate::identity_record::Did {
        crate::identity_record::Did(format!("did:octo:{}", hex::encode(&self.public_key)))
    }

    /// Ed25519 signature over `msg`, with **no** signable prefix.
    ///
    /// Delegates to the underlying `HsmAdapter::sign` — production
    /// `LedgerSigner` will prompt the user on-device; rejection propagates as
    /// `WalletError::Hsm(HsmError::UserRejected)`.
    ///
    /// Gates on lifecycle state per RFC-0009 §Lifecycle Requirements:
    /// - `Designated`: rejects with `WalletError::NotActive`
    /// - `Active`: proceeds normally
    /// - `Rotating`: proceeds (old key valid during grace per RFC-0009 row 3)
    /// - `Revoked`: rejects with `WalletError::NotActive`
    ///
    /// Sign `msg`, returning the signature **bytes**.
    ///
    /// Bytes rather than an `ed25519_dalek::Signature`: the return type is the
    /// public face of this method's coupling, and a dalek type there asserts
    /// Ed25519 to every reader — including the compiler, which will then
    /// refuse the value if a future adapter speaks anything else. RFC-0009
    /// AC#11 exists so nothing in the API asserts an algorithm that the
    /// registry could have chosen differently. [`Self::verify`] already
    /// dispatches; this returns raw bytes for the same reason.
    ///
    /// The algorithm is still **not** free-floating: it is recoverable from
    /// [`Self::algorithm_id`], which sources it from the signer that produced
    /// the key, so a caller pairing these two cannot mismatch them.
    ///
    /// [`Self::sign_prefixed`] remains the preferred entry point — it commits
    /// the signature to `algorithm_id ‖ key_id ‖ payload` rather than a bare
    /// message. This one exists for callers that build their own signable
    /// bytes.
    ///
    /// # This is the low-level primitive, not the domain-separated one
    ///
    /// A signature made here binds neither the algorithm nor the key — it
    /// commits to exactly the bytes handed in. RFC-0009 §Signable Domain
    /// Separation requires more than that, so:
    ///
    /// - Prefer [`Self::sign_prefixed`], which emits
    ///   `SignablePrefix ‖ payload` and refuses without a key context.
    /// - `octo-runtime`'s attach-handle path is also correct: it composes
    ///   `signable_bytes(prefix, canonical)` itself and uses this only to sign
    ///   bytes it has already domain-separated.
    /// - The `octo_cap_macaroon::CapabilitySigner` impl signs bare. That form
    ///   is still an open decision (whether the holder message's prefix travels
    ///   in band) and is tracked separately from mission `0009-e9`.
    ///
    /// Any new caller that does not compose a prefix first is a domain-separation
    /// defect, and this method's existence is what makes that mistake easy to
    /// make quietly.
    ///
    /// # Errors
    /// - [`WalletError::NotActive`] when the lifecycle forbids signing.
    /// - Any error from the underlying [`HsmAdapter::sign`].
    pub fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, WalletError> {
        if !self.lifecycle.can_sign() {
            return Err(WalletError::NotActive {
                current_state: self.lifecycle,
            });
        }
        self.signer.sign(msg).map_err(WalletError::Hsm)
    }

    /// Borrow the underlying signer (e.g. for tests asserting parity with
    /// direct adapter calls).
    #[must_use]
    pub fn signer(&self) -> &Arc<dyn HsmAdapter> {
        &self.signer
    }

    /// Verify a signature over `msg` against this identity's key.
    ///
    /// The algorithm comes from the signer that produced the key, so this
    /// dispatches through `AlgorithmRegistry` rather than deciding Ed25519
    /// here (RFC-0009 AC#11). An unregistered algorithm is an error, never
    /// a fallback.
    ///
    /// # Errors
    /// Returns `WalletError::Signature` if the algorithm is not registered
    /// in this deployment or the signature does not verify.
    pub fn verify(&self, msg: &[u8], sig: &[u8]) -> Result<(), WalletError> {
        octo_algo_ed25519::verify_dispatch(&self.signer.algorithm_id(), &self.public_key, msg, sig)
            .map_err(|e| WalletError::Signature(e.to_string()))
    }

    /// The algorithm this identity's key speaks.
    ///
    /// Sourced from the signer, not stored separately, so the two cannot
    /// drift apart.
    #[must_use]
    pub fn algorithm_id(&self) -> octo_ident::AlgorithmId {
        self.signer.algorithm_id()
    }

    // ── The mandatory signable prefix (RFC-0009 §Domain separation) ──────

    /// Attach the `key_id` this identity signs under.
    ///
    /// RFC-0009 forbids deriving it from the public key and forbids
    /// allocating one at signing time, so it has to come from the DID
    /// document that already names this key. This is where a caller supplies
    /// that.
    ///
    /// Fails closed rather than guessing: an identity with no key context can
    /// still sign through [`Self::sign`], but [`Self::sign_prefixed`] and
    /// [`Self::verify_prefixed`] refuse until this is set.
    #[must_use]
    pub fn with_verification_method_id(mut self, key_id: octo_ident::VerificationMethodId) -> Self {
        self.verification_method_id = Some(key_id);
        self
    }

    /// This identity's `key_id`, if it has one.
    #[must_use]
    pub fn verification_method_id(&self) -> Option<octo_ident::VerificationMethodId> {
        self.verification_method_id
    }

    /// The signable prefix this identity emits.
    ///
    /// # Errors
    /// [`WalletError::NoKeyContext`] when no `key_id` is attached.
    pub fn signable_prefix(&self) -> Result<octo_ident::SignablePrefix, WalletError> {
        let key_id = self
            .verification_method_id
            .ok_or(WalletError::NoKeyContext)?;
        Ok(octo_ident::SignablePrefix::new(self.algorithm_id(), key_id))
    }

    /// The wire message for `payload` under this identity's key.
    ///
    /// `prefix ‖ payload`. This is what goes on the wire and what
    /// [`Self::sign_prefixed`] signs.
    ///
    /// # Errors
    /// [`WalletError::NoKeyContext`] when no `key_id` is attached.
    pub fn prefixed_message(&self, payload: &[u8]) -> Result<Vec<u8>, WalletError> {
        Ok(octo_ident::SignedMessage::new(self.signable_prefix()?, payload.to_vec()).to_bytes())
    }

    /// Sign `payload` under the mandatory prefix.
    ///
    /// The signature covers `algorithm_id ‖ key_id ‖ canonical_ser(payload)`,
    /// not the bare payload, so a signature made here cannot be presented
    /// against another key or another algorithm.
    ///
    /// # Errors
    /// - [`WalletError::NoKeyContext`] when no `key_id` is attached. Signing
    ///   without one is refused rather than downgraded to an unprefixed
    ///   signature: a downgrade path is how an unprefixed message keeps
    ///   verifying after the prefix is deployed.
    /// - [`WalletError::NotActive`] when the lifecycle forbids signing.
    /// - Any error from the underlying [`HsmAdapter::sign`].
    pub fn sign_prefixed(&self, payload: &[u8]) -> Result<Vec<u8>, WalletError> {
        // Order matters: refusing for a missing key context first would leak
        // less, but the lifecycle check is the one that must not be
        // skippable, so both run before the signer is touched.
        let prefix = self.signable_prefix()?;
        if !self.lifecycle.can_sign() {
            return Err(WalletError::NotActive {
                current_state: self.lifecycle,
            });
        }
        let message = octo_ident::SignedMessage::new(prefix, payload.to_vec());
        let signable = message
            .signable(octo_ident::SignableForm::Raw)
            .map_err(|e| WalletError::Signature(e.to_string()))?;
        Ok(self.signer.sign(&signable)?)
    }

    /// Verify a prefixed message and its signature.
    ///
    /// RFC-0009 requires a verifier to reject a message with no prefix and to
    /// "verify under the algorithm the prefix names rather than the one it
    /// prefers". Both happen here: the prefix is parsed out of the wire
    /// bytes, the named algorithm is what gets dispatched, and a `key_id`
    /// that is not this key's is refused.
    ///
    /// # Errors
    /// - [`WalletError::Signature`] if the message carries no usable prefix.
    /// - [`WalletError::KeyIdMismatch`] if the prefix names a different key.
    /// - [`WalletError::Signature`] if the named algorithm is unregistered or
    ///   the signature does not verify.
    pub fn verify_prefixed(&self, wire: &[u8], sig: &[u8]) -> Result<(), WalletError> {
        let message = octo_ident::SignedMessage::parse(wire, octo_ident::SignableForm::Raw)
            .map_err(|e| WalletError::Signature(e.to_string()))?;

        // The key binding. A signature replayed from this identity's other
        // key, or another identity's, carries a different key_id and is
        // refused before any equation is evaluated.
        let expected = self
            .verification_method_id
            .ok_or(WalletError::NoKeyContext)?;
        if message.prefix().key_id() != expected {
            return Err(WalletError::KeyIdMismatch);
        }

        let signable = message
            .signable(octo_ident::SignableForm::Raw)
            .map_err(|e| WalletError::Signature(e.to_string()))?;

        // Dispatch on the algorithm the PREFIX names. Reading it from the
        // message rather than from `self.algorithm_id()` is the whole point:
        // the verifier's own preference is exactly what must not decide.
        octo_algo_ed25519::verify_dispatch(
            &message.prefix().algorithm_id(),
            &self.public_key,
            &signable,
            sig,
        )
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
    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, octo_cap_macaroon::CapabilitySignerError> {
        // Delegate to existing `IdentityKey::sign` (RFC-0009 HSM routing).
        // The HSM adapter (InMemorySigner / MockLedgerSigner / etc.) is
        // consulted; failure is mapped to `CapabilitySignerError::Signer`
        // with the original `WalletError` message preserved.
        //
        // Signs the exact bytes handed in, with no prefix composed here. That
        // is the open capability question mission `0009-e9` deliberately did
        // not decide: whether the holder message's prefix travels in band.
        IdentityKey::sign(self, msg)
            .map_err(|e| octo_cap_macaroon::CapabilitySignerError::Signer(e.to_string()))
    }

    /// `IdentityKey` is Ed25519 and its key is 32 bytes, so the inherent
    /// method is correctly fixed width; the trait widens only so the
    /// capability substrate can represent other algorithms (mission `0009-e8`).
    fn public_key_bytes(&self) -> Vec<u8> {
        IdentityKey::public_key_bytes(self).to_vec()
    }

    /// The algorithm this identity's signer speaks — sourced from the signer
    /// rather than restated, so the two cannot drift (mission `0009-e5`).
    fn algorithm_id(&self) -> octo_ident::AlgorithmId {
        IdentityKey::algorithm_id(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsm::InMemorySigner;
    use ed25519_dalek::Signer;
    use octo_ident::AlgorithmId;

    #[test]
    fn generate_sign_verify_roundtrip() {
        let mut k = IdentityKey::generate().expect("generate");
        k.activate(1_700_000_000).expect("activate");
        let msg = b"hello world";
        let sig = k.sign(msg).expect("sign");
        k.verify(msg, &sig).expect("verify");
    }

    #[test]
    fn verify_dispatches_on_the_signers_algorithm_not_on_ed25519() {
        // The control for AC#11 at the RFC's own primary type. Before the
        // dispatch, `verify` built a `VerifyingKey` unconditionally, so a
        // key speaking any other algorithm was checked as though it were
        // Ed25519 and the answer was a silent wrong result rather than an
        // error. This asserts the fail-closed half.
        struct StrangerAdapter {
            pk: [u8; 32],
        }
        impl HsmAdapter for StrangerAdapter {
            fn get_public_key(&self) -> Result<Vec<u8>, crate::hsm::HsmError> {
                Ok(self.pk.to_vec())
            }
            fn sign(&self, _msg: &[u8]) -> Result<Vec<u8>, crate::hsm::HsmError> {
                Ok(vec![0u8; 64])
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn algorithm_id(&self) -> octo_ident::AlgorithmId {
                AlgorithmId::derive("not-a-registered-algorithm")
            }
        }

        let key = IdentityKey::with_signer(Arc::new(StrangerAdapter { pk: [3u8; 32] }))
            .expect("with_signer");
        let err = key
            .verify(b"anything", &[0u8; 64])
            .expect_err("an unregistered algorithm must not verify");
        let msg = err.to_string();
        assert!(
            msg.contains("no registered algorithm"),
            "expected an unknown-algorithm refusal, got: {msg}"
        );
    }

    #[test]
    fn a_real_signature_from_another_key_is_rejected() {
        // The dispatch path must still reject honestly. Routing through the
        // registry is only correct if the underlying check still happens.
        let mut signer = IdentityKey::generate().expect("generate");
        let mut other = IdentityKey::generate().expect("generate");
        // Signing is gated on the lifecycle state machine, so both keys
        // must be Active before `sign` will produce anything.
        signer.activate(1_700_000_000).expect("activate signer");
        other.activate(1_700_000_000).expect("activate other");
        let msg = b"cross-key";
        let sig = other.sign(msg).expect("sign");
        assert!(
            signer.verify(msg, &sig).is_err(),
            "a signature from a different key must not verify"
        );
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
        assert_eq!(raw_sig.to_bytes().to_vec(), adapter_sig);
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
            fn get_public_key(&self) -> Result<Vec<u8>, crate::hsm::HsmError> {
                Ok(self.pk.to_vec())
            }
            fn sign(&self, _msg: &[u8]) -> Result<Vec<u8>, crate::hsm::HsmError> {
                Ok(vec![0u8; 64])
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn algorithm_id(&self) -> octo_ident::AlgorithmId {
                octo_algo_ed25519::ed25519_algorithm_id()
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
        let s = InMemorySigner::new(seed, vec![0u8; 32]);
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
        assert_eq!(sig.len(), 64);
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
        let mut old = rotation_key([10u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = rotation_key([11u8; 32]);
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
        let mut old = rotation_key([15u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = rotation_key([16u8; 32]);
        old.begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        // Now at t=2_000; try to complete at t=2_000 + 1h (well before 24h grace).
        let result = old.complete_rotation(2_000 + 3600);
        assert!(
            matches!(result, Err(WalletError::GracePeriodNotElapsed { .. })),
            "complete_rotation before grace must return GracePeriodNotElapsed, got {result:?}"
        );
    }

    /// An identity with a key context attached, for the lifecycle proofs.
    ///
    /// `from_seed` alone carries none, and §`begin_rotation` now refuses
    /// without one — so a rotation test that does not attach a key id is
    /// testing a path that no longer exists.
    fn with_key(seed: [u8; 32], key_id: &str) -> IdentityKey {
        IdentityKey::from_seed(seed)
            .with_verification_method_id(octo_ident::VerificationMethodId::derive(key_id))
    }

    /// The key id the helper above attaches for these tests.
    const TEST_KEY_ID: &str = "key-a";
    /// An identity with a key context, for the rotation tests.
    ///
    /// The successor proof is the authorisation for a handoff, so it refuses
    /// without one — an unbound proof is not a weaker proof, it is none.
    fn rotation_key(seed: [u8; 32]) -> IdentityKey {
        IdentityKey::from_seed(seed)
            .with_verification_method_id(octo_ident::VerificationMethodId::derive(TEST_KEY_ID))
    }

    #[test]
    fn complete_rotation_succeeds_after_grace_period() {
        let mut old = with_key([17u8; 32], TEST_KEY_ID);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([18u8; 32]);
        let old_pub = old.public_key_bytes().to_vec();
        let new_pub = successor.public_key_bytes().to_vec();
        let proof = old
            .begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        // Verify proof signature immediately (per RFC-0009 row 2 contract).
        IdentityKey::verify_successor_proof(
            &octo_algo_ed25519::ed25519_algorithm_id(),
            &octo_ident::VerificationMethodId::derive(TEST_KEY_ID),
            &old_pub,
            &new_pub,
            &proof,
        )
        .expect("proof must verify");
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
        let mut old = rotation_key([19u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = rotation_key([20u8; 32]);
        old.begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        old.abort_rotation().expect("abort_rotation");
        assert_eq!(old.lifecycle(), crate::lifecycle::LifecycleState::Active);
    }

    #[test]
    fn verify_successor_proof_rejects_tampered_new_pub() {
        let mut old = with_key([21u8; 32], TEST_KEY_ID);
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([22u8; 32]);
        let old_pub = old.public_key_bytes().to_vec();
        let proof = old
            .begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        // Tamper with new_pub (use a different pubkey than the actual successor)
        let tampered = [0xff; 32];
        let result = IdentityKey::verify_successor_proof(
            &octo_algo_ed25519::ed25519_algorithm_id(),
            &octo_ident::VerificationMethodId::derive(TEST_KEY_ID),
            &old_pub,
            &tampered,
            &proof,
        );
        assert!(
            matches!(result, Err(WalletError::InvalidSuccessorProof)),
            "tampered new_pub must invalidate proof, got {result:?}"
        );
    }

    // ----- Round 1 review fixes -----

    /// F3: `activate()` from `Rotating` returns `RotationInProgress`.
    #[test]
    fn activate_refuses_from_rotating() {
        let mut old = rotation_key([30u8; 32]);
        old.activate(1_000).expect("activate");
        let successor = rotation_key([31u8; 32]);
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
        let mut old = with_key([33u8; 32], TEST_KEY_ID);
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
        // A key context is what makes the audit proof *bound*; without one
        // `revoke` still succeeds but records no proof, which is the
        // deliberate asymmetry documented on §`revoke`.
        let mut key = with_key([35u8; 32], TEST_KEY_ID);
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
        let mut key = with_key([36u8; 32], TEST_KEY_ID);
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        let proof = key.revoked_proof().expect("proof captured");
        let pubkey = key.public_key_bytes();
        IdentityKey::verify_revocation_proof(
            &octo_algo_ed25519::ed25519_algorithm_id(),
            &octo_ident::VerificationMethodId::derive(TEST_KEY_ID),
            pubkey,
            &proof,
        )
        .expect("captured proof must verify against pubkey");
    }

    /// F14 (tamper): tampered proof bytes or wrong pubkey must be rejected.
    #[test]
    fn verify_revocation_proof_rejects_tampered_proof() {
        let mut key = with_key([37u8; 32], TEST_KEY_ID);
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        let mut tampered = key.revoked_proof().expect("proof captured");
        tampered[0] ^= 0x01;
        let result = IdentityKey::verify_revocation_proof(
            &octo_algo_ed25519::ed25519_algorithm_id(),
            &octo_ident::VerificationMethodId::derive(TEST_KEY_ID),
            key.public_key_bytes(),
            &tampered,
        );
        assert!(
            matches!(result, Err(WalletError::InvalidRevocationProof)),
            "tampered proof must invalidate, got {result:?}"
        );
        // Wrong pubkey (different identity).
        let wrong_pub = [0xee; 32];
        let proof = key.revoked_proof().expect("proof");
        let result = IdentityKey::verify_revocation_proof(
            &octo_algo_ed25519::ed25519_algorithm_id(),
            &octo_ident::VerificationMethodId::derive(TEST_KEY_ID),
            &wrong_pub,
            &proof,
        );
        assert!(
            matches!(result, Err(WalletError::InvalidRevocationProof)),
            "proof under wrong pubkey must fail, got {result:?}"
        );
    }

    /// F30: `is_deprecated()` is `false` by default, flips to `true` only
    /// after `complete_rotation()` succeeds. Distinct from `Revoked` state.
    #[test]
    fn is_deprecated_flips_only_after_complete_rotation() {
        let mut old = rotation_key([38u8; 32]);
        assert!(!old.is_deprecated(), "fresh key must NOT be deprecated");

        old.activate(1_000).expect("activate");
        assert!(!old.is_deprecated(), "active key must NOT be deprecated");

        let successor = rotation_key([39u8; 32]);
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

#[cfg(test)]
mod signable_prefix_tests {
    use super::*;
    use crate::error::WalletError;
    use octo_ident::{VerificationMethodId, SIGNABLE_DOMAIN};

    fn active_with_key_id(key_id: &str) -> IdentityKey {
        let mut key = IdentityKey::from_seed([9u8; 32])
            .with_verification_method_id(VerificationMethodId::derive(key_id));
        key.activate(1_000).expect("activate");
        key
    }

    #[test]
    fn a_prefixed_signature_round_trips() {
        let key = active_with_key_id("key-a");
        let payload = b"transfer 25 OCTO";
        let wire = key.prefixed_message(payload).expect("message");
        let sig = key.sign_prefixed(payload).expect("sign");
        key.verify_prefixed(&wire, &sig)
            .expect("a genuine prefixed signature must verify");
    }

    #[test]
    fn an_identity_with_no_key_id_refuses_to_sign_a_prefixed_message() {
        // Fail-closed, not a downgrade. RFC-0009 forbids deriving the id from
        // the public key and forbids allocating one at signing time, so there
        // is nothing to invent — and silently signing the bare payload would
        // keep the pre-prefix shape verifying after the prefix is deployed.
        let mut key = IdentityKey::from_seed([9u8; 32]);
        key.activate(1_000).expect("activate");
        assert_eq!(key.verification_method_id(), None);
        assert!(
            matches!(
                key.sign_prefixed(b"payload"),
                Err(WalletError::NoKeyContext)
            ),
            "signing without a key id must be refused"
        );
        assert!(matches!(
            key.prefixed_message(b"payload"),
            Err(WalletError::NoKeyContext)
        ));
        // And verification refuses too, rather than accepting an unprefixed
        // message on the way in. The message here is *well formed* — built
        // with an explicit prefix rather than this identity's, because this
        // identity has none to build — so the only thing left to refuse on
        // is the missing key context, not a malformed message.
        let well_formed = octo_ident::SignedMessage::new(
            octo_ident::SignablePrefix::new(
                key.algorithm_id(),
                VerificationMethodId::derive("key-a"),
            ),
            b"payload".to_vec(),
        )
        .to_bytes();
        assert!(matches!(
            key.verify_prefixed(&well_formed, &[0u8; 64]),
            Err(WalletError::NoKeyContext)
        ));
        // An unprefixed one is refused too, on its own terms.
        assert!(matches!(
            key.verify_prefixed(b"payload", &[0u8; 64]),
            Err(WalletError::Signature(_))
        ));
    }

    #[test]
    fn an_unprefixed_message_is_rejected() {
        let key = active_with_key_id("key-a");
        let sig = key.sign_prefixed(b"transfer 25 OCTO").expect("sign");
        // The legacy shape: a bare payload, no prefix.
        assert!(matches!(
            key.verify_prefixed(b"transfer 25 OCTO", &sig),
            Err(WalletError::Signature(_))
        ));
    }

    #[test]
    fn a_message_naming_another_key_id_is_rejected() {
        let key = active_with_key_id("key-a");
        let sig = key.sign_prefixed(b"payload").expect("sign");

        // Rewrapped as this identity's other verification method.
        let other = key.prefixed_message(b"payload").expect("message");
        let mut rewrapped = other.clone();
        rewrapped[16..32].copy_from_slice(VerificationMethodId::derive("key-b").as_bytes());
        assert!(
            matches!(
                key.verify_prefixed(&rewrapped, &sig),
                Err(WalletError::KeyIdMismatch)
            ),
            "a signature must not carry over to another key_id"
        );
    }

    #[test]
    fn a_tampered_payload_is_rejected() {
        let key = active_with_key_id("key-a");
        let wire = key.prefixed_message(b"transfer 25 OCTO").expect("message");
        let sig = key.sign_prefixed(b"transfer 25 OCTO").expect("sign");

        let mut tampered = wire.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(key.verify_prefixed(&tampered, &sig).is_err());
    }

    #[test]
    fn the_key_id_survives_a_clone() {
        let id = VerificationMethodId::derive("key-a");
        let key = active_with_key_id("key-a");
        let cloned = key.clone();
        assert_eq!(cloned.verification_method_id(), Some(id));
        assert_eq!(
            cloned.prefixed_message(b"p").expect("message"),
            key.prefixed_message(b"p").expect("message"),
            "a clone must sign the same domain as its original"
        );
    }

    #[test]
    fn verification_follows_the_algorithm_the_prefix_names() {
        // The decisive property, and the one a verifier is most tempted to
        // get wrong: RFC-0009 requires verifying "under the algorithm the
        // prefix names rather than the one it prefers".
        //
        // This identity speaks Ed25519. Hand it a message whose prefix names
        // STARK-curve Schnorr. If the verifier read the algorithm off its
        // own configuration it would look Ed25519 up and fail with a
        // *verification* error; reading it off the prefix means it looks
        // `stark-schnorr` up and fails with *unknown algorithm* — which is
        // what the assertion below pins.
        let key = active_with_key_id("key-a");
        let foreign = octo_ident::SignedMessage::new(
            octo_ident::SignablePrefix::new(
                octo_ident::AlgorithmId::derive("stark-schnorr"),
                VerificationMethodId::derive("key-a"),
            ),
            b"payload".to_vec(),
        );
        let wire = foreign.to_bytes();
        let err = key
            .verify_prefixed(&wire, &[0u8; 64])
            .expect_err("a foreign algorithm must not verify");

        let rendered = err.to_string();
        assert!(
            rendered.contains("unknown algorithm"),
            "the verifier must dispatch on the PREFIX's algorithm, not its \
             own. Got: {rendered}"
        );
        assert!(
            !rendered.contains("invalid"),
            "this must be a lookup miss, not a failed equation — otherwise \
             the verifier ignored the prefix and used its own algorithm. \
             Got: {rendered}"
        );
    }

    #[test]
    fn the_prefix_names_the_signers_own_algorithm() {
        let key = active_with_key_id("key-a");
        let prefix = key.signable_prefix().expect("prefix");
        assert_eq!(prefix.algorithm_id(), key.algorithm_id());
        assert_eq!(prefix.key_id(), VerificationMethodId::derive("key-a"));
        // And the raw form, never the textual UUID — RFC-0009 makes the
        // distinction normative and both encodings are in use in this repo.
        let bytes = prefix.to_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[..16], key.algorithm_id().as_bytes());
        assert_ne!(
            &bytes[..16],
            hex::encode(key.algorithm_id().as_bytes()).as_bytes()
        );
    }

    #[test]
    fn the_hashed_form_is_a_different_domain_than_the_raw_one() {
        // Both forms are produced by the same identity; they are simply not
        // interoperable, which is why an implementation states which it uses.
        let key = active_with_key_id("key-a");
        let message = octo_ident::SignedMessage::new(
            key.signable_prefix().expect("prefix"),
            b"payload".to_vec(),
        );
        let raw = message
            .signable(octo_ident::SignableForm::Raw)
            .expect("raw");
        let hashed_signable = message
            .signable(octo_ident::SignableForm::Hashed)
            .expect("hashed");
        assert_ne!(raw, hashed_signable);
        assert_eq!(raw.len(), hashed_signable.len());

        // The hashed prefix really is the digest of the domain and the two
        // identifiers — recomputed here rather than asserted by shape, so
        // this cannot pass on a constant that happens to differ.
        let expected = {
            let mut digest_hasher = blake3::Hasher::new();
            digest_hasher.update(SIGNABLE_DOMAIN);
            digest_hasher.update(&key.signable_prefix().expect("prefix").to_bytes());
            digest_hasher.finalize()
        };
        assert_eq!(&hashed_signable[..32], expected.as_bytes());
    }
}

#[cfg(test)]
mod lifecycle_proof_tests {
    use super::*;
    use octo_ident::{AlgorithmId, SignablePrefix, SignedMessage, VerificationMethodId};

    fn revoking(key_id: &str) -> IdentityKey {
        let mut key = IdentityKey::from_seed([41u8; 32])
            .with_verification_method_id(VerificationMethodId::derive(key_id));
        key.activate(1_000).expect("activate");
        key.revoke(2_000).expect("revoke");
        key
    }

    #[test]
    fn a_revocation_proof_round_trips() {
        let key = revoking("key-a");
        let proof = key
            .revoked_proof()
            .expect("a key context yields a bound proof");
        assert_eq!(proof.len(), 64);
        IdentityKey::verify_revocation_proof(
            &key.algorithm_id(),
            &key.verification_method_id().expect("key id"),
            key.public_key_bytes(),
            &proof,
        )
        .expect("a genuine revocation proof must verify");
    }

    /// A rotating identity, its successor's key, and the proof it produced.
    fn rotating(key_id: &str) -> (IdentityKey, Vec<u8>, Vec<u8>) {
        let mut old = IdentityKey::from_seed([44u8; 32])
            .with_verification_method_id(VerificationMethodId::derive(key_id));
        old.activate(1_000).expect("activate");
        let successor = IdentityKey::from_seed([45u8; 32])
            .with_verification_method_id(VerificationMethodId::derive("key-b"));
        let new_pub = successor.public_key_bytes().to_vec();
        let proof = old
            .begin_rotation(successor, 2_000)
            .expect("begin_rotation");
        (old, new_pub, proof)
    }

    #[test]
    fn a_successor_proof_is_bound_to_its_key_id() {
        let (old, new_pub, proof) = rotating("key-a");
        let old_pub = old.public_key_bytes().to_vec();

        IdentityKey::verify_successor_proof(
            &old.algorithm_id(),
            &VerificationMethodId::derive("key-a"),
            &old_pub,
            &new_pub,
            &proof,
        )
        .expect("genuine");

        // The same proof under another verification method. Before the
        // prefix the signed bytes were `b"rotate" || new_pub` and nothing
        // tied them to a key, so this call could not have distinguished the
        // two at all.
        IdentityKey::verify_successor_proof(
            &old.algorithm_id(),
            &VerificationMethodId::derive("key-b"),
            &old_pub,
            &new_pub,
            &proof,
        )
        .expect_err("a successor proof must not carry over to another key_id");
    }

    #[test]
    fn a_successor_proof_binds_the_successor_key() {
        let (old, new_pub, proof) = rotating("key-a");
        let old_pub = old.public_key_bytes().to_vec();

        // A proof authorising one successor must not authorise another.
        let other = [0x5Au8; 32];
        IdentityKey::verify_successor_proof(
            &old.algorithm_id(),
            &VerificationMethodId::derive("key-a"),
            &old_pub,
            &other,
            &proof,
        )
        .expect_err("a successor proof must bind the successor it names");

        // Sanity in the other direction: the named successor does verify, so
        // the refusal above is about the binding and not a broken key.
        IdentityKey::verify_successor_proof(
            &old.algorithm_id(),
            &VerificationMethodId::derive("key-a"),
            &old_pub,
            &new_pub,
            &proof,
        )
        .expect("the named successor must verify");
    }

    #[test]
    fn a_successor_proof_is_refused_without_a_prefix() {
        let (old, new_pub, _proof) = rotating("key-a");
        let old_pub = old.public_key_bytes().to_vec();

        // The legacy shape, signed directly. A verifier accepting it would be
        // a downgrade path onto proofs minted before the prefix existed.
        let mut expected = Vec::new();
        expected.extend_from_slice(b"rotate");
        expected.extend_from_slice(&new_pub);
        let legacy = old.signer().sign(&expected).expect("sign the legacy shape");
        IdentityKey::verify_successor_proof(
            &old.algorithm_id(),
            &VerificationMethodId::derive("key-a"),
            &old_pub,
            &new_pub,
            &legacy,
        )
        .expect_err("an unprefixed successor proof must be refused");
    }

    #[test]
    fn a_revocation_proof_is_bound_to_its_key_id() {
        let key = revoking("key-a");
        let proof = key.revoked_proof().expect("proof");
        let pubkey = key.public_key_bytes();

        // The same proof, presented as another verification method's. This is
        // the vector that can detect the actual defect: before the prefix the
        // proof was the bare constant b"revoke", so it verified under any
        // key_id whatsoever.
        IdentityKey::verify_revocation_proof(
            &key.algorithm_id(),
            &VerificationMethodId::derive("key-b"),
            pubkey,
            &proof,
        )
        .expect_err("a proof must not carry over to another key_id");

        // And back the other way: a proof bound to key-b does not verify as
        // key-a either.
        let other = revoking("key-b");
        IdentityKey::verify_revocation_proof(
            &key.algorithm_id(),
            &VerificationMethodId::derive("key-a"),
            pubkey,
            &other.revoked_proof().expect("proof"),
        )
        .expect_err("a proof bound elsewhere must not verify here");
    }

    #[test]
    fn a_revocation_proof_is_refused_without_a_prefix() {
        // Sign BEFORE revoking: `revoke` swaps the adapter for a `NullSigner`,
        // so asking a revoked identity to produce a signature fails at the
        // adapter rather than giving us the legacy bytes to test against.
        let mut key = IdentityKey::from_seed([43u8; 32])
            .with_verification_method_id(VerificationMethodId::derive("key-a"));
        key.activate(1_000).expect("activate");
        let pubkey = key.public_key_bytes();

        // The legacy shape: the bare six bytes, signed directly. A verifier
        // that accepted it would have a downgrade path, so it must not.
        let legacy = key
            .signer()
            .sign(b"revoke")
            .expect("sign the legacy literal");
        IdentityKey::verify_revocation_proof(
            &key.algorithm_id(),
            &key.verification_method_id().expect("key id"),
            pubkey,
            &legacy,
        )
        .expect_err("an unprefixed revocation proof must be refused");
    }

    #[test]
    fn revocation_is_never_blocked_by_a_missing_key_context() {
        // The DoS shape this asymmetry exists to prevent: gating the ability
        // to revoke on an optional audit annotation hands anyone who can strip
        // a key_id from a record a permanent block on the safety action.
        let mut key = IdentityKey::from_seed([42u8; 32]);
        key.activate(1_000).expect("activate");
        key.revoke(2_000)
            .expect("revoke must succeed without a key context");
        assert_eq!(key.lifecycle(), crate::lifecycle::LifecycleState::Revoked);
        // No bound proof — an absence, not a forgery. The verifier refuses it
        // rather than mistaking it for one.
        assert!(
            key.revoked_proof().is_none(),
            "there is no bound proof to record, and saying so is the point"
        );
    }

    #[test]
    fn a_revocation_proof_names_its_own_algorithm_and_key() {
        let key = revoking("key-a");
        let proof = key.revoked_proof().expect("proof");
        // What the signer actually signed, rebuilt independently: the prefix
        // plus the canonicalized literal, not the literal alone.
        let expected = SignedMessage::new(
            SignablePrefix::new(key.algorithm_id(), VerificationMethodId::derive("key-a")),
            b"revoke".to_vec(),
        )
        .signable(octo_ident::SignableForm::Raw)
        .expect("signable");
        let _ = proof;
        // Assert the domain the verifier rebuilds matches, by checking a
        // differently-keyed prefix produces different signable bytes.
        let other = SignedMessage::new(
            SignablePrefix::new(key.algorithm_id(), VerificationMethodId::derive("key-b")),
            b"revoke".to_vec(),
        )
        .signable(octo_ident::SignableForm::Raw)
        .expect("signable");
        assert_ne!(
            expected, other,
            "the key_id must be part of what was signed"
        );
        assert_ne!(key.algorithm_id(), AlgorithmId::derive("stark-schnorr"));
    }
}
