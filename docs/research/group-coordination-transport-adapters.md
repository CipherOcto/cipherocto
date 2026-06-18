# Research: Group Coordination on DOT Transport Adapters

**Date:** 2026-06-17
**Status:** Research
**Scope:** 20 platform adapters in `crates/octo-adapter-*` and the
`PlatformAdapter` trait in `crates/octo-network/src/dot/adapters/mod.rs`.
**Goal:** Catalogue how group coordination is implemented on each adapter
and identify gaps where transports without native groups cannot achieve
the same behavior as transports with native groups, even with extra code.

---

## Executive Summary

The DOT `PlatformAdapter` trait models groups abstractly as
`BroadcastDomainId = (platform_type, BLAKE3-256("prefix:{platform_id}"))`.
The platform_id is whatever the caller passes — a native group ID
(WhatsApp JID, Matrix room ID, Telegram chat_id, …) or a synthetic string
the adapter invents. This is the right level of abstraction for the
"happy path" of cross-platform group coordination, but it has two real
gaps that cause silent routing failures on **two of the twenty adapters**:

1. **`domain_id` format inconsistency** (IRC, Nostr). The static
   `domain_hash(...)` and the trait `domain_id(...)` methods produce
   *different* hashes unless the caller knows the internal format. The
   `send_envelope` lookup uses the static method; if the caller computed
   the `domain_id` via the trait method with a different platform_id
   format, routing fails with "No channel for domain" / "No room for
   domain".
2. **1:1 transport with no fan-out** (BLE, LoRa, QUIC, WebRTC, Webhook,
   Bluesky 1:1 DMs, Twitter DMs, Matrix DMs, Signal 1:1). These
   transports have no native concept of a "group of recipients". The
   adapter either ignores the domain parameter entirely, or only delivers
   to one peer. To get 1:N semantics on a 1:1 transport, the adapter
   needs an explicit membership table and a fan-out loop, and the
   `PlatformAdapter` trait currently has no `group_members(domain)` API
   to expose that membership.

With **extra code**, every 1:1 transport can be brought to functional
parity for many DOT use cases, but the trait itself is missing the
extension point (`group_members`, `add_group_member`, `remove_group_member`)
needed to do it cleanly.

The 20 adapters fall into four tiers by group-coordination capability:

| Tier | Adapters | Native groups? | Adapter handles membership? |
|------|----------|---------------|----------------------------|
| **1. Native groups** | WhatsApp, Telegram, Matrix (+matrix-sdk), Discord, Slack, IRC, Signal, WeChat, QQ, DingTalk, Lark, Reddit | Yes | Partially — uses platform's group IDs but config is local |
| **2. Synthetic channel** | Nostr | No (uses `#t` tag as channel) | One tag per adapter instance |
| **3. Single recipient** | Bluetooth, LoRa, QUIC, WebRTC, Webhook, Bluesky, Twitter | No (1:1 transport) | No — caller routes per-peer |
| **4. Native pub/sub** | NativeP2P | Yes (gossipsub topics) | Yes — gossipsub mesh |

---

## 1. The `PlatformAdapter` trait (RFC-0850 §8.2)

```rust
pub trait PlatformAdapter: Send + Sync {
    async fn send_envelope(&self, domain: &BroadcastDomainId, envelope: &DeterministicEnvelope)
        -> Result<DeliveryReceipt, PlatformAdapterError>;
    async fn receive_messages(&self, domain: &BroadcastDomainId)
        -> Result<Vec<RawPlatformMessage>, PlatformAdapterError>;
    fn canonicalize(&self, raw: &RawPlatformMessage)
        -> Result<DeterministicEnvelope, PlatformAdapterError>;
    fn capabilities(&self) -> CapabilityReport;
    fn domain_id(&self, platform_id: &str) -> BroadcastDomainId;
    fn platform_type(&self) -> PlatformType;
    // ... replay_protection, health_check, shutdown, self_handle,
    //     upload_media, download_media
}
```

