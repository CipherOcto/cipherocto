---
name: 0011-c-octo-runtime-attachhandle-substrate
description: Land AttachHandle token pathway per RFC-0011-c §Follow-on; substrate + CLI + revocation + persistence in single mission
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension-plus-substrate
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-16
  v: "1.1"
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
- Mission `0011-c-agent-attach-subcommand` — `Commands::Agent::Attach` dispatch stub replaced in §Sub-step 2 below
- RFC-0015-a — `transition_agent` (Layer B; signing surface reused by `sign_attach_handle`)
- RFC-0016-a — audit substrate for `mint_attach_handle` event persistence

## Status

Open. RFC-0011-c §Follow-on text refresh + this mission YAML + CLI wiring + substrate code land in this single cycle per paired-acceptance sequencing. DRY gate: R1 → R1.5 → R2 (zero-finding = DRY CLOSED gate) per user Q3 "shorter" directive.

## Substrate (RFC-0011-c §Follow-on)

Per RFC-0011-c §Follow-on (NEW; added by this mission) the AttachHandle token pathway covers four layers of work. The substrate specification (function signatures, encoding format, validation chain, error variants) is the authoritative source per RFC-0011-c §Follow-on §F.1-§F.5 — this section summarizes and points to the RFC; only Path B-specific notes (existing 4-field `AttachHandle` → `RuntimeHandleBinding` rename, `sign_attach_handle_payload` colocation decision) are mission-local content.

- **§F.1 Encoding** — see RFC-0011-c §Follow-on §F.1. `octo_runtime::handle::encoding` (NEW). `encode_token` + `decode_token` round-trip with signature verify-on-decode; canonical version byte `0x00`; mirrors RFC-0016-a §6.10 canonical-bytes-on-write pattern.
- **§F.2 Token Substrate** — see RFC-0011-c §Follow-on §F.2. `octo_runtime::handle`. 6-field `AttachHandle` token (Layer B); existing 4-field `AttachHandle` (in-process binding: `agent_id`, `handle_id`, `session_id`, `spawned_at_unix`) RENAMED to `RuntimeHandleBinding` per Path B (additive, mechanical codemod). Validation chain at `attach_with_token` invocation: (a) signature check, (b) revocation-set membership, (c) `ttl_unix` not expired, (d) `since_unix >= mint_timestamp_unix`, (e) **transport-handler dispatch** via `token.transport.kind` against the process-singleton `octo_runtime::handle::transport::HANDLE_TRANSPORT_REGISTRY` (NEW; built-in `InProcessHandler` registered at lazy init; extension transports (UnixSocket, Raw scheme UUIDs) land via follow-on Layer D crates per [[cipherocto-design-principles]] §per-extension crates + registry pattern — substrate stays filesystem-free + socket-IO-free per §Layer direction). Existing `attach(handle, since)` function UNCHANGED — still consumes `RuntimeHandleBinding`. `attach_with_token(holder_pubkey, token, since_unix)` lives at the `octo_runtime` crate root (re-exported via `pub use handle::{...}`); the leading `holder_pubkey` parameter mirrors `verify_attach_handle_payload`'s static-helper signature shape.
- **§F.3 Persistence** — see RFC-0011-c §Follow-on §F.3. `octo_runtime::persistence` (NEW). `persist_event_cursor` + `load_event_cursor` (Q-deferred 3) gated on `cfg(feature = "octo-runtime-persistence")` (NEW feature flag in `octo_runtime` manifest). `revoke_attach_token` + `is_token_revoked` (Q-deferred 2; in-memory revocation set).
- **§F.4 Errors** — see RFC-0011-c §Follow-on §F.4. `pub enum AttachError` in `octo_runtime::handle::error` with 8 variants (mirror → `OctoCliError` exits 53-59); `InvalidSinceCursor` (exit 53 shared slot with `AttachHandleExpired` per typed-discriminator preservation); `TransportHandlerNotRegistered { kind_label }` (exit 59 per Layer D extension surface per [[cipherocto-design-principles]] §per-extension crates + registry).
- **§F.5 Signing Surface** — see RFC-0011-c §Follow-on §F.5. `sign_attach_handle_payload` + `verify_attach_handle_payload` wrappers colocated in the `octo_runtime::handle::signing` submodule (Layer B; re-exported at crate root via `pub use handle::signing::{...}`). Substrate composition follows the static-helper pattern (`verify_successor_proof` / `verify_revocation_proof`): `sign_attach_handle_payload` takes `holder: &IdentityKey` (caller resolves DID → key) and delegates to `octo_wallet::IdentityKey::sign(msg_bytes)`; `verify_attach_handle_payload` takes `holder_pubkey: &[u8; 32]` (static, mirrors `verify_revocation_proof` shape). NO `octo_wallet::crypto` module (does not exist); no new Layer A types.

