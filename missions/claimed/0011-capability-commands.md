---
name: 0011-capability-commands
description: Land capability subcommands (capability list/mint/attenuate) per RFC-0011
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: RFC-0011 author session
  created: 2026-08-27
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0957
    - RFC-0964
    - RFC-0960
    - RFC-0958
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-08-28
spec_cycle_dry_closed: 2026-08-28
---

# 0011-capability-commands — Capability subcommands (capability list/mint/attenuate)

**Status:** Claimed 2026-08-28 (@mmacedoeu). RFC-0011 spec cycle DRY-closed 2026-08-28 (5-round loop-until-DRY closure). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]].
**Substrate:** RFC-0011 §Subcommand Taxonomy (CapabilityAction), RFC-0957 (Macaroon), RFC-0964 (Caveat Envelope), RFC-0960 (Caveat Catalog)
**Parent:** RFC-0011
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — substrate (`OutputEnvelope<T>` + `OctoCliError` + clap root)
- Mission `0011-identity-commands` — `active_signer()` is exposed by `WalletStore` extensions from identity mission; `holder` parameter on `mint`/`attenuate` depends on identity substrate amendments
  **Blocks:** `0011-deprecation-stub-removal` (final integration)

## Status

Claimed 2026-08-28 (mmacedoeu) — spec cycle DRY-closed 2026-08-28

## RFC

RFC-0011 (see rfcs/draft/process/0011-octo-cli-substrate.md §Subcommand Taxonomy)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: mission 1 → 2 → 3 → 4 → 5 per RFC-0011 §Implementation Phases.

## Acceptance Criteria

- [ ] `octo capability list` implemented + unit-tested (TV-CAP1 pass)
- [ ] `octo capability mint` implemented + unit-tested (TV-CAP2, TV-CAP3, TV-CAP6, TV-CAP7, TV-CAP8 pass)
- [ ] `octo capability attenuate` implemented + unit-tested (TV-CAP4, TV-CAP5 pass)
- [ ] `Hex32` newtype implemented + unit-tested
- [ ] Caveat JSON parser implemented + unit-tested (TV-CAP9..15 pass)
- [ ] Attenuation check implemented + unit-tested
- [ ] Filter parsing implemented + unit-tested (TV-CAP16 pass)
- [ ] Dry-run + confirm gates implemented + unit-tested (TV-CAP17, TV-CAP18, tv_cap19_confirm_required pass)
- [ ] Cross-mission AC: capability commands integrate with core mission's envelope + identity mission's `RedactedHex` wrapper
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test --workspace --lib green
- [ ] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011 type               | Sub-step                  | Notes                                                                                                                                                     |
| --------------------------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CapabilitySummary`         | Sub-step 1 (output types) | Layer B; substrate `[ADD]` struct in octo-cap-macaroon per RFC-0011 §Subcommand Taxonomy entry #8 (`cap_id`, `root_id`, `caveats`, `expires_at`)          |
| `CapabilityMintOutput`      | Sub-step 1 (output types) | Layer C/D; `holder_sig` rendered via `RedactedHex`                                                                                                        |
| `CapabilityAttenuateOutput` | Sub-step 1 (output types) | Layer C/D; `narrowed_from` records parent cap_id                                                                                                          |
| `CaveatSummary`             | Sub-step 1 (output types) | Layer B; new `[ADD]` struct in octo-cap-macaroon per RFC-0011 §Subcommand Taxonomy entry #9 (`caveat_type: String`, `constraint_json: serde_json::Value`) |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Capability Subcommands for Rust snippets + clap wiring patterns.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Notes

`Hex32` and `RedactedHex` are distinct newtypes: `Hex32` is a public digest (`body_hash`); `RedactedHex` is a secret wrapper (`holder_sig`, `signature_proof`).

## Scope

Land 3 capability subcommands per RFC-0011 §Subcommand Taxonomy:

1. **`octo capability list`** — `commands/capability.rs::list`. Flags:
   `--json`, `--filter <field=value>` (repeatable). Substrate call:
   `[ADD] octo_cap_macaroon::list_active(filter) -> Result<Vec<CapabilitySummary>, MacaroonError>`.
   Output:
   `CapabilityListOutput { capabilities: Vec<CapabilitySummary> }` where
   `CapabilitySummary { cap_id, root_id, caveats: Vec<CaveatSummary>,
expires_at }` — `remaining_budget_dqa` is DEFERRED to the audit-window
   sub-amendment per Status header amendment chain because computing it
   requires spend-accounting + an active-capability index that
   octo-cap-macaroon explicitly dropped from its storage dependency. v1.0
   returns caveat-only metadata; budget
   remaining is computed lazily by the consumer from the `expires_at` +
   `Budget.amount_dqa - max_uses_spent` projection. Exit 0 / 64. No side
   effects. No `holder_sig` in output (caveat names + IDs only).

2. **`octo capability mint`** — `commands/capability.rs::mint`. Flags:
   `--caveats <json>` (REQUIRED), `--holder <did>` (REQUIRED), `--root
