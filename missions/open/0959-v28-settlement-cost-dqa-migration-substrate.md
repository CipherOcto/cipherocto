# 0959-v28-settlement-cost-dqa-migration-substrate — SettlementEvent DQA + cost_asset_id + audit-invariant

**Status:** Open
**Substrate:** RFC-0959 v2.8 §2 (SettlementEvent Specification) + §3 (Wire Form)
**Parent:** RFC-0959 v2.8 + RFC-0105 v3.5 §3.13 (tri-invariant consumer)
**Depends on:**

- Mission D (`0105-v35-asset-registry-nonce-registry-substrate.md`) — provides `AssetRegistry`, `AssetError`, `AssetKind`, `AssetMetadata`, `MAX_SCALE`, `NonceRegistry`, `NonceError`, `newtypes::{Nonce, Epoch, GovernanceSignature}`, `sovereign_nonce_namespace`, `verify_governance_signature`, `blake3_hash` canonical substrate imports (per RFC-0959 v2.8 §2.1 L50-56)
- Mission E (`0965-v21-payment-caveat-asset-binding-substrate.md`) — provides `PaymentCaveat` for `verify_settlement_against_payment_caveat` audit invariant (RFC-0105 §3.13 tri-invariant pair: `PaymentCaveat.asset_id == SettlementEvent.cost_asset_id`)
- RFC-0959 v2.7 baseline (already landed: `SettlementError::AlreadyConsumed(String)` at `crates/quota-router-sm-engine/src/lib.rs:266`)

## Scope

Land RFC-0959 v2.8 `SettlementEvent` GREENFIELD substrate: introduce
`SettlementEvent` struct + `SettlementDecision` enum + `SettlementId` /
`AskId` / `EvidenceRef` typed wrappers + `VaultRegistryError` enum +
`SettlementEventError` enum + custom `Deserialize` for legacy-form
rejection + `new()` constructor with scale-resolution + governance
signature + NonceRegistry observation + vault-contains-asset check +
body_hash commitment + `verify_settlement_against_payment_caveat` audit
invariant (RFC-0105 §3.13 tri-invariant pair) + sovereign-asset signature
exemption + wire form.

### Mission G sub-steps

1. **`SettlementEvent` struct** — `crates/quota-router-sm-engine/src/settlement_event.rs`
   (NEW). Per RFC-0959 v2.8 §2.1 L75-89. 13 fields:

   ```rust
   pub struct SettlementEvent {
       pub settlement_id: SettlementId,
       pub ask_id: AskId,
       pub cost_vault_id: VaultId,                  // Layer A frozen tuple struct
       pub cost_asset_id: AssetId,                  // NEW: explicit asset binding
       pub asset_kind: AssetKind,                   // NEW (Round 3 fix #17)
       pub cost: Dqa,
       pub evidence_ref: EvidenceRef,
       pub ledger_height: u64,
       pub created_at_unix_ms: u64,
       pub settlement_decision: SettlementDecision,
       pub governance_signature: GovernanceSignature,
       pub registry_snapshot_epoch: Epoch,
       pub nonce: Nonce,
   }
   ```

2. **`SettlementDecision` enum** — same file. Per §2.1 L91-105. 4 variants:
   `Consumed` / `AlreadyConsumed` (canonical substrate arm per
   `crates/quota-router-sm-engine/src/lib.rs:266`; no rename history;
   `ReceiptReplay` was NEVER a substrate variant) /
   `InsufficientEvidence` / `BudgetExhausted`. Audit* variants REMOVED
   per Round 4 fix #4 — live in `SettlementAuditError` (§2.3).

3. **Typed wrappers** — same file. Per §2.1 L67-69:

   ```rust
   pub struct SettlementId(pub [u8; 32]);
   pub struct AskId(pub [u8; 32]);
   pub struct EvidenceRef(pub [u8; 32]);
   ```

   Local to settlement_event module; no canonical RFC-0105 §3 anchor.

4. **Custom `Deserialize` for legacy-form rejection** — same file. Per
   §2.1 L74 + §2.2 L141-143. Inspects raw envelope BEFORE derived
   `Deserialize` for legacy `{ cost: { amount_micro_octo_w } }` form. If
   present AND `cost_asset_id != OCTO_W_ASSET_ID`, returns
   `LegacyFormOnNonOctoWContext { claimed_asset_id }` error. The string
   `"legacy_form_on_non_octow_context"` is the error context string (per
   RFC §2.2 L142), NOT a JSON discriminator field. Happy path:
   delegate to derived impl.

