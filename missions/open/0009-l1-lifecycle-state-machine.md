# Mission: 0009-l1 — Identity Lifecycle State Machine (RFC-0009 §Lifecycle)

## Status

Open (2026-08-09). Sub-mission of `missions/claimed/0009-a-hsm-routing.md` (top-level substrate wiring per RFC-0009 v1.1). Filed per [[deferred-vs-unspecified]] named-owner rule for the F4 unblock path documented in `missions/claimed/0957-f-future-work.md` §Notes.

## RFC

RFC-0009 (Process): Identity Management — Accepted 2026-07-20 (v1.1 amendment 2026-08-08 added §HsmAdapter Integration + §Canonical DID Validation; §Lifecycle State Machine substrate still RFC-only).

**Sub-mission of:** `missions/claimed/0009-a-hsm-routing.md` (top-level wiring mission, Claimed+Closed 2026-08-09)

## Summary

Wire the **Identity Lifecycle State Machine** defined in RFC-0009 §Lifecycle Requirements into `IdentityKey` (production substrate in `crates/octo-wallet/src/identity.rs`). Scope of THIS sub-mission:

1. `LifecycleState` enum (Designated / Active / Rotating / Revoked) — `repr(u8)` per RFC-0009 Appendix A
2. `IdentityKey` struct extension: `lifecycle: LifecycleState`, `activated_at: Option<u64>`, `revoked_at: Option<u64>` fields
3. `IdentityKey::activate()` — Designated → Active transition (first successful sign or explicit `activate()` API call per §Lifecycle table)
4. `IdentityKey::revoke()` — Active → Revoked direct path; zeroize private key bytes; emit `IdentityRevoked { did, reason }` event per §Lifecycle table
5. Activation event emission via `octo-wallet` observability layer (no RFC-0852 gossip in this sub-mission — gossip ships in `0009-l2` per RFC-0853 §12)
6. Manual redacting `Debug` on `LifecycleState` + `IdentityRevoked` event (preserve operational metadata: `activated_at_unix`, `revoked_at_unix`; redact DIDs)

**OUT OF SCOPE for this sub-mission** (lands in `0009-l2-rotation-successor-linkage.md`):
- `Active ↔ Rotating` transitions
- Successor co-sign helper
- Successor linkage persistence
- Grace period enforcement (24h per RFC-0853 §12)
- RFC-0852 gossip fan-out for activation/revocation events

## Acceptance Criteria

### LifecycleState enum

- [ ] NEW: `crates/octo-wallet/src/lifecycle.rs` — `LifecycleState` enum with `#[repr(u8)]` discriminant values matching RFC-0009 Appendix A:
  - `Designated = 0x00` (named at init, not yet active)
  - `Active = 0x01` (identity in use; signing operations live)
  - `Rotating = 0x02` (successor link established; old key still valid during grace) — declared but no transitions yet (lands in `0009-l2`)
  - `Revoked = 0x03` (identity retired; signature verification rejected)
- [ ] Manual redacting `Debug` impl on `LifecycleState`: `Designated` + `Active` + `Rotating` + `Revoked` unit variants display as `"Designated"` / `"Active"` / `"Rotating"` / `"Revoked"` (no credential material)
- [ ] `is_active()` / `is_revoked()` / `is_rotating()` predicate methods on `LifecycleState`
- [ ] `can_sign()` predicate: returns `true` only for `Active` state (Designated/Rotating/Revoked reject signing)
- [ ] `can_transition_to(target)` predicate enforcing valid state machine edges per RFC-0009 §Identity Lifecycle State Machine mermaid diagram
- [ ] Tests: `lifecycle_repr_u8_matches_appendix_a`, `lifecycle_debug_redacts_unit_variants`, `lifecycle_can_sign_only_for_active`, `lifecycle_can_transition_to_validates_edges`

### IdentityKey struct extension

- [ ] `crates/octo-wallet/src/identity.rs` — `IdentityKey` struct extended with:
  - `lifecycle: LifecycleState` (default `Designated` at `IdentityKey::generate()`)
  - `activated_at: Option<NonZeroU64>` (`None` until first transition; populated by `activate()`)
  - `revoked_at: Option<NonZeroU64>` (`None` unless revoked; populated by `revoke()`)
