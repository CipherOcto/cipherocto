---
name: 0011-c-octo-runtime-attachhandle-substrate
description: Land AttachHandle token pathway per RFC-0011-c §Follow-on; substrate + CLI + revocation + persistence in single mission
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension-plus-substrate
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-16
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
    - RFC-0015-a
    - RFC-0016-a
    - mission 0011-c-agent-run-subcommand
    - mission 0011-c-agent-attach-subcommand
release_gate: substrate mint/decode + multi-process bind + revocation + persistence + CLI run/attach/revoke all land before attach handler binds to in-process RuntimeHandle
release_gate_blocked: 0011-c-agent-attach-subcommand dispatch handler returns RuntimeSubstrateNotReady exit 51 unconditionally until this mission lands
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-16
substrate_unblocked: 2026-09-16
implementation_state: planning
dry_audit: docs/audits/2026-09-16-0011-c-attachhandle-mission-draft.md
---

# 0011-c-octo-runtime-attachhandle-substrate — AttachHandle token pathway

**Status:** Open — designed 2026-09-16 per hard audit 2026-09-15 + user direction (Q1+Q2+Q3). Single mission covers substrate + CLI + revocation + persistence per user direction (Q2: single mission, Q3: deferred-now-in-scope).
**Substrate:** RFC-0011-c §Follow-on (NEW; added per this cycle)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-agent-run-subcommand` — `Commands::Agent::Run` clap variant extended in §Sub-step 1 below
- Mission `0011-c-agent-attach-subcommand` — `Commands::Agent::Attach` dispatch stub at `crates/octo-cli/src/commands/agent.rs:1180` replaced in §Sub-step 2 below
- RFC-0015-a — `transition_agent` (Layer B; signing surface reused by `sign_attach_handle`)
- RFC-0016-a — audit substrate for `mint_attach_handle` event persistence

## Status

Open. RFC-0011-c §Follow-on text refresh + this mission YAML + CLI wiring + substrate code land in this single cycle per paired-acceptance sequencing. DRY gate: R1 → R1.5 → R2 (zero-finding = DRY CLOSED gate) per user Q3 "shorter" directive.

## Substrate (RFC-0011-c §Follow-on)

Per RFC-0011-c §Follow-on (NEW; added by this mission) the AttachHandle token pathway covers four layers of work:

### §F.1 Encoding (NEW)

`crates/octo-runtime/src/handle/encoding.rs` (NEW; ~60 LoC + tests):

- `encode_token(token: &AttachHandle) -> Result<Vec<u8>, AttachError>` — length-prefixed canonical encoding, version byte `0x00`, BLAKE3-checked
- `decode_token(bytes: &[u8]) -> Result<AttachHandle, AttachError>` — symmetric decoder with signature verification on decode
- Canonical encoding pinned to version 0; future versions increment byte `0x01 0x02 …` per octo-runtime canonical-bytes invariant (mirrors RFC-0016-a §6.10)

### §F.2 Token Substrate (NEW)

`crates/octo-runtime/src/handle.rs` (NEW; ~140 LoC + tests):

- `pub struct AttachHandle { session_id: SessionId, mint_timestamp_unix: u64, ttl_unix: u64, signature: Ed25519Signature, payload: AttachPayload, transport: Transport }`
- `pub enum Transport { InProcess, UnixSocket(String) }` — discriminator (Q-deferred 1: multi-process bind)
- `pub struct AttachPayload { agent_id: Uuid, since_cursor: u64 }`
- `pub type SessionId = [u8; 32]` — random per `spawn_agent`
- `pub fn mint_attach_handle(holder_did: &Did, agent_id: Uuid, session_id: SessionId, since_cursor: u64, ttl_unix: u64, transport: Transport) -> Result<AttachHandle, AttachError>` (Layer B; calls `octo_wallet::sign_attach_handle`)
- `pub async fn attach(token: &AttachHandle, since_unix: u64) -> Result<AttachedSession, AttachError>` — validates signature + session_id match + `now_unix <= ttl_unix` + `since_unix >= mint_timestamp_unix`; binds to channel (InProcess: direct handle; UnixSocket: connect `XDG_RUNTIME_DIR/octo-attach-<session_id>.sock` with `os.write_to_temp()` fallback)
- `AttachedSession { event_cursor: u64, broadcast_rx: tokio::sync::broadcast::Receiver<RuntimeEvent> }`

### §F.3 Persistence (NEW — Q-deferred 3)

`crates/octo-runtime/src/persistence.rs` (NEW; ~80 LoC + tests):

- `pub fn persist_event_cursor(agent_id: Uuid, cursor: u64) -> Result<(), PersistenceError>` — Stoolap ledger extension; gated on `cfg(feature = "octo-runtime-persistence")` mirroring RFC-0016-a §6.4 paired-invariance
- `pub fn load_event_cursor(agent_id: Uuid) -> Result<Option<u64>, PersistenceError>` — symmetric
- `pub fn revoke_attach_token(session_id: SessionId) -> Result<(), RevocationError>` — in-memory revocation set; expired-by-revocation surfaces as `AttachHandleExpired` exit 53 (Q-deferred 2)
- `pub fn is_token_revoked(session_id: &SessionId) -> bool` — fast-path check on `attach()` invocation

### §F.4 Errors (NEW)

`pub enum AttachError` in `crates/octo-runtime/src/handle/error.rs`:

- `Expired { session_id: SessionId, expired_at_unix: u64, now_unix: u64 }` (mirror → OctoCliError exit 53)
- `BadSignature { reason: String }` (mirror → OctoCliError exit 54)
- `SessionMismatch { declared: SessionId, actual: SessionId }` (mirror → OctoCliError exit 55)
- `UnknownSession { session_id: SessionId }` (mirror → OctoCliError exit 56)
- `PersistenceError(String)` (mirror → OctoCliError exit 57)
- `RevocationError(String)` (mirror → exit 58; closes the protocol-layer revocation gap)

### §F.5 Signing Surface (REUSED)

`crates/octo-wallet/src/crypto.rs` (EXISTING; additive):

- `pub fn sign_attach_handle(did: &Did, payload: &AttachPayload, mint_timestamp_unix: u64, ttl_unix: u64) -> Result<Ed25519Signature, CryptoError>` — reuses Ed25519 substrate; adds RFC-0015-a §6.4 paired-invariance guarantee

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of RFC-0011 amendment chain). §Follow-on text refresh appends §F.1-§F.5 sections to RFC-0011-c body via RFC doc amendment commit (paired with this mission per [[no-phantom-mission-pointer]]).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing:

1. RFC-0011-c §Follow-on text refresh lands (paired with this mission YAML)
2. Substrate code (mint/decode + persistence + revocation + signing) lands
3. CLI wiring (`agent run --detach --token-file` + `agent attach --token-file` + `octo revoke-attach`) lands
4. Mission DRAM cycle: R1 → R1.5 → R2 zero-finding = DRY CLOSED

## Acceptance Criteria

- [ ] **AC-1** `AttachHandle` struct (Layer B) with 6 fields defined at `crates/octo-runtime/src/handle.rs` per RFC-0011-c §F.2
- [ ] **AC-2** `mint_attach_handle` returns signed token (Layer B) per RFC-0011-c §F.2 (calls `octo_wallet::sign_attach_handle` per RFC-0011-c §F.5)
- [ ] **AC-3** `encode_token` + `decode_token` round-trip (Layer B) per RFC-0011-c §F.1 with signature verification
- [ ] **AC-4** `attach()` binds in-process or via UnixSocket based on `Transport` discriminator (Q-deferred 1) per RFC-0011-c §F.2
- [ ] **AC-5** `agent run --detach --token-file <path>` mints + writes token to file (Layer C/D) per RFC-0011-c §Sub-step §F.2
- [ ] **AC-6** `agent attach --token-file <path>` reads + binds (Layer C/D) per RFC-0011-c §Sub-step §F.3 (replaces stub at `crates/octo-cli/src/commands/agent.rs:1180`)
- [ ] **AC-7** 4 `OctoCliError` variants (exits 53-56) wired at `crates/octo-cli/src/error.rs` per RFC-0011-c §F.4 mirror
- [ ] **AC-8** `octo revoke-attach <token-hex>` primitive (Q-deferred 2) per RFC-0011-c §F.3 with in-memory revocation set
- [ ] **AC-9** `persist_event_cursor` + `load_event_cursor` via Stoolap ledger extension (Q-deferred 3) per RFC-0011-c §F.3 gated on `cfg(feature = "octo-runtime-persistence")`
- [ ] **AC-10** Cargo clippy -p octo-runtime -p octo-wallet -p octo-cli --all-targets -- -D warnings clean
- [ ] **AC-11** Cargo test -p octo-runtime --lib --tests green + -p octo-wallet --lib green + -p octo-cli --lib green
- [ ] **AC-12** Layer direction verified (no reverse deps per [[cipherocto-design-principles]]) + DRY R1+R2 zero-finding gate achieved

### Type Coverage

| RFC-0011-c type                                | Sub-step | Notes                                                                                                    |
| ---------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------- |
| `AttachHandle`                                 | F.2      | Layer B; struct with 6 fields + `Transport` discriminator (Q-deferred 1)                                 |
| `Transport` (enum: `InProcess` / `UnixSocket`) | F.2      | Layer B; binding transport selector (Q-deferred 1)                                                       |
| `mint_attach_handle`                           | F.2      | Layer B; calls `octo_wallet::sign_attach_handle` per F.5                                                 |
| `attach()`                                     | F.2      | Layer B async; binds to in-process channel OR UnixSocket based on discriminator                          |
| `encode_token` + `decode_token`                | F.1      | Layer B; canonical encoding v0 with signature verify-on-decode                                           |
| `revoke_attach_token` + `is_token_revoked`     | F.3      | Layer B; in-memory revocation set (Q-deferred 2)                                                         |
| `persist_event_cursor` + `load_event_cursor`   | F.3      | Layer B; Stoolap ledger extension, feature-gated (Q-deferred 3)                                          |
| `sign_attach_handle`                           | F.5      | Layer B reused; calls Ed25519 substrate (Layer A frozen)                                                 |
| `AttachError` (6 variants)                     | F.4      | Layer B; mirror → `OctoCliError` 4 variants (exits 53-56) + 2 substrate-error passthroughs (exits 57-58) |
| `OctoCliError::AttachHandleExpired {…}`        | CLI      | Layer C/D; mirror exit 53 per RFC-0011-c §F.4 (slot allocation 39-58)                                    |
| `OctoCliError::AttachHandleBadSignature`       | CLI      | Layer C/D; mirror exit 54                                                                                |
| `OctoCliError::AttachSessionMismatch {…}`      | CLI      | Layer C/D; mirror exit 55                                                                                |
| `OctoCliError::AttachSessionUnknown {…}`       | CLI      | Layer C/D; mirror exit 56                                                                                |
| `OctoCliError::PersistenceError(String)`       | CLI      | Layer C/D; passthrough exit 57                                                                           |
| `OctoCliError::RevocationError(String)`        | CLI      | Layer C/D; passthrough exit 58                                                                           |

## Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Agent Subcommands for clap wiring patterns. Mirror the `agent attach` dispatch replace-stub pattern at `crates/octo-cli/src/commands/agent.rs:1180`.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

This mission was scoped per hard audit 2026-09-15 + user direction Q1+Q2+Q3. The `0011-c-agent-attach-subcommand` mission's release gate cleared on this mission landing. Single-mission design per [[no-parallel-abstractions]] principle (one mission = one cohesion: the AttachHandle pathway). 3 deferred items (Q-deferred 1/2/3) brought in scope per user direction so the attach handler binds to in-process `RuntimeHandle` and survives protocol-layer revoke + cursor-persistence primitives.

RFC-0011-c §Follow-on text refresh lands via paired commit with this mission YAML per [[no-phantom-mission-pointer]] rule. The §Follow-on text covers §F.1 (Encoding) through §F.5 (Signing Surface) sections verbatim mirrored from this YAML's §Substrate (RFC-0011-c §Follow-on) section.

## Risk

- **MEDIUM** — UnixSocket path may fail on permission-restricted filesystems (`XDG_RUNTIME_DIR` absent, `$TMPDIR` read-only). Mitigation: `os.write_to_temp()` fallback to `$TMPDIR/octo-attach-<session>.sock` per RFC-0011-c §F.2; surface `AttachError::SocketPathUnavailable` exit 56 alternative
- **MEDIUM** — In-memory revocation set lost on process restart; cross-process revocation propagation out of scope (deferred to RFC-0011-c §Future Work §F.7)
- **LOW** — Persistence feature gate (`octo-runtime-persistence`) may not be enabled in default builds; `persist_event_cursor` returns `PersistenceError("feature disabled")` exit 57 in default builds
- **LOW** — Ed25519 signature substrate (Layer A frozen) reuses `sign_attach_handle`; no new crypto-layer assumptions

## Scope

Land the AttachHandle token pathway end-to-end per RFC-0011-c §Follow-on. Three sibling deferred extensions (Q-deferred 1 multi-process bind, Q-deferred 2 revocation, Q-deferred 3 cursor persistence) all in scope per user direction Q2. Out-of-scope (deferred to next cycle):

- Cross-process revocation propagation (separate process → in-memory set sync; defers to RFC-0011-c §Future Work §F.7)
- Key rotation for `sign_attach_handle` (deferred to RFC-0015-a §Future Work §6.5)
- `octo-runtime-persistence` feature gate activation in default builds (deferred to RFC-0011-c §Future Work §F.8)

## Sub-steps

1. **RFC-0011-c §Follow-on text refresh** — `rfcs/accepted/process/0011-c-agent-lifecycle.md` Layer direction amendment. Append §F.1-§F.5 sections. Pair-commit with mission YAML per [[no-phantom-mission-pointer]].

2. **Substrate code** — `crates/octo-runtime/src/handle.rs` + `crates/octo-runtime/src/handle/encoding.rs` + `crates/octo-runtime/src/handle/error.rs` + `crates/octo-runtime/src/persistence.rs` + `crates/octo-wallet/src/crypto.rs` additive `sign_attach_handle`. ~340 LoC + tests. Layer B.

3. **CLI extension to `agent run --detach --token-file <path>`** — `crates/octo-cli/src/commands/agent.rs` (Layer C/D; substrate reference RFC-0011-c §F.2). Add `--token-file <path>` clap arg to existing `AgentAction::Run` variant. On `--detach` dispatch, after `octo_runtime::spawn_agent(...)` returns, mint token via `octo_runtime::mint_attach_handle(holder_did, ...)` + serialize to file via `octo_runtime::encode_token`. Token NOT included in `AgentRunOutput` payload (side-channel credential, not audit data).

4. **CLI dispatch replacement at `agent attach --token-file <path>`** — replaces stub at `crates/octo-cli/src/commands/agent.rs:1180`. Read token bytes from `--token-file` → `octo_runtime::decode_token(...)` → `octo_runtime::attach(&token, since_unix)` async. Populates `AgentAttachOutput { agent_id, runtime_handle, attached_at_unix, event_cursor }`.

5. **CLI primitive `octo revoke-attach <token-hex>`** — new top-level subcommand at `crates/octo-cli/src/commands/mod.rs`. Parses `<token-hex>` arg, decodes token (signature verified), calls `octo_runtime::revoke_attach_token(token.session_id)`. Returns `OctoCliError::RevocationError(String)` exit 58 on failure OR `exit 0` on success with `RevokeOutput { session_id, revoked_at_unix }`.

6. **CLI error variants** — `crates/octo-cli/src/error.rs` adds 6 `OctoCliError` variants mapping to exits 53-58 per RFC-0011-c §F.4 mirror.

## Cargo deps

```toml
# crates/octo-runtime/Cargo.toml — additive per RFC-0011-c §Follow-on
# (no new deps; tokio + ed25519 + tokio::sync::broadcast already pulled by existing surface)