<cap_id>` (optional), `--dry-run`, `--confirm`, `--confirm-acknowledge`
   (atomic pastejacking gate). Substrate call:
   `[ADD] octo_cap_macaroon::mint(root_secret: &[u8;32], holder: &dyn CapabilitySigner, holder_did: &str, caveats: &[Caveat]) -> Result<CapabilityToken, MintError>`
   (thin wrapper that constructs `CapabilityToken::mint(root_secret, holder, holder_did, caveats)`
   per substrate signature). Per R1 substrate alignment review: substrate
   signature uses `&dyn CapabilitySigner` (cross-ref `octo_cap_macaroon::signer`),
   NOT the phantom `HolderKey` type. CLI obtains the signer via
   `[ADD] WalletStore::active_signer() -> Result<Arc<dyn CapabilitySigner>, WalletError>`
   (HSM-backed; RFC-0011 §Subcommand Taxonomy entry #10 cross-ref).
   Output:
   `CapabilityMintOutput { capability_id: Hex32, body_hash: Hex32, caveats:
Vec<CaveatSummary>, holder_sig: RedactedHex }`. Exit 0 / 7 (caveat parse)
   / 8 (invalid combination per RFC-0960 catalog) / 9 (holder not found) /
   10 (attenuation violation) / 11 (signing failed) / 64.

3. **`octo capability attenuate <cap_id>`** —
   `commands/capability.rs::attenuate`. Args: `<cap_id>` REQUIRED. Flags:
   `--caveats <json>` (REQUIRED), `--dry-run`, `--confirm`,
   `--confirm-acknowledge`. Substrate call:
   `[ADD] octo_cap_macaroon::attenuate(parent: &CapabilityToken, caveats: &[Caveat], holder: &dyn CapabilitySigner, catalog: &dyn CapabilityCatalog) -> Result<CapabilityToken, MintError>`
   (per R1 substrate alignment review: substrate `CapabilityToken::attenuate` takes a SINGLE caveat and does NOT re-sign;
   `attenuate_with_signer` is the re-signing variant;
   the CLI form loops over `caveats.iter()` calling
   `parent.attenuate_with_signer(caveat, holder, catalog)` per caveat,
   threading the result forward). Attenuation validation via
   `[ADD] octo_cap_macaroon::set_subsumes(parent_caveats: &[Caveat], child_caveats: &[Caveat]) -> bool` / `set_subsumes_with_registry`
   which is the substrate's existing helper in `crates/octo-cap-macaroon/src/caveat/mod.rs`. Substrate exposes this on caveat slices; CLI calls it with `caveat_set_of(parent)` and `caveat_set_of(child)` helpers that extract the `&[Caveat]` view from each `CapabilityToken`.
   Output:
   `CapabilityAttenuateOutput { child_cap_id, narrowed_from: cap_id,
caveats: Vec<CaveatSummary> }`. Exit 0 / 7 / 10 (widens parent — rejected OR
   parent caveat removed without narrowing replacement) / 12 (parent not
   found) / 64.

### Sub-steps

1. **Output types** — `crates/octo-cli/src/commands/capability.rs`. Three
   `#[derive(Serialize, schemars::JsonSchema)]` structs:
   `CapabilityListOutput`, `CapabilityMintOutput`,
   `CapabilityAttenuateOutput`. Plus reused `CaveatSummary` (imported from
   `octo_cap_macaroon::caveat::CaveatSummary`).

2. **`Hex32` newtype** — `crates/octo-cli/src/output/types.rs` (canonical
   home per RFC-0011 §Hex32 newtype + impl-guide module tree;
   `redact.rs` holds only `RedactedHex` + `OctoCliRedactor`; `Hex32`
   belongs with `OutputEnvelope<T>`). `pub struct Hex32(pub
[u8; 32])` with `Serialize` impl via `#[serde(with = "hex::serde")]`
   that emits lowercase hex. Distinct from `RedactedHex` because `body_hash`
   is NOT secret — it's a public digest.