- [ ] All existing call sites updated to construct `IdentityKey { ..., lifecycle: LifecycleState::Designated, activated_at: None, revoked_at: None }`
- [ ] `IdentityKey::sign()` checks `can_sign()` BEFORE invoking signer; returns `IdentityError::NotActive` if state ≠ `Active` (defense-in-depth — main path goes through HsmAdapter per RFC-0009 v1.1)
- [ ] Tests: `identity_key_default_lifecycle_is_designated`, `identity_key_sign_rejects_when_designated`, `identity_key_sign_rejects_when_revoked`, `identity_key_sign_accepts_when_active`

### Designated → Active transition

- [ ] `IdentityKey::activate(&mut self, clock: &dyn Clock) -> Result<(), IdentityError>` — explicit activation API (RFC-0009 §Lifecycle table row 1)
- [ ] Determinism: same `clock.now_unix()` + current `Designated` state → identical outcome
- [ ] Idempotent: second `activate()` call from `Active` state returns `Ok(())` no-op (avoids duplicate event emission)
- [ ] Refuses transition from `Revoked` state: returns `IdentityError::AlreadyRevoked` (terminal state — no recovery)
- [ ] Side effects: records `activated_at = Some(clock.now_unix())`; emits `IdentityActivated { did, activated_at_unix }` event via observability layer
- [ ] Manual redacting `Debug` on `IdentityActivated` event: redacts DID (`<redacted>`), preserves `activated_at_unix` (operational metadata)
- [ ] Tests: `activate_from_designated_records_timestamp`, `activate_is_idempotent_from_active`, `activate_refuses_from_revoked`, `identity_activated_debug_redacts_did`, `activate_event_emitted_with_clock_timestamp`

### Active → Revoked transition

- [ ] `IdentityKey::revoke(&mut self, reason: RevocationReason, clock: &dyn Clock) -> Result<IdentityRevoked, IdentityError>` — direct revocation path (RFC-0009 §Lifecycle table row 4)
- [ ] `RevocationReason` enum: `HolderInitiated`, `Compromised`, `GovernanceOrdered`, `KeyLost` — 4 variants per RFC-0009 §Lifecycle + RFC-0853 §12
- [ ] Determinism: same `(reason, clock.now_unix())` → identical outcome (event bytes deterministic)
- [ ] Idempotent: second `revoke()` call from `Revoked` state returns the cached `IdentityRevoked` event (no duplicate emission, no signature re-issuance)
- [ ] Side effects: records `revoked_at = Some(clock.now_unix())`; transitions `lifecycle = Revoked`; **zeroizes private key bytes** via `Zeroize::zeroize` (per RFC-0009 §Security §Key Handling Rule 3); emits `IdentityRevoked { did, reason, revoked_at_unix, signature }` event
- [ ] Holder signature on event: `Ed25519(seed, "revoke")` per RFC-0009 §Lifecycle table row 4 (proof that holder authorized revocation)
- [ ] Manual redacting `Debug` on `IdentityRevoked`: redacts DID + signature, preserves `revoked_at_unix` + `reason` (operational metadata)
- [ ] Tests: `revoke_from_active_zeroizes_seed`, `revoke_is_idempotent_from_revoked`, `revoke_records_timestamp`, `revoke_event_signed_by_holder`, `identity_revoked_debug_redacts_did_and_signature`, `revoke_refuses_from_rotating` (cross-state-machine safety)

### IdentityError enum extension

- [ ] NEW variants on existing `IdentityError` enum:
  - `NotActive { current_state: LifecycleState }` — `sign()` called when lifecycle ≠ Active
  - `AlreadyRevoked` — `activate()` called on Revoked state
  - `RotationInProgress` — `revoke()` called on Rotating state (forces caller to complete or abort rotation first; lands in `0009-l2`)
- [ ] Manual redacting `Debug` on `IdentityError`: existing variants + new variants; no credential material leaks
- [ ] Tests: `identity_error_debug_redacts_not_active`, `identity_error_debug_redacts_already_revoked`, `identity_error_debug_redacts_rotation_in_progress`

### Observability integration

