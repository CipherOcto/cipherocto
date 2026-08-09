# Mission: 0009-l2 — Identity Rotation + Successor Linkage (RFC-0009 §Lifecycle + RFC-0853 §12)

## Status

Open (2026-08-09). Sub-mission of `missions/claimed/0009-a-hsm-routing.md` (top-level substrate wiring per RFC-0009 v1.1). Depends on `missions/open/0009-l1-lifecycle-state-machine.md` (lifecycle state machine + `Rotating` variant declaration). Filed per [[deferred-vs-unspecified]] named-owner rule for the F4 unblock path documented in `missions/claimed/0957-f-future-work.md` §Notes.

## RFC

RFC-0009 (Process): Identity Management — Accepted 2026-07-20 (v1.1 amendment 2026-08-08; §Lifecycle Active↔Rotating transitions + successor co-sign helper specified in §Lifecycle table rows 2-3)
RFC-0853 (Networking): Overlay Cryptography — Accepted v1.0.0 (§12 Key Rotation and Revocation: 24h grace period + revocation tuple `(compromised_key_id, revocation_epoch, successor_key_id, signature_by_successor)`)
RFC-0852 (Networking): Deterministic Gossip Protocol — referenced for revocation event fan-out

**Sub-mission of:** `missions/claimed/0009-a-hsm-routing.md` (top-level wiring mission, Claimed+Closed 2026-08-09)

## Summary

Wire the **Active ↔ Rotating** state transitions + successor linkage substrate into `IdentityKey` per RFC-0009 §Lifecycle Requirements + RFC-0853 §12. Scope of THIS sub-mission:

1. `Active → Rotating` transition (`IdentityKey::begin_rotation(successor: IdentityKey, clock)`)
2. Successor co-sign helper: `Ed25519(old_seed, "rotate" || new_pubkey_bytes)` per RFC-0009 §Lifecycle table row 2
3. `successor_key: Option<IdentityKey>` field on `IdentityKey`
4. `Rotating → Active` transition after grace period elapses (`IdentityKey::complete_rotation(clock)` — verifies successor proof + enforces 24h grace)
5. `Rotating → Revoked` abort path (`IdentityKey::abort_rotation()` — destroys successor + returns old key to Active)
6. Grace period enforcement (24h per RFC-0853 §12; configurable per RFC-0009 §Time Bounds)
7. Revocation event fan-out via RFC-0852 gossip (substrate via `octo_transport::NodeTransport::broadcast`)
8. Manual redacting `Debug` on `IdentityRotated` event + successor co-sign helper

**OUT OF SCOPE for this sub-mission:**

- `LifecycleState` enum + `Designated → Active` + `Active → Revoked` direct path — already in `0009-l1-lifecycle-state-machine.md`
- RFC-0957-A1 G3 gossip for `HolderRecord` sync — separate mission (`0957-c-gossip`)
- RFC-0871 envelope adoption — separate mission (`0870-b`)

## Acceptance Criteria

### Active → Rotating transition

- [ ] `IdentityKey::begin_rotation(&mut self, successor: IdentityKey, clock: &dyn Clock) -> Result<IdentityRotated, IdentityError>` — initiates rotation (RFC-0009 §Lifecycle row 2)
- [ ] Refuses transition if current state ≠ `Active`: returns `IdentityError::NotActive { current_state }`
- [ ] Refuses self-rotation: returns `IdentityError::SelfRotation` if `successor.did() == self.did()`
- [ ] Determinism: same `(self.did(), successor.public_key_bytes(), clock.now_unix())` → identical event bytes (successor proof signature is deterministic Ed25519)
- [ ] Side effects:
  - Records `successor_key: Option<IdentityKey> = Some(successor)` field
  - Records `rotation_started_at: Option<NonZeroU64> = Some(clock.now_unix())`
  - Transitions `lifecycle: LifecycleState::Rotating`
  - Emits `IdentityRotated { old_did, new_did, rotation_started_at_unix, successor_proof }` event
