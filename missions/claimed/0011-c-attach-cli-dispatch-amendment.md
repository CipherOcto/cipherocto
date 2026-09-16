---
name: 0011-c-attach-cli-dispatch-amendment
description: Wire the AttachHandle token pathway into `octo agent run --detach --token-file` and `octo agent attach --token-file` per RFC-0011-c §F.6 — closes M2 + M3 from the attachhandle substrate cycle closure audit
metadata:
  node_type: cli-substrate-extension
  type: cli-dispatch-amendment
  originSessionId: 0011-c-attachhandle-closure-session
  created: 2026-09-16
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0002
    - mission 0011-c-octo-runtime-attachhandle-substrate
    - mission 0011-c-agent-run-subcommand
    - mission 0011-c-agent-attach-subcommand
  paired_mission: 0011-c-agent-attach-subcommand
substrate_unblocked: 2026-09-16
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-16
release_gate: end-to-end CLI dispatch wired per amendment cycle §F.6.1-§F.6.4
---

# 0011-c-attach-cli-dispatch-amendment — RFC-0011-c §F.6 CLI Dispatch Wiring

## Status

Open — substrate unblocked 2026-09-16 per AttachHandle substrate mission
DRY CLOSURE (`docs/audits/2026-09-16-0011-c-attachhandle-dry-closure.md`).
CLI dispatch surface wired end-to-end on `next` (commit `37d7315d`); the
paired sibling mission `0011-c-agent-run-subcommand` (emit side) +
`0011-c-agent-attach-subcommand` (consume side) carry the paired_mission
annotation + the cleared release-gate annotations per §Sub-step 7.

## Substrate

RFC-0011-c §F.6 (new appendix; mirrors substrate cycle §F.1-§F.5).

## Parent

RFC-0011-c (agent lifecycle amendment of RFC-0011).

## Depends on

- Mission `0011-c-octo-runtime-attachhandle-substrate` — provides
  `mint_attach_handle`, `encode_token`, `decode_token`, `attach_with_token`,
  and 8 `OctoCliError` mirror variants (slots 53-59)
- Mission `0011-c-agent-run-subcommand` — `AgentAction::Run` clap root variant
  - `AgentRunOutput` payload struct
- Mission `0011-c-agent-attach-subcommand` — `AgentAction::Attach` clap root
  variant + `AgentAttachOutput` payload struct (currently stub-dispatched;
  this amendment replaces the stub)

## Notes

This amendment closes the deferred sub-steps M2 + M3 from the AttachHandle
substrate mission (`missions/archived/completed/0011-c-octo-runtime-attachhandle-substrate.md`
§Sub-step 3 + §Sub-step 4). The two sides are inseparably coupled: `run` emits
the token that `attach` consumes. Unifying them in one mission = one coherent
surface, one DRY cycle, one audit. The amendment also unblocks
`0011-c-agent-attach-subcommand.md` (the stub `RuntimeSubstrateNotReady` exit 51
is replaced end-to-end with the `attach_with_token` validation chain).

## Risk

- **HIGH** — token file must be created with mode 0o600 (POSIX-only); substrate-
  faithful per RFC-0011-c §Layer direction (the token is a credential). Mitigation:
  `std::fs::OpenOptions::create(true).write(true).truncate(true).mode(0o600)` on
  unix; explicit error on platforms where mode flags are unsupported (e.g. Windows).
- **HIGH** — caller DID → `[u8; 32]` holder_pubkey resolution must compose
  `octo_wallet::active_identity(&store).public_key_bytes()` (CLI boundary concern
  per RFC-0011-c §F.5). Mitigation: helper `resolve_active_identity_key()` at
  `mod common`; substrate returns `BadSignature` exit 54 if the resolved pubkey
  does not match the mint-time IdentityKey.
- **MEDIUM** — `attach_with_token` is `async`; CLI dispatch handler must be
  `async fn` (existing tokio runtime per `#[tokio::main]` at
  `crates/octo-cli/src/main.rs`). Mitigation: handler signature mirrors
  `revoke_attach::handle` (sync) but wraps the `attach_with_token` `.await` in
  a `tokio::runtime::Handle::current().block_on(...)` block — or convert the
  dispatch to async if the calling pipeline permits.
- **MEDIUM** — happy-path attach is bounded by substrate §F.2 step (e)
  session-registry-wiring deferral. Per the closure audit "Out of scope"
  section, the `InProcessHandler::bind` currently returns `AttachSessionUnknown`
  exit 56 for legitimate tokens. The CLI dispatch surface wired in this cycle
  is substrate-faithful; the missing wiring is purely substrate-side
  (follow-on amendment per §F.6.5). Mitigation: TV-CLI-ATTACH-1 documents
  the bounded expected behavior; full happy-path coverage deferred to the
  paired follow-on amendment cycle.