Layer attribution: see §Type Coverage table below for full layer designation per RFC-0011-c type.

## Parent

RFC-0011-c (agent lifecycle amendment; Phase 3 of RFC-0011 amendment chain). §Follow-on text refresh appends §F.1-§F.5 sections to RFC-0011-c body via RFC doc amendment commit (paired with this mission per [[no-phantom-mission-pointers]]).

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing:

1. RFC-0011-c §Follow-on text refresh lands (paired with this mission YAML)
2. Substrate code (mint/decode + persistence + revocation + signing) lands
3. CLI wiring (`agent run --detach --token-file` + `agent attach --token-file` + `octo revoke-attach`) lands
4. Mission DRAM cycle: R1 → R1.5 → R2 zero-finding = DRY CLOSED

## Acceptance Criteria

- [ ] **AC-1** `AttachHandle` struct (Layer B) defined at the `octo_runtime::handle` module per RFC-0011-c §F.2; existing 4-field in-process binding (`agent_id`, `handle_id`, `session_id`, `spawned_at_unix`) RENAMED to `RuntimeHandleBinding` per Path B (additive, mechanical codemod of existing callers)
- [ ] **AC-2** `mint_attach_handle` returns signed token (Layer B) per RFC-0011-c §F.2 (calls `sign_attach_handle_payload` per §F.5; composes `IdentityKey::sign` from `octo-wallet`)
- [ ] **AC-3** `encode_token` + `decode_token` round-trip (Layer B) per RFC-0011-c §F.1 with signature verification
- [ ] **AC-4** `attach_with_token()` step (e) dispatches via `token.transport.kind` against the process-singleton `octo_runtime::handle::transport::HANDLE_TRANSPORT_REGISTRY` (NEW; built-in `InProcessHandler` registered at lazy init; extension transports land via follow-on Layer D crates per [[cipherocto-design-principles]] §per-extension crates + registry). Substrate ships filesystem-free + socket-IO-free per §Layer direction; existing `attach(handle, since)` UNCHANGED
- [ ] **AC-12** (merged into **AC-4** per R14 substrate-faithfulness finding C-SF1; AC-4 now includes both dispatch behavior and substrate-types facet)
- [ ] **AC-5** `agent run --detach --token-file <path>` mints + writes token to file (Layer C/D) per §Sub-step 3 + RFC-0011-c §F.2
- [ ] **AC-6** `agent attach --token-file <path>` reads + binds (Layer C/D) per §Sub-step 4 + RFC-0011-c §F.2 (replaces attach dispatch stub)
- [ ] **AC-7** 7 NEW `OctoCliError` variants wired at `octo_cli::error` per RFC-0011-c §F.4 mirror (`AttachHandleExpired`, `AttachHandleBadSignature`, `AttachSessionMismatch`, `AttachSessionUnknown`, `PersistenceError`, `RevocationError`, `TransportHandlerNotRegistered`); `InvalidSinceCursor` shares exit 53 slot with `AttachHandleExpired` per typed-discriminator preservation; `TransportHandlerNotRegistered` exits 59 (extension surface per [[cipherocto-design-principles]] §per-extension crates + registry)
- [ ] **AC-8** `octo revoke-attach <token-hex>` primitive (Q-deferred 2) per RFC-0011-c §F.3 with in-memory revocation set
- [ ] **AC-9** `persist_event_cursor` + `load_event_cursor` via Stoolap ledger extension (Q-deferred 3) per RFC-0011-c §F.3 gated on `cfg(feature = "octo-runtime-persistence")` (feature flag newly added to `octo-runtime/Cargo.toml` per this mission)
- [ ] **AC-10** Cargo validation per §Validation (zero warnings, all lib+test green)
- [ ] **AC-11** Layer direction verified (no reverse deps per [[cipherocto-design-principles]]) + DRY R1+R2 zero-finding gate achieved

