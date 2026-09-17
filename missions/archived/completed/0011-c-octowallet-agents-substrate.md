---
name: 0011-c-octowallet-agents-substrate
description: Land the AgentRegistry trait facade + InMemoryAgentRegistry impl + transition_agent public surface in octo-wallet Layer A
metadata:
  node_type: substrate-wallet-extension
  type: layer-a-substrate-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-17
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0015
    - RFC-0015-a
    - mission 0011-c-agent-list-subcommand
    - mission 0011-c-agent-destroy-subcommand
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-17
completed_at: 2026-09-17
completed_by: mmacedoeu
implementation_commit: 6aedee18
review_rounds: 8
dry_closure_audit: docs/audits/2026-09-17-0011-c-phase-d1-octowallet-agents-substrate-dry-closure.md
---

# 0011-c-octowallet-agents-substrate — AgentRegistry trait + InMemoryAgentRegistry + transition_agent public surface

**Status:** Completed
**Substrate:** RFC-0011-c §F.6.5 — companion mission for missing `destroy_agent` substrate + unified agent-registry façade
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-list-subcommand` — `list_owned_agents` substrate pre-existing
- Mission `0011-c-agent-destroy-subcommand` — companion CLI subcommand requiring unified registry façade
- RFC-0015-a — paired-acceptance bridge cite for `transition_agent` cfg-gate pattern (§6.5)

## Status

Completed (RFC-0011-c §F.6.5 companion mission; Phase D1 of AttachHandle follow-on cycles A/B/C/D).

## Substrate (RFC-0011-c §F.6.5)

Per RFC-0011-c §F.6.5 + RFC-0015-a §6.4 + §6.5 substrate-faithful state-machine table + paired-acceptance bridge.

## Parent

RFC-0011-c (agent lifecycle amendment; Phase D1 of AttachHandle follow-on cycles). Cycles A/B/C closed prior; this is the cycle-D substrate-side closure.

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: Phase A/B/C AttachHandle follow-on cycles → Phase D1 companion mission.

## Acceptance Criteria

- [x] `pub trait AgentRegistry: Send + Sync + std::fmt::Debug` defined in `crates/octo-wallet/src/agent.rs` (object-safe; `&self` methods only; no generics; no `Self: Sized`)
- [x] `InMemoryAgentRegistry` single canonical impl lands (zero behavior change vs free-fn path)
- [x] `pub fn transition_agent(caller_did, agent_id, target_state, now_unix) -> Result<AgentState, WalletError>` promoted to public surface
- [x] `transition_agent` Preconditions doc-block bullets cite RFC-0015-a §6.5 paired-acceptance bridge (NOT §6.4 cfg-gate — corrected via R4.5)
- [x] `transition_agent` free fn feature-gate error string cite also propagated §6.4 → §6.5 (R4.5 cross-verified via grep)
- [x] 3 unit tests added: `agent_registry_kind_returns_canonical_string`, `agent_registry_dispatches_through_free_fn_surface`, `agent_registry_send_sync_via_arc`
- [x] `cargo fmt --all -- --check` clean
- [x] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean
- [x] `cargo test -p octo-wallet --lib agent_registry` 3/3 pass
- [x] 256/256 octo-wallet lib tests pass (3 NEW + 253 pre-existing)

### Type Coverage

| RFC-0011-c type           | Sub-step             | Notes                                                                                                                                                          |
| ------------------------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pub trait AgentRegistry` | Sub-step 1 (trait)   | Layer A; object-safe (`&self` methods only); mirrors `RevocationStore` pattern per per-extension-crate + registry pattern per [[cipherocto-design-principles]] |
| `InMemoryAgentRegistry`   | Sub-step 2 (impl)    | Layer A; single canonical impl; zero behavior change vs free-fn path                                                                                           |
| `pub fn transition_agent` | Sub-step 3 (free fn) | Layer A; promoted to public surface; `caller_did: &Did` + `agent_id: Uuid` + `target_state: AgentState` + `now_unix: u64`                                      |
| 3 unit tests              | Sub-step 4 (TV)      | Layer A; `Arc<dyn AgentRegistry>` dispatch + Send/Sync + kind identity                                                                                         |

## Implementation Guide

See `docs/07-developers/octo-wallet-implementation-guide.md` §AgentRegistry Façade for Rust snippets. Mirror the `RevocationStore` pattern: trait in core (Layer A), impl in same crate (Layer A), dispatch through `Arc<dyn AgentRegistry>` at CLI layer (Layer C/D).

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

The trait is `Send + Sync + std::fmt::Debug` to mirror `RevocationStore` (per [[cipherocto-design-principles]] §Per-extension crates + registry pattern). All methods are `&self` (no `&mut self`, no `Self: Sized`, no generics) for object-safety proven via the `Arc<dyn AgentRegistry>` dispatch test.

`transition_agent` cite: §6.5 is the canonical cfg-gate paired-acceptance bridge location per RFC-0015-a. §6.4 is the Substrate-faithful state-machine table (different surface). Cross-verified via grep on the RFC source during R4.5 cite correction.

The 3 NEW tests assert:

