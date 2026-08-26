# 0965-v21-payment-caveat-asset-binding-substrate — PaymentCaveat asset binding + scale-binding invariant + NonceRegistry + legacy-form rejection

**Status:** Open
**Substrate:** RFC-0965 v2.1 §2 (PaymentCaveat Specification) + §5 (PermissionKind Co-Bound Caveat)
**Parent:** RFC-0965 v2.1 + RFC-0105 v3.5 §3.13 (tri-invariant consumer)
**Depends on:**

- Mission D (`0105-v35-asset-registry-nonce-registry-substrate.md`) — provides `AssetRegistry`, `AssetError`, `AssetKind`, `MAX_SCALE`, `AssetMetadata`, `NonceRegistry`, `NonceError`, `newtypes::{Nonce, Epoch, GovernanceSignature}`, `sovereign_nonce_namespace`, `verify_governance_signature`, `blake3_hash` canonical substrate imports (per RFC-0965 v2.1 §2.1 L53-57)
- RFC-0965 v2.0 baseline (already landed: `PaymentCaveat`, `PAID_QUERY_CAVEAT_NAME`, `AttenuationError` at `crates/octo-cap-macaroon/src/caveat/payment.rs:40,55,235`)

## Scope

Apply the RFC-0965 v2.1 amendments to `PaymentCaveat` (additive, semver-
minor): add `asset_id: AssetId` field, `registry_snapshot_epoch: Epoch`
field, `nonce: Nonce` field; widen `attenuate` to 4-arg accepting
`new_asset_id: AssetId` + `&dyn AssetRegistry`; add `attenuate_legacy_2arg`
`#[deprecated]` shim; add `AttenuationError::AssetMismatch` /
`ScaleMismatch` / `AssetUnknown` variants; add `PaymentRejectionReason::
ScaleMismatch` / `AssetUnknown` / `StaleSnapshot` / `Replay` /
`LegacyFormOnNonOctoWContext` variants (and re-home
`PaidQueryRejectionReason` as deprecated alias per RFC-0105 v3.5 §5 L688-
694 + RFC-0965 v2.1 §2.3 L218); implement `verify()` and `validate()`
gates with scale-binding + NonceRegistry observation; implement custom
`Deserialize` rejecting legacy `amount_micro_octo_w` form on non-OCTO-W
context (§2.4 L397-415); add `Caveat::Vault(asset_id)` co-bound rule (§5).

### Mission E sub-steps

1. **`PaymentCaveat` field additions** — `crates/octo-cap-macaroon/src/caveat/payment.rs`
   (amend, line 55). Per RFC-0965 v2.1 §2.1 L70-90. Add 3 fields:

   ```rust
   pub struct PaymentCaveat {
       pub caveat_name: String,                  // existing; unchanged
       pub asset_id: AssetId,                    // NEW (L75)
       #[serde(with = "dqa_serde::field")]
       pub budget: Dqa,                          // existing
       pub model: String,                        // existing
       pub expires_at_unix_ms: u64,              // existing
       pub registry_snapshot_epoch: Epoch,       // NEW (L87)
       pub nonce: Nonce,                         // NEW (L89)
   }
   ```

   Discriminator unchanged (`PAID_QUERY_CAVEAT_NAME = "paid-query/v1"` per
   L40 — substrate-canonical name; the discriminator rename in prior draft
   broke 47 call-sites + JSON-RPC + CLI per §2.1 L64-67).

2. **4-arg `attenuate`** — same file. Per §2.2 L113-150. Signature:

   ```rust
   pub fn attenuate(
       &self,
       new_budget: Dqa,
       new_expires_at_unix_ms: u64,
       new_asset_id: AssetId,        // NEW
       registry: &dyn AssetRegistry, // NEW (RFC-0105 §3.1)
   ) -> Result<Self, AttenuationError>
   ```

   Gates: registry resolves `self.asset_id` (else `AssetUnknown`),
   `new_asset_id == self.asset_id` (else `AssetMismatch`), scale-resolution
   check (budget.wire_scale == self.budget.wire_scale ==
   meta.wire_scale, else `ScaleMismatch`), existing budget + expiry gates
   preserved. 47 existing 2-arg call-sites MUST either migrate to 4-arg OR
   use `attenuate_legacy_2arg` shim with `#[allow(deprecated)]`.

