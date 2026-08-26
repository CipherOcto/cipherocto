# 0960-v36-burn-event-dqa-migration-substrate — BurnEventRef DQA + asset-binding + NonceRegistry + legacy-form rejection

**Status:** Open
**Substrate:** RFC-0960 v3.6 §2 (BurnEventRef Specification) + §3 (Wire Form)
**Parent:** RFC-0960 v3.6 + RFC-0105 v3.5 §3.13 (tri-invariant consumer)
**Depends on:**

- Mission D (`0105-v35-asset-registry-nonce-registry-substrate.md`) — provides `AssetRegistry`, `AssetError`, `AssetKind`, `AssetMetadata`, `MAX_SCALE`, `NonceRegistry`, `NonceError`, `newtypes::{Nonce, Epoch, GovernanceSignature}`, `sovereign_nonce_namespace`, `verify_governance_signature`, `blake3_hash` canonical substrate imports (per RFC-0960 v3.6 §2.1 L54-60)
- Mission E (`0965-v21-payment-caveat-asset-binding-substrate.md`) — provides `PaymentCaveat` for `verify_burn_against_caveat` audit invariant (RFC-0105 v3.5 §3.13 tri-invariant)
- RFC-0960 v3.5 (already landed: `VaultId`, `AssetId`, `VaultRegistry`, `transfer_events` v014 schema)

## Scope

Land RFC-0960 v3.6 `BurnEventRef` GREENFIELD substrate: introduce
`BurnEventRef` struct + `BurnEventError` enum + `SettlementId` typed
wrapper + custom `Deserialize` for legacy-form rejection + `new()`
constructor with scale-binding + governance signature + NonceRegistry
observation + vault-contains-asset check + body_hash commitment +
sovereign-asset exemption + `verify_burn_against_caveat` audit invariant
(RFC-0105 v3.5 §3.13 tri-invariant pair) + wire form.

### Mission F sub-steps

1. **`BurnEventRef` struct** — `crates/octo-policy/src/burn_event.rs`
   (NEW). Per RFC-0960 v3.6 §2.1 L71-87. 11 fields:

   ```rust
   pub struct BurnEventRef {
       pub chain_id: ChainId,
       pub vault_id: VaultId,
       pub asset_id: AssetId,
       pub asset_kind: AssetKind,
       pub amount: Dqa,
       pub ledger_height: u64,
       pub settlement_event_ref: SettlementId,
       pub governance_signature: GovernanceSignature,
       pub governance_pubkey: [u8; 32],       // Round 3 CRITICAL #1 — pinned at construction
       pub registry_snapshot_epoch: Epoch,
       pub nonce: Nonce,
   }
   ```

2. **`BurnEventError` enum** — same file. Per §2.1 L89-126. 11 variants:
   `AssetUnknown` / `ScaleMismatch { amount_wire_scale, asset_wire_scale }`
   / `ScaleOutOfRange { scale }` / `InvalidSignature` /
   `Replay { prior_height }` / `StaleSnapshot { snapshot, live }` /
   `AssetKindMismatch { claimed, registered }` /
   `VaultUnknown { vault_id }` /
   `VaultAssetMismatch { vault_id, asset_id }` /
   `AuditSinkFailed { sink_error: Box<AuditError> }` /
   `LegacyFormOnNonOctoWContext { claimed_asset_id }`.
   **Annotated `#[non_exhaustive]`** at the enum level (Layer B additive
   substrate; downstream consumers MUST handle the wildcard arm).

3. **Custom `Deserialize` for legacy-form rejection** — same file. Per
   §2.1 L141-170. Inspects raw envelope BEFORE derived `Deserialize` for
   legacy `{ amount_micro_octo_w: i64 }` key. If present, parses
   `cost_asset_id` field as hex; returns
   `LegacyFormOnNonOctoWContext { claimed_asset_id }` error (sentinel
   `AssetId([0u8; 32])` if `cost_asset_id` missing). Happy path:
   re-serialize through JSON + delegate to derived impl (acceptable
   performance cost; BurnEventRef deserialized at audit boundaries, not
   hot path per L166).

4. **`SettlementId` typed wrapper** — same file. Per §2.2 L181.
   `pub struct SettlementId(pub [u8; 32]);`. `SettlementRef` deprecated
   alias: `pub type SettlementRef = SettlementId;` (one substrate cycle
   per L182).

