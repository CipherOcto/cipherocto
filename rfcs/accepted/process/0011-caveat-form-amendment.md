# RFC-0011 Caveat Form Amendment

| Field      | Value                                                        |
| ---------- | ------------------------------------------------------------ |
| Status     | Accepted (2026-10-03)                                        |
| Version    | v1.0                                                         |
| Layer      | B (`octo-cap-macaroon`, RFC-driven, additive)                |
| Parent RFC | RFC-0011 §Caveat Catalog                                     |
| Companion  | `missions/claimed/0011-caveat-form-amendment.md` (paired)    |
|            | `missions/claimed/0011-vault-asset-chain-id-hex.md` (paired) |

**Number note.** This amendment keeps the number 0011 and was accepted into
`process/` without renumbering, by maintainer decision, so a bare `RFC-0011`
is ambiguous in this repository: it names both the parent substrate RFC and
this amendment. The two are told apart by section, not by number, because
`§Caveat Catalog` and `§Hex32 newtype` are sections of the parent and appear
nowhere in this document, while every section of this document is named
`§Caveat Form Amendment`, `§The 32-byte id hex form`, `§Compatibility`,
`§Known deviations`, or `§Acceptance Criteria`. A later renumbering that
separates the two numbers would remove the ambiguity outright and remains the
cleaner fix.

**Layer note.** An earlier revision of this header read `Layer A
(substrate-frozen octo-cap-macaroon)`. That is wrong on both counts, and
correcting it matters rather than tidying: `octo-cap-macaroon` is **not** in
the Layer A frozen list, so labelling it frozen would license skipping the
freeze discipline for every change in this amendment, including the
`canonical_ser` and enum changes recorded below. The crate is Layer B
(RFC-driven, additive). A pre-existing "Layer A frozen" header inside
`src/substrate.rs` carries the same inaccuracy and is tracked separately.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                      |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-10-03 | Accepted. All four implementation phases landed; six adversarial review rounds plus two correction passes reached a dry closure; 23 `tv_cf_*` vectors green under default features, 3 more under `hex-ids`, plus the inverted guide vector. |

Per `docs/BLUEPRINT.md` §Adversarial Review Process, the final review summary
belongs in this section. Rounds 1 through 4 each found real defects and are
recorded under §Status below; rounds 5 and 6 produced no findings and closed
the review. The two correction passes after that closure were not new rounds —
they re-opened the count that the closure had rested on, and the review is
only dry on the tree that carries their corrections:

- **Round 1** found the acceptance table citing a vector that was never
  written, a normative clause the implementation violated, and a second
  32-byte adapter that had drifted from the first. The parallel adapter was
  consolidated onto `hex_id_32` rather than kept alongside it.
- **Round 2** found the hex-emitting asymmetry surviving for most of the set
  the amendment names, and found the review's own first vector to be blind:
  it fed `canonical_ser` output straight back in, which cannot fail for a
  struct-payload arm for reasons unrelated to the claim. The vector was
  rewritten to compare derived-`Serialize` output against canonical output.
- **Round 3** narrowed the clause preamble to the arms the clauses actually
  govern, added the self-describing-format precondition that
  `deserialize_any` implies, and corrected an overstatement in the drift
  audit.
- **Round 4** corrected layer attribution and vector counts that had drifted
  across the two paired missions and the adapter's module documentation.
- **Rounds 5 and 6** found nothing; the dry closure pair.
- **Correction 1** found the count the closure rested on was itself wrong.
  "Four of five" was corrected to **eight of nine**: `AssetBinding`,
  `Factory`, and `PolicyReference` had been missed because the arm set was
  cross-checked against other documents rather than against `canonical_ser`.
  `tv_cf_22` now derives the set from the source.
- **Correction 2** found the derivation could pass vacuously — the source scan
  stops at the first line beginning `};`, which is the end of `canonical_ser`
  only by coincidence of formatting. `tv_cf_23` asserts the scan reaches the
  enum's final variant.

The pattern worth carrying forward: every one of these findings came from
enumerating a set from the code under test rather than from another document
or a hand-maintained list. Cross-document agreement is not evidence of
completeness when the documents descend from the same unswept list.

## Status

**Draft (2026-10-02) — created as Phase 0 of the Caveat Form Alignment plan.
Accepted (2026-10-03), on the maintainer's direct order; the procedural
shortcuts that implies are disclosed below rather than left to be inferred from
the date.** All four implementation phases have landed and the amendment's
normative clauses are implemented:

- Phase 1 — `visit_str` arm on `dqa_serde::field` plus the `hex_id_32` adapter on `Caveat::Vault` (`next 590ddbff`, review fixes `747f15d7`).
- Phase 2 — `hex-ids` feature (off by default) on `AssetId` / `ChainId` / `VaultId` (`next f675efb0`).
- Phase 2 review follow-ups — position-sensitive adapter vectors, the `hex-ids` CI test gate, and the serde-versus-borsh doc correction (`next 4423454e`, `next a5cd3fc8`).
- Phase 3 — `CaveatSummaryView` serialises as `type` / `value` (`next 6b20f480`).