3. **Caveat JSON parsing** — `commands/capability.rs::parse_caveats(json: &str)
-> Result<Vec<Caveat>, OctoCliError>`. Uses `serde_json::from_str` to
   deserialize to `Caveat`, then re-serializes each parsed caveat via
   `Caveat::canonical_ser` (per substrate signature)
   to verify the round-trippable JSON shape per RFC-0964 envelope. Per R1
   substrate alignment review: substrate does NOT expose `caveat::validate_canonical_form` and
   has no `CatalogError`; RFC-0011 §Subcommand Taxonomy entry #13 codifies
   the [ADD] form `validate_canonical_form(caveats: &[Caveat]) -> Result<(), CatalogError>`
   for a follow-on substrate amendment. Until that amendment lands, CLI
   delegates caveat envelope parsing to `Caveat::canonical_ser` (existing
   substrate helper). Parse error → `OctoCliError::CaveatParse { message }` (exit 7).
   Catalog violation → `OctoCliError::InvalidCaveatCombination { detail }`
   (exit 8). Parser clamps (per RFC-0011 §Caveat Catalog): total bytes ≤ 64 KiB,
   JSON depth ≤ 32, caveat array length ≤ 16, per-caveat payload ≤ 4 KiB.
   Validation error messages pass through the `OctoCliRedactor` so that error
   output never carries offending secret values verbatim.

4. **Attenuation check** — `commands/capability.rs::check_attenuation(parent:
&CapabilityToken, child: &[Caveat]) -> Result<(), OctoCliError>`. Delegates
   to `[ADD] octo_cap_macaroon::set_subsumes(parent_caveats: &[Caveat], child_caveats: &[Caveat]) -> bool` (replaces
   the would-be `is_narrowing` since `set_subsumes` already exists in
   `crates/octo-cap-macaroon/src/caveat/mod.rs`). Substrate exposes this on caveat slices; CLI calls it with `caveat_set_of(parent)` and `caveat_set_of(child)` helpers that extract the `&[Caveat]` view from each `CapabilityToken`.
   Widens → `OctoCliError::AttenuationViolation(message)` (exit 10; tuple-style per RFC §Error Handling).
   Caveat removal is treated as widening: every parent caveat MUST be present
   in the child set OR the child carries a strictly narrower form of the same
   caveat (per RFC-0957 §Attenuation Rules) — drops without replacement are
   rejected.

5. **Dispatch** — `dispatch(action, &Octo) -> Result<(), OctoCliError>`. Matches
   on `CapabilityAction` enum. Each arm parses caveats, builds output, calls
   substrate, renders envelope.

6. **`require_confirm` gate (mutating commands)** — `octo capability mint` and
   `octo capability attenuate` MUST apply `require_confirm` per identity mission
   sub-step 4 (`OctoCliError::ConfirmationRequired { command }`, exit 2 — POSIX
   usage-error convention; declared in the core mission
   `0011-core-output-envelope-redaction` sub-step 1 as a new
   `thiserror` variant). In human mode (`OCTO_HUMAN_MODE=true`), invoking
   either command without both `--confirm` and `--confirm-acknowledge` returns
   `ConfirmationRequired` immediately. Reused, not redefined.

### Caveat catalog consumed (8 caveat variants per RFC-0964)

| Caveat       | Canonical form                                                     | CLI flag pattern                                      |
| ------------ | ------------------------------------------------------------------ | ----------------------------------------------------- |
| Budget       | `{ "type": "amount_max", "value": "<16-byte-DqaEncoding-hex>" }`   | `--caveats '{"type":"amount_max","value":"<hex>"}'`   |
| Expiry       | `{ "type": "before", "value": <u64> }`                             | `--caveats '{"type":"before","value":<u64>}'`         |
| Vesting      | `{ "type": "valid_after", "value": { "not_before_unix": <u64> } }` | `--caveats '{"type":"valid_after","value":{...}}'`    |
| Max uses     | `{ "type": "max_uses", "value": { "count": <u32> } }`              | `--caveats '{"type":"max_uses","value":{...}}'`       |
| Model        | `{ "type": "model", "value": "<model_ref>" }`                      | `--caveats '{"type":"model","value":"..."}'`          |
| Provider     | `{ "type": "provider", "value": [<ProviderId>, ...] }`             | `--caveats '{"type":"provider","value":[...]}'`       |
| Audience     | `{ "type": "audience", "value": "<OverlayIdentity>" }`             | `--caveats '{"type":"audience","value":"..."}'`       |
| Single use   | `{ "type": "max_uses", "value": { "count": 1 } }`                  | `--caveats '{"type":"max_uses","value":{"count":1}}'` |
| Audit window | `{ "type": "audit_window", "value": { "duration_secs": <u64> } }`  | `--caveats '{"type":"audit_window","value":{...}}'`   |