- [ ] Successor co-sign helper: `successor_proof = Ed25519(self.inner, b"rotate" || successor.public_key_bytes())` per RFC-0009 §Lifecycle table row 2 signing requirement
- [ ] Manual redacting `Debug` on `IdentityRotated`: redacts `old_did` + `new_did` + `successor_proof`; preserves `rotation_started_at_unix` (operational metadata)
- [ ] Tests: `begin_rotation_from_active_transitions_to_rotating`, `begin_rotation_refuses_from_designated`, `begin_rotation_refuses_from_revoked`, `begin_rotation_refuses_self_rotation`, `begin_rotation_records_successor_field`, `begin_rotation_emits_signed_event`, `identity_rotated_debug_redacts_dids_and_signature`

### Rotating → Active transition (complete_rotation)

- [ ] `IdentityKey::complete_rotation(&mut self, clock: &dyn Clock) -> Result<(), IdentityError>` — completes rotation after grace period (RFC-0009 §Lifecycle row 3)
- [ ] Grace period: 24h per RFC-0853 §12; configurable via `RotationGracePeriod` constant (`pub const ROTATION_GRACE_PERIOD_SECS: u64 = 24 * 60 * 60;`)
- [ ] Refuses transition if current state ≠ `Rotating`: returns `IdentityError::NotRotating { current_state }`
- [ ] Refuses transition if grace period not elapsed: returns `IdentityError::GracePeriodNotElapsed { elapsed_secs, required_secs }`
- [ ] Verifies successor proof: re-derives `Ed25519(old_public_key, b"rotate" || successor.public_key_bytes())` and compares to stored `successor_proof` (defense against forged successor linkage)
- [ ] Side effects:
  - Marks old identity `deprecated: bool = true` (signs still verify per RFC-0009 §Lifecycle row 3)
  - Clears `successor_key: Option<IdentityKey> = None` (successor takes over; old key becomes legacy)
  - Transitions `lifecycle: LifecycleState::Active` (re-activates old key as deprecated; successor is independently Active via its own `activate()` call)
- [ ] Tests: `complete_rotation_after_grace_period_succeeds`, `complete_rotation_refuses_before_grace_period`, `complete_rotation_refuses_from_active`, `complete_rotation_verifies_successor_proof`, `complete_rotation_marks_old_as_deprecated`, `complete_rotation_clears_successor_field`

### Rotating → Revoked abort path

- [ ] `IdentityKey::abort_rotation(&mut self) -> Result<(), IdentityError>` — destroys successor + returns old key to Active (RFC-0009 §Lifecycle implied — user choice)
- [ ] Refuses transition if current state ≠ `Rotating`: returns `IdentityError::NotRotating { current_state }`
- [ ] Side effects:
  - Clears `successor_key: Option<IdentityKey> = None` (successor destroyed; user must re-generate if rotation desired)
  - Transitions `lifecycle: LifecycleState::Active`
  - Clears `rotation_started_at: Option<NonZeroU64> = None`
  - Emits `IdentityRotationAborted { did, aborted_at_unix }` event
- [ ] Manual redacting `Debug` on `IdentityRotationAborted`: redacts DID; preserves `aborted_at_unix`
- [ ] Tests: `abort_rotation_from_rotating_returns_to_active`, `abort_rotation_clears_successor`, `abort_rotation_refuses_from_active`, `abort_rotation_refuses_from_revoked`, `identity_rotation_aborted_debug_redacts_did`

### Successor proof verification helper