5. **`VaultRegistryError` enum** — same file. Per §2.2 L111-116. 2
   variants: `UnknownVault { vault_id }` / `VaultAssetMismatch {
vault_id, asset_id }`. Reused by RFC-0960 v3.6 BurnEventRef gate 3
   (Mission F). Single-source-of-truth: this enum lives at the bridge
   boundary; consumers import via `use octo_quota_router_sm_engine::settlement_event::VaultRegistryError;`
   OR the canonical home is chosen at Mission F+G landing time (decision
   surfaced to user per `feedback_initiation_user_only`).

6. **`SettlementEventError` enum** — same file. Per §2.2 L118-144. **9**
   variants: `AssetUnknown` / `ScaleMismatch { cost_scale, vault_scale }`
   / `ScaleOutOfRange { scale }` / `InvalidSignature` / `Replay` /
   `StaleSnapshot { snapshot, live }` /
   `VaultAssetMismatch { vault_id, asset_id }` /
   `VaultUnknown { vault_id }` /
   `LegacyFormOnNonOctoWContext { claimed_asset_id }`.
   **Annotated `#[non_exhaustive]`** at the enum level (Layer B additive
   substrate; downstream consumers MUST handle the wildcard arm).

7. **`encode_settlement_decision` helper** — same file. Per §2.2
   L154-173. Length-prefixed discriminant + payload:
   `Consumed = 0x01` / `AlreadyConsumed = 0x02` /
   `InsufficientEvidence = 0x03` / `BudgetExhausted = 0x04`. Public
   associated function on `SettlementEvent` (called via `Self::`). Round
   5 fix promoted from implicit helper to documented API.

8. **`compute_settlement_body_hash` helper** — same file. Per §2.2
   L181-205. Factored shared between `new()` and `validate()` per Round
   6 cleanup (prevents field-set drift between construction and
   verification). Field set: `settlement_id | ask_id | cost_vault_id |
cost_asset_id | kind_tag (1 byte) | cost (Dqa .to_le_bytes) |
ledger_height (u64 .to_le_bytes) | evidence_ref | governance_pubkey
| nonce`. Mirrors RFC-0960 v3.6 §2.2 L371-393 `compute_body_hash`.

9. **`new()` constructor with 7 gates** — same file. Per §2.2 L207-295.
   Signature per L207-223: takes **11 args** (settlement_id, ask_id,
   cost_vault_id, cost_asset_id, cost, evidence_ref, ledger_height,
   created_at_unix_ms, settlement_decision, governance_signature, nonce)
   - `&dyn AssetRegistry` + `&dyn VaultRegistry` + `&mut dyn NonceRegistry`
   - `current_epoch: Epoch`. `asset_kind` is derived from `meta.kind`
     (NOT a constructor arg). `registry_snapshot_epoch` is set from
     `current_epoch` (NOT a constructor arg). Gates (in order, per RFC):
   * Gate 0: `registry.metadata(&cost_asset_id)` resolves + not
     tombstoned (else `AssetUnknown`)
   * Gate 1: `cost.wire_scale == meta.wire_scale` (else `ScaleMismatch`)
   * Gate 2: `cost.wire_scale <= MAX_SCALE = 18` (else `ScaleOutOfRange`)
   * Gate 3: resolve `governance_pubkey` from `meta` — sovereign fallback
     uses `sovereign_nonce_namespace(&asset_id)` per Round 5 fix HIGH-1
     and RFC-0105 v3.5 §3.12 + §3.11 L633 (NOT all-zeros — see TV-SE10
     coverage of the distinction)
   * Gate 4: `compute_settlement_body_hash(...)` per L181-205
   * Gate 5: `verify_governance_signature(&governance_signature.sig,
&body_hash, &governance_pubkey)` (else `InvalidSignature`; sovereign
     EXEMPT per §3.3)
   * Gate 6: `vault_registry.contains_asset(&cost_vault_id,
&cost_asset_id)` returns `Ok(())` (else map `VaultAssetMismatch` /
     `VaultUnknown`) — placed AFTER signature per RFC §2.2 L269-273
   * Gate 7: `nonce_registry.observe(&governance_pubkey, &nonce.0)`
     returns `Ok(())` (else `Replay`)

   **Note:** stale-snapshot detection (RFC §2.3 L343-347) belongs to a
   separate `validate()` function, NOT `new()`. This split matches
   RFC-0960 v3.6 §2.1 (BurnEventRef) pattern: new() = construction-time
   gates only; validate() = post-deser / audit-time re-checks.