Gate vectors, all green: `tv_cf_01..06`, `tv_cf_13..16`, and `tv_cf_18..26`
in `octo-cap-macaroon` under default features; `tv_cf_07..09` in-crate in the
same crate; `tv_cf_10..12` under `--features hex-ids`; and `tv_cf_17` in
`octo-cli`. The inverted guide vector `guide_canonical_form_is_reparseable` is
green. Measured at v1.0: 312 passing on `octo-cap-macaroon` with default
features, 314 with `--features hex-ids`, 0 failures in both.

**Process deviation — recorded rather than smoothed over.** Promotion was
ordered directly by the maintainer and executed the same day the RFC was
filed. Two process gates in `docs/BLUEPRINT.md` were therefore not satisfied,
and the §Status of this document is not evidence that they were:

1. **§RFC Process step 3 — minimum 7-day feedback window: NOT held.** This RFC
   was filed 2026-10-02 and accepted 2026-10-03. The window did not run; it
   could not have, being shorter than the minimum by six days.
2. **§RFC Process step 2 — submission as a discussion PR: NOT done.** No
   discussion PR was opened. The adversarial review recorded in §Version
   History was machine-driven and adversarial, which is not the community
   discussion step 2 asks for. No objection was recorded, but the absence of
   objection is not the same as a discussion having happened.
3. **§RFC Acceptance Process — "At least 2 maintainer approvals": NOT
   evidenced here.** This document records one maintainer instruction. Any
   second approval exists outside this file, and the gap is this record's, not
   necessarily the process's.

§Human vs Agent Roles assigns "Accept RFCs" to the human column and withholds it
from the agent column. The accept decision was therefore the maintainer's; this
promotion is the mechanical execution of that decision, and the deviation
above is disclosed so the artifact does not read as a clean procedural
history. A maintainer wanting the window honoured should re-date the
acceptance.

**Both mission-side gates are now resolved.** The paired missions remain in
`missions/claimed/`, which is their correct state. Earlier revisions of this
section recorded two blocking gates; both are closed:

- The post-implementation multi-round adversarial review closure pair (R-DRY)
  fired: rounds 5 and 6 of the review produced no findings.
- The RFC-side gate — a minimum 7-day feedback window — was waived, not
  satisfied. See the process deviation above. The waiver is the reason this
  gate is listed as closed rather than pending.

The Caveat Form Alignment plan's own promotion criterion (Phases 1-3 landed
plus the gate vectors green) remains narrower than `docs/BLUEPRINT.md` and did
not by itself authorise this promotion.

## Summary

RFC-0011 §Caveat Catalog is reaffirmed and extended to close the canonical-form
asymmetry between `Caveat::canonical_ser` and the `--caveats` parser. The
canonical hex form emitted by `canonical_ser` for the `AmountMax` arm and the
`Vault` arm MUST be accepted by the same `Caveat` enum's `Deserialize` impl.
The asymmetry is a conformance drift and the corrected form is the
canonical-hex form, not the byte-array form.

**Scope, and how far it now reaches.** The amendment closes the gap for the
`AmountMax` and `Vault` arms named above and for the shared adapter discipline.
It originally stopped there, leaving the rest of the hex-emitting set reading
only the byte array; as of 2026-10-03 that is fixed too, and the full
fixed-width set round-trips except for one arm. Nine arms carry a fixed-width
id. Eight now accept their own canonical form; `payment` does not, because
`canonical_ser` projects 4 of `PaymentCaveat`'s 7 fields and the fix for that
is a different change with a different cost. It is recorded in §Known
deviations 1, which is also closed as of 2026-10-03. No fixed-width-id arm
is asymmetric now, and `tv_cf_21` asserts that as a standing property rather
than a one-time milestone.

That paragraph previously read "four further arms emit hex and still read only
the byte array". Four was the count from the first review pass, and it was
wrong — the correct as-found figure was eight of nine, because the enumeration
had been cross-checked against other documents rather than against
`canonical_ser`. §Known deviations 3 carries the full history. It is recorded
here because a Summary that understated the gap is how the gap survived four
review rounds unnoticed.

## Context

The drift audit `docs/audits/2026-09-30-open-limitations-drift-audit.md` §9
records that the `AmountMax` / `Payment` scale-dropping defect is closed at
the canonical-form level; the remaining piece is the round-trip.

## Caveat Form Amendment

Clauses 1 to 3 below are normative for the caveat payload widths this
amendment governs: the 16-byte `DqaEncoding` and the 32-byte id. They are
**not** normative for every arm whose canonical form `canonical_ser` happens
to emit, which is a much larger set. An earlier revision opened this section by
saying the clauses were "normative for any `Caveat` enum whose canonical form
is emitted by `Caveat::canonical_ser`", which on its face swept in the
`Raw` variable-length blob, the `Permission` info-string, and the four arms
recorded in §Known deviations — arms this amendment does not govern. The
preamble is narrowed here to match what the clauses actually require.