> **Substrate variant alignment (per R1 amendment notes):** the CLI caveat types map to substrate
> `Caveat` enum variants as follows, with serde tags per
> substrate `octo_cap_macaroon::caveat` module:
>
> - `amount_max` (Budget) → `Caveat::AmountMax(Dqa)` — serde tag `"amount_max"`,
>   value is 16-byte `DqaEncoding` (NOT a JSON object with `amount_dqa`/`scale`/`currency`).
>   Currency is a SEPARATE `Caveat::AssetBinding(...)`, not nested.
> - `before` (Expiry) → `Caveat::Before(UnixTimeSecs)` — serde tag `"before"`,
>   bare scalar (NOT `at_unix`)
> - `valid_after` (Vesting) → `Caveat::ValidAfter { not_before_unix: u64 }` —
>   serde tag `"valid_after"`, nested struct
> - `max_uses` (n=1 or n=N) → `Caveat::MaxUses { count: u32 }` — serde tag
>   `"max_uses"`, nested struct (NOT flat `n`); SingleUse is `count=1`, not a
>   dedicated substrate variant
> - `model` → `Caveat::Model(ModelRef)` (single ref; multi-model
>   composition requires multiple `Caveat::Model` entries composed via
>   logical-AND — RFC-0011 limitation documented in §Caveat Catalog)
> - `provider` → `Caveat::Provider(Vec<ProviderId>)` — serde tag `"provider"`,
>   bare array (NOT `allow`)
> - `audience` → `Caveat::Audience(OverlayIdentity)` — serde tag `"audience"`,
>   tuple variant (use `value`, NOT `overlay`)
> - `audit_window` → `Caveat::AuditWindow { duration_secs: u64 }` — serde tag
>   `"audit_window"`, nested struct, `duration_secs` is `u64` (NOT u32)
>
> The CLI does NOT define new caveat variants — it consumes the RFC-0960
> catalog via the substrate's existing 27-variant `Caveat` enum.
> Adding new caveat types requires an RFC-0011 amendment.
>
> The CLI uses `Caveat::canonical_ser` (per substrate signature)
> as the canonical-form source-of-truth rather than the substrate's `serde_json`
> derive output, because `canonical_ser` produces deterministic JSON
> (sorted keys, `preserve_order = false`-safe) per RFC-0126.

> **Scale-binding (Budget caveat):** the substrate `Dqa` type is
> `{ amount: i64, scale: u8 }` (Layer A `octo_determin` substrate, re-exported
> via `octo_policy` per `octo_determin::Dqa` as used in `octo-policy/src/burn_event.rs`).
> The canonical form MUST carry `scale` — without it, a CLI form guessing
> `scale=0` against a parent `scale=6` yields a child worth 1,000,000x the
> intended amount that still passes narrowing. `PaymentCaveat::attenuate`
> in `crates/octo-cap-macaroon/src/caveat/payment.rs` rejects
> `new_budget.scale != self.budget.scale`; the CLI MUST surface the same
> invariant.

## Test Vectors (per RFC-0011 §Test Vectors — capability group)

19 new TV (TV-CAP1..TV-CAP19; tv_cap19_confirm_required is TV-CAP19):

- `tv_cap1_list_empty` — wallet has 0 active capabilities; `octo capability
list` → exit 0; stdout `"capabilities":[]`
- `tv_cap2_mint_success` — `octo capability mint --caveats '{"type":"budget",
"amount_dqa":1000,"scale":6,"currency":"octo-w"}' --holder did:octo:test --confirm
--confirm-acknowledge` → exit 0; stdout contains `capability_id`,
  `body_hash`; `holder_sig` is `[REDACTED:sig]`