### Type Coverage

| RFC-0011-c type                                                           | Sub-step | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------------------------------------------------------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AttachHandle`                                                            | F.2      | Layer B; struct with 6 fields + `Transport` discriminator (Q-deferred 1)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `Transport` (struct: `kind: TransportKind` + `addr: Option<String>`)      | F.2      | Layer B; binding transport selector (Q-deferred 1)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `TransportKind` (enum: `InProcess` / `UnixSocket` / `Raw(Uuid)`)          | F.2      | Layer B; `#[non_exhaustive]` typed-discriminator + `Raw` escape hatch per RFC-0855 §Typed UUID discriminators convention                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `mint_attach_handle`                                                      | F.2      | Layer B; calls `sign_attach_handle_payload` per F.5 (composes `IdentityKey::sign` from `octo-wallet`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `attach()`                                                                | F.2      | Layer B sync (legacy); binds via in-process channel using `RuntimeHandleBinding` (unchanged)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `attach_with_token(holder_pubkey, token, since_unix)`                     | F.2      | Layer B async (defined at `octo_runtime` crate root via `pub use attach::{attach, clamp_since}` plus the `attach_with_token` fn); validates via 5-step chain (step (e) = transport-handler dispatch via `octo_runtime::handle::transport::HANDLE_TRANSPORT_REGISTRY`)                                                                                                                                                                                                                                                                                                                                                                                 |
| `encode_token` + `decode_token`                                           | F.1      | Layer B; canonical encoding v0 with signature verify-on-decode                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `revoke_attach_token` + `is_token_revoked`                                | F.3      | Layer B; in-memory revocation set (Q-deferred 2)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `persist_event_cursor` + `load_event_cursor`                              | F.3      | Layer B; Stoolap ledger extension, feature-gated (Q-deferred 3)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `sign_attach_handle_payload` + `verify_attach_handle_payload`             | F.5      | Layer B wrappers colocated in `octo_runtime::handle::signing` (re-exported at crate root); compose `IdentityKey::sign`/`verify` from `octo-wallet` (Layer A frozen `ed25519-dalek`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `Handler` + `Registry` + `HANDLE_TRANSPORT_REGISTRY` + `InProcessHandler` | F.2      | Layer B; transport-handler trait + process-singleton `Registry` (`RwLock<HashMap<TransportKind, Arc<dyn Handler>>>`) + lazy-init `OnceLock` + built-in `InProcessHandler` (Phase 1 stub — `bind` panics in debug + returns `AttachError::UnknownSession` in release per RFC-0011-c §F.2 deferred-wiring note; session-registry-wiring follows in a follow-on amendment). Fail-CLOSED on poisoned `RwLock` (mirrors `REVOCATION_SET` discipline per §F.3). Extension transports land via `Registry::register` from follow-on Layer D crates (`octo-runtime-transport-unix`, …). Substrate stays filesystem-free + socket-IO-free per §Layer direction. |
| `AttachError` (8 variants)                                                | F.4      | Layer B; mirror → `OctoCliError` 6 variants (exits 53-56 + `InvalidSinceCursor` shared slot 53 + `TransportHandlerNotRegistered` exit 59) + 2 substrate-error passthroughs (exits 57-58)                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `OctoCliError::AttachHandleExpired {…}`                                   | CLI      | Layer C/D; mirror exit 53 per RFC-0011-c §F.4 (slot allocation 39-59)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `OctoCliError::AttachHandleBadSignature`                                  | CLI      | Layer C/D; mirror exit 54                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `OctoCliError::InvalidSinceCursor`                                        | CLI      | Layer C/D; mirror exit 53 (shared slot with `AttachHandleExpired` per typed-discriminator preservation; discriminator carries the variant identity)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `OctoCliError::AttachSessionMismatch {…}`                                 | CLI      | Layer C/D; mirror exit 55                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `OctoCliError::AttachSessionUnknown {…}`                                  | CLI      | Layer C/D; mirror exit 56                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `OctoCliError::PersistenceError(String)`                                  | CLI      | Layer C/D; passthrough exit 57                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `OctoCliError::RevocationError(String)`                                   | CLI      | Layer C/D; passthrough exit 58                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `OctoCliError::TransportHandlerNotRegistered { kind_label }`              | CLI      | Layer C/D; mirror exit 59 — step (e) no `Handler` registered for `token.transport.kind`; extension surfaces (UnixSocket, Raw schemes) land via follow-on Layer D crates per [[cipherocto-design-principles]] §per-extension crates + registry                                                                                                                                                                                                                                                                                                                                                                                                         |

## Implementation Guide

See §Agent Subcommands (octo-cli-implementation-guide) for clap wiring patterns. Mirror the `agent attach` dispatch replace-stub pattern at `Commands::Agent::Attach` dispatch stub.

## Pull Request

_(PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])_