3. **`attenuate_legacy_2arg` `#[deprecated]` shim** — same file. Per §1
   L37 + §4.1 "one cycle" = 6 weeks. 2-arg signature carries forward with
   `#[deprecated(note = "use 4-arg attenuate with &dyn AssetRegistry per
RFC-0965 v2.1 §2.2")]`. Migration window per §4.1 L440-442: 6 weeks
   OR 1 major version bump, whichever longer. HARD cutoff, no extensions.

4. **`AttenuationError` variants** — same file. Per §2.2 L98-111. Add 3
   variants to existing enum at L235:

   ```rust
   pub enum AttenuationError {
       BudgetWidened { current: Dqa, proposed: Dqa },  // existing
       ExpiryWidened { current: u64, proposed: u64 },   // existing
       AssetMismatch { current: AssetId, proposed: AssetId },  // NEW (L104)
       ScaleMismatch { current: u8, proposed: u8 },    // NEW (L107)
       AssetUnknown,                                   // NEW (L110)
   }
   ```

5. **`PaymentRejectionReason` enum additions** — `crates/octo-paid-query/src/lib.rs:164`
   (definition) AND `crates/octo-cap-macaroon/src/caveat/mod.rs:17`
   (re-export). Per §2.3 L222-244. Add 5 variants:

   ```rust
   pub enum PaymentRejectionReason {
       BudgetExhausted,                                       // existing
       Expired,                                               // existing
       ModelMismatch,                                         // existing
       CostExceedsBudget,                                     // existing
       ScaleMismatch { caveat_scale: u8, query_cost_scale: u8 },  // NEW (L231)
       AssetUnknown,                                          // NEW (L234)
       StaleSnapshot { snapshot: u64, live: u64 },            // NEW (L237)
       Replay,                                                // NEW (L239)
       LegacyFormOnNonOctoWContext { claimed_asset_id: AssetId },  // NEW (L243)
   }
   ```

   `PaidQueryRejectionReason` alias retained per §2.3 L249:
   `#[deprecated(note = "use PaymentRejectionReason")] pub type
PaidQueryRejectionReason = PaymentRejectionReason;`

6. **`verify()` gates** — `crates/octo-cap-macaroon/src/caveat/payment.rs`
   (amend). Per §2.3 L278-338. 7 gates:
   - Gate 0: `AssetRegistry::metadata(&self.asset_id)` resolves +
     not tombstoned (else `AssetUnknown`)
   - Gate 1: scale-binding — `query_cost.wire_scale == self.budget.wire_scale
== meta.wire_scale` (else `ScaleMismatch`)
   - Gate 2: stale-snapshot detection — `current_epoch.0 <
self.registry_snapshot_epoch.0` (else `StaleSnapshot`)
   - Gate 3: anti-replay — `nonce_registry.observe(&pk, &self.nonce.0)`
     with `pk = meta.governance_pubkey.unwrap_or_else(||
sovereign_nonce_namespace(&self.asset_id))` (else `Replay`); Round 4
     CRITICAL #1 keying by `governance_pubkey` per §2.4 L381
   - Gate 4: expiry (existing, unchanged)
   - Gate 5: model match (existing, unchanged)
   - Gate 6: budget exhaust (existing, unchanged)
   - Gate 7: cost > budget rejects outright via `CostExceedsBudget`
     (previously unreachable per Round 4 IMPORTANT #2; PartialQuery variant
     is separate `verify_partial(...)` entry point per substrate anchor
     `crates/octo-paid-query/src/lib.rs:~280`)

7. **`validate()` post-deserialization invariant check** — same file.
   Per §2.3 L341-372. Same gates 0-3 as `verify()` EXCEPT gate 3 uses
   `observe_readonly` (no nonce marking; `observe-and-mark` reserved for
   `verify()` which commits the spend per L360).