- [ ] `IdentityActivated` + `IdentityRevoked` events emit via existing `octo-wallet::observability::emit_event()` (no new transport — direct call site; gossip fan-out ships in `0009-l2` per RFC-0853 §12)
- [ ] Event ordering: `activate` event emitted AFTER state mutation but BEFORE returning from `activate()` (causal ordering invariant for replay)
- [ ] No event re-emission on idempotent call (second `activate()` from `Active` returns `Ok(())` with NO event)

### Cross-crate compat

- [ ] `cargo test -p octo-wallet --lib identity::lifecycle` zero regressions
- [ ] `cargo test -p octo-wallet --lib capability` zero regressions (capability minting path depends on `IdentityKey::sign()` working in `Active` state)
- [ ] `cargo test -p octo-wallet --lib` zero regressions
- [ ] `cargo test -p octo-cap-macaroon --lib` zero regressions (Phase 2+2b+2c closed 2026-08-09)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires:**

- `missions/claimed/0009-a-hsm-routing.md` — `IdentityKey::sign` routed through `Arc<dyn HsmAdapter>` per RFC-0009 v1.1 (foundation for `can_sign()` gate + `sign()` not-active rejection)
- `missions/claimed/0957-c-holder-registry-impl.md` — `HolderRecord::revoked_at_millis_unix` column (already shipped; lands in same StoolapHolderRegistry row as activation — schema migration needed: add `activated_at_millis_unix` column)
- RFC-0009 §Lifecycle Requirements (Accepted 2026-07-20; §Identity Lifecycle State Machine + Appendix A enum repr)
- RFC-0853 §12 (Accepted v1.0.0; revocation tuple `(compromised_key_id, revocation_epoch, successor_key_id, signature_by_successor)` shape — full successor linkage lands in `0009-l2`, revocation half lands here)

**Mission gates:**

- This mission gates `0009-l2-rotation-successor-linkage.md` (Rotating state needs `LifecycleState::Rotating` variant declared — ships here)
- This mission gates `0957-f-f4-bundle.md` (`CapabilityBundle` needs `revoked_at_unix` + `lifecycle_state` fields — both available after this mission lands)

**Not Requires:**

- RFC-0871 acceptance (independent of NodeEnvelope work)
- RFC-0957-A1 G3 gossip (activation/revocation events are LOCAL-only in this mission; gossip fan-out ships in `0009-l2`)

```yaml
depends_on:
  - 0009-a-hsm-routing # IdentityKey::sign routed through Arc<dyn HsmAdapter>
  - 0957-c-holder-registry-impl # HolderRecord schema (add activated_at_millis_unix column)
  - RFC-0009 # §Lifecycle Requirements + Appendix A enum repr
  - RFC-0853 # §12 revocation tuple shape (revocation half only; successor half in 0009-l2)
```

Real missions + RFC substrate only. No phantom pointers.

## Implementation Guide

### LifecycleState enum

```rust
// crates/octo-wallet/src/lifecycle.rs (NEW)

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LifecycleState {
    Designated = 0x00,
    Active = 0x01,
    Rotating = 0x02, // declared; transitions land in 0009-l2
    Revoked = 0x03,
}

impl LifecycleState {
    #[must_use]
    pub const fn is_active(self) -> bool { matches!(self, Self::Active) }
    #[must_use]
    pub const fn is_revoked(self) -> bool { matches!(self, Self::Revoked) }
    #[must_use]
    pub const fn is_rotating(self) -> bool { matches!(self, Self::Rotating) }

    /// Valid state machine edges per RFC-0009 §Identity Lifecycle State Machine.
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Designated, Self::Active)
                | (Self::Active, Self::Revoked)
                | (Self::Active, Self::Rotating) // lands in 0009-l2
                | (Self::Rotating, Self::Active) // lands in 0009-l2
                | (Self::Rotating, Self::Revoked) // lands in 0009-l2
        )
    }
}
```

### IdentityKey extension