5. **`new()` constructor with 7 gates** — same file. Per §2.2 L196-275.
   Signature per L197-209: takes **8 args** (chain_id, vault_id, asset_id,
   amount, ledger_height, settlement_event_ref, governance_signature,
   nonce) + `&dyn AssetRegistry` + `&dyn VaultRegistry` +
   `current_epoch: Epoch`. Gates (in order, per RFC-0960 v3.6 §2.2
   L213-260 canonical order):
   - Gate 0: `registry.metadata(&asset_id)` resolves + not tombstoned
     (else `AssetUnknown`)
   - Gate 1: `amount.wire_scale == meta.wire_scale` (else `ScaleMismatch`)
   - Gate 2: `amount.wire_scale <= MAX_SCALE` (else `ScaleOutOfRange`)
   - Gate 3: `vault_registry.contains_asset(&vault_id, &asset_id)`
     returns `Ok(())` (else map error variants: `UnknownVault` →
     `VaultUnknown`, `VaultAssetMismatch` → `VaultAssetMismatch`) —
     fails fast on cheap lookup before expensive crypto (RFC-0960 v3.6
     §2.2 L226-230)
   - Gate 4: resolve `governance_pubkey` from `meta` — sovereign
     fallback uses `sovereign_nonce_namespace(&asset_id)` for
     body_hash commitment (Round 3 IMPORTANT #3 + Round 4 CRITICAL #2;
     per RFC-0960 v3.6 §3.3 L504-511)
   - Gate 5: `compute_body_hash(...)` over length-prefixed field set
     per §2.2 L371-393
   - Gate 6: `verify_governance_signature(&governance_signature.sig,
&body_hash, &governance_pubkey)` (else `InvalidSignature`; sovereign
     assets EXEMPT per §3.3)

   **Note:** nonce observation (Gate 7), stale-snapshot detection
   (Gate 8), and asset_kind assert (Gate 9) are NOT in `new()`. Per
   RFC-0960 v3.6 §2.2 L300-308 (asset_kind) + L309-313 (stale_snapshot)
   belong to `validate()`; per L337-368 (nonce_registry.observe) belongs
   to `consume()`. This split is substrate-mandated (Round 1 CRITICAL #4
   TOCTOU mitigation) — nonce marking MUST be bundled into `consume()`
   alongside audit-sink commit to prevent observation-without-commit.

