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

Gate vectors, all green: `tv_cf_01..06`, `tv_cf_13..16`, and `tv_cf_18..23`
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

**Scope, stated up front because it is narrower than it first looks.** This
amendment closes the gap for the two arms named above and for the shared
adapter discipline. It does **not** close the gap for the whole hex-emitting
set: four further arms emit hex and still read only the byte array, and for
two of those the emitter and the input form disagree on envelope shape as well
as encoding. Those are recorded in §Known deviations with their cost, and
pinned by `tv_cf_20` and `tv_cf_21` so the remaining set cannot drift
unnoticed. An earlier revision of this Summary read as though the full set was
covered.

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

   This clause is scoped to the lossless arms on purpose. The `Payment` arm
   is **not** one of them; see §Known deviations. An earlier revision of this
   clause named the `Payment.budget` arm here, which asserted a round trip
   that does not exist and had no vector behind it.

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

### 1. `Caveat::Payment`'s canonical form is a partial projection

`canonical_ser` emits four of `PaymentCaveat`'s seven fields —
`caveat_name`, `budget`, `model`, `expires_at_unix_ms` — and drops
`asset_id`, `registry_snapshot_epoch`, and `nonce`. Feeding the canonical form
back into the `Caveat` enum's `Deserialize` impl therefore **fails** with a
missing-field error, not merely a value mismatch. This is why clause 1 is
scoped to the lossless arms. `AmountMax` and `Vault` carry a single payload,
so for them the canonical form is lossless and does round-trip.

The projection is digest-relevant: `canonical_ser` feeds `caveat_body_hash`,
a BLAKE3 digest that contributes to the capability id. Making the `Payment`
projection lossless would therefore change every payment capability's id, so
that is a decision with a stated cost and not a cleanup. It is left open.

The three dropped fields are not incidental:

- `asset_id` is the asset binding that stops a USDC budget being spent
  against an OCTO-W query.
- `registry_snapshot_epoch` is the staleness guard.
- `nonce` is the anti-replay token.

No consumer may treat `canonical_ser` output as re-input for the `Payment`
arm. `tv_cf_18` pins the deviation so it stays visible: if that vector ever
fails, the projection changed and the capability-id contract needs
re-examining rather than the pin being relaxed.

### 2. `hex-ids` is a substrate-local opt-in, not a coordinated switch

The flag is declared by `octo-cap-macaroon` and enabled by nothing else in the
workspace. A producer that enables it emits hex for the three newtypes; a
consumer that has not enabled it expects the byte-array form and will reject
the input. The flag is therefore a migration affordance for a
coordinated rollout, not a safe default. Clause 4 promises only that
non-opted-in consumers see no change, which is true and is not the same
promise as interoperability.

### 3. Eight of the nine hex-rendering arms still reject their own hex

This is the largest gap between what this amendment claims and what it
delivers, so it is stated first among the structural deviations.

`canonical_ser` renders bytes through `hex::encode` in **eleven** arms: eight
carry a 32-byte id, two carry a 16-byte `Dqa` payload (`amount_max` and
`payment`), and one (`raw`) carries a variable-length blob. `amount_max` is the
one that round-trips, because Phase 1 added the `visit_str` arm to
`dqa_serde::field`. `raw` is outside the 32-byte question entirely.

Of the nine arms carrying a fixed-width id, the amendment gave the hex input
path to **one**.

| Arm                    | Renders 32-byte id as hex | Accepts its own canonical form |
| ---------------------- | ------------------------- | ------------------------------ |
| `vault`                | yes                       | **yes**                        |
| `invocation_hash_bind` | yes                       | no                             |
| `ask_binding`          | yes                       | no                             |
| `wrapped_only`         | yes                       | no                             |
| `redemption_context`   | yes                       | no                             |
| `asset_binding`        | yes                       | no                             |
| `factory`              | yes (`target_vault_id`)   | no                             |
| `policy_reference`     | yes (`policy_id`)         | no                             |
| `payment`              | yes (16-byte `budget`)    | no                             |

So eight of nine. The other eight still exhibit the exact emitter/input
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

**Why they are not simply fixed here.** Adopting the adapter on an arm widens
that arm's _input_ acceptance to both forms, which is free and backward
compatible. It also changes the arm's _emitted_ form from a 32-element array to
a hex string, which breaks every consumer that reads the array form.

The mechanical cost is one attribute per arm. `InvocationHashBind` and
`AskBinding` use the `Blake3` and `AskId` spellings, but both are type ALIASES
for `[u8; 32]` rather than newtypes, so `#[serde(with = "hex_id_32")]` lines up
exactly as it does for `Vault`. An earlier revision of this section claimed the
adapter signature did not fit those two arms; that was checked and is wrong —
clippy's "useless conversion" lint on the `.into()` calls is what surfaced it.

Left as a follow-up with the cost stated, not quietly taken. It is a
wire-format decision per arm, and it belongs to the owner of the capability
wire format, not to a document review.

### 4. The envelope shape is a SECOND, independent cause of the asymmetry