- [ ] NEW: `pub fn verify_successor_proof(old_pub: &[u8; 32], new_pub: &[u8; 32], proof: &[u8; 64]) -> Result<(), IdentityError>` — pure helper, exposed for `complete_rotation()` + external verifiers (RFC-0853 §12 peer verification)
- [ ] Re-derives expected proof: `Ed25519::sign(old_priv, b"rotate" || new_pub)` — but with public-key verification: `ed25519_dalek::Verifier::verify(old_pub, b"rotate" || new_pub, proof)`
- [ ] Returns `Ok(())` if valid; `Err(IdentityError::InvalidSuccessorProof)` if signature mismatch
- [ ] Tests: `verify_successor_proof_accepts_valid_proof`, `verify_successor_proof_rejects_tampered_new_pub`, `verify_successor_proof_rejects_tampered_old_pub`, `verify_successor_proof_rejects_random_signature`

### IdentityKey struct extension (additive to 0009-l1)

- [ ] `crates/octo-wallet/src/identity.rs` (MODIFY — additive over `0009-l1`):
  - `successor_key: Option<IdentityKey>` (default `None`)
  - `rotation_started_at: Option<NonZeroU64>` (default `None`)
  - `deprecated: bool` (default `false`; flipped to `true` on `complete_rotation()`)
- [ ] `IdentityKey::sign()` updated to permit `Rotating` state during grace period (RFC-0009 §Lifecycle row 3: "old key still valid during grace"). New predicate: `can_sign() = matches!(lifecycle, Active | Rotating)`. Excludes `Designated` + `Revoked`.
- [ ] Tests: `identity_key_sign_accepts_when_rotating_within_grace`, `identity_key_sign_rejects_when_rotating_after_grace_not_completed`, `identity_key_sign_rejects_when_deprecated`

### Revocation event gossip fan-out

- [ ] `IdentityRevoked` event published via `octo_transport::NodeTransport::broadcast(&self.transport, gossip_channel_id, event_bytes)` per RFC-0853 §12 (gossip is the revocation distribution channel; no CRL)
- [ ] `IdentityActivated` event published via same gossip channel (RFC-0853 §12 implies activation fan-out for peer awareness of new identity state)
- [ ] Gossip channel id: `GOSSIP_CHANNEL_IDENTITY_LIFECYCLE: &[u8] = b"octo/identity-lifecycle/v1";`
- [ ] Event bytes: `IdentityRevoked` serialized via `canonical_ser` (RFC-0126) — deterministic for cross-node replay
- [ ] Gossip send failures do NOT block local state mutation: if `broadcast()` returns `Err`, log warning + proceed (revocation is locally authoritative; gossip is best-effort distribution per RFC-0853 §12)
- [ ] Tests:
  - Unit: `revoke_publishes_to_gossip_channel` (mock `NodeTransport` captures broadcast call)
  - Integration: `revocation_event_round_trips_via_gossip_substrate` (cross-node via RFC-0852 gossip harness)

### IdentityError enum extension (additive to 0009-l1)

- [ ] NEW variants on existing `IdentityError` enum:
  - `NotRotating { current_state: LifecycleState }` — `complete_rotation()` / `abort_rotation()` called when lifecycle ≠ Rotating
  - `SelfRotation` — `begin_rotation()` with successor.did() == self.did()
  - `GracePeriodNotElapsed { elapsed_secs: u64, required_secs: u64 }` — `complete_rotation()` before 24h
  - `InvalidSuccessorProof` — signature verification failed
- [ ] Manual redacting `Debug` on new variants: redacts no credential material (only operational metadata like `elapsed_secs`/`required_secs`)
- [ ] Tests: `identity_error_debug_redacts_not_rotating`, `identity_error_debug_preserves_grace_period_metadata`

### Cross-crate compat

- [ ] `cargo test -p octo-wallet --lib identity::rotation` zero regressions
- [ ] `cargo test -p octo-wallet --lib capability` zero regressions (capability attenuation path must continue working in `Rotating` state)
- [ ] `cargo test -p octo-wallet --lib` zero regressions
- [ ] `cargo test -p octo-cap-macaroon --lib` zero regressions (Phase 2+2b+2c closed 2026-08-09)
- [ ] `cargo test -p octo-network --lib` zero regressions (gossip substrate compatible with new event bytes)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires:**