- **LOW** — `decode_token` returns `BadSignature` if the token bytes were
  tampered or the holder_pubkey mismatches the mint-time IdentityKey. CLI
  surfaces verbatim (exit 54); no client-side validation needed.
- **LOW** — `since` parameter default. If absent, the CLI defaults to
  `token.mint_timestamp_unix` (start replay from spawn time). Substrate
  rejects `since_unix < mint_timestamp_unix` via step (d) → `InvalidSinceCursor`
  exit 53 shared-slot.

## Scope

Land the RFC-0011-c §F.6 CLI dispatch wiring end-to-end. Two clap args + two
dispatch body extensions + 4 new CLI test vectors + 2 mission YAML paired-updates
(attach + run sibling missions) + 1 closure audit doc + 1 memory card. All in
one DRY closure cycle.

Out of scope (deferred to follow-on amendment cycles per §F.6.5):

- Session-registry-wiring for `InProcessHandler::bind` (substrate-side
  amendment; gates full happy-path attach end-to-end)
- Replay typed-discriminator variant (`Internal(reason)` exit 64 routing)
- Hybrid `node_type` placeholder
- UnixSocket + Raw extension Layer D crates
- Cross-process revocation propagation
- Key rotation for `sign_attach_handle_payload`
- `octo-runtime-persistence` feature gate activation in default builds
- 0011-c-octowallet-agents-substrate companion mission (RFC-0011-c → Accepted gate)

## Sub-steps

1. **RFC-0011-c §F.6 appendix** — `rfcs/accepted/process/0011-c-agent-lifecycle.md`
   (Layer: spec). Append §F.6 CLI Dispatch Wiring section (5 sub-sections:
   §F.6.1 emit side, §F.6.2 consume side, §F.6.3 OctoCliError mirror, §F.6.4
   pairing invariant, §F.6.5 out-of-scope). Pair-commit with this mission YAML
   per [[no-phantom-mission-pointers]].

2. **CLI clap arg: `Run.token_file`** — `crates/octo-cli/src/commands/agent.rs`
   (Layer C/D). Extend `AgentAction::Run` variant with
   `#[arg(long, value_name = "PATH", requires = "detach")] pub token_file: Option<PathBuf>`
   field. clap interlock prevents `--token-file` without `--detach` (per RFC-0011-c
   §F.6.1; the token pathway only activates on detached spawns).

3. **CLI dispatch body: `run::handle` mint + encode + write** — same file
   (Layer C/D; substrate reference RFC-0011-c §F.6.1 + §F.1 + §F.2). After
   `runtime_spawn_agent(agent_id, None)` succeeds + on `--detach --token-file`:
   - Resolve caller DID → `IdentityKey` via new helper `common::resolve_active_identity_key()`
   - Extract `session_id` from the returned `RuntimeHandle`
   - Mint via `octo_runtime::mint_attach_handle(holder: &IdentityKey, agent_id, session_id, since_cursor: u64::default(), ttl_unix: now + 3600, Transport::IN_PROCESS)`
   - Encode via `octo_runtime::encode_token(&handle)`
   - Write to `--token-file` via `std::fs::OpenOptions::create + truncate + mode 0o600 + fsync`
   - Create parent dirs via `std::fs::create_dir_all(parent)` if absent
   - Populate `AgentRunOutput::token_written: Option<TokenWrittenReceipt>`

4. **CLI clap arg: `Attach.token_file`** — same file. Extend
   `AgentAction::Attach` variant with
   `#[arg(long, value_name = "PATH")] pub token_file: PathBuf` field. Required
   on every attach invocation per the substrate-faithful binding pathway.

5. **CLI dispatch body: `attach::handle` decode + `attach_with_token`** —
   same file (Layer C/D; substrate reference RFC-0011-c §F.6.2 + §F.1 + §F.2).
   Replace `Err(OctoCliError::RuntimeSubstrateNotReady)` stub at the documented
   seam (the AttachHandle substrate closure audit §Sub-step 4 dispatch seam) with:
   - Read token bytes from `--token-file` via `std::fs::read`
   - Resolve caller DID → `[u8; 32]` holder_pubkey via
     `common::resolve_active_identity_key().public_key_bytes()`
   - Decode via `octo_runtime::decode_token(&bytes, holder_pubkey)`
   - Call `octo_runtime::attach_with_token(holder_pubkey, &token, since_unix).await`
     (8 `AttachError` variants map to 8 `OctoCliError` slots 53-59 via existing
     `From<octo_runtime::AttachError> for OctoCliError` impl)
   - Populate `AgentAttachOutput { agent_id, runtime_handle: Some(handle.handle_id),
attached_at_unix, event_cursor: Some(cursor), session_id_hex }`