# crates/octo-runtime/Cargo.toml — conditional new dep
[features]
default = []
octo-runtime-persistence = ["dep:stoolap"]  # per RFC-0011-c §F.3 paired-invariance

# crates/octo-wallet/Cargo.toml — additive per RFC-0011-c §F.5
# (no new deps; ed25519-dalek already pulled)

# crates/octo-cli/Cargo.toml — additive per RFC-0011-c §Sub-steps 3-6
# (no new deps; octo-runtime + octo-wallet already listed per RFC-0011-c §Implementation Phases)
```

## Test Vectors (per RFC-0011-c §Test Vectors — `agent attach` group, extended)

6 TV (TV-AGT11-AGT14 existing + TV-AGT15-AGT18 NEW) covering the AttachHandle pathway:

| #        | Subcommand                               | Input                                                        | Expected Output                                                         | Notes                                           |
| -------- | ---------------------------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------- | ----------------------------------------------- |
| TV-AGT11 | `agent attach`                           | Running agent (in-process)                                   | `AgentAttachOutput { runtime_handle: ..., event_cursor: ... }` (exit 0) | Read-only in-process attach                     |
| TV-AGT12 | `agent attach`                           | Terminated agent                                             | `AgentNotRunning(uuid)` (exit 48)                                       | Already exists; per mission 0011-c-agent-attach |
| TV-AGT13 | `agent attach`                           | Token expired (`now_unix > ttl_unix`)                        | `AttachHandleExpired { ... }` (exit 53)                                 | NEW; per §F.2                                   |
| TV-AGT14 | `agent attach`                           | Token signature mismatched                                   | `AttachHandleBadSignature { reason }` (exit 54)                         | NEW; per §F.2 + §F.5                            |
| TV-AGT15 | `agent run --detach --token-file <path>` | Fresh spawn                                                  | Token written to file; `AgentRunOutput` (exit 0) excludes token         | NEW; per §Sub-step 3                            |
| TV-AGT16 | `octo revoke-attach <hex>`               | Revoked session                                              | `RevokeOutput { session_id, revoked_at_unix }` (exit 0)                 | NEW; per §Sub-step 5                            |
| TV-AGT17 | `octo revoke-attach` → re-attach         | After revocation                                             | `AttachHandleExpired { ... }` (exit 53) (mirrors TV-AGT13)              | NEW; per §F.3 revocation                        |
| TV-AGT18 | `agent attach`                           | Multi-process via UnixSocket (`Transport::UnixSocket(path)`) | `AgentAttachOutput` (exit 0)                                            | NEW; per §F.2 UnixSocket path                   |

## Layer direction (RFC-0011-c §Follow-on + per [[cipherocto-design-principles]])

- `octo-runtime` (Layer B) — `AttachHandle` + `Transport` + `encode_token` + `decode_token` + `attach` + `persist_event_cursor` + `revoke_attach_token`. New crate files; no breaking changes to existing `spawn_agent` signature.
- `octo-wallet` (Layer B) — additive `sign_attach_handle`; reuses Ed25519 substrate (Layer A frozen).
- `octo-cli` (Layer C/D) — 3 new top-level subcommands (`agent run --detach --token-file`, `agent attach --token-file`, `octo revoke-attach`) + 6 `OctoCliError` variants (exits 53-58).
- **NO new Layer A types introduced.** Ed25519 signing reuses frozen crypto substrate.

## Validation

```bash
cargo fmt --all -- --check                                              # clean
cargo clippy --workspace --features full --all-targets -- -D warnings  # clean
cargo test -p octo-runtime --features octo-runtime-persistence --lib --tests  # green
cargo test -p octo-wallet --lib --tests                                # green
cargo test -p octo-cli --lib --tests                                   # green (was 286 lib tests + NEW)
```

## Backward compat

- Additive only: new submodule `octo_runtime::handle` + new persistence module + new error variants in Layer B; no breaking changes to existing `spawn_agent`/`attach`/`transition_agent` signatures.
- CLI exit codes match RFC-0011-c §F.4 mirror (6 new variants: exits 53-58; slot allocation extended from 39-52 to 39-58).
- `OutputEnvelope<T>::schema_version = 4` preserved per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.
- New `RevokeOutput` payload type with `schema_version = 4` (NEW); agent attach/run output payload schemas unchanged.
- `cfg(feature = "octo-runtime-persistence")` gating preserves Layer A frozen contract per RFC-0016-a §6.4 paired-invariance.

## Cross-references

- RFC-0011-c §Implementation Phases Phase 1 (octo-runtime substrate base)
- RFC-0011-c §9.3.5 `octo agent attach` subcommand specification
- RFC-0011-c §9.8 Error Handling (extended slot 39-58)
- RFC-0011-c §Follow-on §F.1-§F.5 (NEW; this cycle)
- RFC-0015-a Appendix A (operative signing surface)
- RFC-0016-a §6.10 (canonical-bytes-on-write invariant)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[no-phantom-mission-pointer]] — paired-acceptance sequencing
- [[Initiative user-only]] — user owns commit/push/status transitions
- [[memory-is-never-status-ground-truth]] — provenance rule
- [[0011-c-agent-attach-dry-closure-2026-09-15]] — companion closure card
- [[2026-09-16-0011-c-agent-attach-yaml-revert]] — companion revert audit
- [[phase2-unblock-dry-closure-2026-09-13]] — substrate unblock prior cycle

## Why gate

Release-gated on companion substrate mission RFC-0011-c §Follow-on paired acceptance per [[no-phantom-mission-pointer]] rule. The §Follow-on text refresh + this mission YAML + substrate code + CLI wiring all land in the same commit cycle. Until this mission lands, `0011-c-agent-attach-subcommand`'s dispatch handler returns `RuntimeSubstrateNotReady` exit 51 unconditionally at `crates/octo-cli/src/commands/agent.rs:1180`.

Per [[Initiative user-only]] + [[git-workflow]] user owns the remote-write workflow + status transitions. NO PUSH.

## Substrate Gap Closure (2026-09-16)

Substrate state verified as of 2026-09-16:

- `octo_runtime::handle.rs` does NOT yet exist (NEW; this mission)
- `octo_runtime::handle/encoding.rs` does NOT yet exist (NEW; this mission)
- `octo_runtime::persistence.rs` does NOT yet exist (NEW; this mission)
- `octo_runtime::attach` module EXISTS at `crates/octo-runtime/src/attach.rs` (Layer B; baseline)
- `octo_runtime::spawn_agent` EXISTS at `crates/octo-runtime/src/spawn.rs` (Layer B; baseline)
- `octo_wallet::sign_attach_handle` does NOT yet exist (NEW; this mission)
- `OctoCliError` has slots 39-52 reserved per RFC-0011-c §9.8 (extended to 39-58 by this mission)

Per [[Initiative user-only]] user owns `status: Claimed` → `status: In Progress` transition + DRY cycle kickoff. Mission remains `Claimed` per [[memory-is-never-status-ground-truth]].

## RFC-0011-c §Follow-on text refresh note

This mission lands paired with an RFC-0011-c text amendment that appends §F.1-§F.5 sections to the RFC body. The amendment is `chore(rfcs): 0011-c §Follow-on text refresh for AttachHandle pathway` (paired commit). Per [[no-phantom-mission-pointer]] rule, the RFC text lands in the same commit cycle as the substrate code + CLI wiring + this mission YAML.

## Hard audit findings addressed

Per hard audit 2026-09-15 (`docs/audits/2026-09-16-0011-c-agent-attach-yaml-revert.md`) the `0011-c-agent-attach-subcommand` mission's release gate was blocked on this follow-on cycle. This mission clears that block:

| Finding | Severity | Resolution                                                                                                  |
| ------- | -------- | ----------------------------------------------------------------------------------------------------------- |
| D7      | BLOCKING | This mission lands the AttachHandle pathway that `agent attach` requires to bind in-process `RuntimeHandle` |

## Claimant

@unassigned