8. **Custom `Deserialize` for legacy-form rejection** — same file. Per
   §2.4 L397-415. Inspects raw envelope via `serde_json::Value` BEFORE
   derived `Deserialize`. Detects legacy `{ amount_micro_octo_w: i64 }`
   key; if present AND `claimed_asset_id != OCTO_W_ASSET_ID`, returns
   `LegacyFormOnNonOctoWContext { claimed_asset_id }` error. Happy path:
   re-serialize through JSON + delegate to derived impl.

9. **`Caveat::Vault(asset_id)` co-bound rule** — `crates/octo-cap-macaroon/src/caveat/mod.rs`
   (amend). Per RFC-0965 v2.1 §5 L444-521. When a `PaymentCaveat`
   co-occurs with a `Caveat::Vault(asset_id)` in the same caveat chain,
   the verifier MUST enforce `vault_asset_id == payment.asset_id`
   (Round 1 CRITICAL #5 mitigation — prevents PermissionKind bypass).
   The verifier lives at `Caveat::verify_chain` (substrate anchor at
   `crates/octo-cap-macaroon/src/caveat/mod.rs`); Mission E adds the
   co-bound check before the per-caveat verify loop.

10. **Wire form migration** — `crates/octo-cap-macaroon/src/caveat/payment.rs`
    (amend `BorshSerialize`/`BorshDeserialize` derives). Per §3 L420-434.
    Borsh field order: `caveat_name | asset_id | budget | model |
expires_at_unix_ms | registry_snapshot_epoch | nonce`. JSON
    equivalent. `asset_id` is a 32-byte hex field. Round 4 IMPORTANT #4
    serde visitor support preserved for legacy-form deserialization
    rejection (Round 1 mitigation close at serde layer).

## Test Vectors

Per RFC-0965 v2.1 §10 (Pending, concrete test vectors).

- TV-PC1: `PaymentCaveat::new(budget, model, expires)` (existing) →
  `attenuate(new_budget, new_expires, new_asset_id, &registry)` with
  `new_asset_id == self.asset_id` returns `Ok(narrowed)`
- TV-PC2: `attenuate` with `new_asset_id != self.asset_id` returns
  `Err(AttenuationError::AssetMismatch { current, proposed })` — USDC
  budget cannot attenuate into OCTO-W
- TV-PC3: `attenuate` with `new_budget.wire_scale != self.budget.wire_scale`
  returns `Err(AttenuationError::ScaleMismatch)`
- TV-PC4: `attenuate` with `registry.metadata(self.asset_id) == Err(...)`
  returns `Err(AttenuationError::AssetUnknown)`
- TV-PC5: `attenuate_legacy_2arg` produces `#[deprecated]` warning
  (clippy `deprecated_semver` lint); builds with `#[allow(deprecated)]`
- TV-PC6: `verify()` with tombstoned asset_id returns
  `Reject { reason: AssetUnknown }`
- TV-PC7: `verify()` with `current_epoch.0 < self.registry_snapshot_epoch.0`
  returns `Reject { reason: StaleSnapshot { snapshot, live } }`
- TV-PC8: `verify()` with `nonce_registry.observe(pk, nonce) ==
Err(AlreadyObserved)` returns `Reject { reason: Replay }`
- TV-PC9: `verify()` sovereign-asset nonce keying —
  `meta.governance_pubkey == None` falls back to
  `sovereign_nonce_namespace(&self.asset_id)`
- TV-PC10: `validate()` with `nonce_registry.observe_readonly(pk, nonce)
== true` returns `Err(Replay)` WITHOUT marking the nonce (subsequent
  `observe` succeeds)
- TV-PC11: custom `Deserialize` accepts modern envelope
  `{asset_id, budget: DqaEncoding, ...}`
- TV-PC12: custom `Deserialize` rejects legacy envelope
  `{amount_micro_octo_w: i64, cost_asset_id: BRIDGED-hex}` with
  `LegacyFormOnNonOctoWContext { claimed_asset_id }`
- TV-PC13: custom `Deserialize` accepts legacy envelope in OCTO-W
  context (`{amount_micro_octo_w: i64}` with no asset_id field defaults
  to OCTO_W_ASSET_ID)
- TV-PC14: `Caveat::verify_chain` with `PaymentCaveat(asset_id=X)` +
  `Caveat::Vault(asset_id=Y)` where `X != Y` rejects with co-bound
  violation (Round 1 CRITICAL #5 mitigation)
- TV-PC15: `Caveat::verify_chain` with matching asset_ids proceeds
  normally
- TV-PC16: wire form round-trip — `BorshSerialize → BorshDeserialize`
  preserves all fields including `asset_id` / `registry_snapshot_epoch`
  / `nonce`

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cap-macaroon` (Layer B frozen substrate) — `PaymentCaveat` field
  additions = **additive, semver-minor** (existing struct extended; no
  field removal); new `PaymentRejectionReason` variants = additive
- `octo-paid-query` (Layer B/C-adjacent) — `PaymentRejectionReason`
  enum additions + `PaidQueryRejectionReason` deprecated alias
- All changes are Layer B-additive; no breaking renames; discriminator
  string `"paid-query/v1"` UNCHANGED
- All consumers of the 47 existing 2-arg call-sites MUST migrate OR
  carry `#[allow(deprecated)]` during the 6-week window

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --features full -- -D warnings
cargo test --workspace --lib
cargo test -p octo-cap-macaroon --lib caveat::payment
cargo test -p octo-paid-query --lib  # PaymentRejectionReason consumer tests
```

## Backward compat

- **Field additions** = source-compatible for code constructing
  `PaymentCaveat` via `PaymentCaveat::new(...)` constructor (Mission E
  updates the constructor to accept new args OR adds a separate
  `new_with_registry(...)` constructor). 47 existing call-sites
  compile with deprecation warnings on `attenuate_legacy_2arg`.
- **Discriminator** = wire-form compatible (string `"paid-query/v1"`
  unchanged; existing serialized caveats still parse — they default
  `asset_id` to OCTO_W_ASSET_ID via legacy-form path)
- **Migration window** = 6 weeks per §4.1; HARD cutoff
- **`PaymentRejectionReason` additions** = additive; downstream
  exhaustive `match` blocks on `PaymentRejectionReason` MUST add the 5
  new variants (compile-time enforced via `#[non_exhaustive]` if
  substrate applies, otherwise clippy `match_wildcard_for_single_variants`
  enforces exhaustive coverage)

## Cross-references

- RFC-0965 v2.1 §1 — motivation (asset-binding bypass close)
- RFC-0965 v2.1 §2.1 — PaymentCaveat substrate definition (L45-90)
- RFC-0965 v2.1 §2.2 — AttenuationError + attenuate signature (L93-150)
- RFC-0965 v2.1 §2.3 — verify() + validate() + PaymentRejectionReason (L215-373)
- RFC-0965 v2.1 §2.4 — NonceRegistry keying + legacy-form rejection (L379-415)
- RFC-0965 v2.1 §3 — wire form (L418-434)
- RFC-0965 v2.1 §4 — discriminator unchanged (`"paid-query/v1"`)
- RFC-0965 v2.1 §4.1 — "one cycle" = 6 weeks (HARD cutoff)
- RFC-0965 v2.1 §5 — PermissionKind Co-Bound Caveat (Caveat::Vault)
- RFC-0105 v3.5 §3.13 — tri-invariant declaration (consumer anchor)
- RFC-0105 v3.5 §5 — cross-RFC error-enum ownership
  (`LegacyFormOnNonOctoWContext` declared per-RFC)
- Mission D (`0105-v35-asset-registry-nonce-registry-substrate.md`) —
  provides canonical substrate imports
- Mission F (`0960-v36-burn-event-dqa-migration-substrate.md`) —
  shares `LegacyFormOnNonOctoWContext` semantics + tri-invariant pair
- Mission G (`0959-v28-settlement-cost-dqa-migration-substrate.md`) —
  shares tri-invariant pair
- Mission A (`0960-v37-a-vault-balance-projection-substrate.md`) —
  `PaymentEventProducer` (Mission B) consumes Mission E substrate
- [[cipherocto-design-principles]] — Layer B additive-only rule

## Claimant

@unassigned

## Pull Request

#