6. **CLI test vectors (4 NEW)** — same file `tests` module + `tests/agent.rs`
   integration suite:
   - TV-CLI-RUN-DETACH-1 — `octo agent run --detach --token-file <tmp>` happy path
   - TV-CLI-ATTACH-1 — `octo agent attach --token-file <token>` dispatch path
     (bounded by §F.6.5 session-registry-wiring deferral; expected exit 56
     `AttachSessionUnknown` per substrate-faithful current behavior)
   - TV-CLI-ATTACH-2 — `octo agent attach --token-file <tampered>` → exit 54
     `AttachHandleBadSignature`
   - TV-CLI-ATTACH-3 — `octo agent attach --token-file <revoked>` (after
     `octo revoke-attach --session-id`) → exit 58 `RevocationError`

7. **Sibling mission YAML updates**:
   - `missions/claimed/0011-c-agent-attach-subcommand.md`:
     - `release_gate_cleared_at: 2026-09-16` annotation
     - `implementation_state: cli-dispatch-wired` (replaces `cli-dispatch-stubbed`)
     - REMOVE `stub_variant` + `stub_exit_code` frontmatter annotations
     - ADD `paired_mission: 0011-c-attach-cli-dispatch-amendment`
     - §Sub-step 3 dispatch body — REPLACE stub body with `attach_with_token`
       flow (the schema-faithful substrate-faithful mirror)
   - `missions/archived/completed/0011-c-agent-run-subcommand.md`:
     - ADD `paired_mission: 0011-c-attach-cli-dispatch-amendment` (editorial)
     - §Sub-step 3 dispatch body — APPEND `--detach --token-file` block
       (mint + write)
     - §Test Vectors table — ADD TV-CLI-RUN-DETACH-1 row

8. **Closure artifacts**:
   - `docs/audits/2026-09-16-0011-c-attach-cli-dispatch-dry-closure.md`
     (gitignored scratchpad per [[docs-audits-scratchpad]])
   - `~/.claude/projects/.../memory/0011-c-attach-cli-dispatch-dry-closure-2026-09-16.md`
     (machine-loadable memory card per persistent memory pattern)
   - `~/.claude/projects/.../memory/MEMORY.md` — APPEND closure card line
     (replaces the AttachHandle substrate closure card line per established
     convention)
   - Commit (NO PUSH per [[feedback_initiation_user_only]] + [[git-workflow]])

## Cargo deps

```toml
# crates/octo-cli/Cargo.toml — no new deps required
octo-runtime = { path = "../octo-runtime", version = "0.1.0" }  # pre-existing (Layer B)
octo-wallet = { path = "../octo-wallet", version = "0.1.0" }    # pre-existing (Layer B)
```

All substrate types are defined in `octo-runtime` (mint/encode/decode/attach_with_token)
and `octo-wallet` (IdentityKey + public_key_bytes). No new external crates required.

## Test Vectors (per RFC-0011-c §F.6 — CLI dispatch surface, 4 NEW TV)

| #                   | Subcommand                              | Input                                         | Expected Output                                                                                                     | Notes                                                                                    |
| ------------------- | --------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| TV-CLI-RUN-DETACH-1 | `agent run --detach --token-file <tmp>` | Registered agent                              | `AgentRunOutput { token_written: Some(TokenWrittenReceipt { ... }), ... }` (exit 0); token file exists, 0o600 perms | Happy path emit side; token bytes decode round-trip                                      |
| TV-CLI-ATTACH-1     | `agent attach --token-file <tmp>`       | Token from TV-CLI-RUN-DETACH-1                | `AttachSessionUnknown { session_id }` (exit 56) — bounded by §F.6.5 session-registry-wiring deferral                | Substrate-faithful current behavior; happy-path coverage deferred to follow-on amendment |
| TV-CLI-ATTACH-2     | `agent attach --token-file <tampered>`  | Tampered token bytes (1 byte flipped)         | `AttachHandleBadSignature { reason }` (exit 54)                                                                     | Signature verify pipeline exercised end-to-end                                           |
| TV-CLI-ATTACH-3     | `agent attach --token-file <revoked>`   | Token after `octo revoke-attach --session-id` | `RevocationError(String)` (exit 58)                                                                                 | Revocation-set check exercised end-to-end                                                |

Test fixture: `tempfile::NamedTempFile` + `tempfile::tempdir` (already used in
`crates/octo-cli/tests/agent.rs` for manifest JSON test vectors; pattern reuse).
Token round-trip via in-process `mint_attach_handle` + `encode_token` + `fs::write`

