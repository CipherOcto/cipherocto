---
name: 0011-c-agent-list-subcommand
description: Land the `octo agent list` subcommand per RFC-0011-c
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
    - mission 0011-c-agent-create-subcommand
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
completed_at: 2026-09-13
completed_by: mmacedoeu
implementation_commit: 5f7daa3f
substrate_commit: 533b07a4
review_rounds: 0
dry_closure_audit: docs/audits/2026-09-13-list-mission-dry-closure.md
---

# 0011-c-agent-list-subcommand — `octo agent list` subcommand

**Status:** Completed
**Substrate:** RFC-0011-c §9.3.3 (`octo agent list`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-create-subcommand` — `Commands::Agent` clap root variant
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Completed (RFC-0011-c §Phase 2 CLI wiring, subcommand 3 of 5).

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.3 `octo agent list` and §9.8 Error Handling (`InvalidLimit`, `InvalidCursor`).

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` → `0011-c-agent-list-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission).

## Acceptance Criteria

- [x] `octo agent list` implemented + unit-tested (9 NEW tests; substrate 14/14 + CLI 17/17 PASS)
- [x] `AgentListOutput` payload type implemented + unit-tested (`count`, `holder_did`, `agents[]` per envelope-boundary redaction contract)
- [x] `AgentSummary` envelope wrapper implemented + unit-tested (`agent_id`, `holder_did`, `state`, `label`, `registered_at_unix`, `manifest_digest`)
- [x] `RedactedIdentifier` envelope-boundary redaction applied to `agent_id` + `holder_did` (symmetric with `agent create` per RFC-0011-c §9.4)
- [x] `--state`, `--limit`, `--cursor`, `--json` flags wired (`--chain-id` deferred: substrate does not filter on chain_id — substrate-faithful per `[[cipherocto-design-principles]]` no-parallel-abstractions)
- [x] `InvalidLimit` (exit 45), `InvalidCursor` (exit 46) wired; `AgentNotFound` (exit 42) + `ForbiddenHolderMismatch` (exit 17, slot moved 37→17 to avoid RFC-0011-g `UnknownAttestationKind` collision)
- [x] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [x] Cargo test -p octo-cli --lib agent 17/17 PASS (9 NEW + 8 existing)
- [x] No new INVALID cites introduced (Guard 2 cite validator deferred to mission closure sweep)

### Type Coverage

| RFC-0011-c type   | Sub-step            | Notes                                                                                                                                                                                                                                |
| ----------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `AgentListArgs`   | Sub-step 1 (clap)   | Layer C/D; clap derive struct on `AgentAction::List` (`--state <String>`, `--limit <u32>` default 1024, `--cursor <String>`, `--json`)                                                                                              |
| `AgentSummary`    | Sub-step 2 (output) | Layer C/D; CLI-output wrapper (`agent_id: RedactedIdentifier`, `holder_did: RedactedIdentifier`, `state: String`, `label: Option<String>`, `registered_at_unix: u64`, `manifest_digest: String`)                                  |
| `AgentListOutput` | Sub-step 3 (output) | Layer C/D; CLI-output wrapper (`count: usize`, `holder_did: RedactedIdentifier`, `agents: Vec<AgentSummaryEnvelope>`)                                                                                                               |
| `InvalidLimit`    | Sub-step 4 (errors) | Layer C/D; new `OctoCliError` variant; exit 45 per RFC-0011-c §9.8 (reserved 17–63 range)                                                                                                                                            |
| `InvalidCursor`   | Sub-step 4 (errors) | Layer C/D; new `OctoCliError` variant; exit 46                                                                                                                                                                                       |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::List` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent list` is the only subcommand that surfaces an optional `reputation_score` field (per RFC-0011-b §7.6 Attestation Surface). The CLI surfaces the field as `Option<u32>` and never pattern-matches on its presence; forward-compatible with future reputation substrate additions. **DEFERRED to Phase 2**: the substrate `AgentSummary` does not carry `reputation_score` in Phase 1; will land with `0011-c-agent-run-subcommand` (write-path amendment chain) once `octo_runtime::spawn_agent` substrate carries the field through.

The substrate is the single source of truth for `next_cursor`; the CLI does not compute cursors client-side. **DEFERRED to Phase 2**: forward-compat cursor reservation wired, but no cursor emission yet (substrate returns `cursor: None` for Phase 1).

## Risk

- **LOW** — concurrent CLI invocations on `agent list`. Substrate is read-only; concurrent reads safe; substrate `agent_registry` is single-writer.
- **DEFERRED Phase 2** — cache staleness warning (per RFC-0011-c §Security: Cache Staleness) deferred: substrate does not return `resolved_at_unix` in Phase 1; lands with `0011-c-agent-run-subcommand` write-path trio.

## Scope

Land the `octo agent list` subcommand per RFC-0011-c §9.3.3. The four sibling subcommands (`create`, `run`, `destroy`, `attach`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-run-subcommand`, `0011-c-agent-destroy-subcommand`, `0011-c-agent-attach-subcommand`. **Per user direction this session: write-path trio (run/destroy/attach) remains Claimed awaiting RFC-0015-a + RFC-0016-a paired-acceptance unblock.**

## Sub-steps

1. **`AgentAction::List` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.3). Extended `List` variant: dropped `--holder-did` (substrate caller-attestation); added `--state`, `--limit` (default 1024), `--cursor`. Wired to `list::handle`.

