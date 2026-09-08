---
name: 0011-c-agent-create-subcommand
description: Land the `octo agent create` subcommand per RFC-0011-c
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
    - mission 0011-core-output-envelope-redaction
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-c-agent-create-subcommand — `octo agent create` subcommand

**Status:** Open
**Substrate:** RFC-0011-c §9.3.1 (`octo agent create <manifest-path>`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root

## Status

Open (RFC-0011-c §Phase 2 CLI wiring, subcommand 1 of 5).

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.3.1 `octo agent create <manifest-path>` and §9.8 Error Handling (5 new variants: `ManifestParseError`, `CapabilityValidationFailed`, `AgentAlreadyExists`).

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of the RFC-0011 amendment chain).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: `0011-core-output-envelope-redaction` → `0011-c-agent-create-subcommand` (this mission must land the `Commands::Agent` enum first so the four sibling subcommand missions can attach to it).

## Acceptance Criteria

- [ ] `octo agent create <manifest-path>` implemented + unit-tested (TV-AGT1, TV-AGT2, TV-AGT3 pass per RFC-0011-c §Test Vectors)
- [ ] `AgentCreateOutput` payload type implemented + unit-tested (`agent_id`, `state`, `registered_at_unix`, `manifest_digest`)
- [ ] `OctoCliRedactor` agent-specific patterns applied per RFC-0011-c §Security Considerations — log-time wholesale `REDACTED_KEY` substitution for `agent_id`, `capability_root`, `holder_did` via `FIELD_TABLE` entries, exercised by TV `tv_agt_redact_log_line_replaces_agent_family_fields`.

> Note: envelope-payload redaction (`RedactedString` newtype + conditional `holder_did == active_did` policy + `agent_id` truncation) is deferred to mission `0011-c-agent-redaction-envelope`. That mission's stub YAML lives at `missions/open/0011-c-agent-redaction-envelope.md` and gates the Phase 2 work on the Phase 1 substrate that this mission locks.
- [ ] `Commands::Agent` clap variant wired (the 4 sibling subcommand missions attach here)
- [ ] `ManifestParseError` (exit 39), `CapabilityValidationFailed(usize)` (exit 40), `AgentAlreadyExists(Uuid)` (exit 41) wired (per RFC-0011-c §9.8 slot allocation 39-52)
- [ ] TTY-aware renderer parity: pretty table on TTY, JSON when stdout is not a TTY OR `--json` set
- [ ] Cross-mission AC: clap root from `0011-core-output-envelope-redaction` reused; no parallel envelope
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-c type                  | Sub-step                | Notes                                                                                                                                                      |
| -------------------------------- | ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AgentCreateArgs`                | Sub-step 1 (clap)       | Layer C/D; clap derive struct (`manifest_path: PathBuf`, `--capability-root`, `--label`, `--json`)                                                          |
| `AgentCreateOutput`              | Sub-step 2 (output)     | Layer C/D; CLI-output wrapper (`agent_id: Uuid`, `state: AgentState`, `registered_at_unix: u64`, `manifest_digest: Hex32`)                                 |
| `Commands::Agent(AgentAction::Create)` | Sub-step 3 (dispatch) | Layer C/D; clap root variant                                                                                                                                |
| `ManifestParseError`             | Sub-step 4 (errors)     | Layer C/D; new `OctoCliError` variant; exit 39 per RFC-0011-c §9.8 (reserved 17–63 range)                                                                   |
| `CapabilityValidationFailed(usize)` | Sub-step 4 (errors) | Layer C/D; new `OctoCliError` variant; exit 40                                                                                                             |
| `AgentAlreadyExists(Uuid)`       | Sub-step 4 (errors)     | Layer C/D; new `OctoCliError` variant; exit 41                                                                                                             |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for Rust snippets + clap wiring patterns. Mirror the §Identity Subcommands pattern for `AgentAction::Create` dispatch + `OutputEnvelope<T>` envelope wrappers.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

`agent create` is the **only** state-transitioning subcommand that lands before `octo-runtime`; the four sibling subcommand missions may proceed in parallel after this mission's clap root wiring lands.

The 6-step capability validation pipeline runs in `octo-wallet` substrate per RFC-0002 §Capability Validation; the CLI never implements a parallel validator.

## Risk

- **MEDIUM** — incorrect clap wiring may break the four sibling subcommand missions. Mitigation: the `Commands::Agent` enum variant is the single hook point; sibling missions add `AgentAction::Run/List/Destroy/Attach` variants without touching the parent enum.
- **LOW** — manifest parse errors surface as `ManifestParseError` (exit 39); substrate rejects malformed manifests before signature verification.

## Scope

Land the `octo agent create` subcommand per RFC-0011-c §9.3.1. The four sibling subcommands (`run`, `list`, `destroy`, `attach`) are out of scope here — see companion missions `0011-c-agent-run-subcommand`, `0011-c-agent-list-subcommand`, `0011-c-agent-destroy-subcommand`, `0011-c-agent-attach-subcommand`.

## Sub-steps

1. **`AgentAction::Create` dispatch + clap wiring** — `crates/octo-cli/src/commands/agent.rs` (NEW; Layer C/D per [[cipherocto-design-principles]]; substrate reference RFC-0011-c §9.2). `enum AgentAction { Create(AgentCreateArgs), Run(...), List(...), Destroy(...), Attach(...) }`. Wire into existing clap root from mission `0011-core-output-envelope-redaction`.

2. **`AgentCreateOutput` payload type** — same file. `#[derive(Serialize, Deserialize, Debug, Clone)]`. Wrapped in `OutputEnvelope<T>` with `schema_version = 4` per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.

