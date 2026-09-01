---
name: 0011-c-octo-runtime-substrate
description: Land the `octo-runtime` substrate crate for agent run/attach (Layer B)
metadata:
  node_type: substrate-runtime
  type: substrate-crate-landing
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
release_gate: octo-runtime crate landing
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-c-octo-runtime-substrate — `octo-runtime` substrate crate

**Status:** Open
**Substrate:** RFC-0011-c §9.1 Architecture + §9.10 Substrate `[ADD]` Signatures
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- RFC-0011 — `octo` CLI Substrate (parent RFC)
- RFC-0011-c — `octo agent` Subcommands (consumer of this substrate)
- RFC-0002 — Agent Manifest, Capability, and Lifecycle Substrate

## Status

Open (RFC-0011-c §Implementation Phases Phase 1; Layer B substrate-authoritative).

## Substrate (RFC-0011-c)

Per RFC-0011-c §9.10 Substrate `[ADD]` Signatures, this crate provides:

```rust
pub fn spawn_agent(
    agent_id: Uuid,
    attach_handle: Option<AttachHandle>,
) -> Result<RuntimeHandle, RuntimeError>;

pub fn attach(
    handle: RuntimeHandle,
    since: Option<DateTime<Utc>>,
) -> Result<EventStream, RuntimeError>;
```

The `octo-wallet` additions (`register_agent`, `transition_agent`,
`list_owned_agents`) are independent of this mission and land via
the existing `octo-wallet` crate.

## Parent

RFC-0011-c (Layer C/D operator UX amendment; consumes this Layer B
substrate per the layer direction principle).

## Depends on

See YAML frontmatter `depends_on` block above. This mission is
**substrate-authoritative**: it defines Layer B substrate that
the Layer C/D RFC-0011-c amendment consumes. Per project principle
§RFC Reference Conventions Reaffirmed, the layer direction is
preserved (Layer C/D depends on Layer B; never the reverse).

## Acceptance Criteria

- [ ] `crates/octo-runtime/` workspace crate created with `Cargo.toml` + `src/lib.rs`
- [ ] `spawn_agent(agent_id, attach_handle) -> Result<RuntimeHandle, RuntimeError>` implemented + unit-tested
- [ ] `attach(handle, since) -> Result<EventStream, RuntimeError>` implemented + unit-tested
- [ ] State-machine integration with `octo-wallet` via substrate trait `AgentStateDispatcher`
- [ ] `RuntimeHandle`, `EventStream`, `AttachHandle`, `RuntimeError` types defined (Layer B; years-stable)
- [ ] No new Layer A types introduced
- [ ] Clippy `-p octo-runtime --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-runtime --lib` green
- [ ] Documentation: rustdoc on all public APIs
- [ ] Release gate cleared: crate published in workspace `Cargo.toml`

## Implementation Guide

The `octo-runtime` crate is a Layer B substrate-authoritative
addition. It mirrors the substrate patterns from `octo-wallet`
(state machine integration via shared trait) and `octo-vault`
(LRU + TTL cache pattern). The crate MUST NOT depend on
`octo-cli` (Layer C/D); the dependency direction is
`octo-cli` → `octo-runtime` only.

### File layout

```
crates/octo-runtime/
├── Cargo.toml
└── src/
    ├── lib.rs       # crate root + re-exports
    ├── spawn.rs     # spawn_agent impl
    ├── attach.rs    # attach impl
    ├── handle.rs    # RuntimeHandle + EventStream + AttachHandle
    └── error.rs     # RuntimeError variants (#[non_exhaustive])
```

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

Until this mission lands, the `octo agent run` and
`octo agent attach` CLI subcommands ship as stubs that emit
`RuntimeSubstrateNotReady` (exit 51 per RFC-0011-c §9.8).
The CLI substrate-not-ready behavior is operator-visible only
when invoked against a workspace that lacks the `octo-runtime`
crate; the crate landing unblocks both subcommands.

## Risk