The trait takes a `BroadcastDomainId` per call. It does not expose:

- Group membership: "who is in this group?"
- Group lifecycle: "create", "join", "leave", "destroy"
- Multi-recipient fan-out: "deliver to N peers"
- Group metadata: "name", "topic", "member count", "creation time"

This is fine for Tier-1 adapters where the platform manages the group
on the server side. It is a real gap for Tier-3 (1:1 transport) and
Tier-2 (Nostr) adapters that need to *synthesize* group membership on
the client side.

---

## 2. Adapters by tier

### Tier 1 — Native groups (12 adapters)

These transports have a real "group" primitive on the platform server
side. The adapter maps a native group ID (JID, room ID, chat_id, …) to
a `BroadcastDomainId` and uses the platform's API to send/receive in
that group.

| Adapter | platform_type | Native group ID | `domain_hash` | `send_envelope` uses domain? |
|---------|---------------|----------------|---------------|------------------------------|
| `octo-adapter-whatsapp` | 0x0008 | `xxx@g.us` JID | `BLAKE3("whatsapp:{jid}")` | ✓ (iterates `self.config.groups`) |
| `octo-adapter-telegram` | 0x0001 | chat_id (i64) | `BLAKE3("telegram:{chat_id}")` | ✓ (uses `domain_chat_ids` map) |
| `octo-adapter-matrix` | 0x0003 | `!opaque:server` | `BLAKE3("matrix:{room_id}")` | ✓ (iterates `self.config.rooms`) |
| `octo-adapter-matrix-sdk` | 0x0003 | `!opaque:server` | `BLAKE3("matrix:{room_id}")` | ✓ (iterates `self.config.rooms`) |
| `octo-adapter-discord` | 0x0002 | channel_id | `BLAKE3("discord:{channel_id}")` | ✗ (sends to a single configured webhook) |
| `octo-adapter-slack` | 0x0007 | channel_id | `BLAKE3("slack:{channel_id}")` | ✗ (uses `send_to_channel` helper) |
| `octo-adapter-irc` | 0x0006 | `#channel` on server | `BLAKE3("irc:{server}:{channel}")` | ✓ (iterates `self.config.channels`, **see §3.1**) |
| `octo-adapter-signal` | 0x0005 | group_id | `BLAKE3("signal:{group_id}")` | ✓ (iterates `self.config.groups`) |
| `octo-adapter-wechat` | 0x0011 | group_id | `BLAKE3("wechat:{group_id}")` | (stub) |
| `octo-adapter-qq` | 0x0014 | group_id | `BLAKE3("qq:{group_id}")` | (stub) |
| `octo-adapter-dingtalk` | 0x0012 | group_id | `BLAKE3("dingtalk:{group_id}")` | (stub) |
| `octo-adapter-lark` | 0x0013 | chat_id | `BLAKE3("lark:{chat_id}")` | (stub) |
| `octo-adapter-reddit` | 0x0010 | subreddit | `BLAKE3("reddit:{subreddit}")` | (stub) |

All but Discord and Slack use the configured `groups/rooms/channels`
list to find the native ID for a `BroadcastDomainId` at send time.
Discord/Slack work around the single-webhook limitation by being
configured with one webhook per channel (one adapter instance per group).

**Observation:** Even for Tier-1 transports, the *gateway* must know
the native group ID at config time. There is no `discover_groups()`
API; the gateway cannot say "what groups am I in on WhatsApp?"

### Tier 2 — Synthetic channel (1 adapter: Nostr)

Nostr is a relay-based protocol with no native group concept. The
adapter uses a `channel_tag` field (default `cipherocto-dot`) that is
attached to every published event as a `#t` tag, and the receive filter
subscribes to events with that tag.

