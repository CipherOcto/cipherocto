# Research: Social Platform Transport Patterns for CipherOcto DOT

**Date:** 2026-05-28
**Status:** Research
**Sources:** OpenClaw, IronClaw, Hermes, ZeroClaw, 9Router architecture analysis + CipherOcto DOT (RFC-0850)

---

## Executive Summary

This research analyzes how five multi-platform agent architectures implement transport adapters for social messaging platforms, and synthesizes the patterns into a specification for CipherOcto's Deterministic Overlay Transport (DOT) adapter layer. The goal is to define how CipherOcto nodes use Telegram, Discord, Matrix, Signal, IRC, Nostr, Slack, WhatsApp, and Webhook platforms as **deterministic transport carriers** for overlay consensus traffic.

**Key Finding:** All five architectures converge on a common pattern: a trait/interface-based adapter abstraction with a central gateway orchestrator. CipherOcto's DOT already specifies this pattern (RFC-0850 §8), but the implementation currently has only 3 standalone adapters (Telegram, Discord, Matrix) that don't implement the `PlatformAdapter` trait, and 1 stub (NativeP2P). The remaining 9 platform types have no implementation.

---

## 1. Cross-Architecture Pattern Analysis

### 1.1 Adapter Abstraction Pattern

All five architectures use the same core pattern:

| Architecture | Trait/Interface | Key Methods |
|-------------|----------------|-------------|
| **OpenClaw** | Extension interface (TypeScript) | `connect()`, `sendMessage()`, `onMessage()` |
| **IronClaw** | `Channel` trait (Rust) | `send()`, `receive()` (async stream) |
| **Hermes** | `BasePlatformAdapter` (Python) | `connect()`, `send_message()`, `receive_messages()` |
| **ZeroClaw** | `Channel` trait (Rust) | `send()`, `receive()`, `capabilities()` |
| **CipherOcto DOT** | `PlatformAdapter` trait (Rust) | `send_envelope()`, `receive_messages()`, `canonicalize()`, `capabilities()` |

**CipherOcto's advantage:** The DOT `PlatformAdapter` trait adds `canonicalize()` (platform message → DeterministicEnvelope) and `capabilities()` (max payload, fragmentation support, rate limits). These are essential for deterministic transport but absent from all other architectures.

### 1.2 Gateway Orchestrator Pattern

| Architecture | Gateway | Manages | Key Feature |
|-------------|---------|---------|-------------|
| **OpenClaw** | `src/gateway` | Channel routing, sessions | LRU agent cache, cron scheduler |
| **IronClaw** | `ChannelManager` | `select_all` stream | Multiplexed async streams |
| **Hermes** | `GatewayRunner` | 31 platform adapters | PID guard, LRU agent cache (128) |
| **ZeroClaw** | Channel dispatcher | 35 channels | Trait-based, security modules |
| **CipherOcto DOT** | `DotGateway` | Adapter registry | Replay cache, signature verification |

**CipherOcto's advantage:** The `DotGateway` enforces deterministic processing: version validation → signature verification → replay cache → flags validation → forwarding. No other architecture enforces consensus-safe processing order.

### 1.3 Message Size Limits

| Platform | Max Payload | Fragmentation Required | RFC-0850 Table |
|----------|-----------|----------------------|----------------|
| Telegram | 4096 bytes | Yes (>4KB) | `0x0001` |
| Discord | 2000 bytes | Yes (>2KB) | `0x0002` |
| Matrix | 65536 bytes | Rare | `0x0003` |
| Nostr | 65536 bytes | Rare | `0x0004` |
| Signal | 65536 bytes | Rare | `0x0005` |
| IRC | 512 bytes | Always | `0x0006` |
| Slack | 40000 bytes | Rare | `0x0007` |
| WhatsApp | 65536 bytes | Rare | `0x0008` |
| Webhook | Unlimited | No | `0x0009` |
| NativeP2P | Unlimited | No | `0x000A` |
| Bluetooth | 512 bytes | Always | `0x000B` |
| LoRa | 256 bytes | Always | `0x000C` |
| WebRTC | 65536 bytes | Rare | `0x000D` |

**CipherOcto already implements:** `dot/fragment.rs` provides `fragment_envelope()` and `EnvelopeFragment` reassembly. This is unique — no other architecture has protocol-level fragmentation for social platform transport.

### 1.4 Authentication Patterns