10. **`verify_settlement_against_payment_caveat` audit invariant** —
    same file. Per §2.3. Tri-invariant pairwise check per RFC-0105
    v3.5 §3.13: `SettlementEvent.cost_asset_id == PaymentCaveat.asset_id`.
    Violation REJECTS the audit chain. Function signature:

    ```rust
    pub fn verify_settlement_against_payment_caveat(
        settlement: &SettlementEvent,
        caveat: &PaymentCaveat,
    ) -> Result<(), AuditInvariantViolation>;
    ```

11. **Sovereign-asset signature exemption** — same file. Per §3.3.
    Sovereign role tokens (`AssetKind::SovereignRoleToken`) settled by
    chain rule, NOT by vault governance key. `new()` skips gate 5
    (`InvalidSignature`) when `meta.governance_pubkey.is_none()`.
    `compute_settlement_body_hash` uses
    `sovereign_nonce_namespace(&asset_id)` resolved value
    (= `blake3_hash(b"octo:sovereign-nonce-ns:v1" || asset_id.0)` per
    RFC-0105 §3.12 + §3.11 L633) for `governance_pubkey` in the
    body_hash commitment, NOT all-zeros. All-zeros is reserved for the
    wire-form signature field ONLY (RFC-0959 v2.8 §3.3 L484).

12. **Wire form** — same file. Per §3 L468-512. Borsh field order per
    §3.1: `settlement_id | ask_id | cost_vault_id | cost_asset_id |
asset_kind | cost | evidence_ref | ledger_height | created_at_unix_ms
| settlement_decision | governance_signature |
registry_snapshot_epoch | nonce`. JSON equivalent.
    `LegacyFormOnNonOctoWContext` variant is the wire-form migration
    rejection (per §3.2).

## Test Vectors

Per RFC-0959 v2.8 §8 (Pending, concrete test vectors).

- TV-SE1: `SettlementEvent::new(...)` happy-path — all gates pass,
  returns `Ok(SettlementEvent)` with all 13 fields populated
- TV-SE2: gate 1 violation — `cost.wire_scale != meta.wire_scale`
  returns `Err(ScaleMismatch)`
- TV-SE3: gate 2 violation — `cost.wire_scale > MAX_SCALE = 18`
  returns `Err(ScaleOutOfRange)`
- TV-SE4: gate 3 violation — vault-asset mismatch returns
  `Err(VaultAssetMismatch)`
- TV-SE5: gate 3 violation — vault unknown returns `Err(VaultUnknown)`
- TV-SE6: gate 6 violation — forged governance signature returns
  `Err(InvalidSignature)`
- TV-SE7: gate 6 sovereign exemption — sovereign role token settlement
  succeeds WITHOUT governance signature verification
- TV-SE8: gate 7 violation — `nonce_registry.observe(pk, nonce) ==
Err(AlreadyObserved)` returns `Err(Replay)`
- TV-SE9: `encode_settlement_decision` produces canonical length-prefixed
  bytes per discriminant (`0x01` for Consumed, etc.)
- TV-SE10: `compute_settlement_body_hash` is deterministic across
  `new()` and `validate()` invocations (Round 6 fix verifies no
  field-set drift)
- TV-SE11: custom `Deserialize` accepts modern envelope
  `{cost_asset_id, cost: DqaEncoding, ...}`
- TV-SE12: custom `Deserialize` rejects legacy envelope
  `{cost: {amount_micro_octo_w: i64}, cost_asset_id: BRIDGED-hex}` with
  `LegacyFormOnNonOctoWContext { claimed_asset_id }`
- TV-SE13: `verify_settlement_against_payment_caveat` matches when
  `settlement.cost_asset_id == caveat.asset_id`
