# Mission: RFC-0850 Update — self_handle, Dual Transport, Platform Table

## Status

Completed (all changes applied in commit 919454a)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT)

## Summary

Update RFC-0850 to reflect the current state of the implementation and add missing specifications for `self_handle()`, dual binary transport, and platform coverage.

## Sections to Update

### 1. Section 8.2: Platform Adapter Contract

**Current**: Missing `self_handle()` method in trait definition.

**Update**: Add `self_handle()` to the trait:

```rust
#[async_trait]
trait PlatformAdapter: Send + Sync {
    // ... existing methods ...

    /// Return the bot's own handle/identity on this platform.
    ///
    /// Used by the gateway to drop self-authored messages and prevent
    /// relay loops. Returns None by default (no self-loop protection).
    /// Adapters that handle inbound traffic MUST override this.
    fn self_handle(&self) -> Option<String> { None }
}
```

### 2. Section 8.5: Carrier-Specific Fragmentation

**Current**: Lists Telegram, Discord, Matrix, IRC, LoRa, BLE.

**Update**: Add missing platforms:

| Carrier | Max Payload | Fragment Strategy |
|---------|-------------|-------------------|
| Telegram | 4096 bytes | Document attachment for large fragments |
| Discord | 2000 bytes | Multi-message with sequence markers |
| Matrix | 65536 bytes | Rarely fragmented; media upload for large payloads |
| IRC | 512 bytes | Multi-line with sequence markers |
| Slack | 40000 bytes | Multi-message with sequence markers |
| Signal | 65536 bytes | Text only, no fragmentation |
| Nostr | 65536 bytes | Text only, no fragmentation |
| WhatsApp | 65536 bytes | Text only, no fragmentation |
| LoRa | 256 bytes | Mandatory fragmentation, duty-cycle aware |
| BLE | 244 bytes | Multi-advertisement reassembly |
| Webhook | Unlimited | No fragmentation needed |
| WebRTC | 65536 bytes | DataChannel fragmentation |

### 3. Section 8.6: Payload Encoding

**Current**: Only mentions `DOT/1/{base64}`.

**Update**: Add dual-mode transport wire formats:

```text
DOT/1/{base64}       → Text mode (base64url-encoded envelope bytes)
DOT/2/{msg_id}       → Native upload mode (platform message ID reference)
DOT/F/{base64_frag}  → Fragment mode (base64-encoded fragment with header)
```

Transport mode selection:
- If payload fits in single message → `DOT/1/{base64}`
- If payload exceeds limit AND platform supports upload → `DOT/2/{msg_id}`
- If payload exceeds limit AND no upload → `DOT/F/{fragment}`

Mode is an adapter detail — receivers auto-detect from `DOT/` prefix.

### 4. Section 9: Envelope Fragmentation

**Current**: Only describes text-based fragmentation.

**Update**: Add dual-mode transport section:

#### 9.4 Dual-Mode Transport

For platforms supporting native file upload (Telegram, Discord, Matrix), envelopes MAY be sent via platform media API instead of base64 text. The `DOT/2/{msg_id}` format references an uploaded file by platform message ID.

**Determinism guarantee**: Transport mode does NOT affect envelope identity. `payload_hash` verification ensures reassembled bytes are identical regardless of transport mode.

**Fallback**: If native upload fails, adapters MUST fall back to base64 text mode.

### 5. Line 202: Platform Table

**Current**: `WhatsApp Business API messages`

**Update**: `WhatsApp Web protocol (whatsapp-rust)`

## Acceptance Criteria

- [ ] Section 8.2: `self_handle()` added to PlatformAdapter trait
- [ ] Section 8.5: All 12 platforms listed with max payload and fragment strategy
- [ ] Section 8.6: `DOT/1/`, `DOT/2/`, `DOT/F/` wire formats documented
- [ ] Section 9.4: Dual-mode transport section added
- [ ] Line 202: WhatsApp description updated
- [ ] All changes are backward-compatible (no breaking changes)

## Location

`rfcs/draft/networking/0850-deterministic-overlay-transport.md`

## Complexity

Low (documentation update)

## Prerequisites

None