| Platform | Auth Mechanism | CipherOcto Adapter Pattern |
|----------|---------------|---------------------------|
| Telegram | Bot API token | `TelegramConfig.bot_token` |
| Discord | Bot token + Webhook URL | `DiscordConfig.bot_token` + `webhook_url` |
| Matrix | Access token (homeserver) | `MatrixConfig.access_token` |
| Signal | Signal-CLI REST API | Plugin (C ABI) |
| IRC | NICKSERV / server password | Plugin (C ABI) |
| Nostr | NIP-42 AUTH (relay challenge) | Native (Ed25519) |
| Slack | Bot token (xoxb-) | Plugin (C ABI) |
| WhatsApp | Business API token | Plugin (C ABI) |
| Webhook | HMAC signature verification | Native |

### 1.5 Receive Patterns

| Architecture | Receive Model | CipherOcto Equivalent |
|-------------|---------------|----------------------|
| **OpenClaw** | Event-driven callbacks | `receive_messages()` polling |
| **IronClaw** | `tokio::select_all` stream | Future: async stream adapter |
| **Hermes** | Long-polling + webhook | `receive_messages()` polling |
| **ZeroClaw** | Async channel receiver | `receive_messages()` polling |
| **CipherOcto DOT** | Polling (`receive_messages`) | Current: batch return |

**Improvement opportunity:** The DOT `PlatformAdapter` currently uses batch polling (`receive_messages()` returns `Vec<RawPlatformMessage>`). For production, this should evolve toward an async stream model (`receive_stream()` → `impl Stream<Item=RawPlatformMessage>`) for lower latency. This is compatible with the trait — the existing `receive_messages()` can be the blocking fallback.

---

## 2. Adapter Implementation Strategies

### 2.1 Native Rust Adapters (implement `PlatformAdapter` directly)

These platforms have stable HTTP APIs and are suitable for native implementation:

| Platform | Library | Complexity | Priority |
|----------|---------|-----------|----------|
| **Telegram** | `reqwest` (Bot API) | Low | P0 |
| **Discord** | `reqwest` (Webhook + Gateway) | Medium | P0 |
| **Matrix** | `reqwest` (Client-Server API) | Medium | P0 |
| **Webhook** | `axum` (HTTP server) | Low | P0 |
| **Nostr** | `nostr-sdk` or raw relay protocol | Medium | P1 |

### 2.2 C ABI Plugin Adapters (loaded via `AdapterRegistry`)

These platforms need specialized clients or have unstable APIs:

| Platform | Reason for Plugin | External Dependency |
|----------|-------------------|-------------------|
| **Signal** | Requires signal-cli or libsignal | Java/native |
| **IRC** | Simple but varied server implementations | None |
| **Slack** | OAuth flow, Socket Mode complexity | `slack-sdk` |
| **WhatsApp** | Business API, encryption layer | `whatsapp-business-sdk` |
| **Bluetooth** | OS-specific BLE stack | `bluer` / `btleplug` |
| **LoRa** | Hardware-specific serial protocol | Device SDKs |
| **WebRTC** | ICE/DTLS/SCTP complexity | `webrtc-rs` |

### 2.3 Encoding Strategy for Social Platforms

All DOT envelopes are serialized to wire bytes (`envelope.to_wire_bytes()`, 282 bytes fixed). For social platform transport:

1. **Small payloads (IRC/LoRa/Bluetooth):** Base64-encode the 282-byte envelope → 378 chars. Fits in IRC's 512-byte PRIVMSG limit with room for channel prefix.

2. **Medium payloads (Telegram/Discord):** Base64-encode envelope + optional payload fragment. Use Telegram's `sendMessage` or Discord webhook.

3. **Large payloads (Matrix/Nostr):** Raw binary or Base64-encoded. Matrix supports arbitrary event content up to 65KB.

4. **Fragmented payloads:** Use `dot::fragment::fragment_envelope()` to split into platform-appropriate fragments. Each fragment is independently transportable.

---

## 3. Recommended Architecture

