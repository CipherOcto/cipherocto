# Mission: DOT Dual Binary Transport (Native Upload + Base64 Fallback)

## Status

Open

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §8.6, §9

## Summary

Implement a dual-mode binary transport system where DOT envelopes can be sent either as base64-encoded text (current, always works) or via native platform media upload (more efficient). The transport mode is an adapter detail that does not affect determinism — `payload_hash` verification ensures reassembled bytes are identical regardless of how they were transported.

## Why

Current approach encodes all envelope bytes as base64 text, adding 33% overhead. Platforms like Telegram, Discord, and Matrix support native file/media upload which is more efficient for large payloads. A dual system uses native upload when available, falls back to base64 when not.

## Determinism Guarantee

```
┌─────────────────────────────────────────────────────────────┐
│                    DETERMINISTIC LAYER                       │
│  envelope_id = BLAKE3(content hash)                         │
│  payload_hash = BLAKE3(payload bytes)                       │
│  IDENTICAL on all nodes regardless of transport mode        │
├─────────────────────────────────────────────────────────────┤
│                    TRANSPORT LAYER                           │
│  Mode A: DOT/1/{base64}     → Text (always works)          │
│  Mode B: DOT/2/{msg_id}     → Native upload (efficient)    │
│  Mode C: DOT/F/{fragment}   → Fragmented (large payloads)  │
└─────────────────────────────────────────────────────────────┘
```

## Wire Formats

| Format | Mode | Description |
|--------|------|-------------|
| `DOT/1/{base64}` | Text | Base64url-encoded envelope bytes (current) |
| `DOT/2/{msg_id}` | Native | Platform message ID referencing uploaded file |
| `DOT/F/{base64_fragment}` | Fragment | Base64-encoded fragment with header |

## Acceptance Criteria

### PlatformAdapter Trait Extensions

- [ ] Add `MediaCapabilities` to `CapabilityReport`:
  ```rust
  pub struct MediaCapabilities {
      pub supports_upload: bool,
      pub max_upload_bytes: usize,
      pub supported_mime_types: Vec<String>,
  }
  ```
- [ ] Add `upload_media()` method to `PlatformAdapter`:
  ```rust
  async fn upload_media(
      &self,
      filename: &str,
      data: &[u8],
      mime_type: &str,
  ) -> Result<String, PlatformAdapterError>;  // Returns platform message_id
  ```
- [ ] Add `download_media()` method to `PlatformAdapter`:
  ```rust
  async fn download_media(
      &self,
      message_id: &str,
  ) -> Result<Vec<u8>, PlatformAdapterError>;  // Returns raw bytes
  ```

### Transport Mode Selection

- [ ] Gateway selects transport mode based on payload size and capabilities:
  - If `payload.len() <= max_text_bytes` → Use `DOT/1/{base64}` (text mode)
  - If `payload.len() > max_text_bytes && capabilities.supports_upload` → Use `DOT/2/{msg_id}` (native mode)
  - If `payload.len() > max_text_bytes && !capabilities.supports_upload` → Use `DOT/F/{fragment}` (fragment mode)
- [ ] Mode selection is deterministic: same payload + same capabilities → same mode

### Receiver Implementation

- [ ] Receiver auto-detects mode from `DOT/` prefix:
  - `DOT/1/` → Decode base64
  - `DOT/2/` → Download from platform using message_id
  - `DOT/F/` → Collect fragments and reassemble
- [ ] All modes verify `payload_hash` after obtaining bytes
- [ ] Mode is NOT part of envelope identity — same envelope can be received via different modes

### Platform Adapters

| Adapter | Text | Native Upload | Fragment | Default Mode |
|---------|------|---------------|----------|--------------|
| Telegram | 4KB | 50MB (sendDocument) | Yes | Native for >4KB |
| Discord | 2KB | 25MB (attachments) | Yes | Native for >2KB |
| Matrix | 65KB | 50MB (media upload) | Yes | Native for >65KB |
| Slack | 40KB | No | Yes | Fragment for >40KB |
| Signal | 65KB | No | No | Text only |
| IRC | 512B | No | Yes | Fragment for >512B |
| Nostr | 65KB | No | No | Text only |
| WhatsApp | 65KB | No | No | Text only |
| LoRa | 256B | No | Yes | Fragment for >256B |

### Tests

- [ ] Test mode selection for each platform
- [ ] Test `DOT/2/{msg_id}` encode/decode
- [ ] Test native upload + download roundtrip
- [ ] Test `payload_hash` verification after native download
- [ ] Test fallback: native upload fails → falls back to base64
- [ ] Test deterministic mode selection (same input → same mode)

## Design Reference

- **RFC-0850 §8.6**: Payload encoding (currently base64 only)
- **RFC-0850 §9**: Envelope fragmentation
- **Current fragmentation**: `crates/octo-network/src/dot/fragment.rs`
- **PlatformAdapter trait**: `crates/octo-network/src/dot/adapters/mod.rs`

## Implementation Notes

### Upload URL Stability

For determinism, the upload reference must be stable. Options:
1. **Platform message_id** (recommended) — platform returns a stable ID after upload
2. **Content-hash URL** — `https://gateway.example.com/envelope/{blake3_hash}`
3. **Inline reference** — embed upload reference in the DOT/2/ message

### Error Handling

- If native upload fails, fall back to base64 text mode
- If download fails, request re-send via base64 mode
- Log mode used for debugging

### Backward Compatibility

- `DOT/1/` prefix remains unchanged
- `DOT/2/` and `DOT/F/` are new prefixes
- Old receivers ignore unknown `DOT/` prefixes (graceful degradation)

## Location

- `crates/octo-network/src/dot/adapters/mod.rs` (trait extensions)
- `crates/octo-network/src/dot/transport.rs` (new: mode selection logic)
- `crates/octo-adapter-*/src/lib.rs` (adapter implementations)

## Complexity

Medium-High

## Prerequisites

- Mission 0850e: DOT Adapter Registry & Plugin ABI