## Notes

This mission was scoped per hard audit 2026-09-15 + user direction Q1+Q2+Q3. The `0011-c-agent-attach-subcommand` mission's release gate cleared on this mission landing. Single-mission design per [[cipherocto-design-principles]] §No parallel abstractions principle (one mission = one cohesion: the AttachHandle pathway). 3 deferred items (Q-deferred 1/2/3) brought in scope per user direction so the attach handler binds to in-process `RuntimeHandle` and survives protocol-layer revoke + cursor-persistence primitives.

RFC-0011-c §Follow-on text refresh lands via paired commit with this mission YAML per [[no-phantom-mission-pointers]] rule. The §Follow-on text covers §F.1 (Encoding) through §F.5 (Signing Surface) sections verbatim mirrored from this YAML's §Substrate (RFC-0011-c §Follow-on) section.

## Risk

- **MEDIUM** — UnixSocket handler is NOT shipped by this mission (substrate-stays-filesystem-free + socket-IO-free per §Layer direction). UnixSocket binding lands via follow-on `octo-runtime-transport-unix` crate which registers a `Handler` impl at process startup; UnixSocket-specific filesystem concerns (path resolution, permission fallbacks, `SocketPathUnavailable` variant) are deferred to that crate's RFC (substrate ships `TransportHandlerNotRegistered` exit 59 for the not-yet-registered case).
- **MEDIUM** — In-memory revocation set lost on process restart; cross-process revocation propagation out of scope (deferred to RFC-0011-c §Future Work)
- **LOW** — Persistence feature gate (`octo-runtime-persistence`) may not be enabled in default builds; `persist_event_cursor` returns `PersistenceError("feature disabled")` exit 57 in default builds
- **LOW** — Ed25519 signature substrate (Layer A frozen) composed via `IdentityKey::sign` (Layer B, RFC-0015-a Appendix A); no new crypto-layer assumptions

## Scope

Land the AttachHandle token pathway end-to-end per RFC-0011-c §Follow-on. Three sibling deferred extensions (Q-deferred 1 multi-process bind, Q-deferred 2 revocation, Q-deferred 3 cursor persistence) all in scope per user direction Q2. Out-of-scope (deferred to next cycle):

- Cross-process revocation propagation (separate process → in-memory set sync; defers to RFC-0011-c §Future Work)
- Key rotation for `sign_attach_handle_payload` (deferred to RFC-0015-a §Future Work §6.5)
- `octo-runtime-persistence` feature gate activation in default builds (deferred to RFC-0011-c §Future Work)