- `missions/open/0009-l1-lifecycle-state-machine.md` — `LifecycleState::Rotating` variant + `can_transition_to` edges (must land first)
- `missions/claimed/0009-a-hsm-routing.md` — `IdentityKey::sign` routed through `Arc<dyn HsmAdapter>` per RFC-0009 v1.1 (required for successor co-sign helper + revocation signature)
- `missions/claimed/0957-c-holder-registry-impl.md` — `HolderRecord` schema (add `rotation_started_at_millis_unix` + `deprecated: bool` columns in same schema migration as `0009-l1`)
- RFC-0009 §Lifecycle Requirements (Accepted 2026-07-20; §Lifecycle table rows 2-3)
- RFC-0853 §12 (Accepted v1.0.0; 24h grace + revocation tuple)
- RFC-0852 gossip substrate (Accepted; available via `octo_transport::NodeTransport::broadcast` per commit `4ed4ff1f` precedent)

**Mission gates:**

- This mission gates `0957-f-f4-bundle.md` for the `successor_proof: Option<Ed25519Signature>` bundle field (required for offline replay across rotation boundaries)

**Not Requires:**

- RFC-0871 acceptance (independent of NodeEnvelope work)
- RFC-0957-A1 G3 gossip for `HolderRecord` sync (separate mission `0957-c-gossip`)

```yaml
depends_on:
  - 0009-l1-lifecycle-state-machine # LifecycleState::Rotating + can_transition_to edges
  - 0009-a-hsm-routing # IdentityKey::sign routed through Arc<dyn HsmAdapter>
  - 0957-c-holder-registry-impl # HolderRecord schema (rotation_started_at + deprecated columns)
  - RFC-0009 # §Lifecycle Requirements rows 2-3
  - RFC-0853 # §12 rotation grace period + revocation tuple
  - RFC-0852 # gossip substrate for revocation fan-out
```

Real missions + RFC substrate only. No phantom pointers.

## Implementation Guide

### begin_rotation

```rust
// crates/octo-wallet/src/identity.rs (MODIFY — additive over 0009-l1)

impl IdentityKey {
    /// RFC-0009 §Lifecycle row 2: Active → Rotating.
    pub fn begin_rotation(
        &mut self,
        successor: IdentityKey,
        clock: &dyn Clock,
    ) -> Result<IdentityRotated, IdentityError> {
        if self.lifecycle != LifecycleState::Active {
            return Err(IdentityError::NotActive { current_state: self.lifecycle });
        }
        if successor.did() == self.did() {
            return Err(IdentityError::SelfRotation);
        }
        let now = clock.now_unix();
        let successor_proof = self.inner.sign(b"rotate")?; // partial: real impl uses HSM
        // Successor proof canonical bytes (deterministic across impls):
        let proof_message = {
            let mut msg = Vec::with_capacity(6 + 32);
            msg.extend_from_slice(b"rotate");
            msg.extend_from_slice(&successor.public_key_bytes());
            self.inner.sign(&msg)?
        };
        let event = IdentityRotated {
            old_did: self.did(),
            new_did: successor.did(),
            rotation_started_at_unix: now,
            successor_proof: proof_message,
        };
        self.successor_key = Some(successor);
        self.rotation_started_at = NonZeroU64::new(now);
        self.lifecycle = LifecycleState::Rotating;
        emit_event(event.clone());
        Ok(event)
    }
}
```

### complete_rotation

