---
name: 0011-b-reputation-subcommands
description: Implement `octo reputation show` per RFC-0011-b
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-b author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-b
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
status: Open
---

# 0011-b-reputation-subcommands — Implement `octo reputation show`

**Status:** Open — implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] on RFC-0011-b acceptance. Read-only amendment; no release-cycle gate (per §Why 1 release cycle gate below).
**Substrate:** RFC-0011-b §Specification
**Parent:** RFC-0011-b
**Depends on:**

- Mission `0011-core-output-envelope-redaction` (`OutputEnvelope<T>`, `OctoCliError`, `OctoCliRedactor`, exit code table, confirmation gate matrix, redaction patterns)
- Mission `0011-identity-commands` (`OctoOperator` mode substrate + active-identity resolution for default `--did`)
- Mission `0011-capability-commands` (macaroon caveat substrate; `AuditorAuth` derivation per RFC-0957)
- Mission `0011-policy-commands` (no policy read is required by `show`, but substrate ordering per parent §Implementation Phases is 1 → 2 → 3 → 4)
- Substrate additions (mission-scoped, see §Sub-steps 1–2):
  - `crates/octo-reputation/src/projection.rs` (NEW) — `project()`, `attestations()` per RFC-0011-b §Substrate `[ADD]` Signatures
  - `crates/octo-reputation/src/lib.rs` (MODIFY) — re-export `projection` module + re-export `Role`, `ReputationRecord`, `AnchorRef`, `AttestationSummary`
- RFC-0968 substrate (Accepted; provides `ReputationRecord`, `Did`, `RecorderId`, `SignalKind`, `ReputationLayer`, `ReputationError` discriminant table, `RotationReceipt`, attestation append-only audit trail)
- RFC-0968-a2 substrate (`ControllerIdMissing = 0x34` reserved discriminant; `controller_id = blake3(governance_pubkey)` derivation)
- RFC-0955-r1 substrate (anchor chain semantics; `ReputationDigest`; `ReputationAnchorBatch`; BLAKE3 `anchor_digest` over canonical 24-byte Dfp BLOB)
- RFC-0104 substrate (`Dfp` deterministic floating-point for score projection; required for RFC-0008 Class B determinism)
  **Blocks:** none (read-only v1.0; follow-on amendments per RFC-0011 Status header chain land attest/anchor independently)

## Status

Open — implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] once RFC-0011-b is Accepted. Per parent RFC-0011 §Implementation Phases Phase 3 sequence: identity → capability → policy → reputation. No prior mission blocks this one in the substrate DAG beyond the Phase 1–3 prerequisites listed above. The amendment is read-only — no release-cycle hard-error gate is required (N/A; see §Why 1 release cycle gate).

## RFC