- TV-SE14: `verify_settlement_against_payment_caveat` rejects when
  `settlement.cost_asset_id != caveat.asset_id` (RFC-0105 §3.13
  tri-invariant pair)
- TV-SE15: wire form round-trip — `BorshSerialize → BorshDeserialize`
  preserves all 13 fields
- TV-SE16: audit-batch replay path (RFC-0105 §3.13 L669) bypasses
  per-event validate() cache and runs fresh
  `verify_settlement_against_payment_caveat` pairwise check on every
  `(caveat, burn, settlement)` tuple; cache HIT must NOT short-circuit
  the batch-replay path
- TV-SE17: `new()` arg count = 11 (not 13); `asset_kind` derived from
  `meta.kind`, `registry_snapshot_epoch` set from `current_epoch` (no
  constructor args for either)

## Layer direction (per [[cipherocto-design-principles]])

- `quota-router-sm-engine` (Layer C specialized node role) —
  `settlement_event.rs` = **Layer B additive type hosted in Layer C node
  crate**; substrate-mandated invariant carrier across the audit chain
- `SettlementDecision::AlreadyConsumed` aligns with pre-existing
  `SettlementError::AlreadyConsumed(String)` at `crates/quota-router-sm-engine/src/lib.rs:266`
  (canonical substrate variant name per §0; no rename history)
- `VaultRegistryError` enum — single-source-of-truth decision needed
  (this Mission G site OR shared `octo-vault` home); surfaced to user
  per `feedback_initiation_user_only`
- No cross-layer inversion; all new types additive (semver-minor)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
cargo test -p quota-router-sm-engine --lib settlement_event
```

## Backward compat

- All new types are additive (no existing crate modification)
- `SettlementDecision::AlreadyConsumed` aligns with existing
  `SettlementError::AlreadyConsumed(String)` substrate variant (no
  rename — substrate-canonical name per RFC-0959 v2.8 §0)
- `VaultId(pub [u8; 32])` tuple struct UNCHANGED (Layer A frozen; no
  rename history per §0)
- `SettlementId`, `AskId`, `EvidenceRef` typed wrappers are NEW local
  (no RFC-0105 §3 anchor)
- `SettlementEvent` is GREENFIELD; no consumers existed before this
  Mission G lands
- `SettlementEventError::LegacyFormOnNonOctoWContext` is the wire-form
  migration close per RFC-0105 v3.5 §5

## Cross-references

- RFC-0959 v2.8 §0 — GREENFIELD marker (explicit per §0 L20-26)
- RFC-0959 v2.8 §1 — motivation (cost_asset_id audit-invariant)
- RFC-0959 v2.8 §2.1 — SettlementEvent substrate (L42-106)
- RFC-0959 v2.8 §2.2 — scale-resolution invariant + new() (L108-389)
- RFC-0959 v2.8 §2.3 — verify_settlement_against_payment_caveat (RFC-0105 §3.13)
- RFC-0959 v2.8 §2.4 — error scenario matrix
- RFC-0959 v2.8 §2.5 — cryptographic primitives (forward-pointer to
  RFC-0105 §3.12)
- RFC-0959 v2.8 §3 — wire form + sovereign-asset signature exemption
- RFC-0105 v3.5 §3.13 — tri-invariant declaration (consumer anchor)
- RFC-0105 v3.5 §3.13 L669 — **audit-batch replay enforcement** (NEW
  v3.5-r6): per-tuple fresh pairwise check
  `(PaymentCaveat, BurnEventRef, SettlementEvent)` in the audit-batch
  replay path; per-event validate() cache MUST NOT be used. Mission G's
  `verify_settlement_against_payment_caveat` (TV-SE16) MUST be invoked
  from the batch-replay path with NO caching.
- RFC-0105 v3.5 §5 — cross-RFC error-enum ownership
- Mission D — canonical substrate imports
- Mission E — PaymentCaveat (audit-invariant pair)
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) —
  `SettlementEventProducer` consumes Mission G substrate (wraps
  `SettlementEventRepository::insert` per RFC-0960 v3.7 §2.5)
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) —
  cache table consumer (cache invalidation envelope source)
- [[cipherocto-design-principles]] — Layer B additive-only rule

## Claimant

@unassigned

## Pull Request

#