- `tv_cap3_mint_bad_caveats` — `octo capability mint --caveats '{"type":"foo"}'
--holder did:octo:test --confirm --confirm-acknowledge` → exit 8; stderr
  `invalid caveat combination`
- `tv_cap4_attenuate_widens_rejected` — child budget > parent → exit 10;
  stderr `attenuation violation`
- `tv_cap5_attenuate_parent_not_found` — `octo capability attenuate
cap_id:01ab.. --caveats '{...}' --confirm --confirm-acknowledge` → exit 12;
  stderr `parent capability not found`
- `tv_cap6_signing_failed` — HSM disconnected during mint → exit 11 — NEW
- `tv_cap7_holder_not_found` — `--holder did:octo:nonexistent` → exit 9 — NEW
- `tv_cap8_caveat_json_syntax_error` — `--caveats '{not_json'` → exit 7 — NEW
- `tv_cap9_caveat_budget` — valid budget caveat → exit 0 — NEW
- `tv_cap10_caveat_before` — valid expiry caveat → exit 0 — NEW
- `tv_cap11_caveat_valid_after` — valid vesting caveat → exit 0 — NEW
- `tv_cap12_caveat_max_uses` — valid max-uses caveat → exit 0 — NEW
- `tv_cap13_caveat_model` — valid model allowlist caveat → exit 0 — NEW
- `tv_cap14_caveat_provider` — valid provider caveat → exit 0 — NEW
- `tv_cap15_caveat_audit_window` — valid audit_window caveat → exit 0 — NEW
- `tv_cap16_filter_parsing` — `--filter field=value` (invalid form) → exit 16
  `InvalidFilter` — NEW
- `tv_cap17_mint_dry_run` — `--dry-run` → exit 0; stdout `"preview_only":true`;
  no new capability persisted — NEW
- `tv_cap18_attenuate_dry_run` — `--dry-run` → exit 0; stdout
  `"preview_only":true`; no child persisted — NEW
- `tv_cap19_confirm_required` — `octo capability mint --caveats '{"type":"budget",
"amount_dqa":1000,"scale":6,"currency":"octo-w"}' --holder did:octo:test` (no
  `--confirm --confirm-acknowledge`, `OCTO_HUMAN_MODE=true`) → exit 2
  (POSIX usage-error); stderr `ConfirmationRequired: --confirm required for
mutating command capability mint in human mode`.

### Cargo deps added

None new. All deps added by mission `0011-core-output-envelope-redaction`.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `CapabilityAction` dispatch + 3 output structs
  - caveat parsing helpers (using existing `Caveat::canonical_ser` until
    the substrate amendment adding `validate_canonical_form` lands)
- `octo-cap-macaroon` (Layer B) — substrate `list_active()`,
  `mint()`, `attenuate()`, `set_subsumes()` (each is the `[ADD]` form per
  RFC-0011 §Subcommand Taxonomy entries #7, #10, #11, #12); also adds
  `CapabilitySummary` + `CaveatSummary` + `CapabilityFilter` types as
  `[ADD]` declarations (entries #8, #9). The `caveat::validate_canonical_form`
  - `CatalogError` form (entry #13) is a future amendment; CLI uses
    `Caveat::canonical_ser` until that lands.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --all-features -- -D warnings
cargo test -p octo-cli --lib --all-features
cargo test -p octo-cli --test capability --all-features
```

## Backward compat

- Additive only: no breaking changes to octo-cap-macaroon substrate API for
  the existing 27-variant `Caveat` enum; all `[ADD]` entries are additive
  per RFC migration etiquette. `CapabilitySummary` + `CaveatSummary` are
  new types; existing public API is unchanged.
- CLI exit codes match RFC-0011 §Exit Code table
- `Hex32` and `RedactedHex` wrappers internal to `octo-cli`
- Caveat JSON form unchanged — same RFC-0964 envelope consumers already use

## Cross-references

- RFC-0011 §Subcommand Taxonomy CapabilityAction table
- RFC-0957 — Macaroon substrate
- RFC-0964 — Caveat envelope canonical form
- RFC-0960 — Caveat catalog root
- (Future work, no RFC filed yet): WhatsApp/Telegram Auth Onboarding redaction
  pattern (applied to `holder_sig`) — see whatsapp/telegram CLI substrate RFC
  when filed
- [[cipherocto-design-principles]] — Layer B stability contract

## Claimant

@unassigned