- **HIGH** — state machine integration drift between `octo-wallet`
  and `octo-runtime`. Mitigation: shared `AgentStateDispatcher`
  trait lives in `octo-wallet`; `octo-runtime` consumes it without
  re-implementing the state machine.
- **MEDIUM** — runtime container spawn failure under load.
  Mitigation: `RuntimeError` variants surface the substrate
  reason verbatim to the CLI (`RuntimeSpawnFailed { reason }` exit 44,
  `RuntimeAttachFailed { reason }` exit 49 per RFC-0011-c §9.8).
- **LOW** — long-running `attach` handles leaking. Mitigation:
  `RuntimeHandle::Drop` revokes the handle and unsubscribes from
  the pub-sub bus.

## Scope

Land the `octo-runtime` substrate crate. The `octo-wallet` substrate
additions (`register_agent`, `transition_agent`, `list_owned_agents`)
are out of scope here — they land via the existing `octo-wallet` crate
per RFC-0002 §Capability Validation.

## Cargo deps

```toml
# crates/octo-runtime/Cargo.toml — Layer B; substrate-authoritative
[dependencies]
tokio = { version = "1", features = ["full"] }   # async runtime (Layer D transport; substrate-authoritative)
async-trait = "0.1"                              # async trait dispatch (Layer D transport)
libp2p = { version = "0.55", features = ["tokio"] }   # pub-sub bus substrate (Layer D transport)
octo-wallet = { path = "../octo-wallet" }        # Layer B; state machine substrate (RFC-0002)
serde = { version = "1", features = ["derive"] } # envelope codec (Layer B; RFC-0011)
serde_json = "1"                                 # envelope codec
thiserror = "1"                                  # error enum derivation (Layer C/D)
chrono = { version = "0.4", features = ["serde"] }   # Timestamps (RFC-0011)
```

All deps are Layer B/D substrate (RFC-0011 §Dependencies). No Layer A
deps introduced.

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-runtime` (Layer B) — substrate crate; substrate-authoritative for spawn/attach.
- `octo-wallet` (Layer B) — shared state machine substrate (RFC-0002 §Agent State Machine).
- `octo-cli` (Layer C/D) — consumes this substrate (Layer C/D → Layer B; one-way).
- NO new Layer A types introduced.

## Backward compat

- Additive only: new `octo-runtime` crate lands; no breaking changes
  to existing public API per RFC migration etiquette.
- CLI exit codes: `RuntimeSubstrateNotReady` (exit 51) until this
  mission lands; after landing, `RuntimeSpawnFailed { reason }`
  (exit 44) and `RuntimeAttachFailed { reason }` (exit 49) take effect
  per RFC-0011-c §9.8.
- `OutputEnvelope<T>::schema_version = 4` consumed from RFC-0011-c
  (field renames documented in §9.4.1 Divergence slot table).

## Cross-references

- RFC-0011-c §9.10 Substrate `[ADD]` Signatures — substrate surface spec
- RFC-0011-c §9.1 Architecture — `octo-runtime` Layer B substrate dependency
- RFC-0011-c §Implementation Phases Phase 1 — this mission is Phase 1
- RFC-0011-c §Mission Decomposition — companion mission row
- RFC-0002 §Agent State Machine — shared state machine substrate
- RFC-0002 §Capability Validation — 6-step pipeline substrate
- RFC-0002 §Implementation Phases Phase 3 — `octo-runtime` delivery phase
- RFC-0011 §Dependencies — Layer B/D substrate deps (tokio, async-trait, libp2p)
- [[cipherocto-design-principles]] — Layer B stability contract
- [[no-line-refs-anywhere]] — §section refs only
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern

## Why gate

Release-gated on `octo-runtime` crate landing (the crate MUST be in
the workspace `Cargo.toml` and pass `cargo test`). The two consumer
missions (`0011-c-agent-run-subcommand`, `0011-c-agent-attach-subcommand`)
cite this mission via `release_gate:` frontmatter; they cannot be
marked Completed without this mission being Closed first.

## Claimant

@unassigned