```rust
// crates/octo-adapter-nostr/src/lib.rs
pub fn domain_hash(relay_url: &str, channel_tag: &str) -> [u8; 32] {
    *blake3::hash(format!("nostr:{normalized}:{channel_tag}").as_bytes()).as_bytes()
}
fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
    BroadcastDomainId::new(PlatformType::Nostr, platform_id)
}
async fn send_envelope(&self, _domain: &BroadcastDomainId, envelope: ...) {
    // ignores domain; uses self.config.channel_tag and self.config.relays
}
```

**Gap:** Two different hash formats.

- `domain_hash(relay_url, channel_tag)` → `BLAKE3("nostr:wss://relay.com:tag")`
- `domain_id("wss://relay.com:tag")` → `BLAKE3("nostr:wss://relay.com:tag")`

These match IF the caller passes `"wss://relay.com:tag"` as the
platform_id. But the static method's two-argument signature invites
callers to compute it directly from the two-arg form. The `send_envelope`
ignores the domain and just uses `self.config`, so a mismatch is silent.

To get "multiple groups" on Nostr, the current design is to instantiate
one NostrAdapter per (relay_set, channel_tag) pair. That's "extra code
outside the adapter" — the user is doing the fan-out, not the adapter.

### Tier 3 — Single-recipient / 1:1 transport (7 adapters)

These transports have no group concept at all. A "group" is either a
synthetic string identifier the caller invents, or a single configured
endpoint.

| Adapter | platform_type | Transport | "Group" model | Group support today |
|---------|---------------|-----------|---------------|---------------------|
| `octo-adapter-bluetooth` | 0x000B | BLE 1:1 GATT | Synthetic string ID; no fan-out | Adapter ignores domain; pushes to local TX buffer |
| `octo-adapter-lora` | 0x000C | LoRa 1:1 radio | Synthetic device_id | Adapter ignores domain; same stub as BLE |
| `octo-adapter-quic` | 0x0009 | QUIC 1:1 stream | Synthetic peer_id; `peers` map | Sends to **first trusted peer** in map, not all |
| `octo-adapter-webrtc` | 0x000D | WebRTC 1:1 data channel | Synthetic peer_id | Adapter ignores domain; stub |
| `octo-adapter-webhook` | 0x0010 | HTTP POST | One URL per adapter | Adapter ignores domain; posts to `config.send_url` |
| `octo-adapter-bluesky` | 0x000E | AT Protocol (1:1 DMs / public posts) | DIDs (no group) | Adapter ignores domain; stub |
| `octo-adapter-twitter` | 0x000F | X/Twitter 1:1 DMs | DMs (no group) | Adapter ignores domain; stub |

These adapters *cannot* do 1:N broadcast without:

- A group membership table (a `BTreeMap<BroadcastDomainId, Vec<PeerId>>`)
- A fan-out loop in `send_envelope` that iterates the members
- A way for the gateway to add/remove members (peer join/leave)

The current `PlatformAdapter` trait has no API for any of this. To
build a Bluetooth "group" of three phones, the user would have to
subclass `BluetoothAdapter` (or wrap it) and add the membership table
themselves.

### Tier 4 — Native pub/sub via gossipsub (1 adapter: NativeP2P)

`octo-adapter-p2p` uses libp2p gossipsub. Each `BroadcastDomainId` is
mapped deterministically to a gossipsub topic name, and any peer
subscribed to that topic receives the broadcast:

```rust
// crates/octo-adapter-p2p/src/lib.rs
fn domain_to_topic(domain: &BroadcastDomainId) -> String {
    format!("cipherocto-{}", hex_encode(&domain.domain_hash))
}
```

Gossipsub handles:

- Peer discovery (mesh formation)
- Membership (peers subscribe/unsubscribe at runtime)
- Message propagation (mesh fan-out, gossip)
- Failure handling (peer churn, splits, joins)

This is the cleanest implementation in the codebase. The other Tier-3
adapters would benefit from a similar pattern: rather than reimplementing
membership in every adapter, route through a single gossipsub mesh that
is keyed by `BroadcastDomainId`. The transport-specific adapter would
then just be a thin wrapper that translates wire formats.