`Caveat` is adjacently tagged (`tag = "type"`, `content = "value"`). A
struct-payload arm therefore expects an OBJECT under `value`, while
`canonical_ser` writes a bare hex STRING there. `wrapped_only`,
`redemption_context`, `asset_binding`, `factory`, `policy_reference`, and
`payment` are all affected. Their canonical forms cannot be parsed by the
CLI's own `--caveats` parser **regardless of encoding**.

This is not the same defect as the hex-versus-array one, and fixing one does
not fix the other. That was measured, not assumed: adding
`#[serde(with = "hex_id_32")]` to `AssetBinding::asset_id` on its own leaves
`tv_cf_20` green, because the envelope still carries a string where the input
form wants an object. Only when the adapter **and** the envelope shape are both
corrected does the arm's canonical form start parsing, and at that point
`tv_cf_20` and `tv_cf_21` fail and demand the deviation list be updated. So
each of these arms needs two changes, not one, and any plan that fixes only the
adapter will appear to succeed and change nothing.

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

### 5. `Caveat::Permission` remains asymmetric

Unchanged by this amendment and out of its scope: `canonical_ser` emits the
full HMAC info string for the `Permission` arm while the input form expects
the short tag. It is a separate defect with its own follow-up.

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

| Vector                                                                | Crate / feature      | Property locked                                                                                                                                            |
| --------------------------------------------------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tv_cf_01_dqa_hex_round_trip`                                         | default              | `dqa_serde::field::deserialize` accepts a 64-hex string and decodes to the same `Dqa` as the 16-byte byte-array form.                                      |
| `tv_cf_02_dqa_hex_rejects_odd_length`                                 | default              | odd-length hex strings reject.                                                                                                                             |
| `tv_cf_03_dqa_hex_rejects_non_hex_chars`                              | default              | non-hex characters reject.                                                                                                                                 |
| `tv_cf_04_hex_id_32_round_trip_string`                                | default              | `hex_id_32` adapter round-trips through a 64-char hex string.                                                                                              |
| `tv_cf_05_hex_id_32_accepts_legacy_array_form`                        | default              | `hex_id_32` adapter parses the legacy 32-element array form.                                                                                               |
| `tv_cf_06_hex_id_32_rejects_short_hex`                                | default              | short hex strings reject.                                                                                                                                  |
| `tv_cf_07_canonical_amount_max_reparses_through_input_form`           | default (in-crate)   | `Caveat::AmountMax`'s `canonical_ser` output re-parses through `#[serde(with = "dqa_serde::field")]`.                                                      |
| `tv_cf_08_canonical_vault_reparses_through_input_form`                | default (in-crate)   | `Caveat::Vault`'s `canonical_ser` output re-parses through `#[serde(with = "hex_id_32")]`.                                                                 |
| `tv_cf_09_legacy_vault_array_form_still_parses`                       | default (in-crate)   | legacy 32-element array form on `Caveat::Vault` continues to parse.                                                                                        |
| `tv_cf_14_hex_id_32_preserves_byte_positions`                         | default              | the adapter encodes byte _i_ at hex position _i_, so no permutation is invisible.                                                                          |
| `tv_cf_15_hex_id_32_rejects_short_array_form`                         | default              | a 31- or 33-element array is rejected rather than truncated or panicked into acceptance.                                                                   |
| `tv_cf_16_hex_id_32_accepts_uppercase_hex_input`                      | default              | uppercase hex is accepted on input as documented leniency, and is never the emitted form.                                                                  |
| `tv_cf_18_payment_canonical_form_is_a_partial_projection`             | default              | **pins the deviation** in §Known deviations: the `Payment` canonical form does not re-parse, and the dropped fields are named.                             |
| `tv_cf_19_all_32byte_id_fields_accept_both_forms`                     | default              | every 32-byte id-bearing field accepts hex and legacy array alike, checked through its owning envelope.                                                    |
| `tv_cf_20_hex_emitting_arms_acceptance_set_is_explicit`               | default              | the hex-emitting arm set is enumerated, and each arm's hex-input acceptance is stated rather than assumed.                                                 |
| `tv_cf_21_known_asymmetric_arms_still_reject_their_own_canonical_hex` | default              | pins the eight-arm deviation recorded in §Known deviations, and that those arms still accept the legacy array form.                                        |
| `tv_cf_22_hex_rendering_set_matches_canonical_ser_source`             | default              | the hex-rendering arm set is derived from the `canonical_ser` source and compared against the literal, so a new hex-emitting arm cannot be added silently. |
| `tv_cf_23_hex_scan_reaches_the_final_caveat_variant`                  | default              | the `canonical_ser` scan reaches the enum's final variant, so a truncated scan cannot make `tv_cf_22` agree with a short list.                             |
| `tv_cf_13_default_newtype_serde_is_byte_array`                        | default (bare types) | under default features, a bare `AssetId` / `ChainId` / `VaultId` serialises as a 32-element byte array.                                                    |

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