1. **Round-trip MUST hold for every caveat arm whose canonical form is
   lossless.** The canonical hex form emitted by `Caveat::canonical_ser` for
   the `AmountMax` arm and the `Vault` arm MUST be accepted by the same
   `Caveat` enum's `Deserialize` impl. The asymmetry — emitter produces hex,
   input form accepts only a byte-array — is a conformance drift; the
   corrected form is the canonical-hex form, not the byte-array form.

   This clause was scoped to the lossless arms on purpose while the `Payment`
   arm was one of the exceptions; it is not any more. The `Payment` projection
   carried four of seven fields until 2026-10-03 and is now lossless, so every
   fixed-width-id arm round-trips and `tv_cf_21` asserts it. An earlier
   revision of this clause named the `Payment.budget` arm here, which asserted
   a round trip that did not exist and had no vector behind it.

2. **Substrate-owned serialization paths are normative.**
   - `#[serde(with = "dqa_serde::field")]` is the substrate-owned serialization
     path for the 16-byte `DqaEncoding` payload carried by `AmountMax` and
     `Payment.budget`.
   - `#[serde(with = "hex_id_32")]` is the substrate-owned serialization path
     for any 32-byte id-bearing field. The first field to adopt the adapter is
     `Caveat::Vault([u8; 32])`. Any future caveat payload that is 32-byte
     id-bearing inherits the hex form.

   Every 32-byte id-bearing field routes through this one adapter. An earlier
   revision of this clause was false as written: `PaymentCaveat::asset_id` and
   `PaymentCaveat::nonce` used a parallel private adapter that accepted the hex
   form only, so the same canonical form was accepted by one 32-byte field and
   rejected by its siblings. That adapter now delegates to `hex_id_32`, and
   `tv_cf_19` pins the parity so a second implementation cannot reappear.

3. **Every 32-byte id-bearing field accepts both forms on input.** This is a
   per-field property, not a per-crate one: a field that adopts the hex form
   and rejects the legacy array form is non-conformant even if a sibling field
   accepts both.
   - Legacy 32-element array form: preserved for migration. A serialized
     `{ "type": "vault", "value": [ … 32 numbers … ] }` envelope continues to
     parse.
   - Canonical 64-hex form: preferred. A serialized
     `{ "type": "vault", "value": "<64-char hex>" }` envelope parses to the
     same value. Lowercase is the canonical spelling; uppercase hex is
     accepted on input as deliberate leniency (`tv_cf_16`) and is never
     emitted.

4. **Default-feature stability for the newtypes, with a stated boundary.**
   `AssetId`, `ChainId`, and `VaultId` keep their derived 32-element
   byte-array `Serialize` form under default features. The hex form is gated
   behind the `hex-ids` feature flag (off by default) so any external consumer
   that already pins the byte-array form is not broken by this amendment.

   The boundary matters and an earlier revision of this clause did not state
   it: this is about the **bare newtype's** derived impl, not about every
   appearance of the type. A field carrying a `#[serde(with = ...)]` adapter
   ignores the newtype's own impl entirely, so `PaymentCaveat::asset_id`
   serialises as hex under default features even though a bare `AssetId` in
   the same build serialises as a byte array. Default-feature stability for
   the bare type is therefore not a statement that the type is
   byte-array-shaped everywhere, and `tv_cf_13` tests the bare type for that
   reason.

## The 32-byte id hex form

The `hex_id_32` adapter is the substrate-owned path for every 32-byte
id-bearing field, not only `Caveat::Vault`; the section title in earlier
revisions of this document said `Vault` and understated the scope after the
parallel adapter was folded in. The adapter accepts:

- A 64-char hex string (canonical, preferred), via `visit_str`.
- A 32-element byte array (legacy, preserved for migration), via `visit_seq`.

A `deserialize_str` / `deserialize_seq` dispatch would force the consumer to
commit to one form via a tag; the cleaner substrate-owned solution is to accept
both forms transparently. The migration-window contract is that the adapter's
`deserialize_any` is the documented public surface of the path.

**Precondition: the format must be self-describing.** `deserialize_any` asks
the format to describe the incoming value, which JSON and MessagePack do and
bincode and postcard do not. A `Caveat` carrying a 32-byte id therefore cannot
be read through a non-self-describing serde format once this adapter is on the
path, and the failure is a runtime error rather than a compile error. Nothing in
the workspace hits this today — `octo-cap-macaroon` only ever moves a `Caveat`
through `serde_json`, and the workspace's bincode users are in other crates —
so this is a constraint on future work rather than a live defect. It is stated
because the alternative is a consumer adopting this adapter, then discovering
the format limit through a runtime error on a wire path. A future
non-self-describing consumer needs a tag or an out-of-band form, not this
adapter.

## Compatibility