---

## 3. The two real bugs

### 3.1 IRC `domain_hash` format mismatch

`crates/octo-adapter-irc/src/lib.rs:225` defines the static:

```rust
pub fn domain_hash(server: &str, channel: &str) -> [u8; 32] {
    *blake3::hash(format!("irc:{}:{}", server.trim().to_lowercase(), channel).as_bytes()).as_bytes()
}
```

and the trait method at line 506:

```rust
fn domain_id(&self, platform_id: &str) -> BroadcastDomainId {
    BroadcastDomainId::new(PlatformType::IRC, platform_id)
}
```

`BroadcastDomainId::new(PlatformType::IRC, platform_id)` produces
`BLAKE3("irc:{platform_id}")` — a single-argument hash.

For these to match, the caller must pass `"server:channel"` (e.g.
`"irc.libera.chat:#cipherocto"`) as the platform_id. The static
method's two-arg form tempts callers to do the right thing in the
wrong place, and any caller using `domain_id("#cipherocto")` (just the
channel) will get a hash that doesn't match any configured channel,
and `send_envelope` will fail with "No channel for domain …".

**Recommendation:** Pick one canonical format and document it. Either
(a) keep the two-arg form and have `domain_id(server, channel)`, or
(b) collapse to single-arg and have callers pass `"server:channel"`.
Option (a) is more discoverable; option (b) matches every other
adapter. Pick (b) for consistency.

### 3.2 Nostr `domain_hash` format mismatch

Same shape as IRC. `crates/octo-adapter-nostr/src/lib.rs:204`:

```rust
pub fn domain_hash(relay_url: &str, channel_tag: &str) -> [u8; 32] {
    *blake3::hash(format!("nostr:{normalized}:{channel_tag}").as_bytes()).as_bytes()
}
```

`domain_id(platform_id)` (line 414) calls `BroadcastDomainId::new(...)`
which produces `BLAKE3("nostr:{platform_id}")`. The caller must pass
`"relay_url:channel_tag"` for the hashes to match.

`send_envelope` (line 334) **ignores the domain parameter entirely** and
just uses `self.config.channel_tag` and `self.config.relays`. So the
format mismatch is masked: the caller can't actually use the domain
parameter to address different Nostr "groups" on the same adapter
instance.

**Recommendation:** Either (a) make `send_envelope` honour the domain
parameter and look up `(relay_set, channel_tag)` by hash, or (b) drop
the static `domain_hash(relay_url, channel_tag)` and have a single
config-level channel.

---

## 4. What "extra code" actually looks like for Tier-3 parity

For each Tier-3 transport, here's the additional code (not in the
codebase today) needed to bring group coordination to the same level
as a Tier-1 transport like WhatsApp:

### 4.1 Bluetooth / LoRa (mesh over short-range radio)

Both adapters are 1:1 with no IP layer. The realistic "group" model is
**a shared pre-arranged group_id that all member devices have agreed
on out of band** (a QR code scan, a setup code, etc.).

Extra code needed:

1. A `BTreeMap<[u8; 32], Vec<DeviceId>>` in the adapter: domain → members.
2. A setup API (`register_group_member(domain, device_id)`) called by
   the gateway when a new device pairs.
3. A `send_envelope` that iterates the member list and writes to each
   device's TX buffer (sequentially, since BLE is half-duplex and LoRa
   is single-frequency).
4. Inbound filter in `receive_messages` that drops any message whose
   `BroadcastDomainId` isn't in the member set.

Parity: **YES**, but for small N (≤8) due to BLE/LoRa timing. The
"platform manages membership" property is replaced by an
"adapter has a local membership table" property.

### 4.2 QUIC / WebRTC (peer-to-peer with optional relay)

Both are 1:1 streams. The realistic "group" model is **either (a)
gossip/membership via libp2p (already done in `NativeP2P`), or (b) a
star topology with a designated hub peer**.

