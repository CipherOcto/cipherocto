---
name: 0011-c-agent-redaction-envelope
description: Envelope-payload redaction for `octo agent` (Phase 2 follow-on to 0011-c-agent-create-subcommand)
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-c author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - mission 0011-c-agent-create-subcommand
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-13
completed_at: 2026-09-13
completed_by: mmacedoeu
implementation_commit: 913d1348
review_rounds: 7
dry_closure_audit: docs/audits/2026-09-13-A1-agent-redaction-envelope-dry-closure.md
---

# 0011-c-agent-redaction-envelope — Envelope-payload redaction for `octo agent`

**Status:** Open (Phase 2 follow-on to `0011-c-agent-create-subcommand`)
**Substrate:** RFC-0011-c §Security Considerations + RFC-0011 §Redaction Layer
**Parent:** mission `missions/claimed/0011-c-agent-create-subcommand.md` (AC #49 Phase 2)

## Scope

Phase 1 of mission `0011-c-agent-create-subcommand` registers the
`agent_id` / `capability_root` / `holder_did` log-time redaction
patterns via `OctoCliRedactor::FIELD_TABLE` (wholesale substitution
with `REDACTED_KEY`) — see AC #49 Phase 1. This mission (Phase 2)
extends redaction to the envelope payload boundary itself, so that
`octo agent create --json` output never leaks the agent substrate
identifiers verbatim even when piped through `jq`.

1. **`RedactedIdentifier` newtype for envelope-boundary redaction** —
   `crates/octo-cli/src/redact.rs` (Layer C/D). New wrapper
   `RedactedIdentifier(String)` with a custom `Serialize` impl
   that emits `"[REDACTED:key]"` regardless of inner value. The
   wrapper enforces the redaction invariant at the type level —
   downstream JSON consumers cannot accidentally render the inner
   value via a forgotten custom serializer.
2. **Conditional `holder_did` redaction** — `holder_did` MUST be
   redacted unless `holder_did == active_did` (the operator is
   permitted to see their own DID in the output, but must not see
   other DIDs they happen to share an active identity with). The
   conditional lives in the envelope-rendering path
   (`crates/octo-cli/src/output.rs`), not in `FIELD_TABLE` (which
   has no notion of context-aware redaction).
3. **`agent_id` truncation** — `agent_id` MUST be truncated to
   the first 8 hex chars + `...` (e.g.,
   `"00000000..."`) per RFC-0011 §Hex32 newtype redaction. Distinct
   from wholesale `REDACTED_KEY` because the operator needs to
   correlate the truncated form with substrate logs without
   identifying the agent.
4. **Envelope-renderer integration** — `OutputEnvelope::render`
   walks the payload via `serde_json::Value` tree walk and applies
   the conditional `holder_did` + `agent_id` truncation rules. The
   renderer MUST skip redaction for the dry-run `{redacted: true}`
   envelopes (where the payload is already a preview shape, not the
   live substrate view).

## Phase 1 / Phase 2 boundary

Per AC #49 of mission `0011-c-agent-create-subcommand`, Phase 1
log-time redaction is locked at mission close. This mission
extends the contract from the log-time to the envelope-payload
boundary. The two phases are independent invariants — a future
amendment could regression-test either in isolation. Phase 2 is
strictly additive: no Phase 1 surface is removed or weakened.

## Mission scope (4 sub-steps)

1. **`RedactedIdentifier` newtype** — `crates/octo-cli/src/redact.rs`.
   Custom `Serialize` impl (no `Deref<Target = str>` — in-process
   `&r` would yield the plaintext silently, defeating the audit),
   `RedactedIdentifier::reveal()` for the single server-internal
   escape hatch (guarded by an audit-log entry).
2. **`AgentCreateOutput` field migration** — wrap `holder_did` and
   `agent_id` in `RedactedIdentifier` so the type-level invariant makes
   accidental leakage a compile error. Keep public accessors for
   the conditional reveal (operator-owned DID, truncated agent_id).
3. **`OutputEnvelope::render` integration** — walk payload, apply
   redaction, write JSON. Do NOT walk dry-run envelope payloads
   (the preview shape is operator-owned).
4. **Test vectors** — exercise the new redaction shape in
   `crates/octo-cli/tests/agent.rs`: holder_did==active_did pass
   through, holder_did≠active_did redact, agent_id truncate,
   wholesale REDACTED_KEY for capability_root.

## Dependencies

- `missions/claimed/0011-c-agent-create-subcommand.md` MUST be
  completed first (this mission's substrate).
- `rfcs/accepted/process/0011-c-agent-lifecycle.md` — substrate
  RFC; cite §Security Considerations in mission YAML comments.
- `crates/octo-cli/src/redact.rs` (existing) — field table + helpers.

## Risk

- **MEDIUM** — `RedactedString` Serialize impl could regress the
  schemars contract for envelope payloads. Mitigation: add a
  schemars roundtrip test pin like the existing
  `agent_create_output_schema_declares_agent_id_as_string` test.
- **LOW** — operator UX regression when their own DID is
  conditionally visible (surprise reveal). Mitigation: a CLI
  `--redact-own-did` flag to force redaction even when matched.

## Layer direction

- `octo-cli` (Layer C/D) — redaction envelope renderer + newtype.
- `octo-wallet` (Layer B) — substrate record shape unchanged.
- NO new Layer A types introduced.

## Acceptance Criteria

| #   | Criterion                                                                                  | Status     | Evidence                                                                                         |
| --- | ------------------------------------------------------------------------------------------ | ---------- | ------------------------------------------------------------------------------------------------ |
| 1   | `RedactedIdentifier` newtype with custom `Serialize`/`Display`/`Debug` masking inner value | [x] LANDED | `crates/octo-cli/src/redact.rs:190-244`; doctest pins contract                                   |
| 2   | `AgentCreateOutput.agent_id` + `.holder_did` wrapped in `RedactedIdentifier`               | [x] LANDED | `crates/octo-cli/src/commands/agent.rs:222-223`                                                  |
| 3   | `OutputEnvelope::render` integrates `RedactionContext` walker                              | [x] LANDED | `crates/octo-cli/src/output.rs:235-263` `render_with_redaction`                                  |
| 4   | 4+ redaction test vectors in `tests/agent.rs`                                              | [x] LANDED | 6 vectors: `tv_agt_redact_*` (`TV-AGT-ENV-1..6`)                                                 |
| 5   | Conditional `holder_did` un-redact when `holder_did_raw == active_did`                     | [x] LANDED | `redact.rs:320-348` walker branch                                                                |
| 6   | `agent_id` truncation to first 8 chars + `...`                                             | [x] LANDED | `redact.rs:403-406` `truncate_id`                                                                |
| 7   | Audit emission `holder_did_un_redacted` fires only on match                                | [x] LANDED | `redact.rs:344-347`                                                                              |
| 8   | Audit emission `redacted_identifier_revealed` on `reveal()`                                | [x] LANDED | `redact.rs:213-220`                                                                              |
| 9   | Dry-run skip via `if !self.redacted` gate                                                  | [x] LANDED | `output.rs:251` + `tv_env_redact_dry_run_envelope_skips_redactor`                                |
| 10  | `#[schemars(with = "String")]` annotations on `RedactedIdentifier` fields                  | [x] LANDED | `commands/agent.rs:292, 302`; `agent_create_output_schema_declares_agent_id_as_string` test pins |
| 11  | Phase 1 log-time `FIELD_TABLE` redaction untouched                                         | [x] LANDED | additive-only diff; `redact.rs:80-155` unchanged                                                 |
| 12  | NO new Layer A types; octo-wallet (B) record shape unchanged                               | [x] LANDED | R6 layer-model CLEAN                                                                             |
| 13  | `cargo clippy --workspace --features full --all-targets -- -D warnings` clean              | [x] LANDED | R7 verification                                                                                  |
| 14  | `cargo test -p octo-cli --lib --tests` PASS                                                | [x] LANDED | 228 lib + 18 agent + (binary integration) all PASS                                               |
| 15  | `scripts/validate_cites.sh` 6/6 VALID                                                      | [x] LANDED | R7 verification                                                                                  |
| 16  | `cargo fmt --all` clean                                                                    | [x] LANDED | R7 verification                                                                                  |
| 17  | 7-round multi-lens DRY review converges                                                    | [x] LANDED | R6+R7 = 2 consecutive zero-finding rounds = DRY CLOSED                                           |
| 18  | Substrate-Faithful Amendment Trail documents 4 spec deviations                             | [x] LANDED | §Substrate-Faithful Amendment Trail below                                                        |

## Validation

- `cargo clippy --workspace --all-targets --features full -- -D warnings` ✓
- `cargo test -p octo-cli --lib --tests` ✓
- `timeout 30 bash scripts/validate_cites.sh` (Guard 2 cite validator) ✓
- `cargo fmt --all` ✓

## Notes

This mission unblocks the `octo agent` operator UX contract: the
JSON envelope is the single public surface that downstream tooling
(`jq`, `terraform-provider-octopus`, `ops scripts`) consumes. Until
the envelope is redacted, every tool that pipes `octo agent create
--json` would need its own redaction layer — a policy-drift landmine
that this mission removes by enforcing the invariant at the
type level.

## Substrate-Faithful Amendment Trail

| Amendment                                     | Rationale                                                                                                                                                                                                                                                                                                                                                                              |
| --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `RedactedString` → `RedactedIdentifier`       | Name collision with the pre-existing memo-plaintext wrapper `crate::output::RedactedString` (carries a length signal in the redaction marker). `RedactedIdentifier` carries NO length signal — substrate identifiers are fixed-size hex / DID-form — so a length signal would itself leak information.                                                                                 |
| `&'static str` inner → `String` inner         | `Zeroize` + `ZeroizeOnDrop` derive requires owned storage; `&'static str` cannot represent a runtime-determined agent identifier anyway (DID form is determined at registration, capability_root is operator input).                                                                                                                                                                   |
| `Deref<Target = &'static str>` omitted        | Deliberate deviation: in-process `&r` would yield plaintext silently, defeating the audit-log invariant. `RedactedIdentifier::reveal()` is the only read path, and it emits `tracing::warn!(target: "octo_cli.audit", event = "redacted_identifier_revealed", ...)`.                                                                                                                   |
| `RedactionContext` walker added (not in spec) | Spec said "in envelope-rendering path" without specifying a struct. `RedactionContext` is the explicit carrier so callers (future `agent list` / `agent run` sibling subcommands) can reuse the same context shape without per-callsite re-derivation. Audit emission moved here (`event = "holder_did_un_redacted"`) so the un-redact forensic trail mirrors the `reveal()` emission. |