### 3.1 Layered Adapter Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    DotGateway (RFC-0850)                     │
│  version check → signature → replay → flags → forward       │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                   AdapterRegistry                            │
│  BTreeMap<BroadcastDomainId, Box<dyn PlatformAdapter>>       │
│  + C ABI plugin loading (libloading)                        │
│  + health monitoring + hot-reload                           │
└──────────────────────────┬──────────────────────────────────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
┌───────▼───────┐ ┌───────▼───────┐ ┌───────▼───────┐
│  Native Rust  │ │  Native Rust  │ │   C ABI       │
│  Adapters     │ │  Adapters     │ │   Plugins     │
│               │ │               │ │               │
│  Telegram     │ │  Webhook      │ │  Signal       │
│  Discord      │ │  Nostr        │ │  IRC          │
│  Matrix       │ │               │ │  Slack        │
│               │ │               │ │  WhatsApp     │
└───────────────┘ └───────────────┘ └───────────────┘
```

### 3.2 Adapter Lifecycle

1. **Registration:** Adapter registered with `AdapterRegistry` (native or C ABI plugin)
2. **Capability Report:** Adapter reports `CapabilityReport` (max payload, fragmentation, encryption, rate limits)
3. **Domain Mapping:** Each adapter provides `domain_id(platform_id)` → `BroadcastDomainId`
4. **Send Path:** `DotGateway` → `AdapterRegistry` → `PlatformAdapter::send_envelope()` → platform API
5. **Receive Path:** Platform event → `PlatformAdapter::receive_messages()` → `RawPlatformMessage` → `canonicalize()` → `DeterministicEnvelope` → `DotGateway::process_envelope()`
6. **Health Check:** Periodic `health_check()` probe
7. **Shutdown:** Graceful `shutdown()` with pending message flush

### 3.3 Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Native Rust for P0 platforms | Lower latency, no external runtime dependency, direct DOT integration |
| C ABI for P1+ platforms | Isolation, independent compilation, community contribution model |
| Base64 encoding for social platforms | Universal compatibility, human-debuggable, survives text-only channels |
| Fragmentation at DOT layer | Already implemented in `dot/fragment.rs`, adapter-agnostic |
| Replay cache at gateway level | Single source of truth, not duplicated per adapter |
| Per-adapter rate limiting | Platform-specific limits (Telegram: 30 msg/s, Discord: 5 msg/s) |

---

## 4. Comparison with Existing CipherOcto Adapters

The current `octo-adapter-*` crates are **standalone HTTP clients** that don't implement `PlatformAdapter`. They were designed as reference implementations for the plugin model. The design note in each says:

> "The adapter is a standalone HTTP client and does NOT implement the PlatformAdapter trait directly. A future FfiAdapter wrapper in octo-network will bridge the C ABI to PlatformAdapter."

**Two paths forward:**

1. **Refactor existing adapters** to implement `PlatformAdapter` directly (native Rust integration)
2. **Build the FfiAdapter bridge** and keep existing adapters as C ABI plugins

**Recommendation:** Path 1 for P0 platforms (Telegram, Discord, Matrix) — lower complexity, better integration. Path 2 for P1+ platforms (Signal, IRC, Slack, WhatsApp).

---

## 5. Third-Party Library Analysis

| Library | Language | Purpose | CipherOcto Use |
|---------|----------|---------|---------------|
| `teloxide` (in IronClaw) | Rust | Telegram Bot framework | Too heavy; use raw `reqwest` + Bot API |
| `serenity` (in ZeroClaw) | Rust | Discord API | Too heavy; use webhook + partial gateway |
| `matrix-sdk` | Rust | Matrix client | Consider for production; start with raw API |
| `nostr-rs-sdk` | Rust | Nostr relay client | Good fit for Nostr adapter |
| `irc-rs` / `irc` | Rust | IRC client | Good fit for IRC adapter |
| `reqwest` | Rust | HTTP client | Core dependency for all HTTP-based adapters |

---

## 6. Risk Analysis

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Platform API rate limits | Medium | Per-adapter rate limiter, exponential backoff |
| Platform API breaking changes | Medium | Versioned adapter interfaces, C ABI isolation |
| Message ordering non-determinism | High | DOT logical timestamp ordering (not arrival order) |
| Platform metadata leakage | High | Consensus isolation (RFC-0850 G3) — platform metadata NEVER affects consensus |
| Bot token compromise | High | Token rotation, minimal scope, environment-only storage |
| Platform censorship | High | Multi-carrier propagation (DGP), automatic failover |

---

## Recommendations

1. **Implement native Rust adapters** for Telegram, Discord, Matrix, Webhook (P0) — 0850f/g/h/q
2. **Implement Nostr adapter** using `nostr-rs-sdk` (P1 — censorship resistance) — 0850k
3. **Build WASM plugin bridge** for Signal, IRC, Slack, WhatsApp (P1) — 0850i
4. **Add async stream receive** to `PlatformAdapter` trait (P2 — latency optimization)
5. **Implement adapter health monitoring** with automatic failover (P2)

## Next Steps

- [x] Research complete
- [x] Use Case: Social Platform Transport Layer → `docs/use-cases/social-platform-transport-layer.md`
- [x] Canonicalize pipeline: all 3 Tier 1 adapters now use `from_wire_bytes()`
- [ ] Mission 0850i: WASM Plugin Runtime
- [ ] Tier 2/3 missions: 0850j-o/q/r

## Related Missions

The social platform transport layer is covered by 19 missions under RFC-0850 (see `docs/plans/2026-05-28-social-platform-transport-adapters-design.md`). This research provides the cross-architecture analysis that informed those missions.