Extra code needed (star topology):

1. A `peers: BTreeMap<[u8; 32], Vec<PeerId>>` table: domain → members.
2. A `send_envelope` that iterates members and opens a 1:1 stream to
   each. With QUIC this is one `open_bi()` per peer.
3. Inbound filter as above.
4. A heartbeat/keepalive to detect dead members and evict.

Parity: **YES** for small N (≤50). At larger N, the per-peer
connection setup latency dominates; gossipsub would be better.

### 4.3 Webhook (HTTP POST)

Already 1:1 with a configured URL. The "group" model is **one URL per
group, configured statically**.

Extra code needed:

1. Replace the single `send_url: Option<String>` with
   `send_urls: BTreeMap<[u8; 32], String>` (domain → URL).
2. Have `send_envelope` look up the URL by domain hash.
3. The HMAC signing (currently in `send_envelope`) is per-URL, so
   `auth_header` and `hmac_secret` need to be per-URL too.

Parity: **YES**, and the change is mechanical. The user pays for it in
config-file complexity (one entry per group instead of one global
entry).

### 4.4 Bluesky / Twitter (1:1 DMs + public posts)

The realistic "group" model is **a list of DIDs that the bot knows
about, with the bot DMing each member individually** (for private
groups) or **posting to a public thread that subscribers watch** (for
public groups).

Extra code needed:

1. A `BTreeMap<[u8; 32], Vec<Did>>` table: domain → members.
2. `send_envelope` iterates members and DMs each.
3. Inbound filter as above.

Parity: **YES for small N** (≤50 DMs per broadcast; Bluesky rate limit
is 5000/hour for DMs, Twitter is 1000/day for DMs).

### 4.5 Matrix / Signal (already have groups, but the model is
server-mediated)

These are Tier-1 transports. No extra code needed; the platform
manages the group.

---

## 5. Recommendations

### 5.1 Fix the two real bugs (IRC, Nostr)

Pick a canonical `domain_id` format. Recommended:

- **IRC**: collapse to `BLAKE3("irc:{server}:{channel}")` and require
  callers to pass `server:channel` as the platform_id. Remove the
  static `domain_hash(server, channel)` and inline it into
  `domain_id`.
- **Nostr**: same shape — `BLAKE3("nostr:{relay_url}:{channel_tag}")`
  with the colon-separator format. Make `send_envelope` honour the
  domain by looking up the right `(relay_set, channel_tag)` from a
  `BTreeMap<[u8; 32], (Vec<String>, String)>` keyed by the domain hash.

Estimated effort: ~50 LOC + 4 regression tests.

### 5.2 Add a "group membership" trait extension (optional)

For Tier-3 adapters that want to support 1:N semantics without
requiring the gateway to know the membership table, add:

```rust
#[async_trait]
pub trait GroupMembership: Send + Sync {
    async fn group_members(&self, domain: &BroadcastDomainId) -> Vec<PeerId>;
    async fn add_group_member(&self, domain: &BroadcastDomainId, peer: PeerId)
        -> Result<(), PlatformAdapterError>;
    async fn remove_group_member(&self, domain: &BroadcastDomainId, peer: PeerId)
        -> Result<(), PlatformAdapterError>;
}
```

with a default impl that returns `Err(Unimplemented)` for adapters
that don't support it. Tier-1 adapters would inherit the default
(the platform manages membership). Tier-3 adapters that want to
synthesize groups would override it.

This is the cleanest way to express "transports without native groups
but with extra code can have the same behavior" — the trait has a
clear extension point for membership, and adapters opt in.

### 5.3 Unify the static `domain_hash` / instance `domain_id` API

Today, 17 of 20 adapters define a static `domain_hash(...)` and an
instance `domain_id(...)`. Two of those 17 have inconsistent formats
(IRC, Nostr). The static is used for config-time lookups and tests;
the instance is used for runtime lookups.