6. **`validate()` post-deser check (separate function)** — same file. Per
   §2.2 L279-332. **7 checks** (RFC-0960 v3.6 §2.2 validate() L279-332):
   - (a) re-run Gate 0: `registry.metadata(&asset_id)` resolves +
     not tombstoned (else `AssetUnknown`; DIRECT-DESERIALIZE BYPASS
     MITIGATION per RFC §2.2 L283-289)
   - (b) re-run Gate 1: `amount.wire_scale == meta.wire_scale` (else
     `ScaleMismatch`)
   - (c) re-run Gate 3: `vault_registry.contains_asset(&vault_id,
&asset_id)` (else `VaultUnknown` / `VaultAssetMismatch`; mandatory
     re-run for direct Deserialize bypass mitigation per RFC §2.2
     L295-302)
   - (d) asset_kind equality check — `self.asset_kind ==
meta.kind` (else `AssetKindMismatch { claimed, registered }`)
   - (e) stale_snapshot detection — `current_epoch.0 <
self.registry_snapshot_epoch.0` (else `StaleSnapshot { snapshot,
live }`)
   - (f) re-run `compute_body_hash` over stored fields
   - (g) re-run Gate 6: signature verify against recomputed body_hash

   Returns `BurnEventError::{AssetUnknown, ScaleMismatch, VaultUnknown,
VaultAssetMismatch, AssetKindMismatch, StaleSnapshot,
InvalidSignature}` for offline audit integrity check (Round 1
   IMPORTANT #9). Direct Deserialize bypass MUST be closed by (a)
   - (c) re-runs (Round 3 SECURITY HIGH finding F1).

7. **`consume()` nonce observe + audit sink write** — same file. Per
   §2.2 L337-368. Steps:
   - Call `validate()` (returns Err on first failed gate)
   - Call `nonce_registry.observe(NonceEventKind::Burn,
&governance_pubkey, &nonce.0)` — `governance_pubkey` is the
     PINNED `[u8;32]` struct field set at `new()` Gate 4: managed
     assets use `meta.governance_pubkey`; sovereign role tokens use
     `sovereign_nonce_namespace(&asset_id)` (per RFC-0960 v3.6 §3.3
     L510). Single-role equivalence: body_hash commitment AND
     NonceRegistry observation key both derive from this single
     pinned value. Sovereign burns therefore NEVER collide on
     `[0u8; 32]` zero-pubkey namespace. Returns
     `Err(NonceError::AlreadyObserved)` mapped to
     `BurnEventError::Replay { prior_height }`.
   - **Atomicity guarantee (Round 5 fix + R6 substrate completion +
     R7 CRITICAL #3 cross-sink extension):**
     `consume()` orchestrates THREE sinks in sequence:
     (1) `nonce_registry.observe(NonceEventKind::Burn, &pk, &nonce)`
     (2) `audit_sink.write(...)`
     (3) `producer.log.insert(...)` (the TransferEventLog write
     triggered via Mission B's `BurnEventProducer::produce()`;
     `consume()` invokes the producer between audit_sink.write
     and the consumed-mark step).

     If ANY sink fails AFTER a prior sink succeeded, ALL prior
     sinks MUST be rolled back atomically:
     - **(3) fails** → rollback (2) via
       `audit_sink.compensate(...)` + rollback (1) via
       `nonce_registry.unobserve(NonceEventKind::Burn,
       &governance_pubkey, &nonce.0)`. Return
       `BurnEventError::AuditSinkFailed { sink_error:
       AuditError::LogInsertFailed { sink: Box::AuditError,
       nonce_rolled_back: true, audit_compensated: true } }`.
     - **(2) fails** → rollback (1) via
       `nonce_registry.unobserve(NonceEventKind::Burn,
       &governance_pubkey, &nonce.0)`. Return
       `BurnEventError::AuditSinkFailed { sink_error: AuditError::UnobserveFailed(inner) }`
       with structured log + alert (NOT silently swallowed).
     - **(1) fails** → no rollback needed (nothing to undo); return
       `Err(NonceError::AlreadyObserved)` mapped to
       `BurnEventError::Replay { prior_height }`.

     If `unobserve` itself fails (WAL outage during rollback),
     return
     `BurnEventError::AuditSinkFailed { sink_error: AuditError::UnobserveFailed(inner) }`
     with structured log + alert (NOT silently swallowed). The
     retry-friendly error variant from Round 3 MED #7 is preserved
     (sink failure = infrastructure fault, not cryptographic
     rejection), but the contract is now atomic across all three
     sinks: a failed `consume()` leaves no observable side-effect
     (no nonce bucket consumed + no audit record + no log row
     without the other two matching). Without 3-sink rollback,
     a log-insert failure would burn the nonce bucket + write
     audit + skip the TransferEventLog — caller sees `Replay` on
     retry but `VaultBalanceProjection` is silently short by the
     burn amount (fund accounting drift, R7 #3 CRITICAL finding).
     **Cross-event isolation:** the `event_kind
     = NonceEventKind::Burn` discriminator (per Mission D v3.5-r8
     PROPOSAL surface change) namespaces the observe key per event
     type; BurnEventRef vs SettlementEvent vs PaymentCaveat nonces
     each use distinct LRU buckets via `(event_kind, pk, nonce)`
     triple. For sovereign role tokens with the same asset_id, the
     SAME `sovereign_nonce_namespace(&asset_id)` pk is shared by
     both BurnEventRef and SettlementEvent — without event_kind
     discriminator the nonces would collide; with it, they are
     distinct buckets.
   - GREENFIELD `AuditSink` trait + `AuditError` enum are declared
     **inline** within the same `burn_event.rs` file per RFC-0960 v3.6
     §2.2 L408-411 (NOT a separate `audit_sink.rs` file). `AuditError`
     extended (R7 CRITICAL #3) with `LogInsertFailed { sink,
     nonce_rolled_back, audit_compensated }` variant for the
     3-sink rollback path.

8. **`verify_burn_against_caveat` audit invariant** — same file. Per §2.3.
   Tri-invariant pairwise check per RFC-0105 v3.5 §3.13:
   `BurnEventRef.asset_id == PaymentCaveat.asset_id`. Violation REJECTS
   the audit chain. Function signature:

   ```rust
   pub fn verify_burn_against_caveat(
       burn: &BurnEventRef,
       caveat: &PaymentCaveat,
   ) -> Result<(), AuditInvariantViolation>;
   ```

9. **Sovereign-asset exemption** — same file. Per §3.3 L504-511. Sovereign
   role tokens (`AssetKind::SovereignRoleToken`) burned by chain rule,
   NOT by vault governance key. `new()` skips gate 5 (`InvalidSignature`)
   when `meta.governance_pubkey.is_none()`. Body_hash commitment uses
   `sovereign_nonce_namespace(&asset_id)` resolved value
   (= `blake3_hash(b"octo:sovereign-nonce-ns:v1" || asset_id.0)` per
   RFC-0105 v3.5 §3.12 + §3.11 L633), NOT all-zeros. All-zeros is reserved
   for the wire-form signature field ONLY (RFC-0960 v3.6 §3.1 L484
   wire-form line, with §3.3 sovereign-exemption context).

10. **Wire form** — same file. Per §3 L470-512. Borsh field order per
    §3.1: `chain_id | vault_id | asset_id | asset_kind | amount |
ledger_height | settlement_event_ref | governance_signature |
governance_pubkey | registry_snapshot_epoch | nonce`. JSON
    equivalent. `LegacyFormOnNonOctoWContext` variant is the wire-form
    migration rejection (per §3.2).

## Test Vectors

Per RFC-0960 v3.6 §9 (Pending, concrete test vectors).

- TV-BE1: `BurnEventRef::new(...)` happy-path — all gates pass, returns
  `Ok(BurnEventRef)` with all fields populated
- TV-BE2: gate 1 violation — `amount.wire_scale != meta.wire_scale`
  returns `Err(ScaleMismatch { amount_wire_scale, asset_wire_scale })`
- TV-BE3: gate 2 violation — `amount.wire_scale > MAX_SCALE = 18`
  returns `Err(ScaleOutOfRange { scale })`
- TV-BE4: Gate 3 violation — vault unknown returns `Err(VaultUnknown)`
- TV-BE5: Gate 3 violation — vault-asset mismatch returns
  `Err(VaultAssetMismatch)`
- TV-BE6: Gate 6 violation — forged governance signature returns
  `Err(InvalidSignature)`
- TV-BE7: Gate 6 sovereign exemption — sovereign role token burn
  succeeds WITHOUT governance signature verification
- TV-BE7a: sovereign single-role equivalence — `governance_pubkey`
  pinned struct field = `sovereign_nonce_namespace(&asset_id)` for
  sovereign path (RFC-0960 v3.6 §3.3 L510); body_hash commitment AND
  NonceRegistry observation key both derive from this single pinned
  value (single-role equivalence, distinct from Mission G TV-SE10a's
  two-role distinction where body_hash uses `[0u8;32]` sentinel and
  NonceRegistry uses the namespace per RFC-0959 v2.8 §3.3 L506-511)
- TV-BE8: gate 7 violation — `nonce_registry.observe(NonceEventKind::Burn,
&pk, &nonce.0) == Err(AlreadyObserved)` returns `Err(Replay)`
- TV-BE9: gate 9 violation — `stored asset_kind != registered
asset_kind` returns `Err(AssetKindMismatch { claimed, registered })`
- TV-BE10: custom `Deserialize` accepts modern envelope
  `{asset_id, amount: DqaEncoding, ...}`
- TV-BE11: custom `Deserialize` rejects legacy envelope
  `{amount_micro_octo_w: i64, cost_asset_id: BRIDGED-hex}` with
  `LegacyFormOnNonOctoWContext { claimed_asset_id }`
- TV-BE12: `consume()` happy-path — audit sink writes succeed; event
  recorded
- TV-BE13: `consume()` AuditSink failure returns
  `AuditSinkFailed { sink_error }`; caller can retry
- TV-BE14: `verify_burn_against_caveat` matches when
  `burn.asset_id == caveat.asset_id`
- TV-BE15: `verify_burn_against_caveat` rejects when
  `burn.asset_id != caveat.asset_id` (RFC-0105 v3.5 §3.13 tri-invariant pair)
- TV-BE16: wire form round-trip — `BorshSerialize → BorshDeserialize`
  preserves all 11 fields
- TV-BE17: audit-batch replay path (RFC-0105 v3.5 §3.13 L669) bypasses
  per-event validate() cache and runs fresh `verify_burn_against_caveat`
  pairwise check on every `(caveat, burn, settlement)` tuple; cache HIT
  must NOT short-circuit the batch-replay path
- TV-BE18: `consume()` 3-sink atomicity — `producer.log.insert` failure
  triggers `audit_sink.compensate(...)` + `nonce_registry.unobserve(...)`;
  returns `AuditSinkFailed { sink_error: LogInsertFailed { sink,
  nonce_rolled_back: true, audit_compensated: true } }`. R7 CRITICAL #3
  cross-sink rollback coverage.
- TV-BE19: `body_hash` stability under perturbation (RFC-0960 v3.6 §9
  Round 3 MED #4 + Round 4 CRITICAL #2) — re-encode + perturb any
  contributing field by 1 byte → body_hash changes
- TV-BE20: nonce replay-by-variation (RFC-0960 v3.6 §9 Round 5 CRIT-1) —
  two BurnEventRef events with identical body fields but distinct
  nonces produce distinct `(event_kind, pk, nonce)` bucket hashes;
  neither collides
- TV-BE21: `validate()` signature re-verify after governance rotation
  (RFC-0960 v3.6 §9 Round 4 LOW #11) — BurnEventRef signed by `pk_old`
  rejected with `Err(InvalidSignature)` when registry's current
  `meta.governance_pubkey` is `Some(pk_new)`; validate() MUST re-run
  Gate 6 against the CURRENT registry state, not the snapshot used at
  `new()` time
- TV-BE22: `validate()` StaleSnapshot (RFC-0960 v3.6 §9 Round 4 LOW #11) —
  BurnEventRef with `registry_snapshot_epoch.0 > current_epoch.0`
  returns `Err(StaleSnapshot { snapshot: <future>, live: <current> })`

## Layer direction (per [[cipherocto-design-principles]])

- `octo-policy` (Layer C specialized node role) — `burn_event.rs` =
  **Layer B additive type hosted in Layer C node crate**; substrate-
  mandated invariant carrier across the audit chain
- `AuditSink` trait at `octo-policy` (Layer C) — produced by
  specialized node; consumers depend on the trait via DI
- No cross-layer inversion; all new types additive (semver-minor)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
cargo test -p octo-policy --lib burn_event
```

## Backward compat

- All new types are additive (no existing crate modification)
- `VaultId`, `AssetId`, `ChainId`, `Dqa`, `Epoch`, `Nonce`,
  `GovernanceSignature`, `AssetRegistry`, `NonceRegistry`,
  `VaultRegistry` are imported from their canonical homes (Mission D +
  pre-existing Layer A frozen types) — NO parallel declarations
- `SettlementRef` deprecated alias retained for one substrate cycle (6
  weeks per RFC-0965 v2.1 §4.1)
- `AuditSink` trait is NEW (additive); consumers without `consume()`
  integration are unaffected

## Cross-references

- RFC-0960 v3.6 §0 — GREENFIELD marker (explicit)
- RFC-0960 v3.6 §1 — motivation (asset-binding + audit-invariant)
- RFC-0960 v3.6 §2.1 — BurnEventRef + BurnEventError substrate (L39-172)
- RFC-0960 v3.6 §2.2 — construction + scale-binding invariant + new() (L173+)
- RFC-0960 v3.6 §2.3 — audit-invariant: verify_burn_against_caveat (RFC-0105 v3.5 §3.13)
- RFC-0960 v3.6 §2.4 — AssetKind cryptographic commitment (kind_tag + blake3)
- RFC-0960 v3.6 §2.5 — error scenario matrix
- RFC-0960 v3.6 §3 — wire form + legacy-form migration
- RFC-0960 v3.6 §3.3 — sovereign-asset signature exemption
- RFC-0105 v3.5 §3.13 — tri-invariant declaration (consumer anchor)
- RFC-0105 v3.5 §3.13 L669 — **audit-batch replay enforcement** (NEW
  v3.5-r6): per-tuple fresh pairwise check
  `(PaymentCaveat, BurnEventRef, SettlementEvent)` in the audit-batch
  replay path; per-event validate() cache MUST NOT be used. Mission F's
  `verify_burn_against_caveat` (TV-BE17) MUST be invoked from the
  batch-replay path with NO caching.
- RFC-0105 v3.5 §5 — cross-RFC error-enum ownership
- Mission D — canonical substrate imports
- Mission E — PaymentCaveat (audit-invariant pair)
- Mission B (`0960-v37-b-event-log-producer-wiring.md`) — `BurnEventProducer`
  consumes Mission F substrate
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) —
  cache table consumer
- [[cipherocto-design-principles]] — Layer B additive-only rule

## Claimant

@unassigned

## Pull Request

#
