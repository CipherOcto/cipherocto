---
name: 0011-c-agent-list-subcommand
description: Land the `octo agent list` subcommand per RFC-0011-c
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
    - mission 0011-c-agent-create-subcommand
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-c-agent-list-subcommand — `octo agent list` subcommand

**Status:** Open
**Substrate:** RFC-0011-c §9.3.3 (`octo agent list`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-create-subcommand` — `Commands::Agent` clap root variant
- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Open (RFC-0011-c §Phase 2 CLI wiring, subcommand 3 of 5).

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.3 `octo agent list` and §9.8 Error Handling (`InvalidLimit`, `InvalidCursor`).

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-c-agent-create-subcommand` → `0011-c-agent-list-subcommand` (this mission attaches to the `Commands::Agent` enum landed by the create mission).

## Acceptance Criteria

- [ ] `octo agent list` implemented + unit-tested (TV-AGT6, TV-AGT7, TV-AGT8 pass per RFC-0011-c §Test Vectors)
- [ ] `AgentListOutput` payload type implemented + unit-tested (`agents`, `next_cursor`, `resolved_at_unix`)
- [ ] `AgentSummary` payload type implemented + unit-tested (`agent_id`, `state`, `holder_did`, `capability_root`, `registered_at_unix`, optional `reputation_score`)
- [ ] `OctoCliRedactor` patterns applied (same set as `agent create` per RFC-0011-c §Security)
- [ ] `--state`, `--chain-id`, `--limit`, `--cursor`, `--json` flags wired (per RFC-0011-c §9.3.3)
- [ ] `InvalidLimit` (exit 45), `InvalidCursor` (exit 46) wired (per RFC-0011-c §9.8 slot allocation 39-52)
- [ ] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [ ] Cache staleness warning surfaced when projection older than cache TTL (per RFC-0011-c §Security: Cache Staleness)
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-c type   | Sub-step            | Notes                                                                                                                                                                               |
| ----------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AgentListArgs`   | Sub-step 1 (clap)   | Layer C/D; clap derive struct (`--state <AgentState>`, `--chain-id <cid>`, `--limit <u32>`, `--cursor <string>`, `--json`)                                                          |
| `AgentSummary`    | Sub-step 2 (output) | Layer C/D; CLI-output wrapper (`agent_id: Uuid`, `state: AgentState`, `holder_did: Did`, `capability_root: MacaroonId`, `registered_at_unix: u64`, `reputation_score: Option<u32>`) |
| `AgentListOutput` | Sub-step 3 (output) | Layer C/D; CLI-output wrapper (`agents: Vec<AgentSummary>`, `next_cursor: Option<String>`, `resolved_at_unix: u64`)                                                                 |
| `InvalidLimit`    | Sub-step 4 (errors) | Layer C/D; new `OctoCliError` variant; exit 45 per RFC-0011-c §9.8 (reserved 17–63 range)                                                                                           |
| `InvalidCursor`   | Sub-step 4 (errors) | Layer C/D; new `OctoCliError` variant; exit 46                                                                                                                                      |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::List` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent list` is the only subcommand that surfaces an optional `reputation_score` field (per RFC-0011-b §7.6 Attestation Surface). The CLI surfaces the field as `Option<u32>` and never pattern-matches on its presence; forward-compatible with future reputation substrate additions.

The substrate is the single source of truth for `next_cursor`; the CLI does not compute cursors client-side.

## Risk

- **MEDIUM** — cache staleness may surface stale agent records. Mitigation: cache hit surfaces `>N seconds old; consider --no-cache` warning to stderr (per RFC-0011-c §Security: Cache Staleness); staleness is informational, not an error.
- **LOW** — concurrent CLI invocations on `agent list`. Substrate is read-only; concurrent reads safe; substrate `agent_registry` is single-writer.

## Scope

Land the `octo agent list` subcommand per RFC-0011-c §9.3.3. The four sibling subcommands (`create`, `run`, `destroy`, `attach`) are out of scope here — see companion missions `0011-c-agent-create-subcommand`, `0011-c-agent-run-subcommand`, `0011-c-agent-destroy-subcommand`, `0011-c-agent-attach-subcommand`.

## Sub-steps

1. **`AgentAction::List` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §9.3.3). Add `List(AgentListArgs)` variant to existing `AgentAction` enum from mission `0011-c-agent-create-subcommand`.

2. **`AgentSummary` + `AgentListOutput` payload types** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table. `reputation_score: Option<u32>` is conditional on RFC-0011-b substrate; if absent, the field is `None`.

3. **CLI handler** — same file. `agent list` calls `octo_wallet::list_owned_agents(filter, active_did)`; applies `--state` (server-side filter) and `--chain-id` (canonical form per RFC-0010); respects `--limit`/`--cursor` defensively. Respects `--json` (TTY-override).

4. **`InvalidLimit`, `InvalidCursor` error variants + exit 45/46 mapping** — `crates/octo-cli/src/error.rs` (Layer C/D). Add two variants to the `#[non_exhaustive] OctoCliError` enum; map to exits 45/46 per RFC-0011-c §9.8 (slot allocation 39-52).

5. **Cache staleness warning** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; per RFC-0011-c §Security). When projection age exceeds the cache TTL, emit a warning to stderr: `>N seconds old; consider --no-cache`. Staleness is informational, not an error.

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Implementation Phases
octo-wallet = { path = "../octo-wallet" }   # Layer B substrate (RFC-0011-c §Key Files to Modify)
```

No new external crates required; all substrate types (`AgentSummary`, `AgentState`, `list_owned_agents`) are defined in `octo-wallet` and re-used by the CLI.

## Test Vectors (per RFC-0011-c §Test Vectors — `agent list` group)

3 TV (TV-AGT6..TV-AGT8) covering `agent list`:

| #       | Subcommand   | Input                               | Expected Output                                          | Notes                                                  |
| ------- | ------------ | ----------------------------------- | -------------------------------------------------------- | ------------------------------------------------------ |
| TV-AGT6 | `agent list` | 50 owned agents, no filter          | `AgentListOutput { agents: [...50], next_cursor: None }` | All 50 in single page                                  |
| TV-AGT7 | `agent list` | `--state ACTIVE --chain-id chain-a` | Filtered list of ACTIVE agents on chain-a                | Client-side filter                                     |
| TV-AGT8 | `agent list` | `--limit 0`                         | `InvalidLimit` (exit 45)                                 | Defensive validation; substrate validates `limit >= 1` |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `AgentAction::List` dispatch + `AgentSummary` + `AgentListOutput` payload types + 2 error variants + cache staleness warning.
- `octo-wallet` (Layer B) — substrate `list_owned_agents`, `AgentSummary`, `AgentState` (existing types).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `AgentAction::List` variant added; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-c §9.8 Error Handling (2 new variants: exit 45/46; slot allocation 39-52).
- `OutputEnvelope<T>::schema_version = 4` pinned (RFC-0011-c §9.4 / §9.4.1 Divergence slot table). Field renames from parent v2:
  - `data: T` → `payload: T`
  - `generated_at: DateTime` → `executed_at_unix: u64`
  - `preview_only: bool` → `redacted: bool`
  - `command: String` (ADDED)
    Old CLI ignores unknown fields.
- `AgentSummary` may grow new optional fields (e.g., `last_active_at_unix`); CLI surfaces without pattern-matching on field presence.

## Cross-references

- RFC-0011-c §9.3.3 `octo agent list` subcommand specification
- RFC-0011-c §9.8 Error Handling (2 new variants: `InvalidLimit`, `InvalidCursor`)
- RFC-0011-c §9.4 Output Envelope (`OutputEnvelope<T>` wrapper, `schema_version = 4`)
- RFC-0011-c §Security (redaction patterns + cache staleness warning)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0011-b §7.6 Attestation Surface (optional `reputation_score` field source)
- RFC-0010 §2 ledger_chain_registry Table for --chain-id)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why gate

No release gate. The `agent list` subcommand is substrate-only and depends only on `0011-c-agent-create-subcommand` for the clap root wiring.

## Substrate Gap (hard-checked 2026-09-11)

Substrate verification (`octo_wallet::agent` module map) confirms the
type surface (`AgentManifest`, `AgentState`, `AgentSummary`,
`AgentFilter`, `CapabilityId`) exists at `crates/octo-wallet/src/agent.rs`.
The function surface required by this mission is **absent**:

- `octo_wallet::list_owned_agents(filter)` referenced by Sub-step 3
  (CLI handler) does not exist in `crates/octo-wallet/src/` (verified
  via `grep -rE "pub (fn|async fn) " crates/octo-wallet/src/`).

**Unblock path:** add `pub fn list_owned_agents(filter: AgentFilter) -> Result<Vec<AgentSummary>, WalletError>`
to `crates/octo-wallet/src/agent.rs` (small additive; ~20 LoC + tests)
before this mission's Sub-step 3 lands. The new function lives in
`octo-wallet` (Layer B) and is reusable by `octo_runtime::spawn_agent`
guard logic + future RFC-0011-f mesh RPC surface.

**Implementation cannot proceed** until the substrate addition lands.
Mission remains `Claimed` per [[memory-is-never-status-ground-truth]].

## Claimant

@unassigned