1. **`CaveatSummaryView` wire-format break.** The operator-visible
   `CaveatSummaryView` projected the canonical form using pre-amendment field
   names. This amendment renames the wire form to `{ type, value }` to mirror
   the canonical `Caveat` envelope. The struct's Rust field identifiers
   (`kind`, `body`) are unchanged — only the `#[serde(rename)]` wire form
   changes.

   Any external consumer of `CaveatSummaryView` that reads `{ kind, body }`
   MUST migrate to `{ type, value }`. The only in-repo consumer was the
   operator-guide `jq` filter, updated in Phase 3. Nothing else read the old
   spelling: the full CLI lib suite and every integration bin stayed green
   across the rename with no test change, which is precisely why the guide
   needed correcting by hand. An earlier revision of this section also named
   "the test fixtures that construct `CaveatSummaryView` literals in JSON" as
   a consumer to update; no such fixture exists, in JSON or otherwise. Any
   external Rust consumer that destructures the struct is unaffected (struct
   field identifiers unchanged).

2. **`Caveat::Vault` input-form acceptance is a positive change.** Consumers
   that currently pass the 32-element array form continue to parse; consumers
   that pass the 64-hex form now also parse. There is no consumer that breaks
   from the `hex_id_32` adapter landing.

3. **`hex-ids` feature flag is off by default.** Consumers that did not opt
   into the feature see no wire-format change for `AssetId`, `ChainId`,
   `VaultId`. Consumers that opt in accept the 64-hex form on those types.

## Known deviations

Recorded here rather than left implicit, because each one is a place where
this amendment does **not** deliver the uniform story its Summary implies.

### 1. `Caveat::Payment`'s canonical form was a partial projection — **CLOSED 2026-10-03**

> **Status: closed.** `canonical_ser` now projects all seven fields and the
> `Payment` arm round-trips. There was never a design reason for the drop — see
> the analysis below, which is kept because the mechanism is the cautionary
> part. `tv_cf_18` now pins losslessness and `tv_cf_25` guards completeness
> against the struct's own source.

`canonical_ser` used to emit four of `PaymentCaveat`'s seven fields —
`caveat_name`, `budget`, `model`, `expires_at_unix_ms` — and dropped
`asset_id`, `registry_snapshot_epoch`, and `nonce`. Feeding the canonical form
back into the `Caveat` enum's `Deserialize` impl therefore **failed** with a
missing-field error, not merely a value mismatch. `AmountMax` and `Vault` carry
a single payload, so for them the canonical form was always lossless.

**There was no reason. It was a sixteen-day maintenance drift, and the shape
of it is the lesson.**

- The projection was written on **2026-08-10** (`5cda2eb7`, the `PaymentCaveat`
  migration). At that moment those four fields _were_ the whole struct, so the
  projection was lossless and correct.