## Sub-steps

**Scope:** Sub-steps 1+2 land in this mission cycle (RFC text + substrate code). Sub-steps 3-5 are SPECIFIED here (signature, invocation shape, error envelope mapping) but EXECUTED in a follow-on amendment mission that wires the Layer C/D CLI dispatch against the substrate this mission ships. The current cycle's in-scope deliverables are: (a) RFC-0011-c §Follow-on §F.1-§F.5 text refresh, (b) substrate code (handle split + encoding + signing + persistence + 8 AttachError variants), (c) `OctoCliError` mirror envelope (7 NEW variants at `octo_cli::error`) so the follow-on CLI wiring has substrate-faithful error surfaces to bind against. Sub-steps 3-5 prose preserved verbatim below for the follow-on mission to bind against.

1. **RFC-0011-c §Follow-on text refresh** — `rfcs/accepted/process/0011-c-agent-lifecycle.md` Layer direction amendment. Append §F.1-§F.5 sections. Pair-commit with mission YAML per [[no-phantom-mission-pointers]].

2. **Substrate code** — `octo_runtime::handle` module (new 6-field `AttachHandle` token + `sign_attach_handle_payload` + `verify_attach_handle_payload` wrappers in `handle::signing` submodule; existing 4-field `AttachHandle` RENAMED to `RuntimeHandleBinding`) + `octo_runtime::handle::encoding` submodule + `octo_runtime::handle::error` submodule + `octo_runtime::handle::transport` submodule (`Handler` trait + `Registry` + `HANDLE_TRANSPORT_REGISTRY` `OnceLock` + built-in `InProcessHandler` per RFC-0011-c §F.2 step (e)) + `octo_runtime::persistence` module. ~340 LoC + tests (substrate modules) + ~365 LoC + tests (transport module). Layer B. No `octo_wallet::crypto` module (does not exist; signing wrappers colocate in `octo_runtime::handle::signing` and compose `octo_wallet::IdentityKey::sign` per RFC-0015-a Appendix A). Substrate stays filesystem-free + socket-IO-free per §Layer direction; transport protocol I/O lives in follow-on Layer D crates (`octo-runtime-transport-unix`, …).

3. **CLI extension to `agent run --detach --token-file <path>`** (follow-on amendment) — `octo_cli::commands::agent` (Layer C/D; substrate reference RFC-0011-c §F.2). Add `--token-file <path>` clap arg to existing `AgentAction::Run` variant. On `--detach` dispatch, after `octo_runtime::spawn_agent(...)` returns, mint token via `octo_runtime::mint_attach_handle(holder_did, ...)` + serialize to file via `octo_runtime::encode_token`. Token NOT included in `AgentRunOutput` payload (side-channel credential, not audit data).

4. **CLI dispatch replacement at `agent attach --token-file <path>`** (follow-on amendment) — replaces `Commands::Agent::Attach` dispatch stub. Read token bytes from `--token-file` → `octo_runtime::decode_token(...)` → `octo_runtime::attach_with_token(holder_pubkey, &token, since_unix)` async (3 args: `holder_pubkey: &[u8; 32]` resolved from caller DID at CLI boundary per §F.5). Populates `AgentAttachOutput { agent_id, runtime_handle, attached_at_unix, event_cursor }`. Includes the 8 `OctoCliError` variants (exits 53-59) wired at `octo_cli::error` per RFC-0011-c §F.4 mirror (merged sub-step per R1 finding S-M4 — CLI dispatch and error envelope are one cohesive surface; `TransportHandlerNotRegistered` exit 59 is the new variant added in this cycle for the handler-registry dispatch surface).

5. **CLI primitive `octo revoke-attach <token-hex>`** (follow-on amendment) — new top-level subcommand at `octo_cli::commands`. Parses `<token-hex>` arg, decodes token (signature verified), calls `octo_runtime::revoke_attach_token(token.session_id)`. Returns `OctoCliError::RevocationError(String)` exit 58 on failure OR `exit 0` on success with `RevokeOutput { session_id, revoked_at_unix }`.

