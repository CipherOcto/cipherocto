# Use Case: Social Platform Transport Layer

**Date:** 2026-05-28
**Status:** Draft

---

## Problem

CipherOcto's DOT overlay network (RFC-0850) specifies 13 platform types for transport but only has 3 standalone adapters (Telegram, Discord, Matrix) that don't implement the `PlatformAdapter` trait, and 1 stub (NativeP2P). Without working social platform adapters, CipherOcto nodes can only communicate via direct P2P, limiting reach to users who run their own nodes.

Social platforms have **billions** of users. By operating as a parasitic overlay on existing communication infrastructure, CipherOcto can reach users without requiring them to install specialized software. A Telegram user can interact with a CipherOcto agent by messaging a bot. A Discord server can host a CipherOcto mission overlay through a webhook integration.

## Stakeholders

- **Primary:** CipherOcto node operators who need multi-transport resilience
- **Secondary:** End users who interact with CipherOcto agents via social platforms
- **Affected:** Platform operators (passive — CipherOcto traffic is encrypted overlay data)

## Motivation

### Why This Matters for CipherOcto

1. **Censorship Resistance:** If libp2p is blocked, the network survives through Telegram/Discord/Matrix transports. RFC-0852 DGP's multi-carrier propagation requires multiple working carriers.

2. **Reach:** 2B+ Telegram users, 200M+ Discord users, millions of Matrix users. Social platform transport is the path to mass adoption without requiring node installation.

3. **Onion Routing:** RFC-0858 ORR specifies multi-transport onion paths (e.g., Telegram → Matrix → QUIC → Bluetooth). This only works if the adapters actually function.

4. **Proof-of-Relay:** RFC-0860 PoRelay requires gateways to forward messages across platforms. Working adapters are the foundation.

5. **Agent Economy:** Missions (RFC-0855 MON) need transport diversity. Agents that only use one transport are vulnerable to platform-specific censorship.

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Working adapters | 7+ platforms | Adapter passes integration test |
| End-to-end latency | <2s social platform hop | Measured from send to receive |
| Fragmentation success | 100% for IRC/LoRa | All fragments reassembled correctly |
| Multi-carrier propagation | 3+ simultaneous | Same envelope via 3+ platforms |
| Censorship failover | <30s recovery | Automatic switch on platform block |

## Constraints

- **Must not:** Store platform credentials in protocol state
- **Must not:** Allow platform metadata to affect consensus ordering
- **Must not:** Require platform-specific knowledge at the DOT layer
- **Limited to:** Platforms listed in RFC-0850 §3.1 platform type table

## Non-Goals

- Building a chat application (we are a transport layer)
- Replacing platform-native encryption (we add our own)
- Optimizing for human readability (base64-encoded envelopes are opaque)

## Impact

When implemented, CipherOcto nodes can:
- Send DOT envelopes through Telegram bots, Discord webhooks, Matrix rooms
- Receive and process envelopes from any connected social platform
- Route onion-encrypted messages across heterogeneous social transports
- Survive single-platform censorship through automatic failover
- Earn Proof-of-Relay rewards for cross-platform forwarding

## Related RFCs

- RFC-0850: Deterministic Overlay Transport — adapter trait, envelope format, fragmentation
- RFC-0851: Gateway Discovery Protocol — gateway advertisements
- RFC-0851p-a (Networking): Network Bootstrap Protocol — peer-to-peer bootstrap channels
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding — Tier 3 transport (WhatsApp)
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `domain_id` ↔ physical group mapping
- RFC-0852: Deterministic Gossip Protocol — multi-carrier propagation
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — coordinator state machine
- RFC-0855p-c (Networking): DomainCoordinator Role — physical-platform coordinator
- RFC-0856: Deterministic Route Selection — transport diversity scoring
- RFC-0858: Onion Relay Routing — multi-transport onion paths
- RFC-0860: Proof-of-Relay — relay verification across platforms

## Related Missions (RFC-0850)

The social platform transport layer is covered by 19 missions under RFC-0850, organized by tier:

| Tier | Missions | Status |
|------|----------|--------|
| Foundation | 0850 (Core Envelope), 0850a-d (Fragmentation, Federation, Privacy, Reliability) | 0850 Implemented |
| Tier 1 (High Reach) | 0850e (Registry), 0850f (Telegram), 0850g (Discord), 0850h (Matrix) | **All Implemented** |
| Tier 1.5 (WASM) | 0850i (WASM Plugin Runtime) | Spec'd |
| Tier 2 (Privacy) | 0850k (Nostr), 0850l (Signal) | Spec'd |
| Tier 3 (Opportunistic) | 0850j (IRC), 0850o (Slack), 0850p (WhatsApp), 0850q (Webhook), 0850r (WebRTC), 0850m (Bluetooth), 0850n (LoRa) | Spec'd |

**Active work item:** The `canonicalize()` pipeline — updating Tier 1 adapters to use `to_wire_bytes()`/`from_wire_bytes()` on `DeterministicEnvelope`.

## Related Documentation

- [Social Platform Transport Design](../plans/2026-05-28-social-platform-transport-adapters-design.md) — Approved design doc
- [Social Platform Transport Patterns (Research)](../research/social-platform-transport-patterns.md) — Cross-architecture analysis
- [OpenClaw Architecture](../research/openclaw-architecture.md)
- [IronClaw Architecture](../research/ironclaw-architecture.md)
- [Hermes Agent Architecture](../research/hermes-agent-architecture.md)
- [ZeroClaw Architecture](../research/zeroclaw-architecture.md)
