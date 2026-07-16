# octo-whatsapp

Long-lived daemon for the WhatsApp adapter. Owns the WhatsApp WebSocket,
exposes JSON-RPC over a unix-domain socket, MCP over stdio, and a structured
CLI for operators and AI agents.

## Status: Phase 1 (MVP) — implemented

Phase 1 covers the daemon + unix socket + JSON-RPC + the 12 method surfaces
listed in the design's §Rollout, plus CLI and MCP mirrors. Phases 2-5
(outbound matrix, events, rules/triggers, hardening) follow the same plan.

## Build

```bash
cargo build -p octo-cli-meta --features whatsapp-cli
```

(The crate is excluded from the default workspace build per the project's
meta-crate pattern. See `crates/octo-cli-meta/Cargo.toml`.)

## Test

```bash
cargo test -p octo-whatsapp
```

79 tests pass: 63 unit tests + 16 integration tests covering the
unix-socket JSON-RPC surface, the MCP handshake, and the 65,536-byte
`send.text` ceiling.

## Daemon API surface (Phase 1)

12 RPC methods + `version.get` + `health.get`:

| Method | Phase 1 status |
|---|---|
| `version.get` | Returns `daemon_api_version: "1.0.0+phase4"` |
| `status.get` | Returns 4-signal readiness (Connected/SessionValid/Synced/Ready) |
| `health.get` | Returns `{ok: true}` |
| `send.text` | Pre-flight 65,536-byte ceiling enforced |
| `groups.create` / `list` / `info` / `leave` | Stubbed — return `NotConnected` |
| `messages.list` | Stub — returns empty list |
| `rules.list` / `get` | Read-only stubs |
| `triggers.list` / `get` | Read-only stubs |
| `events.list` / `show` | Read-only stubs |
| `reconnect.now` | No-op in Phase 1 |
| `shutdown` | Cancels the daemon's cancellation token |

Non-Phase-1 methods return `-32601 MethodNotFound` with `data.api_version`
and `data.available_in`.

## CLI

```bash
# All commands mirror an RPC method.
octo-whatsapp version --socket <path>
octo-whatsapp status --socket <path>
octo-whatsapp send text +15551234567 --text "hello" --socket <path>
octo-whatsapp groups create --subject "ops" --members +15551234567,+15559876543 --socket <path>
octo-whatsapp groups list --socket <path>
octo-whatsapp messages list --limit 50 --socket <path>

# Onboard works WITHOUT a daemon running:
octo-whatsapp onboard qr-link --help
```

## MCP

```bash
octo-whatsapp mcp --socket <path>
```

Speaks stdio JSON-RPC 2.0. Initialize returns `protocolVersion: "2025-06-18"`.
Phase 1 exposes a minimal tool subset; full ~50 tools land in Phase 4.

## Stoolap invariant

The runtime crate MUST NOT directly depend on stoolap. All stoolap access
goes via `Arc<StoolapStore>` cloned from `octo-adapter-whatsapp`. The
`it_stoolap_uniqueness` grep test enforces this at CI time.

## See also

- Design: `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`
- Implementation plan: `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-phase1.md`