## Cargo deps

```toml
# crates/octo-runtime/Cargo.toml — diff vs R7.5 baseline
[dependencies]
# octo-wallet edge was pre-existing (Layer B sibling per §F.5 sign_attach_handle_payload
# composition). Removed: octo-core + octo-registry (unused in this crate's source;
# substrate-faithful dep graph).
octo-wallet = { path = "../octo-wallet", version = "0.1.0" }
serde_bytes = "0.11"  # NEW: workspace-standard byte-array serde adapter for [u8; 64] signature

# crates/octo-runtime/Cargo.toml — conditional new dep
[features]
default = []
octo-runtime-persistence = ["dep:stoolap"]  # NEW feature flag per RFC-0011-c §F.3

# crates/octo-wallet/Cargo.toml — unchanged per RFC-0011-c §F.5
# (no new deps; ed25519-dalek + IdentityKey already pulled)

# crates/octo-cli/Cargo.toml — unchanged per RFC-0011-c §Sub-steps 3-5
# (no new deps; octo-runtime + octo-wallet already listed per RFC-0011-c §Implementation Phases)
```

## Test Vectors (per RFC-0011-c §Test Vectors — `agent attach` group, extended)

8 TV (TV-AGT11-AGT12 EXISTING per RFC-0011-c §Test Vectors; TV-AGT19-AGT24 NEW for AttachHandle pathway; TV-AGT13 already defined in RFC as `agent create` ReplayDetected per RFC-0011-c §Test Vectors — RE-numbered to avoid collision per R1 finding C-H3):

| #        | Subcommand                               | Input                                                        | Expected Output                                                         | Notes                                                                                                                                                                                                                                            |
| -------- | ---------------------------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| TV-AGT11 | `agent attach`                           | Running agent (in-process)                                   | `AgentAttachOutput { runtime_handle: ..., event_cursor: ... }` (exit 0) | EXISTING; per RFC §Test Vectors                                                                                                                                                                                                                  |
| TV-AGT12 | `agent attach`                           | Terminated agent                                             | `AgentNotRunning(uuid)` (exit 48)                                       | EXISTING; per RFC §Test Vectors                                                                                                                                                                                                                  |
| TV-AGT19 | `agent attach`                           | Token expired (`now_unix > ttl_unix`)                        | `AttachHandleExpired { ... }` (exit 53)                                 | NEW; per §F.2 chain step (c)                                                                                                                                                                                                                     |
| TV-AGT20 | `agent attach`                           | Token signature mismatched                                   | `AttachHandleBadSignature { reason }` (exit 54)                         | NEW; per §F.2 chain step (a)                                                                                                                                                                                                                     |
| TV-AGT21 | `agent run --detach --token-file <path>` | Fresh spawn                                                  | Token written to file; `AgentRunOutput` (exit 0) excludes token         | NEW; per §Sub-step 3                                                                                                                                                                                                                             |
| TV-AGT22 | `octo revoke-attach <hex>`               | Revoked session                                              | `RevokeOutput { session_id, revoked_at_unix }` (exit 0)                 | NEW; per §Sub-step 5                                                                                                                                                                                                                             |
| TV-AGT23 | `agent attach`                           | Multi-process via UnixSocket (`Transport::UnixSocket(path)`) | `TransportHandlerNotRegistered { kind_label: "UnixSocket" }` (exit 59)  | NEW; per §F.2 UnixSocket path — substrate ships only `InProcessHandler`; UnixSocket handler registration defers to follow-on `octo-runtime-transport-unix` crate mission (substrate stays filesystem-free + socket-IO-free per §Layer direction) |
| TV-AGT24 | `agent attach`                           | Session id mismatched (declared vs actual)                   | `AttachSessionMismatch { declared, actual }` (exit 55)                  | NEW; per §F.2 chain step (e)                                                                                                                                                                                                                     |