Recommendation: remove all the static methods and have callers use
`adapter.domain_id(...)` only. The `BroadcastDomainId::new(...)` is
already a static method on the type, and that produces the correct
hash. Adapters that need config-time lookup (e.g. to compute the
hash from `config.rooms`) can use `BroadcastDomainId::new(...)` with
the platform's static `PLATFORM_TYPE` constant.

This eliminates 17 redundant static methods and the two bugs above
in one stroke. Estimated effort: ~80 LOC + 12 regression tests.

### 5.4 Document the contract for "transports without native groups"

For each Tier-3 adapter, add a doc comment explaining:

- What a "group" means on this transport (e.g. "an agreed-upon string
  identifier; the adapter doesn't manage membership")
- How group coordination is achieved (e.g. "the gateway must call
  `send_envelope(domain, envelope)` once per recipient, with each
  recipient's adapter instance")
- The maximum realistic group size (e.g. "≤8 for BLE; ≤50 for QUIC;
  ≤50 for Webhook")
- Whether the adapter supports the `GroupMembership` trait (default
  is no)

This is a doc-only change. Estimated effort: 7 doc comments.

---

## 6. Summary table

| Adapter | Tier | Domain format match? | `send_envelope` honours domain? | Native group support | Group size limit |
|---------|------|----------------------|---------------------------------|----------------------|------------------|
| whatsapp | 1 | ✓ | ✓ | Platform server | platform limit |
| telegram | 1 | ✓ | ✓ | Platform server | platform limit |
| matrix | 1 | ✓ | ✓ | Platform server | platform limit |
| matrix-sdk | 1 | ✓ | ✓ | Platform server | platform limit |
| discord | 1 | ✓ | ✗ (single webhook) | Platform server | platform limit |
| slack | 1 | ✓ | ✗ (single webhook) | Platform server | platform limit |
| irc | 1 | **✗ (mismatch)** | ✓ | Platform server | platform limit |
| signal | 1 | ✓ | ✓ | Platform server | platform limit |
| wechat | 1 | ✓ | (stub) | Platform server | platform limit |
| qq | 1 | ✓ | (stub) | Platform server | platform limit |
| dingtalk | 1 | ✓ | (stub) | Platform server | platform limit |
| lark | 1 | ✓ | (stub) | Platform server | platform limit |
| reddit | 1 | ✓ | (stub) | Platform server | platform limit |
| nostr | 2 | **✗ (mismatch)** | ✗ (uses config) | Synthetic tag | 1 tag per adapter |
| bluesky | 3 | ✓ | ✗ (stub) | None (DIDs) | ≤50 (DM rate) |
| twitter | 3 | ✓ | ✗ (stub) | None (DMs) | ≤50 (DM rate) |
| bluetooth | 3 | ✓ | ✗ (stub) | None (1:1 GATT) | ≤8 (timing) |
| lora | 3 | ✓ | ✗ (stub) | None (1:1 radio) | ≤8 (timing) |
| quic | 3 | ✓ | partial (1 peer) | None (1:1 stream) | ≤50 (with fan-out) |
| webrtc | 3 | ✓ | ✗ (stub) | None (1:1 datachannel) | ≤50 (with fan-out) |
| webhook | 3 | ✓ | ✗ (1 URL) | None (1:1 HTTP) | unlimited (1 URL each) |
| nativep2p | 4 | ✓ | ✓ (gossipsub topic) | libp2p gossipsub | mesh-dependent |

**Tier 1:** 12 adapters (with 2 bugs in domain format and 2 adapters
that don't honour the domain in `send_envelope`).
**Tier 2:** 1 adapter (with 1 domain format bug and 1 ignored-domain bug).
**Tier 3:** 6 adapters (no native groups; would need extra code for 1:N).
**Tier 4:** 1 adapter (gossipsub — already correct).

**Net assessment:** 13 of 20 adapters have at least one real issue
preventing correct group coordination. The fixes are small (~130 LOC
+ 16 regression tests total) and the architecture is sound.