RFC-0011-b §Specification (rfcs/draft/process/0011-b-reputation-subcommands.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing per RFC-0011 §Implementation Phases: mission 1 → 2 → 3 → 4 → 5 → reputation (this mission). RFC-0011-b itself amends RFC-0011 and builds on RFC-0968, RFC-0968-a2, RFC-0955-r1; all "Requires" RFCs are Accepted on the date of RFC-0011-b draft per RFC-0011-b §Dependencies

## Acceptance Criteria

- [ ] Substrate additions land: `crates/octo-reputation/src/projection.rs` NEW (`project()`, `attestations()`, `Role::parse`); `octo_reputation/src/lib.rs` MODIFY (re-export)
- [ ] CLI additions land: `crates/octo-cli/src/commands/reputation.rs` NEW (`reputation show` impl); `main.rs` MODIFY (`Reputation` enum variant); `error.rs` MODIFY (3 `OctoCliError` variants); `commands/mod.rs` MODIFY (`pub mod reputation;`); `Cargo.toml` MODIFY (`octo-reputation` + `octo-determin` deps)
- [ ] Output struct `ReputationShowOutput` matches RFC-0011-b §Output Envelope exactly (no parallel type drift; `ReputationRecord → ReputationShowOutput` via `.into()`)
- [ ] `AttestationSummary` + `AnchorRef` re-exported from `octo-reputation` (no parallel CLI-side type)
- [ ] Exit codes 20 / 21 / 22 wired to `ReputationNotFound` / `ReputationRevoked` / `AnchorChainBroken` respectively (per RFC-0011-b §Error Handling + Appendix C)
- [ ] `--no-anchor-verify` flag rejected in Human / Ci / Auditor modes (DEV-ONLY escape hatch per RFC-0011-b §Security Considerations 1a)
- [ ] Auditor mode fail-closed on revoked DID (exit 21 regardless of mode per RFC-0011-b §Security Considerations 3)
- [ ] `--limit` hard cap 1000 enforced via clap `value_parser` range (rejects `--limit 1001` with exit 2)
- [ ] All three new `OctoCliError` `Display` strings pass through `sanitize_substrate_error` per parent §Error Handling variant sanitization rule
- [ ] Layer direction verified (CLI Layer C → substrate Layer B; no reverse dep per [[cipherocto-design-principles]])
- [ ] Cross-mission AC: `octo reputation show` composes with `octo identity show` (DID lookup) and `octo capability list` (auditor capability attestation); substrate ordering 1 → 2 → 3 → 4 → reputation holds
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test --workspace --lib green
- [ ] No new INVALID cites introduced (manual review per CLAUDE.md §RFC Reference Conventions)

### Type Coverage

| RFC-0011-b type                                               | Sub-step                | Notes                                                                                     |
| ------------------------------------------------------------- | ----------------------- | ----------------------------------------------------------------------------------------- |
| `ReputationShowOutput` (CLI operator UX; Layer C)             | Sub-step 3 (CLI impl)   | Wraps `ReputationRecord` via `.into()`; no parallel substrate type                        |
| `ReputationComponents` (4 Dfp fields; Layer C)                | Sub-step 3 (CLI impl)   | Mirrors substrate component shape; identity / stake / performance / social                |
| `AttestationSummary` (re-exported from `octo-reputation`)     | Sub-step 1 + 3          | Public on the wire per RFC-0968 §Roles + §11; no redaction                                |
| `AnchorRef` (re-exported from `octo-reputation`)              | Sub-step 1 + 3          | BLAKE3 `anchor_digest` + `chain_block_height` + `submitted_at_unix` per RFC-0955-r1       |
| `Role` newtype (`pub struct Role(String)`)                    | Sub-step 1 (substrate)  | `Role::parse(s)` accepts any non-empty lowercase string for v1.0; catalog via RFC-0011-d  |
| `ReputationRecord` (substrate Layer B)                        | Sub-step 1 (substrate)  | RFC-0968 §10 projection-shaped struct; one module-level `[ADD]` per RFC-0011-b §7.4       |
| `OctoCliError::ReputationNotFound { did, role }` (exit 20)    | Sub-step 4 (error impl) | Mapped from `ReputationError::AggregateEmpty` per RFC-0011-b §Substrate `[ADD]` map       |
| `OctoCliError::ReputationRevoked { did }` (exit 21)           | Sub-step 4 (error impl) | All operator modes (Auditor fail-closed per §Security Considerations 3)                   |
| `OctoCliError::AnchorChainBroken { did, last_anchor_unix }`   | Sub-step 4 (error impl) | Mapped from `ReputationError::AnchorDigestMismatch` per RFC-0011-b §Substrate `[ADD]` map |
| `octo_reputation::project(did, role)` (Layer B `[ADD]`)       | Sub-step 1 (substrate)  | Aggregate projection; Dfp arithmetic per RFC-0104; anchor verify per RFC-0955-r1          |
| `octo_reputation::attestations(did, role, since_unix, limit)` | Sub-step 1 (substrate)  | Window SELECT; `ORDER BY recorded_at_unix DESC LIMIT N` per RFC-0011-b §7.6               |
| `ReputationShowArgs` (clap struct; Appendix A)                | Sub-step 3 (CLI impl)   | `--role`, `--since`, `--limit`, `--no-anchor-verify` per RFC-0011-b §7.2 Flags            |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Substrate `[ADD]` Surface Patterns for the canonical CLI-→-substrate wiring pattern. Clap fragment per RFC-0011-b Appendix A. JSON output schema per RFC-0011-b Appendix B. Error → exit code table per RFC-0011-b Appendix C.

## Pull Request

# (PR opened after mission claim transitions to Claimed per BLUEPRINT.md §Mission Lifecycle)

## Risk

- **Substrate `Role` catalog deferral.** v1.0 `--role` accepts any non-empty lowercase string per RFC-0011-b §7.4 catalog deferral note. Invalid role strings yield exit 2 (`ClapParse`-style; v1.0 does NOT reject unknown roles against a catalog). Mitigation: catalog enumeration lands via RFC-0011-d (role provisioning amendment); v1.0 inputs remain valid post-catalog per RFC-0011-b §Compatibility
- **Anchor verification false positives under partial substrate drift.** `--live` is the default; substrate `project()` verifies the last `anchor_digest` against the current aggregate. A network-partition-induced un-anchored state could yield `AnchorChainBroken` (exit 22) when the chain-side anchor is just late. Mitigation: `--no-anchor-verify` DEV-ONLY escape hatch per RFC-0011-b §Security Considerations 1a; production substrate does not accept the flag.
- **Auditor mode revocation privacy leak.** Pre-amendment designs could expose a revoked DID's prior score under Auditor mode. Per RFC-0011-b §Security Considerations 3, Auditor mode invocation of `show --did <revoked>` MUST exit 21 (`ReputationRevoked`) — fail-closed at dispatch time. Test vector `revoked-did-denied` asserts all four operator modes.
- **Dfp arithmetic cross-replica determinism.** Score projection is RFC-0008 Class B; identical input bytes MUST yield identical projection bytes per RFC-0104. Mitigation: no `f64` conversion in substrate projection; Dfp-only arithmetic; canonical serialization per RFC-0104 §3 Wire Form Contract
- **Append-only audit-trail assumption.** Substrate `attestations()` trusts the RFC-0968 §11 append-only invariant. A substrate bug that exposes an UPDATE path on `reputation_events` would corrupt `score_ewma`. Mitigation: substrate-side invariant test `append_only_no_update_path`; RFC-0968 substrate invariant + RFC-0011-b §Implicit Assumptions Audit row 2.
- **Stale projection (no caching in v1.0).** v1.0 is always-live per RFC-0011-b §Security Considerations 2. Future caching layer MUST remain `--live` by default per RFC-0011-b §Adversarial Review row 4 (DOCUMENTED for follow-on amendment).

## Notes

- **Follow-on dependency:** `--role` catalog enumeration depends on RFC-0011-d (role provisioning amendment). v1.0 accepts any non-empty lowercase string per RFC-0011-b §7.4 catalog deferral note; the catalog enum lands via RFC-0011-d per parent Status header chain. Existing role names (`octo-a`, `octo-b`, `octo-o`, `octo-w`) are accepted unchanged in v1.0 per RFC-0011-b §Compatibility
- **Amendment chain context:** RFC-0011-b is the **second amendment** of the RFC-0011 amendment chain per parent Status header. Follow-on amendments per parent Status header: audit (RFC-0011-a), reputation (this RFC), agent lifecycle (RFC-0011-c), role provisioning (RFC-0011-d), vault operations (RFC-0011-e), mesh operations (RFC-0011-f), governance (RFC-0011-g). Authority for write paths (`reputation attest`, `reputation anchor`) is reserved for future amendments per RFC-0011-b §Rationale "Why single-subcommand amendment (not full RFC)".
- **No substrate discriminant ratification required (v1.0).** RFC-0011-b assumes RFC-0968 §13 + RFC-0968-a2 §3 reserved discriminant range `0x2A..=0xFF` already ratifies `AggregateEmpty` + `AnchorDigestMismatch`. Verify against the substrate at mission-claim time; ratify any new discriminant via a follow-on RFC-0968 amendment before this mission lands if the reserved range does not yet cover them.
- **Layer direction audit:** Per CLAUDE.md §Architectural Principles, `octo-cli` (Layer C) depends on `octo-reputation` (Layer B) and `octo-determin` (Layer B substrate via RFC-0104); neither substrate crate depends on `octo-cli`. The new `projection.rs` module is a pure Layer-B addition (no new Layer-A types; `Dfp` is reused via `octo_determin::Dfp` per RFC-0011-b §Rationale "Why `Dfp` re-export vs new `ReputationScore` type").

## Scope

Implement `octo reputation show` per RFC-0011-b. One subcommand; one Layer-B `[ADD]` module; one clap fragment; three `OctoCliError` variants. No write paths; no attestation issuance; no anchor submission. Read-only projection of `ReputationRecord` + attestation window for one `(did, role)` tuple.

### Sub-steps

1. **Substrate additions** — `crates/octo-reputation/src/projection.rs` NEW. Implement `pub fn project(did: &Did, role: &Role) -> Result<ReputationRecord, ReputationError>` per RFC-0011-b §Substrate `[ADD]` Signatures: aggregate projection (Dfp arithmetic per RFC-0104); four-component weighted mean (identity 0.20 + stake 0.25 + performance 0.35 + social 0.20 per RFC-0011-b §7.5); last `anchor_digest` verification per RFC-0955-r1 §Wire Contract (mismatch → `AnchorDigestMismatch`). Implement `pub fn attestations(did: &Did, role: &Role, since_unix: i64, limit: u32) -> Result<Vec<AttestationSummary>, ReputationError>` per RFC-0011-b §7.6: `ORDER BY recorded_at_unix DESC LIMIT N` over the RFC-0968 §11 audit trail. Implement `pub struct Role(String)` + `pub fn Role::parse(s: &str) -> Result<Role, ParseRoleError>` (v1.0: non-empty lowercase only; catalog via RFC-0011-d). Re-export `projection`, `Role`, `AnchorRef`, `AttestationSummary` from `crates/octo-reputation/src/lib.rs`.

2. **Substrate invariant tests** — Add substrate-side test fixtures covering: `append_only_no_update_path` (RFC-0011-b §Implicit Assumptions Audit row 2); `RecorderId::Revoked` filter (row 3); `(did_hash, role) UNIQUE` constraint enforcement (row 4); last-anchor digest verification happy-path + mismatch (row 1). Substrate-side; surfaces `ReputationError` discriminants for the CLI mapping in sub-step 4.

3. **CLI additions** — `crates/octo-cli/src/commands/reputation.rs` NEW. Implement `pub fn show(args: ReputationShowArgs)` per RFC-0011-b Appendix A clap fragment: parse `--role` via `Role::parse`; resolve default `--did` from active identity per mission `0011-identity-commands`; call `octo_reputation::project` + `octo_reputation::attestations`; wrap in `OutputEnvelope<ReputationShowOutput>` per RFC-0011-b §7.3. Apply `OctoCliRedactor` (no-op for `show` per RFC-0011-b §7.7). Mode-aware dispatch: reject `--no-anchor-verify` in Human / Ci / Auditor modes (exit 2); Auditor mode fail-closed on `ReputationRevoked` per RFC-0011-b §Security Considerations 3.

4. **Error additions** — `crates/octo-cli/src/error.rs` MODIFY. Add three `OctoCliError` variants per RFC-0011-b §Error Handling: `ReputationNotFound { did: String, role: String }` (exit 20), `ReputationRevoked { did: String }` (exit 21), `AnchorChainBroken { did: String, last_anchor_unix: i64 }` (exit 22). Map from `ReputationError::AggregateEmpty` / `SubjectRevoked` / `AnchorDigestMismatch` per RFC-0011-b §Substrate `[ADD]` map. Pass all three `Display` strings through `sanitize_substrate_error` per parent §Error Handling variant sanitization rule.

5. **Cargo deps** — `crates/octo-cli/Cargo.toml` MODIFY. Add `octo-reputation` (Layer B substrate per RFC-0968) + `octo-determin` (Layer B substrate per RFC-0104 Dfp type). No other crate deps added or removed.

6. **CLI wiring** — `crates/octo-cli/src/main.rs` MODIFY: add `Reputation(ReputationCmd)` variant to the `Commands` enum; route `Commands::Reputation(ReputationCmd::Show(args))` to `commands::reputation::show(args)`. `crates/octo-cli/src/commands/mod.rs` MODIFY: add `pub mod reputation;`. `crates/octo-cli/src/redact.rs` NO CHANGE (redactor already covers all redaction patterns; `show` is a no-op per RFC-0011-b §7.7).

7. **CLI tests** — Test vectors per §Test Vectors below (at least 6 of the 11 RFC-0011-b test vectors land as `octo-cli` integration tests; substrate-side fixtures cover the rest). Verify exit codes 0, 2, 20, 21, 22 via clap / dispatch; verify JSON output schema against RFC-0011-b Appendix B.

8. **Docs** — Update `docs/07-developers/octo-cli-implementation-guide.md` §Substrate `[ADD]` Surface Patterns with the reputation projection example. Update `docs/schemas/octo-cli/reputation-show.json` (or equivalent) per RFC-0011-b Appendix B. Add CHANGELOG entry: "`octo reputation show` added in v1.2 (RFC-0011-b)".

### Cargo deps

Two additions to `crates/octo-cli/Cargo.toml`:

```toml
# Reputation registry substrate (Layer B years-stable; RFC-0968 §10 Core Interfaces + §Roles)
octo-reputation = { path = "../../crates/octo-reputation", version = "0.1" }
# Deterministic floating-point substrate (Layer B; RFC-0104 §3 Wire Form Contract — Dfp arithmetic for score projection)
octo-determin = { path = "../../crates/octo-determin", version = "0.1" }
```

Per CLAUDE.md §Crate dependency rationale: `octo-reputation` is Layer B substrate mandated by RFC-0968; `octo-determin` is Layer B substrate mandated by RFC-0104 for RFC-0008 Class B determinism. Both are Layer B additions; CLI depends on them, not reverse. No other crates added or removed.

## Test Vectors

Per RFC-0011-b §Test Vectors, all 11 vectors are substrate-side test fixtures (live in `crates/octo-reputation/tests/`). The mission surfaces at least 6 as `octo-cli` integration tests in `crates/octo-cli/tests/reputation_show.rs` to assert the CLI ↔ substrate wiring end-to-end (exit codes, JSON envelope, mode-aware dispatch):

1. **`show-success-full-data`** (RFC-0011-b TV 1) — Subject DID with 10 attestations across all four signal kinds, dual-stake present, performance EWMA converged. CLI exits 0; output envelope `schema_version = 2`, composite score rendered as Dfp canonical decimal; four components all non-zero; `attestations.len() == 10`; `anchor_ref` present with `chain_block_height`. Substrate-side fixture: full-reputation subject.
2. **`show-success-no-attestations`** (RFC-0011-b TV 2) — Subject DID with identity state but zero attestations, zero stake, no performance signal. CLI exits 0; composite `score == 0.0`; all components `== 0.0`; `attestations` empty; `anchor_ref: None`. Asserts RFC-0011-b §7.5 no fabrication under missing-data-component (G2).
3. **`show-success-single-component-zero`** (RFC-0011-b TV 3) — Subject with full stake + performance but zero social attestations. CLI exits 0; `identity ≈ 0.20`, `stake ≈ 0.25`, `performance ≈ 0.35`, `social == 0.0`, composite `≈ 0.80`. Asserts RFC-0011-b §7.5 zero-component handling.
4. **`show-error-anchor-chain-broken`** (RFC-0011-b TV 4) — Subject with tampered anchor chain (substrate fixture: `anchor_digest` mismatched against current aggregate). Substrate returns `AnchorDigestMismatch`; CLI maps to `AnchorChainBroken` (exit 22). Output envelope `exit_code == 22`. Asserts RFC-0011-b §Security Considerations 1 + G3.
5. **`show-error-reputation-revoked`** (RFC-0011-b TV 5) — Subject DID in `Revoked` lifecycle state per RFC-0968 §2.1. ALL FOUR operator modes (Human / Ci / Dev / Auditor) MUST exit 21. Auditor mode fail-closed asserted explicitly per RFC-0011-b §Security Considerations 3 + R5 Adversary Analysis row 2.
6. **`show-error-reputation-not-found`** (RFC-0011-b TV 6) — `--role <unknown>` for a valid subject DID. Substrate returns `AggregateEmpty`; CLI maps to `ReputationNotFound` (exit 20). Output envelope `exit_code == 20`.
7. **`show-error-no-active-identity`** (RFC-0011-b TV 7) — Operator has no active identity; CLI exits 2 (`NoActiveIdentity` per parent §Exit Codes). Asserts parent §NoActiveIdentity path.
8. **`show-success-limit-5`** (RFC-0011-b TV 8) — Subject with 20 attestations; `--limit 5` returns exactly 5 (most-recent-first). Asserts RFC-0011-b §7.6 sort order.
9. **`show-success-since-filter`** (RFC-0011-b TV 9) — Subject with 20 attestations across two time windows; `--since <midpoint>` returns only the latter window. Asserts RFC-0011-b §7.6 filter semantics.
10. **`show-error-limit-overflow`** (RFC-0011-b TV 10) — CLI rejects `--limit 1001` with clap `value_validation` error (exit 2). Asserts RFC-0011-b §7.6 hard cap.
11. **`show-error-no-anchor-verify-rejected`** (RFC-0011-b TV 11) — Human mode invocation of `--no-anchor-verify` rejected by dispatch-time mode check (exit 2). Asserts RFC-0011-b §Security Considerations 1a (DEV-ONLY escape hatch).

Cross-mission AC: final integration — `octo reputation show` composes with `octo identity show` (DID lookup) and `octo capability list` (auditor capability attestation) without substrate ordering inversion.

## Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — adds one clap subcommand struct + three `OctoCliError` variants; depends on Layer B
- `octo-reputation` (Layer B) — adds one new module `projection.rs` + re-exports; depends on Layer A only (`octo-determin::Dfp` per RFC-0104 + RFC-0968 substrate)
- `octo-determin` (Layer B substrate) — UNCHANGED; reused for `Dfp` arithmetic (RFC-0104 §3 Wire Form Contract)
- NO substrate crate changes outside `octo-reputation` + `octo-cli`
- Dependency direction: C → B → A. CLI depends on substrate; substrate does not depend on CLI. Audit per CLAUDE.md §Architectural Principles Layer table: Layer C is per-RFC; this mission adds one Layer-C clap struct + one Layer-B module. Direction holds.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cli --all-targets --features full -- -D warnings
cargo clippy -p octo-reputation --all-targets --features full -- -D warnings
cargo test -p octo-cli --features full --test reputation_show
cargo test -p octo-reputation --features full --lib
# Exit code matrix spot-check:
octo reputation show --did did:octo:b7z4v9a4n6q5r2s8t1u3w5x7y9z1a2b3c4d5e6f7g8h9i0j1k2l3 --role octo-a; echo $?  # expect 0 (success) OR 20 (no signals for role)
octo reputation show --did did:octo:b<revoked> --role octo-a; echo $?  # expect 21 (ReputationRevoked; all 4 modes)
octo reputation show --no-anchor-verify; echo $?  # expect 2 (rejected in Human mode)
```

## Backward compat

- **Additive only** per RFC-0011-b §Compatibility One subcommand added; two `[ADD]` substrate entry points added; no existing subcommand modified; no existing substrate API renamed or removed; no existing `OutputEnvelope<T>` schema broken.
- **Exit code reservation:** Reserves codes 20-22 per parent §Exit Codes "17-63 reserved per Status header amendment chain". No existing code renumbered.
- **Role name compatibility:** `--role <role>` accepts any non-empty lowercase string per RFC-0011-b §7.4 catalog deferral note. Existing role names from RFC-0011 parent (`octo-a`, `octo-b`, `octo-o`, `octo-w`) are accepted unchanged in v1.0. Future catalog enumeration via RFC-0011-d will be backward-compatible — v1.0 inputs remain valid per RFC-0011-b §Compatibility
- **Output schema compatibility:** New `T` payload only; `schema_version` unchanged at parent value (currently 2). Consumers ignoring unknown fields per JSON deserialization default behavior are unaffected.
- **`octo` surface after this mission lands:** `whoami`, `identity {show,rotate,revoke}`, `capability {list,mint,attenuate}`, `policy {show,list}`, `reputation show` (+ the follow-on amendments per Status header amendment chain as they land).

## Cross-references

- RFC-0011-b — `octo reputation` Subcommands (parent RFC of this mission; canonical spec for `octo reputation show` and the substrate `[ADD]` projection surface)
- RFC-0011 — `octo` CLI Substrate (grandparent; canonical `OutputEnvelope<T>`, `OctoCliError`, `OctoCliRedactor`, exit code table, confirmation gate matrix, redaction patterns; amendment chain context)
- RFC-0968 — Reputation Registry (substrate for `ReputationRecord`, `Did`, `RecorderId`, `SignalKind`, `ReputationLayer`, `ReputationError` discriminant table, attestation append-only audit trail)
- RFC-0968-a2 — Discriminant Stability Sub-amendment (`ControllerIdMissing = 0x34`; `controller_id = blake3(governance_pubkey)` derivation; reserved discriminant range `0x2A..=0xFF`)
- RFC-0955-r1 — Reputation Anchoring Amendment (anchor chain semantics; `ReputationDigest`; `ReputationAnchorBatch`; BLAKE3 `anchor_digest` over canonical 24-byte Dfp BLOB)
- RFC-0104 — Deterministic Floating-Point (Dfp substrate for `score_ewma` projection; required for RFC-0008 Class B determinism)
- RFC-0008 — Deterministic AI Execution Boundary (Class B for projection; Class C for the CLI invocation; Class A for envelope rendering)
- RFC-0009 — Identity Management (lifecycle state machine + rotation grace; substrate for `IdentityRecord.rotation_history`)
- RFC-0957 — Macaroon Substrate (capability token structure; `AuditorAuth` derives from macaroon caveats)
- RFC-0011-d — Role Provisioning (follow-on amendment; required for `--role` catalog enumeration; per §Notes)
- [[cipherocto-design-principles]] — Layer A/B/C/D/E; no central enums; per-extension crates; no wallet→storage coupling

## Why 1 release cycle gate

**N/A** — read-only amendment. Per CLAUDE.md + RFC migration etiquette, the 1-release-cycle gate applies to **breaking** changes only. RFC-0011-b is **strictly additive** per RFC-0011-b §Compatibility: one subcommand added, two `[ADD]` substrate entry points, three new exit codes reserved, no existing surface modified. No deprecation window is required because nothing is being deprecated or renamed. Implementation can ship in any minor release (e.g., v1.2) immediately after RFC-0011-b is Accepted, with no migration step for existing operators.

Contrast with the parent `0011-deprecation-stub-removal` mission: that mission IS breaking (five stub commands removed per RFC-0011 §Compatibility timeline) and therefore DOES require the v1.1 hard-error cycle to elapse before v2.0 removal. `show` is non-breaking and ships without a deprecation window.

## Claimant

@unassigned