```rust
impl IdentityKey {
    /// RFC-0009 §Lifecycle row 3: Rotating → Active (after grace).
    pub fn complete_rotation(&mut self, clock: &dyn Clock) -> Result<(), IdentityError> {
        if self.lifecycle != LifecycleState::Rotating {
            return Err(IdentityError::NotRotating { current_state: self.lifecycle });
        }
        let started_at = self.rotation_started_at.expect("invariant: Rotating state has started_at");
        let elapsed = clock.now_unix().saturating_sub(started_at.get());
        if elapsed < ROTATION_GRACE_PERIOD_SECS {
            return Err(IdentityError::GracePeriodNotElapsed {
                elapsed_secs: elapsed,
                required_secs: ROTATION_GRACE_PERIOD_SECS,
            });
        }
        let successor = self.successor_key.as_ref().expect("invariant: Rotating state has successor");
        verify_successor_proof(&self.public_key, &successor.public_key_bytes(), &self.stored_proof())?;
        self.deprecated = true;
        self.successor_key = None;
        self.lifecycle = LifecycleState::Active; // re-activated as deprecated
        Ok(())
    }
}

pub const ROTATION_GRACE_PERIOD_SECS: u64 = 24 * 60 * 60;
```

### verify_successor_proof

```rust
pub fn verify_successor_proof(
    old_pub: &[u8; 32],
    new_pub: &[u8; 32],
    proof: &[u8; 64],
) -> Result<(), IdentityError> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let vk = VerifyingKey::from_bytes(old_pub)
        .map_err(|_| IdentityError::InvalidSuccessorProof)?;
    let sig = Signature::from_bytes(proof);
    let mut msg = Vec::with_capacity(6 + 32);
    msg.extend_from_slice(b"rotate");
    msg.extend_from_slice(new_pub);
    vk.verify(&msg, &sig).map_err(|_| IdentityError::InvalidSuccessorProof)
}
```

### Test fixture pattern

- `MockClock` with configurable `now_unix()` advances for grace period testing
- `InMemorySigner` (per RFC-0009 v1.1) — captures `sign(b"rotate" || new_pub)` calls for successor proof byte assertion
- `MockNodeTransport` — captures `broadcast(channel_id, event_bytes)` calls for revocation fan-out assertions (pattern from `4ed4ff1f` RFC-0862 gossip binding precedent)

## Decomposition Rationale

Sub-mission `l2` covers rotation + successor linkage + grace + revocation gossip. Decomposed from monolithic lifecycle mission (would have been ~800+ lines) into `l1` (state machine + activate + revoke) + `l2` (rotation + successor + gossip). Each sub-mission stays below BLUEPRINT §Multi-Mission Decomposition pushable-unit threshold (~500 lines per mission). Estimated LoC for this sub-mission: ~500-600 lines (identity.rs extensions 250 lines + rotation helpers 100 lines + tests 200 lines).

## Claimant

@unassigned (per [[feedback_initiation_user_only]] — user initiates the claim)

## Pull Request

(unset)

## Notes

- Mission captured in `missions/claimed/0957-f-future-work.md` §Notes line 117 ("F4 is the only item not yet fully spec'd (TV F4 placeholder) because it depends on RFC-0009 §Identity evolution")
- Mission unblocks `0957-f-f4-bundle.md` for the `successor_proof: Option<Ed25519Signature>` bundle field (replay across rotation boundaries)
- Per [[no-phantom-mission-pointers]]: mission file now exists; `0957-f-f4-bundle.md` §Status line + §Depends on YAML get updated to cite this mission (avoid phantom pointer)
- Per [[cargo-fmt-workflow]] + [[feedback_clippy_zero_warnings]]: `cargo fmt` + `cargo clippy -D warnings` green before commit
- Per [[no-line-refs-anywhere]]: all references use §section-name / symbol form
- Per [[rfc-referencing-convention]]: RFCs referenced by number only

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures Active↔Rotating transitions + successor co-sign helper + 24h grace + revocation gossip fan-out per RFC-0009 §Lifecycle + RFC-0853 §12. Depends on `0009-l1` for `LifecycleState::Rotating` variant. Unblocks `0957-f-f4-bundle` for `successor_proof` field. |

Last Updated: 2026-08-09
Version: 0.1