```rust
// crates/octo-wallet/src/identity.rs (MODIFY)

pub struct IdentityKey {
    pub(crate) inner: Arc<dyn HsmAdapter>,
    pub(crate) public_key: [u8; 32],
    pub(crate) lifecycle: LifecycleState, // NEW
    pub(crate) activated_at: Option<NonZeroU64>, // NEW
    pub(crate) revoked_at: Option<NonZeroU64>, // NEW
}

impl IdentityKey {
    /// RFC-0009 §Lifecycle row 1: Designated → Active.
    pub fn activate(&mut self, clock: &dyn Clock) -> Result<(), IdentityError> {
        if self.lifecycle == LifecycleState::Revoked {
            return Err(IdentityError::AlreadyRevoked);
        }
        if self.lifecycle == LifecycleState::Active {
            return Ok(()); // idempotent no-op
        }
        let now = clock.now_unix();
        self.lifecycle = LifecycleState::Active;
        self.activated_at = NonZeroU64::new(now);
        emit_event(IdentityActivated {
            did: self.did(),
            activated_at_unix: now,
        });
        Ok(())
    }

    /// RFC-0009 §Lifecycle row 4: Active → Revoked.
    pub fn revoke(
        &mut self,
        reason: RevocationReason,
        clock: &dyn Clock,
    ) -> Result<IdentityRevoked, IdentityError> {
        if self.lifecycle == LifecycleState::Revoked {
            // Idempotent: return cached event (re-issue signature? NO — would change bytes)
            return Err(IdentityError::AlreadyRevoked);
        }
        let now = clock.now_unix();
        let event = IdentityRevoked {
            did: self.did(),
            reason,
            revoked_at_unix: now,
            signature: self.inner.sign(b"revoke")?,
        };
        self.lifecycle = LifecycleState::Revoked;
        self.revoked_at = NonZeroU64::new(now);
        self.inner.zeroize(); // RFC-0009 §Security §Key Handling Rule 3
        emit_event(event.clone());
        Ok(event)
    }
}
```

### Test fixture pattern

- `MockClock` (already exists in `crates/octo-wallet/src/test_util.rs`) — deterministic `now_unix()` returns
- `InMemorySigner` (already exists per RFC-0009 v1.1 InMemorySigner default impl) — captures `sign(b"revoke")` calls for revocation signature assertion
- `assertion::lifecycle_state_round_trip` — `LifecycleState::try_from(u8)` + `as_u8()` for Appendix A repr compatibility

## Decomposition Rationale

Single sub-mission (l1) covers ONLY the state machine + activate + revoke half. Rotation (`Active ↔ Rotating`) + successor linkage + grace period split into `0009-l2` to keep each mission within the BLUEPRINT §Multi-Mission Decomposition pushable-unit threshold (~500 lines per mission). Estimated LoC for this sub-mission: ~400-500 lines (lifecycle.rs 150 lines + identity.rs extensions 200 lines + tests 150 lines).

## Claimant

@unassigned (per [[feedback_initiation_user_only]] — user initiates the claim)

## Pull Request

(unset)

## Notes

- Mission captured in `missions/claimed/0957-f-future-work.md` §Notes line 117 ("F4 is the only item not yet fully spec'd (TV F4 placeholder) because it depends on RFC-0009 §Identity evolution")
- Mission unblocks `0957-f-f4-bundle.md` for the `revoked_at_unix` + `lifecycle_state` bundle fields
- Mission unblocks `0009-l2-rotation-successor-linkage.md` for the `Rotating` state variant + `can_transition_to` edges
- Per [[no-phantom-mission-pointers]]: mission file now exists; `0957-f-f4-bundle.md` §Status line + `0957-f-future-work.md` §Notes get updated to cite this mission (avoid phantom pointer)
- Per [[cargo-fmt-workflow]] + [[feedback_clippy_zero_warnings]]: `cargo fmt` + `cargo clippy -D warnings` green before commit
- Per [[no-line-refs-anywhere]]: all references use §section-name / symbol form (e.g., §Identity Lifecycle State Machine, `IdentityKey::activate`, `LifecycleState::Active`)
- Per [[rfc-referencing-convention]]: RFCs referenced by number only (`RFC-0009`, `RFC-0853`)

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures Identity Lifecycle State Machine + Designated→Active + Active→Revoked transitions (split from rotation work into `0009-l2`). Closes the "blocks on RFC-0009 §Identity evolution" deferral in `0957-f-future-work.md` Band A closure. |

Last Updated: 2026-08-09
Version: 0.1