- `fs::read` + `decode_token`. The TV-CLI-ATTACH-2 + TV-CLI-ATTACH-3 vectors
  verify the substrate-faithful signature verify + revocation set check pipelines
  that are already wired in the substrate cycle (no substrate gaps blocking these).

## Layer direction (RFC-0011-c §9.1 Architecture + [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `--token-file` clap args on `Run` + `Attach`;
  dispatch body extensions; 4 new CLI test vectors
- `octo-runtime` (Layer B) — reuses pre-existing `mint_attach_handle`,
  `encode_token`, `decode_token`, `attach_with_token` from substrate cycle
- `octo-wallet` (Layer B) — reuses pre-existing `IdentityKey` +
  `public_key_bytes` (CLI boundary resolution per RFC-0011-c §F.5)
- NO new Layer A types introduced
- NO new `OctoCliError` variants introduced (mirror surface landed in substrate cycle)

## Validation

```bash
cargo fmt --all -- --check                       # clean
cargo clippy --workspace --all-targets -- -D warnings  # clean
cargo build -p octo-cli -p octo-runtime         # EXIT=0
cargo test -p octo-cli --lib                     # green (287 → 291 tests)
cargo test -p octo-cli                           # green (integration + unit)
cargo test -p octo-runtime --lib                  # green (73 tests; no new tests)
npx prettier --check <modified .md files>        # clean (RFC §F.6 + 3 mission YAMLs)
```

## Backward compat

- Additive only: `AgentAction::Run` and `AgentAction::Attach` variants gain
  one new field each (`token_file`); no breaking changes to existing public
  API per RFC migration etiquette
- The `runtime_spawn_agent(agent_id, None)` call signature is UNCHANGED
  (the `None` placeholder remains — the in-process `RuntimeHandleBinding` for
  cross-process attach is wired via the token pathway, not via spawn-time
  binding)
- Existing `octo agent run` invocations (without `--detach --token-file`) work
  identically — the mint + write step is gated on both flags being present
- Existing `octo agent attach` invocations were stubbed (exit 51); after this
  amendment they require `--token-file <path>` (the substrate-faithful binding
  pathway)
- `OutputEnvelope<T>::schema_version = 4` unchanged (RFC-0011-c §9.4 / §9.4.1
  Divergence slot table); `AgentRunOutput::token_written: Option<...>` and
  `AgentAttachOutput::session_id_hex: String` are additive fields (old CLI
  ignores unknown fields)

## Cross-references

- RFC-0011-c §F.1 Encoding (canonical token bytes)
- RFC-0011-c §F.2 Token Substrate (mint + signature + validation chain)
- RFC-0011-c §F.3 Persistence + Revocation (revocation-set check)
- RFC-0011-c §F.4 Errors (8 OctoCliError mirror variants slots 53-59)
- RFC-0011-c §F.5 Signing Surface (sign_attach_handle_payload + verify_attach_handle_payload)
- RFC-0011-c §F.6 CLI Dispatch Wiring (this amendment)
- RFC-0011-c §9.3.5 `octo agent attach` subcommand specification
- RFC-0011-c §9.3.6 `octo agent revoke-attach` subcommand specification
- RFC-0011-c §9.8 Error Handling (exit code slots 39-59)
- RFC-0011-c §Implementation Phases Phase 1 (octo-runtime substrate release gate)
- RFC-0011 §Output Envelope, §Redaction Layer, §Error Handling — substrate sections
- RFC-0015-a Appendix A (operative `IdentityKey::sign` semantics)
- RFC-0002 §Agent State Machine (canonical state machine substrate)
- [[cipherocto-design-principles]] — Layer B stability contract + no-parallel-abstractions principle
- [[rfc-0011-loop-dry-gate-closure]] — review loop closure pattern from parent chain
- [[memory-is-never-status-ground-truth]] — provenance rule
- [[no-phantom-mission-pointers]] — paired-commit convention for RFC + mission YAML
- [[docs-audits-scratchpad]] — closure audit location
- [[0011-c-attachhandle-dry-closure-2026-09-16]] — predecessor substrate closure state

## Why gate

Release-gated on companion substrate mission `0011-c-octo-runtime-attachhandle-substrate`
landing (per RFC-0011-c §Implementation Phases Phase 1). Substrate mission DRY CLOSED
2026-09-16 per [[0011-c-attachhandle-dry-closure-2026-09-16]]; release gate cleared.

The cross-process happy-path attach (TV-CLI-ATTACH-1 exit 0) is bounded by
substrate §F.2 step (e) session-registry-wiring deferral (per §F.6.5 out-of-scope).
The amendment wires the CLI dispatch surface faithfully; the substrate-side wiring
lands in a paired follow-on amendment cycle.