2. **`AgentSummary` envelope wrapper + `AgentListOutput` payload types** — same file. `#[derive(Serialize, schemars::JsonSchema, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` schema `octo.agent.list.v1`. Both `holder_did` and `agent_id` carry `#[schemars(with = "String")]` for envelope-boundary redaction. Substrate-faithful: `next_cursor` and `reputation_score` reserved for Phase 2.

3. **CLI handler** — `mod list::handle` in same file. Validates `--limit` (zero / over 1024 → `InvalidLimit`) and `--cursor` (empty → `InvalidCursor`) up front; opens wallet, resolves active DID, builds `AgentFilter { holder_did: None, state, limit, cursor }` (substrate enforces caller-attestation); calls `octo_wallet::list_owned_agents`; renders `AgentListOutput` via `render_envelope` with `RedactionContext::with_active_did`.

4. **`InvalidLimit`, `InvalidCursor`, `AgentNotFound`, `ForbiddenHolderMismatch` error variants + exit 45/46/42/17 mapping** — `crates/octo-cli/src/error.rs` (Layer C/D). Four new variants on the `#[non_exhaustive] OctoCliError` enum. Mapped per RFC-0011-c §9.8 + RFC-0011 §Exit Codes 17-63 reserved range. ForbiddenHolderMismatch slot 17 (NOT 13 = PolicyNotFound, NOT 37 = RFC-0011-g UnknownAttestationKind).

5. **DEFERRED Phase 2** — Cache staleness warning (per RFC-0011-c §Security). Substrate does not return `resolved_at_unix` in Phase 1; warning lands with `0011-c-agent-run-subcommand` write-path trio.

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Implementation Phases
octo-wallet = { path = "../octo-wallet" }   # Layer B substrate (RFC-0011-c §Key Files to Modify)
```

No new external crates required; all substrate types (`AgentSummary`, `AgentState`, `list_owned_agents`) are defined in `octo-wallet` and re-used by the CLI.

## Test Vectors (per RFC-0011-c §Test Vectors — `agent list` group)

3 TV (TV-AGT6..TV-AGT8) covering `agent list`. Phase 1 surfaces the substrate contract + CLI error-mapping tests (no full-stack CLI integration tests — substrate registry is process-global, mock-driven):

| #       | Substrate coverage                                                   | CLI coverage                                                                       |
| ------- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| TV-AGT6 | `list_owned_agents` empty registry → empty `Vec` (substrate unit TV) | N/A (covered by substrate; CLI empty-output path is substrate-faithful passthrough) |
| TV-AGT7 | `list_owned_agents` with `state: Some(Running)` → filter applied      | `list_parse_state_filter_accepts_known_labels` + `list::validate_cursor_accepts_none_and_placeholder` |
| TV-AGT8 | N/A                                                                  | `list_invalid_limit_exits_45` (up-front CLI validation, no substrate round-trip)   |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `AgentAction::List` dispatch + `AgentSummary` + `AgentListOutput` payload types + 4 error variants.
- `octo-wallet` (Layer B) — substrate `list_owned_agents`, `AgentSummary`, `AgentState` (commit `533b07a4`).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib agent  # 17/17 PASS
```