- `asset_id`, `registry_snapshot_epoch`, and `nonce` were added on
  **2026-08-26** (`726960ea`, "PaymentCaveat 8-gate verify + `Caveat::AssetBinding`
  - substrate canonical-home"). That commit added a new caveat arm and the
    asset-binding enforcement, and did not touch the `Caveat::Payment` match arm
    in `canonical_ser`.
- Nothing failed, because a hand-written field list does not break when a struct
  gains a field. There was no exhaustive-field check anywhere.

**The four that survived were the four inert fields, and the three that were
dropped were the three that constrain the capability:**

- `asset_id` — the asset binding that stops a USDC budget being spent against
  an OCTO-W query.
- `registry_snapshot_epoch` — the staleness guard.
- `nonce` — the anti-replay token.

**Two consequences, and the second is the one that mattered.** `canonical_ser`
feeds `caveat_body_hash`, which has two consumers:

1. It contributes to the capability id, so two payment caveats differing only in
   asset, epoch, or nonce hashed identically. Enforcement still reads the real
   fields, so this is an identifier collision rather than a signature bypass.
2. It backs the `--dry-run` pastejacking echo in `octo-cli capability mint`
   (RFC-0011 §Subcommand Taxonomy entry #13), whose entire purpose is to give
   the operator a last-mile view of what they are about to authorize. The echo
   was hiding the asset binding and the anti-replay nonce from the one surface
   that exists to display them.

**What stopped anyone noticing for sixteen days: a vector that pinned the
defect.** `tv_cf_18` asserted that reparsing `Payment`'s canonical form FAILED,
and asserted that the three field names were ABSENT from the output. Written to
document a known deviation, it read as a considered decision and actively
enforced the drift. A vector that pins a defect is not neutral, and this is the
clearest instance of that in the amendment's history.

**What the fix does NOT change.** The budget still round-trips as a 16-byte hex
`DqaEncoding` carrying `scale`, which `PaymentCaveat::attenuate` Gate 3 depends
on. Two details were found and rejected while implementing it:

- The projection could not be replaced wholesale with
  `serde_json::to_value(p)`. The `budget` field's adapter
  (`dqa_serde::field`) serializes via `serialize_bytes`, which
  `serde_json::Value` renders as an **array of numbers** rather than the
  canonical hex string, so that substitution broke
  `canonical_payment_budget_carries_scale`. The projection stays explicit.
- The round-trip is not a `PartialEq` identity. `Dqa` has several
  representations of one value and `DqaEncoding::from_dqa` canonicalizes before
  encoding — "CRITICAL: Canonicalizes before encoding to ensure deterministic
  Merkle hashes", in the frozen Layer A `determin` crate. So
  `Dqa { value: 1_000_000, scale: 6 }` re-reads as `Dqa { value: 1, scale: 0 }`:
  the same economic value in canonical form. `tv_cf_18` therefore asserts the
  round-trip is a **fixed point** of `canonical_ser` rather than a struct
  equality, which is the honest property and is strictly stronger than a
  parse-success check.

**Cost: the id changes, and nothing else.** Every payment capability's id
changes, because `caveat_body_hash` changes. There is no migration to perform:
the project is pre-production, no capability has been minted against the old
projection, and nothing is stored that would need re-minting. The only
stakeholders are the crates in this workspace, all of which are recompiled
together.

This is why the change was straightforward rather than a wire-format
deliberation. Every section of this amendment that described a "wire-format
cost" was reasoning about a constraint that does not bind here, and the honest
reading of the `Payment` projection is the one in §Known deviations 1: it was
sixteen days of unmaintained drift, not a trade-off anyone weighed.

**Pre-production context, recorded so it is not re-litigated.** Capability ids
and canonical bytes carry no compatibility obligation in this repository while
no capability exists outside a test or a local run. A future RFC that proposes
a wire-format change SHOULD NOT cite "breaks existing consumers" as a cost
without first establishing that such consumers exist.

### 2. `hex-ids` is a substrate-local opt-in, not a coordinated switch

The flag is declared by `octo-cap-macaroon` and enabled by nothing else in the
workspace. A producer that enables it emits hex for the three newtypes; a
consumer that has not enabled it expects the byte-array form and will reject
the input. The flag is therefore a migration affordance for a
coordinated rollout, not a safe default. Clause 4 promises only that
non-opted-in consumers see no change, which is true and is not the same
promise as interoperability.

### 3. Eight of the nine hex-rendering arms still reject their own hex — **CLOSED for seven of the eight, 2026-10-03**

> **Status: closed for seven arms.** The gap this section described existed
> while the amendment was in review and has since been implemented. `vault`
> already worked; `invocation_hash_bind`, `ask_binding`, `wrapped_only`,
> `redemption_context`, `asset_binding`, `factory`, and `policy_reference` now
> accept their own canonical form. `payment` is the **one remaining** asymmetric
> arm, and its blocker was never this section's — see §Known deviations 1.
> `tv_cf_20` and `tv_cf_21` were updated in the same commit, and `tv_cf_24` was
> added to pin what the fix owes. The analysis below is retained because it is
> the record of how the gap was found and why the count was wrong twice.

`canonical_ser` renders bytes through `hex::encode` in **eleven** arms: eight
carry a 32-byte id, two carry a 16-byte `Dqa` payload (`amount_max` and
`payment`), and one (`raw`) carries a variable-length blob. `amount_max` is the
one that round-trips, because Phase 1 added the `visit_str` arm to
`dqa_serde::field`. `raw` is outside the 32-byte question entirely.

Of the nine arms carrying a fixed-width id, the amendment gave the hex input
path to **one**. The table below is the state **as found**, not as shipped:

| Arm                    | Renders 32-byte id as hex | Accepted its own canonical form (as found) | Now              |
| ---------------------- | ------------------------- | ------------------------------------------ | ---------------- |
| `vault`                | yes                       | **yes**                                    | yes              |
| `invocation_hash_bind` | yes                       | no                                         | **yes**          |
| `ask_binding`          | yes                       | no                                         | **yes**          |
| `wrapped_only`         | yes                       | no                                         | **yes**          |
| `redemption_context`   | yes                       | no                                         | **yes**          |
| `asset_binding`        | yes                       | no                                         | **yes**          |
| `factory`              | yes (`target_vault_id`)   | no                                         | **yes**          |
| `policy_reference`     | yes (`policy_id`)         | no                                         | **yes**          |
| `payment`              | yes (16-byte `budget`)    | no                                         | no — deviation 1 |

So eight of nine as found, one of nine remaining. The other eight still exhibited the exact emitter/input
asymmetry this amendment exists to remove. The drift audit cited in §Context
already named six of them by name — "Six other arms render 32-byte ids as hex:
`Vault`, `AskBinding`, `WrappedOnly`, `InvocationHashBind`, `RedemptionContext`,
`AssetBinding`" — plus the `Factory` and `PolicyReference` struct fields. The
set was enumerated in prose and, until this review's second pass, nowhere in
code.

**This table was wrong twice and both errors are instructive.** The first
revision of this section listed five arms and missed `asset_binding`, `factory`,
and `policy_reference`, because the enumeration was checked against other
documents rather than against `canonical_ser` itself. Cross-document agreement
is not evidence of completeness when the documents all descend from the same
unswept list. `tv_cf_22` now derives the set mechanically from the
`canonical_ser` source and compares it against the vector's literal, so an arm
that renders hex cannot be added without failing a test.

That check was wrong twice, both times in the direction of reporting agreement
it had not earned, and both are recorded in the vector:

- It first dropped every single-line match arm, including `Vault`.
- Its scan bound is the first line beginning `};` while an arm is open, which
  today is the end of `canonical_ser` by coincidence of formatting rather than
  by construction. An ordinary nested block inside any early arm would stop the
  scan early, and the comparison would then hold for a short list. `tv_cf_23`
  guards exactly that, by asserting the scan reaches the enum's final variant.

**What the fix cost, and why it was left open while it was open.** Adopting the
adapter on an arm widens that arm's _input_ acceptance to both forms, which is
free and backward compatible. It also changes the arm's _emitted_ form from a
32-element array to a hex string, which breaks every consumer that reads the
array form. Input widening is free; output flipping is not, and that is a
wire-format decision belonging to the owner of the capability wire format.

The mechanical cost is one attribute per arm. `InvocationHashBind` and
`AskBinding` use the `Blake3` and `AskId` spellings, but both are type ALIASES
for `[u8; 32]` rather than newtypes, so `#[serde(with = "hex_id_32")]` lines up
exactly as it does for `Vault`. An earlier revision of this section claimed the
adapter signature did not fit those two arms; that was checked and is wrong —
clippy's "useless conversion" lint on the `.into()` calls is what surfaced it.

**What implementing it actually changed, beyond the emitted form.** Two
consequences that a reader of the table above would not predict:

- **Every capability id carrying a `wrapped_only`, `redemption_context`, or
  `asset_binding` caveat changes.** `canonical_ser` feeds `caveat_body_hash`, so
  correcting the envelope changes the digest preimage. Nothing needs to be
  re-minted: the project is pre-production and no capability was minted under
  the previous projection. The id change is real, the migration is not.
- **Previously-emitted canonical bytes for those three arms no longer parse**,
  because the old bare-string form is not the object the input form wants. Only
  in-repo producers of those bytes exist, and all of them are updated in the
  same change, so the breakage is confined to the workspace's own test corpus.

### 4. The envelope shape is a SECOND, independent cause of the asymmetry

`Caveat` is adjacently tagged (`tag = "type"`, `content = "value"`). A
struct-payload arm therefore expects an OBJECT under `value`, while
`canonical_ser` writes a bare hex STRING there. `wrapped_only`,
`redemption_context`, `asset_binding`, `factory`, `policy_reference`, and
`payment` are all affected. Their canonical forms cannot be parsed by the
CLI's own `--caveats` parser **regardless of encoding**.

### 4. The envelope shape is a SECOND, independent cause of the asymmetry — **CLOSED for the three arms that emitted a bare string, 2026-10-03**

> **Status: closed.** `canonical_ser` now emits an object under `value` for
> `wrapped_only`, `redemption_context`, and `asset_binding`. The other three
> named below — `factory`, `policy_reference`, `payment` — already emitted an
> object and needed only the adapter. The two causes below are retained as the
> record of why this was a separate fix and not a consequence of the other one.

`Caveat` is adjacently tagged (`tag = "type"`, `content = "value"`). A
struct-payload arm therefore expects an OBJECT under `value`, while
`canonical_ser` wrote a bare hex STRING there for `wrapped_only`,
`redemption_context`, and `asset_binding`. Their canonical forms could not be
parsed by the CLI's own `--caveats` parser **regardless of encoding**.
`factory`, `policy_reference`, and `payment` emitted an object already, so for
those the envelope was never the blocker.

This is not the same defect as the hex-versus-array one, and fixing one does
not fix the other. That was measured, not assumed: adding
`#[serde(with = "hex_id_32")]` to `AssetBinding::asset_id` on its own leaves
`tv_cf_20` green, because the envelope still carries a string where the input
form wants an object. Only when the adapter **and** the envelope shape are both
corrected does the arm's canonical form start parsing, and at that point
`tv_cf_20` and `tv_cf_21` fail and demand the deviation list be updated. So
each of these arms needs two changes, not one, and any plan that fixes only the
adapter will appear to succeed and change nothing. That prediction held: the
three envelope arms were the three that stayed broken after the adapter landed
on all eight.

Correcting the envelope shape means the encoder emits
`{"type": "asset_binding", "value": {"asset_id": "…"}}` rather than
`{"type": "asset_binding", "value": "…"}`, which changes `caveat_body_hash` and
therefore the capability id. Same cost as §3, and for the same reason.

### How the vectors measure this

`tv_cf_20` compares each arm's own **derived** `Serialize` output, which always
round-trips, against its **canonical** output, which is the hex-rendering one.
An arm is asymmetric when the first parses and the second does not. Measuring
against the derived form avoids hand-built envelopes entirely, and picks up the
shape disagreement as a side effect instead of needing a separate check for it.

An earlier draft of the vector instead fed `canonical_ser` output straight back
in. That measures nothing useful, because for a struct-payload arm the input
can never parse for reasons unrelated to hex-versus-array. Adopting the
adapter on `wrapped_only` left that version green. A check that cannot fail for
the claim it names is worse than none, because it reads as evidence.

`tv_cf_24` was added when the fix landed. `tv_cf_20` and `tv_cf_21` pin the
accept/reject SET, but every assertion in them is satisfied by "it parses"; none
of them would notice if the adapter had been narrowed to accept hex ONLY, which
would break every existing consumer sending a 32-element array while leaving all
the hex-path tests green. `tv_cf_24` pins the legacy array form explicitly, and
its id is position-distinct (`0x00..=0x1f`) so that a byte-order defect in the
hex path is visible. Four mutations were run against it — refusing the array
form, reversing the bytes on serialize, reversing them on deserialize, and
reverting one envelope to the bare string — and each was caught by the assertion
that names the corresponding half of the contract.

### 5. `Caveat::Permission` emitted the HMAC info string — **CLOSED 2026-10-03**

> **Status: closed.** `canonical_ser` now emits the short tag and the arm
> round-trips. `tv_cf_26` pins every `PermissionKind` variant, deriving the
> variant set from this crate's source.

`canonical_ser` emitted `PermissionKind::as_str()` for this arm, which is the
full HMAC info string — `"cipherocto/cap/v1/permission/vault_mutation"`. The
input form derives `Serialize` with `rename_all = "snake_case"` and therefore
expects `"vault_mutation"`. Those are two different _vocabularies_, not two
encodings of one value, so the canonical form could not be re-read at all:

```
{"type":"permission","value":"cipherocto/cap/v1/permission/vault_mutation"}
```

serde rejects it with `unknown variant`, listing the five tags it does accept.

**Why no vector caught it.** This is a different defect CLASS from everything
else in this amendment. The other arms failed because of the _encoding_ of a
32-byte id or the _shape_ of the envelope, so the vectors that found them all
enumerate arms whose `canonical_ser` renders bytes through `hex::encode`.
`permission` does not render hex, so it was outside the set every one of them
enumerates. A fix to all eight hex arms left this one still broken, and the
suite stayed green — which is the same failure mode as the missed arms in
§Known deviations 3, one level up: a set nobody enumerated.

`PermissionKind::as_str()` was not wrong in itself. It is the `info` parameter
to HMAC-BLAKE3, and `caveat_name_stable` deliberately pins the distinction
between the HMAC string and the wire tag. The defect was using it on the wire.

The arm now emits the derived form, so `rename_all` covers any variant added
later. `tv_cf_26` still enumerates the variants from source and fails on a new
one, because "the encoder is right by construction" is precisely the claim
that was false here.

## Acceptance Criteria

The amendment is promoted to `Accepted` once every vector below passes. The
list spans three crates, because the amendment's clauses do: the substrate
carries the encoding rules, and the CLI carries the operator-visible
projection. An earlier revision scoped the whole table to `octo-cap-macaroon`
while also listing a CLI vector, which no gate could satisfy.

An earlier revision of this table also cited `tv_cf_14` for the
`CaveatSummaryView` projection. That name belonged to a vector that was never
written, and the number it occupies now belongs to a different vector; the
numbering is recorded per vector below so a rename cannot silently detach a
row from its test.

### `octo-cap-macaroon` — encoding substrate

| Vector                                                                        | Crate / feature      | Property locked                                                                                                                                                                                                                                                                              |
| ----------------------------------------------------------------------------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tv_cf_01_dqa_hex_round_trip`                                                 | default              | `dqa_serde::field::deserialize` accepts a 64-hex string and decodes to the same `Dqa` as the 16-byte byte-array form.                                                                                                                                                                        |
| `tv_cf_02_dqa_hex_rejects_odd_length`                                         | default              | odd-length hex strings reject.                                                                                                                                                                                                                                                               |
| `tv_cf_03_dqa_hex_rejects_non_hex_chars`                                      | default              | non-hex characters reject.                                                                                                                                                                                                                                                                   |
| `tv_cf_04_hex_id_32_round_trip_string`                                        | default              | `hex_id_32` adapter round-trips through a 64-char hex string.                                                                                                                                                                                                                                |
| `tv_cf_05_hex_id_32_accepts_legacy_array_form`                                | default              | `hex_id_32` adapter parses the legacy 32-element array form.                                                                                                                                                                                                                                 |
| `tv_cf_06_hex_id_32_rejects_short_hex`                                        | default              | short hex strings reject.                                                                                                                                                                                                                                                                    |
| `tv_cf_07_canonical_amount_max_reparses_through_input_form`                   | default (in-crate)   | `Caveat::AmountMax`'s `canonical_ser` output re-parses through `#[serde(with = "dqa_serde::field")]`.                                                                                                                                                                                        |
| `tv_cf_08_canonical_vault_reparses_through_input_form`                        | default (in-crate)   | `Caveat::Vault`'s `canonical_ser` output re-parses through `#[serde(with = "hex_id_32")]`.                                                                                                                                                                                                   |
| `tv_cf_09_legacy_vault_array_form_still_parses`                               | default (in-crate)   | legacy 32-element array form on `Caveat::Vault` continues to parse.                                                                                                                                                                                                                          |
| `tv_cf_14_hex_id_32_preserves_byte_positions`                                 | default              | the adapter encodes byte _i_ at hex position _i_, so no permutation is invisible.                                                                                                                                                                                                            |
| `tv_cf_15_hex_id_32_rejects_short_array_form`                                 | default              | a 31- or 33-element array is rejected rather than truncated or panicked into acceptance.                                                                                                                                                                                                     |
| `tv_cf_16_hex_id_32_accepts_uppercase_hex_input`                              | default              | uppercase hex is accepted on input as documented leniency, and is never the emitted form.                                                                                                                                                                                                    |
| `tv_cf_18_payment_canonical_form_is_lossless`                                 | default              | the `Payment` canonical form reparses, and re-canonicalizing the reparsed caveat reproduces identical bytes (a fixed point, not a `PartialEq` identity, because `Dqa` canonicalizes at the wire). It used to assert the OPPOSITE, which is how a sixteen-day projection drift stayed frozen. |
| `tv_cf_25_payment_canonical_projection_covers_every_declared_field`           | default              | every field `PaymentCaveat` declares appears in the canonical projection, with the field set read out of the struct's own source. This is the guard that makes the hand-written list safe.                                                                                                   |
| `tv_cf_19_all_32byte_id_fields_accept_both_forms`                             | default              | every 32-byte id-bearing field accepts hex and legacy array alike, checked through its owning envelope.                                                                                                                                                                                      |
| `tv_cf_20_hex_rendering_arm_acceptance_set_is_explicit`                       | default              | the hex-emitting arm set is enumerated, and each arm's hex-input acceptance is stated rather than assumed. True for every fixed-width-id arm except `payment`.                                                                                                                               |
| `tv_cf_21_no_fixed_width_arm_rejects_its_own_canonical_form`                  | default              | NO fixed-width-id arm rejects its own canonical form. The count behind this went 8, then 1, then 0, each transition landing with its wire-format decision.                                                                                                                                   |
| `tv_cf_22_hex_rendering_set_matches_canonical_ser_source`                     | default              | the hex-rendering arm set is derived from the `canonical_ser` source and compared against the literal, so a new hex-emitting arm cannot be added silently.                                                                                                                                   |
| `tv_cf_23_hex_scan_reaches_the_final_caveat_variant`                          | default              | the `canonical_ser` scan reaches the enum's final variant, so a truncated scan cannot make `tv_cf_22` agree with a short list.                                                                                                                                                               |
| `tv_cf_24_fixed_width_arms_round_trip_and_still_accept_the_legacy_array_form` | default              | every fixed-width-id arm accepts its own canonical form AND still accepts the legacy 32-element array form, with byte order preserved in both directions. This is the half `tv_cf_20` / `tv_cf_21` cannot see.                                                                               |
| `tv_cf_13_default_newtype_serde_is_byte_array`                                | default (bare types) | under default features, a bare `AssetId` / `ChainId` / `VaultId` serialises as a 32-element byte array.                                                                                                                                                                                      |

### `octo-cap-macaroon` — `hex-ids` feature

| Vector                             | Crate / feature | Property locked                                                   |
| ---------------------------------- | --------------- | ----------------------------------------------------------------- |
| `tv_cf_10_asset_id_hex_round_trip` | `hex-ids`       | under `--features hex-ids`, `AssetId` round-trips through 64-hex. |
| `tv_cf_11_chain_id_hex_round_trip` | `hex-ids`       | under `--features hex-ids`, `ChainId` round-trips through 64-hex. |
| `tv_cf_12_vault_id_hex_round_trip` | `hex-ids`       | under `--features hex-ids`, `VaultId` round-trips through 64-hex. |

### `octo-cli` — operator-visible projection

| Vector                                                  | Crate      | Property locked                                                                                               |
| ------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------- |
| `tv_cf_17_caveat_summary_view_serialises_as_type_value` | `octo-cli` | `CaveatSummaryView` serialises as `{ type, value }`, not `{ kind, body }`, asserted on the emitted JSON keys. |

Inverted guide vector:

- `guide_canonical_form_is_reparseable` (`octo-cli`) — pre-amendment this was
  `guide_canonical_form_is_not_reparseable` and pinned the asymmetry. After
  this amendment, the canonical form MUST round-trip through the input form
  for the lossless arms. The vector's doc comment records the inversion; the
  pre-amendment name is removed.

Gate mechanics, so these cannot pass vacuously:

- The `hex-ids` rows require the flag. The workspace test gate runs default
  features, which compiles the `hex-ids` test file to an empty binary, so CI
  carries an explicit step for the flagged run. Compiling a vector is not
  running it.
- The `default (in-crate)` rows live in `src/`, not `tests/`, and are reached
  by the lib target rather than an integration bin.
- `tv_cf_14` exists because a 32-byte input built from a single repeated byte
  is a fixed point of every byte permutation. A reversal defect inside the
  adapter passed all 13 vectors that preceded it.

## References

Both entries name sections of the parent RFC-0011 rather than of this
amendment; see the number note in the header table for how the two
same-numbered documents are told apart.

- RFC-0011 §Caveat Catalog — defines the canonical envelope `{ type, value }`
  for every caveat arm, including the 16-byte `DqaEncoding` hex form for
  `AmountMax` and the 32-byte id form for `Vault`.
- RFC-0011 §Hex32 newtype — defines the 32-byte hex newtype for the
  operator-visible view; this amendment extends the same hex discipline to
  substrate-owned serialization paths via `hex_id_32`.