## Layer direction (RFC-0011-c §Follow-on + per [[cipherocto-design-principles]])

- `octo-runtime` (Layer B) — additive `sign_attach_handle_payload` + `verify_attach_handle_payload` wrappers colocated in `octo_runtime::handle::signing` (re-exported at crate root); compose `octo_wallet::IdentityKey::sign` (Layer B substrate) → `ed25519-dalek` (Layer A frozen). `octo-wallet` (Layer B) — unchanged signing surface per RFC-0015-a Appendix A.
- **NO new Layer A types introduced.** See §Substrate §F.5 for full layer attribution + §Type Coverage table below for per-type designation.

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
- CLI exit codes match RFC-0011-c §F.4 mirror (slot allocation extended from 39-52 to 39-59 per RFC-0011-c §9.8; new exit 59 = `TransportHandlerNotRegistered` per the Layer D extension surface).
- `OutputEnvelope<T>::schema_version = 4` preserved per RFC-0011-c §9.4 / §9.4.1 Divergence slot table.
- New `RevokeOutput` payload type with `schema_version = 4` (NEW); agent attach/run output payload schemas unchanged.
- `cfg(feature = "octo-runtime-persistence")` gating is a NEW feature flag being added to `octo_runtime` manifest per this mission (no RFC-0016-a §6.4 paired-invariance claim per R1 finding SF-H4; canonical-bytes-on-write pattern is a coding reference per RFC-0016-a §6.10, not a paired-acceptance contract).

## Cross-references

- RFC-0011-c §Implementation Phases Phase 1 (octo-runtime substrate base)
- RFC-0011-c §9.3.5 `octo agent attach` subcommand specification
- RFC-0011-c §9.8 Error Handling (extended slot 39-59)
- RFC-0011-c §Follow-on §F.1-§F.5 (NEW; this cycle)
- RFC-0015-a Appendix A (operative signing surface)
- RFC-0016-a §6.10 (canonical-bytes-on-write invariant)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[no-phantom-mission-pointers]] — paired-acceptance sequencing
- [[feedback_initiation_user_only]] — user owns commit/push/status transitions
- [[memory-is-never-status-ground-truth]] — provenance rule
- [[0011-c-agent-attach-closure-2026-09-15]] — companion closure card

## Why gate

Release-gated on companion substrate mission RFC-0011-c §Follow-on paired acceptance per [[no-phantom-mission-pointer]] rule. Until this mission lands, `0011-c-agent-attach-subcommand`'s dispatch handler returns `RuntimeSubstrateNotReady` exit 51 unconditionally at `Commands::Agent::Attach` dispatch stub.

## Substrate Gap Closure (2026-09-16, R12.5 Option A update)

Substrate additions landed as of 2026-09-16 (CRIT 1 Option A per user direction): `Handler` trait + `Registry` + `HANDLE_TRANSPORT_REGISTRY` `OnceLock` + built-in `InProcessHandler` in `octo_runtime::handle::transport` (NEW; ~365 LoC + 12 NEW tests); `attach_with_token` step (e) replaced from `unimplemented!()` stub to handler-registry dispatch; `AttachError::TransportHandlerNotRegistered { kind_label }` (CLI exit 59) added; `OctoCliError::TransportHandlerNotRegistered { kind_label }` mirror + `From<AttachError>` arm added. Per-extension surfaces (UnixSocket, Raw scheme UUIDs) defer to follow-on Layer D crates (`octo-runtime-transport-unix`, …) per [[cipherocto-design-principles]] §per-extension crates + registry pattern. Substrate-first ordering per RFC-0015 R40 restructure: this mission lands substrate first, then CLI dispatch wires (Phase 3). Mission remains Claimed per [[memory-is-never-status-ground-truth]] + [[Initiative user-only]].

## Hard audit findings addressed

D7 (BLOCKING) cleared — AttachHandle pathway lands per AC-1..AC-9; `0011-c-agent-attach-subcommand` release gate unblocks.
