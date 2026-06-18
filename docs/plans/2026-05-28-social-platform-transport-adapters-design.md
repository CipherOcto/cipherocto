# Design: Social Platform Transport Adapters

**Date:** 2026-05-28
**Status:** Approved
**RFC:** RFC-0850 (Deterministic Overlay Transport)

## Overview

Extend DOT with concrete platform adapter implementations using a hybrid plugin architecture: `cdylib` for official Rust adapters (Tier 1), WASM for community adapters (sandboxed), and external bridge processes for platforms requiring native SDKs or hardware (Tier 2/3).

## Architecture

### Three Layers

1. **Adapter Registry** (`dot/adapters/registry.rs`) — Discovers and loads adapters at startup. Scans `adapters/native/` for `.so` files and `adapters/wasm/` for `.wasm` files. Maps `PlatformType` → loaded adapter.

2. **Adapter Plugin ABI** (`dot/adapters/abi.rs`) — Stable C ABI for `cdylib` adapters:
   ```rust
   extern "C" fn adapter_version() -> u32;
   extern "C" fn platform_type() -> u16;
   extern "C" fn create_adapter(config: *const u8, config_len: usize) -> *mut ();
   ```
   WASM adapters use same logical interface via wasmtime component model.

3. **Platform Adapters** — Each adapter is a separate crate in `crates/octo-adapter-{name}/` compiled to `cdylib`. Depends on `octo-network` for `PlatformAdapter` trait. Configuration passed as JSON at construction.

### Tier 1 Adapters (Pure Rust, cdylib)

| Platform | Crate | API | Max Payload | Key Feature |
|----------|-------|-----|-------------|-------------|
| Telegram | `octo-adapter-telegram` | Bot API (HTTP) | 4096 bytes | Long-polling/webhook, file attachments |
| Discord | `octo-adapter-discord` | Webhooks + Gateway | 2000 bytes | Webhook send, Gateway WS receive |
| Matrix | `octo-adapter-matrix` | Client-Server API | 65536 bytes | Federated, largest payload |

### Tier 2 Adapters (Privacy-first, bridge processes)

| Platform | Crate | Mechanism | Notes |
|----------|-------|-----------|-------|
| Nostr | `octo-adapter-nostr` | NIP relay protocol (pure Rust) | Relay federation, censorship-resistant |
| Signal | `octo-adapter-signal` | Bridge to `signal-cli` | Requires Java process, E2E encrypted |
| Session | `octo-adapter-session` | Bridge to session SDK | Onion-routed, no phone number |

### Tier 3 Adapters (Opportunistic, bridge/hardware)

| Platform | Crate | Mechanism | Notes |
|----------|-------|-----------|-------|
| IRC | `octo-adapter-irc` | Pure Rust IRC client | Simple text protocol, legacy |
| Slack | `octo-adapter-slack` | Bot API (HTTP) | Enterprise, rate-limited |
| Briar | `octo-adapter-briar` | Bridge to Briar daemon | P2P, Tor-based |
| Bluetooth | `octo-adapter-bluetooth` | Bridge to BLE daemon | Local mesh, no internet |
| LoRa | `octo-adapter-lora` | Bridge to serial/HW | Long-range, ultra-low bandwidth |

### WASM Plugin API

Sandboxed execution for community adapters. Host functions:
- `http_request` — TLS-only, domain allowlists, size limits
- `log` — structured logging
- `current_epoch` — consensus time

WASM adapters cannot access filesystem, network, or environment directly. Best for webhook/poll-based platforms.

## Gateway Integration

Adapter selection by `BroadcastDomainId.platform_type` → registry lookup → `CapabilityReport` check → dispatch.

Fragmentation: when envelope exceeds `max_payload_bytes`, `FragmentAssembler` splits into carrier-sized fragments with sequence headers.

Canonicalization pipeline: `RawPlatformMessage` → `canonicalize()` → `verify()` → `replay_cache.check()` → `process_envelope()`.

## Implementation Order

1. `0850e` — Adapter Registry & Plugin ABI
2. `0850f` — Telegram Adapter (parallel with 0850g, 0850h)
3. `0850g` — Discord Adapter
4. `0850h` — Matrix Adapter
5. `0850i` — WASM Plugin Runtime
6. Tier 2/3 missions as separate workstreams