1. Object-safety via `Arc<dyn AgentRegistry>` dispatch (kind test)
2. Trait methods dispatch through to free fn surface (zero behavior drift)
3. `Send + Sync` compile-time assertion (required for cross-process use)

## Risk

- **NONE** — substrate-only additive extension. No breaking changes; no new abstractions outside the existing pattern.
- **DEFERRED Phase D2** — Key rotation for `sign_attach_handle_payload` is HARD BLOCKED on PQC direction per CLAUDE.md §Architectural Principles. Will land when PQC direction is known.

## Scope

Land the AgentRegistry trait facade + InMemoryAgentRegistry impl + transition_agent public surface per RFC-0011-c §F.6.5. **DEFERRED Phase D2**: Key rotation (PQC-blocked).

## Sub-steps

1. **`pub trait AgentRegistry` definition** — `crates/octo-wallet/src/agent.rs` (Layer A). Object-safe trait with `Send + Sync + std::fmt::Debug` super-traits. Methods: `kind() -> &'static str` (per-impl diagnostic identity), `transition_agent(caller_did, agent_id, target_state, now_unix) -> Result<AgentState, WalletError>` (cfg-gate paired-acceptance pattern). Preconditions doc-block cites RFC-0015-a §6.5 paired-acceptance bridge.

2. **`InMemoryAgentRegistry` impl** — same file (Layer A). Single canonical impl; `kind` returns `"InMemoryAgentRegistry"`; `transition_agent` delegates to the existing `transition_agent` free fn (zero behavior drift).

3. **`pub fn transition_agent` promotion** — change visibility from `pub(crate)` to `pub` on the free fn (Layer A; per RFC-0015-a Appendix A substrate-faithful state-machine table).

4. **3 unit tests** — `mod tests` in same file. `agent_registry_kind_returns_canonical_string` (Arc<dyn> dispatch + canonical string); `agent_registry_dispatches_through_free_fn_surface` (trait fn → free fn round-trip); `agent_registry_send_sync_via_arc` (compile-time Send + Sync assertion).

## Cargo deps

```toml
# crates/octo-wallet/Cargo.toml — additive per RFC-0011-c §F.6.5
# No new external crates; Layer A substrate-only extension.
```

No new external crates required; `AgentRegistry` is defined in `octo-wallet` (Layer A) and re-exported through `octo-wallet::lib.rs` per the existing pattern.

## Test Vectors (per RFC-0011-c §F.6.5)

3 unit tests covering the AgentRegistry trait surface:

| #          | Substrate coverage                                                                  |
| ---------- | ----------------------------------------------------------------------------------- |
| TV-AGT-D1a | `agent_registry_kind_returns_canonical_string` — Arc<dyn> dispatch + literal kind   |
| TV-AGT-D1b | `agent_registry_dispatches_through_free_fn_surface` — trait fn → free fn round-trip |
| TV-AGT-D1c | `agent_registry_send_sync_via_arc` — Send + Sync compile-time assertion             |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-wallet` (Layer A) — `AgentRegistry` trait + `InMemoryAgentRegistry` impl + `pub fn transition_agent` promotion + 3 tests
- NO new Layer B/C/D/E types introduced

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings  # clean
cargo test -p octo-wallet --lib agent_registry  # 3/3 PASS
cargo test -p octo-wallet --lib  # 256/256 PASS
```

## Backward compat

- Additive only: `AgentRegistry` trait + `InMemoryAgentRegistry` impl are new public surface; no breaking changes to existing public API per RFC migration etiquette.
- `transition_agent` visibility change `pub(crate)` → `pub` is substrate-faithful per RFC-0015-a Appendix A (already documented as public surface).
- No CLI exit code changes; no CLI variant changes; no new variants on `WalletError`.

## Cross-references

- RFC-0011-c §F.6.5 — companion mission scope
- RFC-0015-a §6.4 — Substrate-faithful state-machine table (free fn behavior source)
- RFC-0015-a §6.5 — Layer A Paired-Acceptance Bridge (cfg-gate cite for trait facade Preconditions)
- RFC-0015-a Appendix A — public surface for `transition_agent`
- [[cipherocto-design-principles]] — Layer A stability contract + per-extension-crate + registry pattern
- [[0011-c-attachhandle-followon-phase-a]] — Phase A prose-only closure (RFC-0011-c §9.8 variant arithmetic)
- [[0011-c-transport-extension-phase-b-dry-closure-2026-09-17]] — Phase B Layer D extension crates
- [[0011-c-phase-c-cross-process-revocation-dry-closure]] — Phase C cross-process revocation propagation

## Why gate

No release gate. Phase D1 is substrate-only additive; the only substrate change is the visibility flip on `transition_agent` (already documented in RFC-0015-a Appendix A as public surface).

## Closure audit

See `docs/audits/2026-09-17-0011-c-phase-d1-octowallet-agents-substrate-dry-closure.md` for the multi-round DRY review chain summary + cite correction + substrate-faithful verification.

## Memory card

See `~/.claude/projects/.../memory/0011-c-octowallet-agents-substrate-phase-d1-dry-closure-2026-09-17.md` for closure card.

## Claimant

@unassigned