3. **CLI handler** — same file. `agent create` calls `octo_wallet::register_agent(manifest, capability_root, active_did)`; applies optional `--label` (stored in substrate); respects `--json` (TTY-override). Substrate runs the 6-step pipeline per RFC-0002 §Capability Validation.

4. **`ManifestParseError`, `CapabilityValidationFailed`, `AgentAlreadyExists` error variants + exit 39/40/41 mapping** — `crates/octo-cli/src/error.rs` (Layer C/D). Add three variants to the `#[non_exhaustive] OctoCliError` enum; map to exits 39/40/41 per RFC-0011-c §9.8 (slot allocation 39-52).

5. **Agent-specific redaction patterns (Phase 1)** — `crates/octo-cli/src/redact.rs` (Layer C/D; per RFC-0011-c §Security Considerations). Add `FIELD_TABLE` entries for `agent_id` / `capability_root` / `holder_did` so the live `OctoCliRedactor` tracing layer (`OctoCliRedactor::on_event` → `redact_by_field`) substitutes `REDACTED_KEY` wholesale in log lines. `state`, `registered_at_unix`, `manifest_digest` are NOT redacted (operator-owned info, not substrate secrets).→ Phase 2 envelope-payload redaction (`RedactedString` newtype + conditional `holder_did == active_did` policy + `agent_id` truncation) deferred to mission `0011-c-agent-redaction-envelope` (stub at `missions/open/0011-c-agent-redaction-envelope.md`).

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Implementation Phases
octo-wallet = { path = "../octo-wallet" }   # Layer B substrate (RFC-0011-c §Key Files to Modify)
```

No new external crates required; all substrate types (`AgentManifest`, `AgentState`, `register_agent`) are defined in `octo-wallet` and re-used by the CLI.

## Test Vectors (per RFC-0011-c §Test Vectors — `agent create` group)

3 TV (TV-AGT1..TV-AGT3) covering `agent create`:

| #       | Subcommand     | Input                              | Expected Output                                              | Notes                                                                |
| ------- | -------------- | ---------------------------------- | ------------------------------------------------------------ | -------------------------------------------------------------------- |
| TV-AGT1 | `agent create` | Valid manifest, valid capability, valid HSM | `AgentCreateOutput { state: REGISTERED, ... }` (exit 0)      | Happy path; substrate returns success                                |
| TV-AGT2 | `agent create` | Invalid manifest signature         | `CapabilityValidationFailed(1)` (exit 40)                    | Fails step 1 of 6-step pipeline                                      |
| TV-AGT3 | `agent create` | Duplicate `agent_id`               | `AgentAlreadyExists(uuid)` (exit 41)                         | Substrate rejects duplicate manifest digest                          |
| TV-AGT13| `agent create` | yesterday's manifest digest        | `ReplayDetected { digest }` (exit 50)                        | RFC-0011-c §9.7 Replay Protection; per RFC-0002 §Security Considerations  |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `AgentAction::Create` dispatch + `AgentCreateOutput` payload type + 3 error variants + agent-specific redaction patterns.
- `octo-wallet` (Layer B) — substrate `register_agent`, `AgentManifest`, `AgentState` (existing types).
- NO new Layer A types introduced.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli --all-targets -- -D warnings  # clean
cargo test -p octo-cli --lib --tests  # green
```

## Backward compat

- Additive only: `Commands::Agent` enum variant + `AgentAction` enum added; no breaking changes to existing public API per RFC migration etiquette.
- CLI exit codes match RFC-0011-c §9.8 Error Handling (3 new variants: exit 39/40/41; slot allocation 39-52).
- `OutputEnvelope<T>::schema_version = 4` pinned (RFC-0011-c §9.4 / §9.4.1 Divergence slot table). Field renames from parent v2:
  - `data: T` → `payload: T`
  - `generated_at: DateTime` → `executed_at_unix: u64`
  - `preview_only: bool` → `redacted: bool`
  - `command: String` (ADDED)
  Old CLI ignores unknown fields.
- Stub deprecation: legacy `octo agent` stub (no subcommand) emits `StaleStub` (exit 65) starting at CLI v1.0 per RFC-0011-c §Compatibility

## Cross-references

- RFC-0011-c §9.3.1 `octo agent create` subcommand specification
- RFC-0011-c §9.8 Error Handling (3 new variants: `ManifestParseError`, `CapabilityValidationFailed`, `AgentAlreadyExists`)
- RFC-0011-c §9.4 Output Envelope (`OutputEnvelope<T>` wrapper, `schema_version = 4`)
- RFC-0011-c §Security (redaction patterns: `agent_id`, `holder_did`, `capability_root`)
- RFC-0011-c §Compatibility
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0002 §Capability Validation (6-step pipeline substrate)
- RFC-0002 §Agent Manifest (manifest wire form substrate)
- RFC-0002 §Security Considerations (replay substrate)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain

## Why gate

No release gate. The `agent create` subcommand is substrate-only (no runtime dependency). It can land as soon as `0011-core-output-envelope-redaction` is merged and the clap root is in place.

## Claimant

@unassigned