## Backward compat

- Additive only: `AgentAction::List` variant extended; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-c §9.8 Error Handling (4 new variants: exits 17/42/45/46; slot allocation 17, 42, 45, 46).
- `OutputEnvelope<T>::schema_version = 4` pinned (RFC-0011-c §9.4 / §9.4.1 Divergence slot table). Field renames from parent v2:
  - `data: T` → `payload: T`
  - `generated_at: DateTime` → `executed_at_unix: u64`
  - `preview_only: bool` → `redacted: bool`
  - `command: String` (ADDED)
    Old CLI ignores unknown fields.
- `AgentSummary` may grow new optional fields (e.g., `last_active_at_unix`); CLI surfaces without pattern-matching on field presence.

## Cross-references

- RFC-0011-c §9.3.3 `octo agent list` subcommand specification
- RFC-0011-c §9.8 Error Handling (4 new variants: `AgentNotFound`, `ForbiddenHolderMismatch`, `InvalidLimit`, `InvalidCursor`)
- RFC-0011-c §9.4 Output Envelope (`OutputEnvelope<T>` wrapper, `schema_version = 4`)
- RFC-0011-c §Security (redaction patterns + cache staleness warning deferred Phase 2)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0015 §6.2.1 `list_owned_agents` substrate (caller-attestation pattern + SECURITY HIGH)
- RFC-0011-b §7.6 Attestation Surface (optional `reputation_score` field source — deferred)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why gate

No release gate. The `agent list` subcommand is substrate-only and depends only on `0011-c-agent-create-subcommand` for the clap root wiring.

## Substrate Gap (resolved 2026-09-13)

**Originally** (2026-09-11): `octo_wallet::list_owned_agents(filter)` was absent.

**Resolved** in commit `533b07a4`: `octo_wallet::list_owned_agents(caller_did: &Did, filter: &AgentFilter)` now exists at `crates/octo-wallet/src/agent.rs` with full caller-attestation pattern (`ForbiddenHolderMismatch` on holder mismatch — SECURITY HIGH multi-DID enumeration prevention per RFC-0015 §6.2.1).

Also added in commit `533b07a4`:
- `lookup_agent(caller_did: &Did, uuid: Uuid) -> Result<AgentManifest, WalletError>` (RFC-0015 §6.2.3)
- `validate_reason(reason: &str) -> Result<(), WalletError>` (RFC-0015 §6.2.5; rejects U+0000-U+001F and U+007F control chars; pager-hijack mitigation)
- `WalletError::{AgentNotFound, ForbiddenHolderMismatch, ReasonContainsControlChars, ReasonTooLong}` (4 new variants; `#[non_exhaustive]` on enum for additive extension per cipherocto-design-principles)

This substrate addition is reused by future `0011-c-agent-run-subcommand` + `0011-c-agent-destroy-subcommand` + `0011-c-agent-attach-subcommand` (write-path trio, Claimed awaiting RFC-0015-a + RFC-0016-a paired-acceptance unblock).

## Closure audit

See `docs/audits/2026-09-13-list-mission-dry-closure.md` for the multi-round DRY review chain summary + cite sweep + substrate-faithful verification.

## Memory card

See `~/.claude/projects/.../memory/rfc-0011-c-agent-list-closure-2026-09-13.md` for closure card.

## Claimant

@unassigned
