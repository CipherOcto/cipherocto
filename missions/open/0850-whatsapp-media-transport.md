# Mission: 0850 WhatsApp Native Media Transport

> Implements RFC-0850's `DOT/2/{msg_id}` mode for WhatsApp — sender + receiver + mode-selection dispatch + fallback to `DOT/1/`.

## Status

Open

## RFC

RFC-0850 (Networking): Deterministic Overlay Transport — **the Platform Translation Layer payload-encoding rules (`DOT/2/{msg_id}` native upload mode + the mode-selection algorithm)** and **the dual-mode transport MUST-fallback when native upload fails**

**Note on file naming:** this mission is filed under `0850-` (not `0850p-`) because it implements the RFC-0850 native-upload mode + MUST-fallback rule directly. No companion RFC-0850p-b exists in `rfcs/`. See the "RFC compliance traceability" section below for the RFC amendment notes (capability table, native-upload enumeration, mode-selection coverage).

## Dependencies

- **Mission 0850p:** DOT WhatsApp Adapter (Implemented) — the `WhatsAppWebAdapter` struct, `start_bot` lifecycle, `Event::Connected` handler, `self_handle` resolver, and stoolap session store this mission extends
- **Mission 0850v:** DOT Dual Binary Transport (Implemented) — the trait surface (`PlatformAdapter::upload_media`, `PlatformAdapter::download_media`, `CapabilityReport::media_capabilities`) and the `select_mode` logic in `crates/octo-network/src/dot/transport.rs` that already routes `media_capabilities.is_some()` to `TransportMode::Native`
- **Mission 0850e:** DOT Adapter Registry & Plugin ABI (Implemented) — the registry that loads this adapter as a `cdylib` and dispatches the override

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none)

## Summary

Wire the WhatsApp Web adapter to the native media transport mode (`DOT/2/{msg_id}`) defined in RFC-0850 (Platform Translation Layer payload encoding), replacing the text-only fallback with a dual-mode pipeline that uses WhatsApp's CDN-backed media upload for envelopes that exceed the text-mode threshold. The current `WhatsAppWebAdapter` declares `media_capabilities: None` and does not override `upload_media` / `download_media`, so every DOT envelope is forced through the 33%-overhead `DOT/1/{base64}` text path even when both endpoints could carry a 100 MB encrypted attachment for the same wire bytes. This mission closes that gap by:

1. **Sender-side:** Extending `send_message` to dispatch on payload size via `octo_network::dot::transport::select_mode_with_max_text(payload_len, caps, 65_536)` — small envelopes use the existing `DOT/1/{base64}` text path; large envelopes call `upload_media` to push via WhatsApp's CDN and send a `DOT/2/{media_ref}` text reference. RFC-0850's MUST-fallback (Envelope Fragmentation / dual-mode transport) is implemented: if the native upload fails AND the envelope fits in text mode, fall back to `DOT/1/`.
2. **Sender-side override:** Adding `upload_media` that calls `wacore::upload` (via `Client::upload`) with `MediaType::Document` and returns an opaque `MediaRef` blob (base64url-encoded JSON of `UploadResponse`-shaped fields) that round-trips every CDN field needed to redeliver the bytes
3. **Receiver-side:** Extending `accept_message` to accept `DOT/2/` prefix (was `DOT/1/` only); extending the `on_event` handler to pre-download `DOT/2/{msg_id}` messages by calling `download_media` within the async context, then pushing the raw envelope wire bytes (not base64) to `inbound_tx`; extending `canonicalize` to handle two payload shapes (text base64 vs raw wire bytes)
4. **Receiver-side override:** Adding `download_media` that decodes the `MediaRef`, reconstructs a `waproto::message::DocumentMessage`, and calls `Client::download` (which decrypts and `payload_hash`-verifies end-to-end via the Signal Protocol envelope)
5. Populating `media_capabilities` in `capabilities()` with `max_upload_bytes = 100 MiB` (the WhatsApp server-side Document ceiling per public WhatsApp documentation) and a single MIME `application/octet-stream` (the only type the Document channel accepts)
6. Adding unit tests for the `MediaRef` encode/decode round-trip + field-count drift guard + no-panic contract + no-leak contract, the receive-path `accept_message` extension, the native→text fallback, and the capability contract. Plus a feature-gated `live-whatsapp` integration test that uploads a real envelope and downloads it back through the same adapter to assert RFC-0850 determinism guarantees (mode-independent `payload_hash`)

Implements RFC-0850's `DOT/2/{msg_id}` native upload mode and the MUST-fallback to `DOT/1/` for the WhatsApp adapter. The mode-selection algorithm is implemented inside `WhatsAppWebAdapter::send_message` per the capability-driven rule. Closes the gap where 0850v (archived mission `0850v-dot-dual-binary-transport.md` "receiver auto-detect" claim) said "Receiver auto-detects mode from `DOT/` prefix" but the WhatsApp adapter never implemented the `DOT/2/` receive path.

## RFC compliance traceability

This mission is justified **purely by RFC-0850 (Platform Translation Layer payload encoding + Envelope Fragmentation dual-mode transport)** and the WhatsApp adapter's existing code. The following RFC traceability notes document what this mission changes about the RFC's characterization of WhatsApp:

- **RFC-0850 capability table amendment:** RFC-0850's capability table currently characterizes WhatsApp as `"Text only, no fragmentation"`. This mission amends that characterization to: `"Text (up to 65 KB) and native upload (up to 100 MiB via WhatsApp CDN); no fragmentation needed in either mode"`. The amendment is implementation-driven — by declaring `media_capabilities` with `supports_upload: true` (per RFC-0850 mode-selection rule), the adapter satisfies the capability check that gates `DOT/2/{msg_id}` dispatch. A future RFC-0850 revision should update the capability table accordingly.

- **RFC-0850 native-upload enumeration:** RFC-0850 enumerates "Telegram, Discord, Matrix" as **example** platforms supporting native file upload, but the rule is generic ("For platforms supporting native file upload"). The actual gating criterion is RFC-0850 mode-selection rule's `capabilities.supports_upload` flag, which the mission enables via `media_capabilities`. No RFC amendment needed — the generic rule already covers WhatsApp.

- **RFC-0850 mode-selection coverage:** RFC-0850 enumerates 4 modes (`DOT/1/`, `DOT/2/`, `DOT/F/`, `RAW/`). This mission implements `DOT/1/` (existing behavior) + `DOT/2/` (new). The `DOT/F/{base64_frag}` fragment mode is intentionally not implemented because declaring `media_capabilities.supports_upload = true` routes all payloads > 65 KB to `DOT/2/` instead — no fragmentation is needed. The `RAW/{binary}` raw binary mode is also not implemented because WhatsApp Web is a text/media platform, not a raw-byte transport (RFC-0850 restricts `RAW/` to QUIC, WebRTC, NativeP2P).

- **RFC-0850 `max_text_bytes`:** RFC-0850's per-platform capability tables both confirm WhatsApp's `max_text_bytes = 65536`. The mission's `select_mode_with_max_text(payload_len, caps, 65_536)` call correctly uses the RFC-specified WhatsApp threshold. The RFC's default of 4096 is overridden per-platform.

## Design

See RFC-0850 Platform Translation Layer for the transport-mode selection algorithm. Companion doc with code-level patterns: this mission's "Implementation Guide" section.

## Acceptance Criteria

### Phase 1: Adapter change (additive, non-breaking)

#### `MediaCapabilities` declaration

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs:1368-1377`: replace `media_capabilities: None` with:
  ```rust
  media_capabilities: Some(MediaCapabilities {
      max_upload_bytes: 100 * 1024 * 1024, // 100 MiB, WhatsApp server-side Document ceiling
      supported_mime_types: vec!["application/octet-stream".to_string()],
  }),
  ```
  - R1: `MediaType::Document` is the only media type that accepts arbitrary opaque blobs per `wacore/src/download.rs:33-47` (Image/Video/Audio re-encode; AppState/History/StickerPack/StickerPackThumbnail/LinkThumbnail/ProductCatalogImage have app-specific shapes that reject arbitrary payloads). R1-M3 (round 1): the 100 MiB / 16 MiB size figures are WhatsApp server-side limits per public WhatsApp documentation as of 2026-06; they are NOT compile-time constants in wacore and the adapter's pre-flight check is the only local enforcement point. The mission AC does not pin specific size limits for non-Document media types because those limits change without wacore releases.
  - R2: `application/octet-stream` is the only MIME the `Document` channel accepts without re-encoding (WhatsApp's CDN rejects `text/*` for the Document endpoint; Image/Video/Audio MIME matching is enforced by WhatsApp-side validators)
  - R3: leaving the capabilities struct populated but `upload_media`/`download_media` returning the default trait error would be a silent failure (the transport router would pick `Native` mode and then crash on download). The override methods below MUST ship in the same PR — split it across commits only if the second commit lands before the first is merged

#### `send_message` mode-dispatch (R1-C2 + R1-H1 + R1-H2 fixes)

> **Architecture decision (R1-C2):** RFC-0850's `select_mode` function (defined in `crates/octo-network/src/dot/transport.rs::select_mode_with_max_text`) has zero production callers as of `next`. The mission therefore makes the **adapter own mode selection** — `WhatsAppWebAdapter::send_message` internally dispatches between text and native mode based on payload size. RFC-0850's mode-selection text-mode branch specifies `If payload.len() <= max_text_bytes → DOT/1/{base64} (text mode)` and the native-mode branch specifies `If payload.len() > max_text_bytes && capabilities.supports_upload → DOT/2/{msg_id} (native mode)` — the adapter-local dispatch implements both branches of this decision tree. The gateway is unaware of per-adapter quirks; the dispatch is fully encapsulated per RFC-0850's capability-driven rule.

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs:1266-1325` (`send_message`): refactor to dispatch on payload size:
  - Compute `let caps = self.capabilities();` and `let mode = octo_network::dot::transport::select_mode_with_max_text(encoded.len(), &caps, 65_536)?;` (R1-H2 fix: pass `65_536` as `max_text_bytes`, NOT the RFC default `4096`. WhatsApp's text message limit is ~65 KB; using the RFC default would route envelopes >4 KB to native mode unnecessarily, adding CDN round-trip latency and bandwidth waste for envelopes that fit in a single text message.) **R8-H1 fix:** the threshold argument is `encoded.len()` (the on-wire text-message body — base64-enveloped form, ~33% larger than the wire bytes), NOT `wire_bytes.len()`. This is the actual constraint on the text-mode send: the base64-expanded body must fit in a single 65 KB WhatsApp text message. The RFC's `payload.len()` wording in the mode-selection text-mode branch is read by the adapter as "the bytes that would actually be transmitted on the wire in text mode", not the pre-encoding envelope size. The earlier spec text (`wire_bytes.len()`) was a simplification that would have routed envelopes between 49 KB and 65 KB wire-bytes into text mode where they'd fail to fit after base64 expansion — see R8-H1 in `docs/reviews/2026-06-20-r8-mission-0850-review.md` for the original finding.
  - On `TransportMode::Text`: existing path — `Self::encode_envelope(&wire_bytes)` then `client.send_message(to, Message { conversation: Some(encoded), .. })`
  - On `TransportMode::Native`:
    1. Call `self.upload_media("envelope.bin", &wire_bytes, "application/octet-stream").await` to obtain the `DOT/2/{media_ref}` token
    2. Build `Message { conversation: Some(octo_network::dot::transport::encode_native_ref(&media_ref)), .. }` and send via `client.send_message`
    3. R1-M4: `upload_media` internally allocates `data.to_vec()` because `Client::upload` takes `Vec<u8>` by value (`whatsapp-rust/src/upload.rs:316-321`); this is the current wacore API contract and cannot be avoided without an SDK bump
  - On `TransportMode::PayloadTooLarge` error: return `PlatformAdapterError::PayloadTooLarge { size: encoded.len(), max: caps.max_payload_bytes, platform: "whatsapp" }` (R8-H1: consistent with the threshold — the size reported is the on-wire body, which is what actually exceeded the platform's payload limit per RFC-0850 Platform Translation Layer).
  - **R1-H1 (RFC-0850 MUST fallback):** if step 1 returns `PlatformAdapterError::Unreachable` AND `encoded.len() <= 65_536` (the on-wire text-message body fits in a single WhatsApp text message — see R8-H1 clarification above for why this is `encoded.len()` rather than `wire_bytes.len()`), fall back to `TransportMode::Text` and retry the send. Log the fallback at `tracing::warn!` level with the redacted error message (no `media_key` in the log — see R1-H4 below). If `encoded.len() > 65_536`, propagate the `Unreachable` error (no fallback possible; envelope is too large for text mode). The fallback is a single retry attempt — exponential backoff is the gateway's responsibility, not the adapter's.

#### `upload_media` override

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs`: add `async fn upload_media(&self, filename: &str, data: &[u8], mime_type: &str) -> Result<String, PlatformAdapterError>` to `impl PlatformAdapter for WhatsAppWebAdapter`
  - Validates `data.len() <= 100 MiB` against `media_capabilities.max_upload_bytes`; return `PlatformAdapterError::PayloadTooLarge { size: data.len(), max: 100 * 1024 * 1024, platform: "whatsapp" }` if exceeded (R4: this is a pre-flight check; `Client::upload` would also reject the upload at the WhatsApp CDN, but the error path on the wire layer is harder to recover from — fail fast at the adapter boundary instead. The `PayloadTooLarge` variant already exists at `crates/octo-network/src/dot/error.rs:61-66` and is handled at the `select_mode` layer)
  - Acquires the `client: Arc<Mutex<Option<Arc<Client>>>>` read-locked, errors `PlatformAdapterError::Unreachable { platform: "whatsapp", reason: "client not connected" }` if `None` (matches existing `send_message` precondition at `crates/octo-adapter-whatsapp/src/adapter.rs:432-438`)
  - Calls `client.upload(data.to_vec(), MediaType::Document, UploadOptions::new()).await`. The `to_vec()` clone is required because `Client::upload` takes `Vec<u8>` (see `whatsapp-rust/src/upload.rs:316-321`); for a 100 MiB worst-case upload this allocates 100 MiB on the heap. R1-M4: the cost is unavoidable under the current wacore API; if a future wacore release adds a `&[u8]` overload, this clone can be removed.
  - Maps `wacore::Result<UploadResponse>` errors via `PlatformAdapterError::Unreachable { platform: "whatsapp", reason: format!("upload failed: {e}") }` (matches `send_message`'s error-mapping convention at `crates/octo-adapter-whatsapp/src/adapter.rs:454-460`)
  - Calls `MediaRef::from_upload_response(&response, filename)` (helper below) to produce the opaque wire reference
  - Returns `MediaRef::encode_base64url()` as the `String` message_id
  - R5: `mime_type` argument is intentionally ignored — WhatsApp's `Document` channel hardcodes `application/octet-stream` regardless of the upload MIME. Logging the requested MIME at `tracing::debug!` level is acceptable for operator visibility but the wire format MUST NOT vary by MIME

#### `download_media` override

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs`: add `async fn download_media(&self, message_id: &str) -> Result<Vec<u8>, PlatformAdapterError>` to `impl PlatformAdapter for WhatsAppWebAdapter`
  - Calls `MediaRef::decode_base64url(message_id)` to recover the `MediaRef`. Errors `PlatformAdapterError::ApiError { code: 400, message: format!("invalid media ref: {e}") }` on base64 or JSON parse failure (R6: a malformed `DOT/2/{msg_id}` is a wire-protocol violation, not a transient transport error — use the 4xx-shaped variant so the gateway can refuse the envelope rather than retry indefinitely. R1-H4 fix: the error message MUST NOT include the input `message_id` or the decoded `MediaRef` because they contain `media_key`; use a generic `'invalid media ref format'` string. Sanitize ALL error paths in `download_media` accordingly.)
  - Reconstructs `waproto::whatsapp::DocumentMessage { media_key: Some(media_ref.media_key.to_vec()), direct_path: Some(media_ref.direct_path.clone()), file_enc_sha256: Some(media_ref.file_enc_sha256.to_vec()), file_sha256: Some(media_ref.file_sha256.to_vec()), file_length: Some(media_ref.file_length), ..Default::default() }`. The `..Default::default()` covers fields WhatsApp's CDN ignores on re-download (`mimetype`, `file_name`, `title`, `page_count`, etc.). R1-L2 fix: variable named `media_ref` (consistent with the `encode_base64url(media_ref: &MediaRef, ...)` parameter rename elsewhere in the mission).
  - Acquires the `client` (same precondition as upload), calls `client.download(&document_message).await`. The `&DocumentMessage` coercion to `&dyn Downloadable` is provided by the blanket `impl_downloadable!` at `wacore/src/download.rs:202-206` (`MediaType::Document`)
  - Maps `wacore::Result<Vec<u8>>` errors via `PlatformAdapterError::Unreachable { platform: "whatsapp", reason: format!("download failed: {e}") }`
  - Returns the decrypted plaintext bytes
  - R7: `Client::download` calls `payload_hash` verification internally via `wacore::upload::decrypt_media_with_key` (the inverse of the encrypt step used at upload). A `file_enc_sha256` mismatch surfaces as `wacore::Error::HashMismatch` which the error mapping preserves. **R2-H1 fix:** the gateway's outer `payload_hash` check is at `crates/octo-network/src/dot/envelope.rs::verify_payload_hash` (the `verify_payload_hash` method on `DeterministicEnvelope`), NOT in the `transport.rs` `decode_fragment_ref` + `detect_mode` functions (which have nothing to do with payload integrity per the R1 review). The defense-in-depth check MUST NOT be removed — it's the method `verify_payload_hash` on `DeterministicEnvelope` (already covered by the existing test `test_sealed_envelope_payload_hash`).

#### Receive-path extension (R1-C1 fix)

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs` (`accept_message` prefix check): **R2-M1 fix:** the actual prefix check `if !text_trimmed.starts_with("DOT/1/")` is in `accept_message` (function body). Extend the check from `text_trimmed.starts_with("DOT/1/")` to `if !text_trimmed.starts_with("DOT/1/") && !text_trimmed.starts_with("DOT/2/")`. R1-M2 fix: the new test cases `accept_message_accepts_dot1` (existing behavior preserved), `accept_message_accepts_dot2` (new), and `accept_message_rejects_other_prefix` (e.g., `DOT/F/` rejected; the `DOT/F/` receive path is out of scope for this mission) pin the dispatch.
- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs` (`on_event` closure span — full closure, not just the `Event::Message` arm): when `accept_message` returns `Accept` AND the text starts with `DOT/2/`, dispatch to a new pre-download branch. **R2-C1 fix (CRITICAL):** the `on_event` closure does NOT capture `self` — only `inbound_tx`, `self_phone`, `groups`, `runtime_groups`, `sender_allowlist`, and `connected_notify` are captured (see the captured-fields comment block in the closure definition). The R1-C1 AC's `self.download_media(media_ref).await` will NOT compile because `self` is not in scope. Use the **download-request channel pattern** (option 2 in the R2 review):
  1. **Field declaration on `WhatsAppWebAdapter`** (near the `inbound_tx` field declaration — **R4-C1 fix:** the R3-M1 attempt to init the channel in `new` was broken because `download_rx` (the receiver) had no owner and would drop immediately when `new` returned, closing the channel before any consumer task could be spawned). New field:
     ```rust
     /// Channel for routing DOT/2/ download requests from the sync on_event
     /// closure to the async download_rx consumer task. The `on_event`
     /// closure (which does not capture `self`) pushes a `DownloadRequest`;
     /// the consumer task does the actual wacore `Client::download` call
     /// and pushes the resulting wire bytes to `inbound_tx`.
     /// Wrapped in `Arc<tokio::sync::Mutex<Option<...>>>` (mirrors the
     /// existing `client` field declaration on `WhatsAppWebAdapter`) so `start_bot(&self)`
     /// can populate the `Some(_)` variant without `&mut self` and the
     /// `on_event` closure can hold an `Arc` clone without owning `self`.
     /// Initialized to `None` in `new`; populated in `start_bot` (R4-C1).
     download_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<DownloadRequest>>>>,
     ```
     And in `new` (in the `WhatsAppWebAdapter::new` body), add `download_tx: Arc::new(tokio::sync::Mutex::new(None)),` to the struct literal (mirrors the `client` field init).
  2. **Define `pub(crate) struct DownloadRequest { pub(crate) msg_id: String, pub(crate) chat: String, pub(crate) sender: String }`** near `RawPlatformMessage` definitions in the adapter module.
  3. **R3-C1 fix (CRITICAL):** introduce a `WhatsAppHandlerHandle` struct + `clone_for_handler` method on `WhatsAppWebAdapter`. The R2-C1 design assumed `WhatsAppWebAdapter: Clone` (NOT implemented) OR `Arc<WhatsAppWebAdapter>` constructor (currently returns bare `Self` per `pub fn new(config) -> Self` at adapter.rs:263). NEITHER is available. Use **Option A: `clone_for_handler` helper** (preferred — no public API change, no Clone refactor, type-level least-privilege — the handle only exposes `client` and `inbound_tx`, NOT `config`/`bot_handle`/`inbound_rx`/`self_phone`/`runtime_groups`):
     ```rust
     // New struct near the bottom of adapter.rs (after the impl block):
     #[derive(Clone)]
     pub(crate) struct WhatsAppHandlerHandle {
         pub(crate) client: Arc<Mutex<Option<Arc<whatsapp_rust::Client>>>>,
         pub(crate) inbound_tx: tokio::sync::mpsc::Sender<RawPlatformMessage>,
     }

     impl WhatsAppWebAdapter {
         /// Clone the fields needed by background tasks (the download_rx consumer task).
         /// Does NOT clone `inbound_rx` because the consumer pushes via `inbound_tx`,
         /// not drains `inbound_rx`. `receive_messages()` still holds the original `inbound_rx`.
         pub(crate) fn clone_for_handler(&self) -> WhatsAppHandlerHandle {
             WhatsAppHandlerHandle {
                 client: self.client.clone(),
                 inbound_tx: self.inbound_tx.clone(),
             }
         }
     }
     ```
  4. **Spawn a tokio task in `start_bot` (after `start_bot`'s initialization phase) — R4-C1 fix:** create the channel INSIDE `start_bot` (NOT in `new`) so the receiver has an immediate owner (the spawned task). After the existing `let inbound_tx = self.inbound_tx.clone();` initialization, add:
     ```rust
     // R4-C1 fix: create the download channel here, not in `new`.
     let (download_tx, mut download_rx) = tokio::sync::mpsc::channel::<DownloadRequest>(64);
     *self.download_tx.lock().await = Some(download_tx);
     let download_handle = self.clone_for_handler();
     let inbound_tx_for_consumer = self.inbound_tx.clone();
     tokio::spawn(async move {
         while let Some(req) = download_rx.recv().await {
             let wire_bytes = match download_handle.client.lock().await.as_ref() {
                 Some(client) => match client.download(&req.msg_id).await {
                     Ok(bytes) => bytes,
                     Err(e) => {
                         // R1-H4: redacted reason; msg_id is not logged.
                         tracing::warn!("download failed: {}", e);
                         continue;
                     }
                 },
                 None => {
                     tracing::warn!("download failed: client not connected");
                     continue;
                 }
             };
             let raw = RawPlatformMessage {
                 platform_id: format!("{}:{}", req.chat, uuid::Uuid::new_v4()),
                 payload: wire_bytes,
                 metadata: [
                     ("chat".to_string(), req.chat),
                     ("sender".to_string(), req.sender),
                     ("dot_mode".to_string(), "native".to_string()),  // R2-M5
                 ]
                 .into_iter()
                 .collect(),
             };
             if let Err(e) = inbound_tx_for_consumer.try_send(raw) {
                 tracing::warn!("inbound channel full or closed: {e}");
             }
         }
         tracing::debug!("download_rx consumer task exiting (channel closed)");
     });
     ```
     The task exits cleanly when the `Sender` stored in `self.download_tx` is dropped — which happens when the user drops the adapter (`Arc<Mutex<Option<Sender>>>` is dropped, the inner `Sender` is dropped, `download_rx.recv()` returns `None`, the `while let` loop ends).
     **R4-L1 fix:** the consumer task does NOT call `download_handle.download_media(...)` (because `WhatsAppHandlerHandle` doesn't have that method — by design, least-privilege). Instead it calls the wacore `Client::download` API directly via `download_handle.client.lock().await.as_ref().unwrap().download(&req.msg_id).await`. This duplicates ~10 lines of error-handling vs. `WhatsAppWebAdapter::download_media` but avoids the indirection. R4-L1 alternative (extract a free function `pub(crate) async fn download_via_client(client: &Client, msg_id: &str) -> Result<Vec<u8>, PlatformAdapterError>` that both `WhatsAppWebAdapter::download_media` and this consumer task call) is recommended but not required.
  5. **In the `on_event` closure span:** when text starts with `DOT/2/`, do NOT call `self.download_media` — instead call `self.download_tx.lock().await.as_ref().and_then(|tx| tx.try_send(DownloadRequest { msg_id, chat: chat.clone(), sender: sender.clone() }).ok())` (capturing `download_tx` in the closure like `inbound_tx` is captured). The download task does the actual `Client::download` call and pushes the wire bytes to `inbound_tx`. **R4-C1 fix:** the closure captures `download_tx` by cloning the `Arc<Mutex<Option<Sender>>>` (NOT the inner `Sender` — the inner `Sender` is only set by `start_bot`, which is called AFTER the closure is registered).
  6. The receive-path pipeline becomes: `wacore Event::Message` → `on_event` closure (sync, pushes to `download_tx`) → `download_rx` consumer task (async, calls `client.download` + pushes wire bytes to `inbound_tx`) → `receive_messages()` drains `inbound_rx` → `canonicalize` (dispatches on `metadata["dot_mode"] == "native"` → wire-bytes path; else → text-decode path).
  7. **Why Option A over B/C:** Option A adds ONE new private struct (`WhatsAppHandlerHandle`) + ONE new method (`clone_for_handler`) — no public API change, no Clone refactor. Option B (implement Clone on `WhatsAppWebAdapter`) requires changing `inbound_rx` to `Arc<Mutex<Receiver<...>>>` (already done!) and refactoring every test that uses `inbound_rx`. Option C (`Arc<Self>` constructor) is BREAKING — affects 9 call sites in 4 files (`pair_link.rs:45`, `qr_link.rs:43`, 6 adapter.rs tests, 1 live_e2e test). Option A is the locked design. **R4 refutation note:** all fields of `WhatsAppWebAdapter` are Arc-wrapped or inherently Clone (verified at adapter.rs:217-244), so `#[derive(Clone)]` would technically compile — but it would give the consumer task access to `config` (session path, groups, sender allowlist), `bot_handle` (shutdown control), `inbound_rx` (could drain messages), `self_phone`/`runtime_groups` (state that should not be touched by download tasks). The handle is a type-level firewall.
- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs:1342-1360` (`canonicalize`): since the `download_rx` consumer task now pre-downloads `DOT/2/` and pushes raw wire bytes with a `dot_mode: "native"` metadata tag (per R2-C1 fix), `canonicalize` needs to handle TWO payload shapes. **R2-M5 fix:** the discriminator is the `dot_mode` metadata field, NOT a sniff of the payload bytes (which is fragile to future wire-format changes — see R2-M5 refutation note):
  - For `dot_mode == "native"` (wire bytes from `DOT/2/` pre-download): pass `raw.payload` directly to `DeterministicEnvelope::from_wire_bytes` (no decode)
  - For `dot_mode == "text"` OR missing `dot_mode` (legacy `DOT/1/` text): existing path — `decode_envelope(text)` → base64-decode → wire bytes
  - The discriminator: `if raw.metadata.get("dot_mode").map(String::as_str) == Some("native") { /* raw path */ } else { /* text decode path */ }`. The `metadata["dot_mode"]` tag is set by the `download_rx` consumer task (R4-C1 fix step 4) with value `"native"`. **R4-L2 fix:** for `DOT/1/` messages, the existing on_event closure in `WhatsAppWebAdapter::on_event` must add `metadata.insert("dot_mode".to_string(), "text".to_string());` immediately before pushing the `RawPlatformMessage` into `inbound_tx`. The discriminator treats missing keys as `text`, so this insertion is for clarity/explicitness rather than correctness — but it pins the contract so future readers don't have to deduce the implicit fallback. The exact insertion point is after the existing `RawPlatformMessage { ... }` struct construction and before `.into_iter().collect()` — the field order in the Acceptance Criteria section is `(chat, sender, dot_mode)` and should match for `DOT/1/` messages to keep the metadata `HashMap` deterministic.
  - R7-reinforced: `DeterministicEnvelope::from_wire_bytes` performs the `payload_hash` verification that RFC-0850 Platform Translation Layer mandates for mode-independent identity. A hash mismatch is `PlatformAdapterError::ApiError { code: 400 }`, NOT `Unreachable`. The `verify_payload_hash` method itself is at `crates/octo-network/src/dot/envelope.rs:449-452` (see R2-H1).

#### `MediaRef` helper module

- [ ] `crates/octo-adapter-whatsapp/src/media_ref.rs` (new file): private module exposing `pub(crate) struct MediaRef` with `pub(crate) fn from_upload_response(response: &UploadResponse, filename: &str) -> Self` and `pub(crate) fn to_document_message(&self) -> wa::message::DocumentMessage`
  - **R1-C3 fix:** `MediaRef` is a STANDALONE `#[derive(Serialize, Deserialize)]` struct that mirrors the `UploadResponse` field shape (NOT a newtype around `UploadResponse`). The reason: `UploadResponse` at `whatsapp-rust/src/upload.rs:242-251` does NOT derive `Serialize` (only `Deserialize` is derived for `RawUploadResponse` and `UploadProgressResponse`). A newtype wrapping `UploadResponse` would not compile when `serde_json::to_string` is called.
  - Field set: `url: String`, `direct_path: String`, `media_key: [u8; 32]`, `file_enc_sha256: [u8; 32]`, `file_sha256: [u8; 32]`, `file_length: u64`, `media_key_timestamp: i64`, `filename: String` (operator metadata; not used by `to_document_message`)
  - **R1-C3 drift guard:** add a `#[allow(dead_code)] const _: () = assert!(std::mem::size_of::<MediaRef>() == ...)` or a unit test `media_ref_field_count_matches_upload_response` that asserts the MediaRef struct has exactly 8 fields (7 UploadResponse + filename). Drift catches at test time.
  - **R8: do NOT add new fields to MediaRef beyond the UploadResponse shape + filename metadata.** If a future wacore version adds fields to `UploadResponse`, extend MediaRef in a follow-up commit without changing the wire format. JSON's default behavior of ignoring unknown fields keeps backward compatibility.
  - `from_upload_response` copies field-by-field from `UploadResponse` to `MediaRef`, stores `filename` for operator-visible logging on download (the filename is not used by `to_document_message` — it's metadata only)
  - `to_document_message` returns the `DocumentMessage` shape described in the `download_media` AC above. Field-by-field assignment with explicit type coercion (the `[u8; 32]` fields stay as fixed-size arrays; `DocumentMessage` fields are `Vec<u8>`)
  - The module is `pub(crate)` only — `MediaRef` is an implementation detail of the adapter's wire format, not part of the public API. Tests live in a sibling `#[cfg(test)] mod tests` block in the same file

- [ ] `encode_base64url` / `decode_base64url` functions (in `media_ref.rs`):
  - **R1-M1 fix:** use `octo_network::dot::transport::b64url_encode` and `b64url_decode` instead of the `base64::engine::general_purpose::URL_SAFE_NO_PAD` engine. Reasons: (a) the octo-network helpers exist for exactly this purpose (`crates/octo-network/src/dot/transport.rs:171-200`); (b) avoids duplicate implementations of the same algorithm that could drift if the `base64` crate upgrades; (c) ensures the wire format matches whatever the gateway uses for `DOT/1/` decoding.
  - `pub(crate) fn encode_base64url(media_ref: &MediaRef) -> String` — R1-L2 fix: renamed `ref_` parameter to `media_ref` (idiomatic; no `ref_` underscore-suffix needed). JSON-serializes via `serde_json` (already a dep at `crates/octo-adapter-whatsapp/Cargo.toml:53`), then `b64url_encode`s.
  - `pub(crate) fn decode_base64url(s: &str) -> Result<MediaRef, MediaRefError>` — `b64url_decode`s, then JSON-deserializes. Errors are `MediaRefError::Base64(b64url_decode error)` and `MediaRefError::Json(serde_json::Error)`; the outer adapter mapping (in the `download_media` AC above) collapses both into `PlatformAdapterError::ApiError`.
  - R9: base64url (NOT standard base64) is required because the `DOT/2/{msg_id}` token sits inside a text message body and must not contain `+` or `/` (which would force the wire layer to escape them). URL-safe encoding is the same convention used for `DOT/1/{base64}` in the existing `decode_envelope` helper at `crates/octo-adapter-whatsapp/src/adapter.rs::decode_envelope`. **R2-M2 fix:** the R1 cite of `set_subject` function range was wrong; the actual `decode_envelope` (and its `base64::engine::general_purpose::URL_SAFE_NO_PAD` usage) is the R1-confirmed helper body (per the original R1 review).
  - R10: do NOT use `bincode` or `postcard` — JSON keeps the wire format human-debuggable from a `tracing::debug!` dump and matches the rest of the adapter's serialization convention. The size overhead (~2x) is acceptable because the `MediaRef` is ~120 bytes regardless of the underlying envelope size
  - **R1-H4 fix:** the `decode_base64url` function MUST NOT panic on any input. All error paths return `Err(MediaRefError::...)` with a redacted message that does not include the input bytes (which contain `media_key`). Unit test `decode_base64url_does_not_panic_on_arbitrary_input` passes a 1 MiB random byte string and asserts no panic.

#### Module wiring

- [ ] `crates/octo-adapter-whatsapp/src/lib.rs` (or wherever the module root lives — verify with `grep -n "mod adapter" crates/octo-adapter-whatsapp/src/lib.rs`): add `mod media_ref;` (private; not `pub`)
- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs`: `use crate::media_ref::MediaRef;` at the top of the file (next to the existing `use` block)
- [ ] Verify `cargo build -p octo-adapter-whatsapp` compiles without warnings
- [ ] Verify `cargo clippy --all-targets --all-features -- -D warnings` for the crate passes (R11: matching the project-wide clippy policy enforced in commit `ae5602c`)

### Phase 2: Unit tests

- [ ] `crates/octo-adapter-whatsapp/src/media_ref.rs` — `#[cfg(test)] mod tests` block with the following cases (each pinned to specific behavior so accidental changes fail loudly):
  - `media_ref_roundtrip` — build a `MediaRef` from a synthetic `UploadResponse`, encode_base64url, decode_base64url, assert every field matches the original (R12: regression guard for the wire format — if a future refactor drops a field, the round-trip will catch it)
  - `media_ref_to_document_message` — build a `MediaRef`, call `to_document_message`, assert `media_key`, `direct_path`, `file_enc_sha256`, `file_sha256`, `file_length` are correctly populated and that the other `DocumentMessage` fields are `None` or `Default::default()` (R13: guards against accidentally leaking operator-supplied metadata into the download request, which could confuse WhatsApp's CDN validators)
  - `encode_base64url_no_special_chars` — assert the encoded string contains only `[A-Za-z0-9_-]` and no `+`/`/` (R14: the `+` and `/` chars would break the `DOT/2/{msg_id}` parser in the canonicalize path)
  - `decode_base64url_invalid_base64` — pass `"!!!"`, assert `MediaRefError::Base64` returned
  - `decode_base64url_invalid_json` — pass a valid base64 string that decodes to `"not json"`, assert `MediaRefError::Json` returned
  - `decode_base64url_empty_string` — pass `""`, assert `MediaRefError::Base64` (not a panic — R15: panics in adapter code paths are DoS vectors; test the empty-string case explicitly)
  - **R1-C3 fix:** `media_ref_field_count_matches_upload_response` — assert `std::mem::size_of::<MediaRef>() == std::mem::size_of::<UploadResponse>() + std::mem::size_of::<String>()` (UploadResponse is ~120 bytes; String is 24 bytes on 64-bit; so MediaRef is ~144 bytes). Drift catches when a future wacore version adds a field to `UploadResponse` without updating `MediaRef`'s `from_upload_response`.
  - **R1-H4 fix:** `decode_base64url_does_not_panic_on_arbitrary_input` — generate a 1 MiB random byte string (not valid base64url), pass to `decode_base64url`, assert no panic and a redacted `MediaRefError::Base64` is returned. Companion: `decode_base64url_does_not_leak_input_in_error` — pass a known `MediaRef`-shaped input that fails JSON parse, assert the error message does NOT contain the input bytes (no `media_key` leak via `eprintln!` or panic message).

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs` — add a `#[cfg(test)] mod upload_download_tests` block at the bottom of the `mod tests` (the existing test module spans most of the file; **R2-M4 fix:** the R1 cite `2080-2200` was wrong by 64 lines). Mirror the test layout from `test_encode_decode_envelope` for the new round-trip tests, and from `test_health_check_not_running` for the new pre-condition tests.
  - `capabilities_includes_media` — call `adapter.capabilities()`, assert `media_capabilities.is_some()`, assert `max_upload_bytes == 100 MiB`, assert `supported_mime_types == vec!["application/octet-stream"]` (R16: pins the capability declaration to prevent accidental downgrade to text-only mode if a future refactor touches the `capabilities()` method)
  - **R1-H3 fix:** `upload_media_client_not_connected` — build a `WhatsAppWebAdapter` with `session_path: "/tmp/test_media_transport_upload.db".into()` (literal path; matches the existing `test_health_check_not_running` pattern; `WhatsAppWebAdapter::new` does not touch the file system). DO NOT call `start_bot()`. Call `adapter.upload_media("test.bin", b"hello", "application/octet-stream").await`, assert `Err(PlatformAdapterError::Unreachable { reason: "client not connected", .. })`. `tempfile::TempDir` is unnecessary for this test — it's only required for the live integration test which DOES call `start_bot`.
  - **R1-H3 fix:** `download_media_invalid_message_id` — same setup as above, call `adapter.download_media("not-base64!!!").await`, assert `Err(PlatformAdapterError::ApiError { code: 400, .. })` AND assert the error message is exactly `"invalid media ref format"` (the redacted string — no leak of the input `"not-base64!!!"` in the message). R18: pins the malformed-ref handling; a regression to `PlatformAdapterError::Unreachable` would cause the gateway to retry the download indefinitely, blocking the envelope.
  - **R1-C1 + R1-M2 fix:** `accept_message_accepts_dot1` (existing behavior pinned — `accept_message` accepts text starting with `DOT/1/`)
  - `accept_message_accepts_dot2` (new behavior pinned — `accept_message` accepts text starting with `DOT/2/`; `DOT/2/test_msg_id` returns `Accept`)
  - `accept_message_rejects_other_prefix` (e.g., `DOT/F/...` is rejected with reason `"not a DOT envelope"`; the `DOT/F/` receive path is out of scope for this mission)
  - **R1-H1 fix:** `send_message_falls_back_to_text_when_native_fails` — stubbed test: configure the adapter with a stubbed `Client` whose `upload` method always returns `Err(Unreachable)`. Call `adapter.send_message(domain, envelope)` with an envelope whose `wire_bytes.len() == 5000` (above the 4096 default but well below 65_536). Assert the result is `Ok(DeliveryReceipt)` and that `client.send_message` was called with `DOT/1/{base64}` content (NOT `DOT/2/{...}`). Verifies RFC-0850's MUST fallback (Platform Translation Layer + Envelope Fragmentation). The stubbed `Client` requires either a trait-object refactor (out of scope for this mission — flag in R2 if needed) or a test-only constructor that bypasses `start_bot`. Document the approach taken.
  - **R1-H1 fix:** `send_message_does_not_fall_back_when_payload_exceeds_text_threshold` — same setup as above, but `wire_bytes.len() == 70_000` (above the 65_536 text limit). Assert the result is `Err(PlatformAdapterError::Unreachable)` — no fallback attempt because the envelope wouldn't fit in text mode anyway.

- [ ] **R3-M2 + R4-M3 fix:** `crates/octo-adapter-whatsapp/src/adapter.rs` — add async lifecycle tests for the `download_rx` consumer task in a new `#[cfg(test)] mod download_rx_tests` block. The tests use a test-only constructor `spawn_download_consumer_for_test` (R4-M3 fix — `start_bot` requires authenticated wacore session, so the tests can't call it):
  ```rust
  #[cfg(test)]
  impl WhatsAppWebAdapter {
      /// Test-only: spawns the download_rx consumer task without
      /// requiring an authenticated wacore session. Mirrors the
      /// channel creation + spawn logic in `start_bot` but bypasses
      /// the wacore `Bot` setup. Returns the `Sender` so tests can
      /// push `DownloadRequest`s directly.
      pub(crate) fn spawn_download_consumer_for_test(&self) -> tokio::sync::mpsc::Sender<DownloadRequest> {
          let (tx, mut rx) = tokio::sync::mpsc::channel(64);
          // R4-C1: set the field so `on_event`-style code paths could also work.
          // We can't `.await` on the Mutex in a sync fn; use `try_lock` + spin
          // for tests (acceptable because the test is single-threaded).
          if let Ok(mut guard) = self.download_tx.try_lock() {
              *guard = Some(tx.clone());
          }
          let handle = self.clone_for_handler();
          let inbound_tx = self.inbound_tx.clone();
          tokio::spawn(async move {
              while let Some(req) = rx.recv().await {
                  // Test stub: pretend the download always succeeds, pushing a
                  // synthetic `wire_bytes = b"native"` payload.
                  let raw = RawPlatformMessage {
                      platform_id: format!("test:{}", req.chat),
                      payload: b"native".to_vec(),
                      metadata: [
                          ("chat".to_string(), req.chat),
                          ("sender".to_string(), req.sender),
                          ("dot_mode".to_string(), "native".to_string()),
                      ].into_iter().collect(),
                  };
                  let _ = inbound_tx.try_send(raw);
              }
          });
          tx
      }
  }
  ```
  Test cases:
  - `download_rx_consumer_exits_on_channel_close` — call `adapter.spawn_download_consumer_for_test()`, then `drop(tx)`. Within `tokio::time::timeout(Duration::from_millis(100), ...)` assert the spawned task logs `"download_rx consumer task exiting (channel closed)"` (use `tracing_subscriber::fmt::TestWriter` to capture logs). **R4-C1 fix:** the test drops the `Sender` returned by the constructor (not a struct field), which is the canonical way to close the channel. This pins the lifecycle behavior — a regression that blocks the task on a closed channel would fail this test.
  - `download_rx_consumer_processes_valid_request` — call `adapter.spawn_download_consumer_for_test()`, push a `DownloadRequest { msg_id: "test".into(), chat: "test@g.us".into(), sender: "1234@s.whatsapp.net".into() }` via `tx`, then `tokio::time::sleep(Duration::from_millis(50))`, then assert `adapter.inbound_rx.try_recv()` returns `Ok(raw)` with `raw.payload == b"native"` and `raw.metadata["dot_mode"] == "native"`. Pins the happy path. **R4-M2 note:** this test does NOT exercise error handling; the production consumer task (started by `start_bot`) DOES log-and-drop on download error per R1-H4. The error path is covered by the production code's `tracing::warn!` branch, which is hard to test in isolation without a mock wacore Client. Documented as a follow-up test in mission 0850p-c.
  - `download_tx_try_send_returns_full_when_channel_full` — **R4-M2 fix:** push 65 messages (not 64) into the channel returned by `spawn_download_consumer_for_test()`. The 65th `tx.try_send(...)` returns `Err(TrySendError::Full(_))`. Renaming: `download_tx_try_send_returns_full_when_capacity_exceeded`. The hardcoded count of 65 (channel size + 1) is more robust to future size changes than hardcoding 64.
  - **Implementation note:** the existing `Cargo.toml` has `tokio = { version = "1", features = ["sync", "time", "fs", "rt-multi-thread", "macros"] }` (verified at `crates/octo-adapter-whatsapp/Cargo.toml`), so `#[tokio::test]` works without Cargo.toml changes. The dev-dependencies section already has `tokio = { version = "1", features = ["macros", "rt-multi-thread"] }` (verified earlier). Both regular and dev deps must be present; the regular dep provides `mpsc::channel`/`Mutex` types, the dev dep provides the `#[tokio::test]` macro.

### Phase 3: Integration test (feature-gated `live-whatsapp`)

- [ ] `crates/octo-adapter-whatsapp/tests/whatsapp_media_transport_test.rs` — feature-gated `#[cfg(feature = "live-whatsapp")]`
  - Mirrors the structure of the existing live E2E test at `crates/octo-adapter-whatsapp/tests/live_e2e_group_setup_test.rs` (read that file first to confirm the auth pattern; the new test reuses the same `default_session_base_dir` helper)
  - Test 1: `upload_then_download_roundtrip` — start the bot (or skip if `start_bot` already ran in a prior test), generate a 64 KiB random payload (R19: large enough to exceed the 4096-byte text-mode threshold, small enough to fit in the `MediaType::Document` ceiling), call `upload_media`, capture the returned message_id, call `download_media(message_id)`, assert `decoded == original_payload`. Failure modes:
    - If the bot is not connected: skip with `tracing::warn!` and `return` (same skip pattern as the existing live tests — they require an existing `.session.db` from `octo-whatsapp-onboard`)
    - If `Client::upload` returns an error: assert the error maps to `PlatformAdapterError::Unreachable` and skip (rate limiting in CI is common; the test is informational, not a CI gate)
  - Test 2: `media_capabilities_match_upload_limit` — call `adapter.capabilities()`, assert `media_capabilities.max_upload_bytes == 100 * 1024 * 1024`, then call `upload_media` with a payload of `100 MiB + 1 byte` and assert `Err(PlatformAdapterError::PayloadTooLarge { .. })`. The pre-flight check rejects before any network round-trip, so this test runs even without an authenticated session (it tests the adapter boundary, not the network) (R20: this is the only test in the suite that doesn't require `start_bot` — run it as the first assertion in the file to fail fast on capability regressions)
- [ ] Run command documented in the test file header (mirroring `crates/octo-adapter-whatsapp/tests/live_session_test.rs:1-30`):
  ```bash
  cargo test -p octo-adapter-whatsapp \
    --features live-whatsapp \
    --test whatsapp_media_transport_test \
    -- --include-ignored --nocapture --test-threads=1
  ```

### Phase 4: Capability report test (always-on)

- [ ] `crates/octo-adapter-whatsapp/src/adapter.rs`: extend the existing `capabilities_test` (if any) or add a new `#[test] fn capabilities_includes_media_capabilities()` that asserts the full `CapabilityReport` shape:
  ```rust
  let adapter = WhatsAppWebAdapter::new(test_config());
  let caps = adapter.capabilities();
  assert_eq!(caps.max_payload_bytes, 65_536);
  assert!(caps.supports_encryption);
  assert!(!caps.supports_fragmentation);
  assert!(!caps.supports_raw_binary);
  let media = caps.media_capabilities.expect("media_capabilities must be populated for DOT/2 transport");
  assert_eq!(media.max_upload_bytes, 100 * 1024 * 1024);
  assert_eq!(media.supported_mime_types, vec!["application/octet-stream".to_string()]);
  ```
  - R21: this test runs under the default `cargo test` (no feature gate) and pins the capability declaration as a contract for `crates/octo-network/src/dot/transport.rs:92` (the `media_capabilities.is_some() → Native` branch). Without this test, a future refactor that drops `media_capabilities` would silently break `select_mode` routing for WhatsApp envelopes

### Quality gates

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo clippy --manifest-path crates/octo-adapter-whatsapp/Cargo.toml --all-targets --all-features -- -D warnings` passes (the `live-whatsapp` feature path)
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo test --workspace` passes (no regression in the 13 existing `octo-adapter-whatsapp` tests)
- [ ] `cargo test -p octo-adapter-whatsapp --features live-whatsapp --test whatsapp_media_transport_test -- --include-ignored --nocapture --test-threads=1` passes (requires an authenticated session — operator runs manually, not a CI gate)
- [ ] `cargo doc --no-deps -p octo-adapter-whatsapp` passes (no broken doc-links; the `MediaRef` helper has `///` doc comments explaining the wire format and the round-trip)

### Type Coverage

| RFC-0850 Platform Translation Layer Type / Method | Implemented By |
|------------------------------|----------------|
| `PlatformAdapter::upload_media` trait signature | Mission 0850v (Implemented) |
| `PlatformAdapter::download_media` trait signature | Mission 0850v (Implemented) |
| `CapabilityReport::media_capabilities` field | Mission 0850v (Implemented) |
| `MediaCapabilities { max_upload_bytes, supported_mime_types }` | Mission 0850v (Implemented) |
| `TransportMode::Native` selection (`media_capabilities.is_some()`) | Mission 0850v (Implemented — `crates/octo-network/src/dot/transport.rs:92`) |
| `select_mode_with_max_text` adapter-local dispatch | **This mission** (R1-C2 fix) |
| WhatsApp `upload_media` override | This mission |
| WhatsApp `download_media` override | This mission |
| WhatsApp `media_capabilities` population (100 MiB Document) | This mission |
| `MediaRef` wire format (base64url JSON of `UploadResponse`-shaped struct) | This mission |
| `MediaRef` <-> `DocumentMessage` conversion | This mission |
| Pre-flight payload size check | This mission |
| Native→text fallback (RFC-0850 MUST fallback) | **This mission** (R1-H1 fix) |
| `accept_message` `DOT/2/` prefix acceptance | **This mission** (R1-C1 + R1-M2 fix) |
| `on_event` pre-download for `DOT/2/` messages | **This mission** (R1-C1 fix) |
| `canonicalize` dual-mode payload dispatch | **This mission** (R1-C1 fix) |
| Round-trip test (live-whatsapp feature) | This mission |
| Malformed-ref handling (ApiError, redacted message) | This mission |
| `MediaRef` Debug redaction | **This mission** (R1-H4 fix) |
| `WhatsAppHandlerHandle` struct + `clone_for_handler` method | **This mission** (R3-C1 fix, type-level least-privilege) |
| `DownloadRequest` struct + `download_tx`/`download_rx` channels (initialized in `start_bot` per **R4-C1 fix** — not in `new` like R3-M1 attempted) | **This mission** (R2-C1 + R4-C1 fix) |
| `download_rx` consumer task in `start_bot` (drives `client.download` async via the handle, exits on channel close) | **This mission** (R2-C1 + R3-C1 + R4-C1 fix) |
| Test-only `spawn_download_consumer_for_test` constructor (no auth needed) + async lifecycle tests | **This mission** (R3-M2 + R4-M3 fix) |
| `dot_mode` metadata tag on `RawPlatformMessage` (text vs native) | **This mission** (R2-M5 fix) |
| `canonicalize` `dot_mode`-based dispatch (no payload-byte sniffing) | **This mission** (R2-M5 fix) |

## Implementation Guide

Companion guide for code-level patterns:

- **RFC-0850 Platform Translation Layer** — `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` (mode-selection algorithm + `DOT/1/`/`DOT/2/`/`DOT/F/`/`RAW/` format spec + MUST-fallback rule)
- **RFC-0850 Envelope Fragmentation / dual-mode transport** — `rfcs/accepted/networking/0850-deterministic-overlay-transport.md` (`DOT/2/{msg_id}` format + MUST-fallback to `DOT/1/` + mode-independent `payload_hash` determinism guarantee)
- **`MediaCapabilities` struct** — `crates/octo-network/src/dot/adapters/mod.rs:56-63`
- **`PlatformAdapter::upload_media` / `download_media` defaults** — `crates/octo-network/src/dot/adapters/mod.rs:140-164`
- **`select_mode_with_max_text` mode-selection algorithm** (the variant; `select_mode` is a one-line wrapper that delegates here) — `crates/octo-network/src/dot/transport.rs::select_mode_with_max_text`
- **`Client::upload` signature** — `whatsapp-rust/src/upload.rs:316-321` (takes `Vec<u8>`, `MediaType`, `UploadOptions`; returns `Result<UploadResponse>`)
- **`UploadResponse` shape** — `whatsapp-rust/src/upload.rs:242-251` (`url`, `direct_path`, `media_key`, `file_enc_sha256`, `file_sha256`, `file_length`, `media_key_timestamp`)
- **`Client::download` signature** — `whatsapp-rust/src/download.rs:235-244` (takes `&dyn Downloadable`; returns `Result<Vec<u8>>`)
- **`Downloadable` impl for `DocumentMessage`** — `wacore/src/download.rs:202-206` (`MediaType::Document`, `file_length`)
- **`MediaType` enum** — `wacore/src/download.rs:33-47` (Document is the only type accepting arbitrary opaque blobs)
- **`WhatsAppConfig` schema** — `crates/octo-adapter-whatsapp/src/adapter.rs:25-36` (existing adapter change is purely additive to the impl block)
- **`send_message` precondition pattern** — `crates/octo-adapter-whatsapp/src/adapter.rs:432-460` (template for the `client.is_none()` check)
- **Live E2E test pattern** — `crates/octo-adapter-whatsapp/tests/live_e2e_group_setup_test.rs` (template for the new `whatsapp_media_transport_test.rs`)

## Location

- `crates/octo-adapter-whatsapp/src/adapter.rs` (additive: `send_message` mode-dispatch refactor + 2 method overrides + 1 capability population + 1 `accept_message` prefix extension + 1 `download_tx` field + 1 `download_rx` consumer task spawned in `start_bot` + 1 `canonicalize` dual-mode dispatch (R2-M5 `dot_mode` metadata-based) + multiple unit tests)
- `crates/octo-adapter-whatsapp/src/media_ref.rs` (new, private helper module with redacted `Debug`)
- `crates/octo-adapter-whatsapp/src/lib.rs` (additive: `mod media_ref;`)
- `crates/octo-adapter-whatsapp/tests/whatsapp_media_transport_test.rs` (new, feature-gated on `live-whatsapp`)

## Complexity

Medium-High (4 new code paths + 1 helper module + 3 test layers + adapter mode-dispatch refactor + receive-path extension (with new download_tx channel + consumer task per R2-C1); no RFC changes; no cross-crate refactor)

## Prerequisites

- Mission 0850p: DOT WhatsApp Adapter (Implemented) — base adapter struct + stoolap session store
- Mission 0850v: DOT Dual Binary Transport (Implemented) — `PlatformAdapter` trait surface + `select_mode` routing
- Mission 0850e: DOT Adapter Registry & Plugin ABI (Implemented) — registry that loads the adapter cdylib

## Notes

### Why `MediaType::Document`?

The wacore API exposes 11 media types (Image, Video, Audio, Document, History, AppState, Sticker, StickerPack, StickerPackThumbnail, LinkThumbnail, ProductCatalogImage) but only `Document` is suitable for arbitrary DOT envelope bytes:

| Media Type | Use Case | Arbitrary Bytes? | Notes |
|------------|----------|------------------|-------|
| Image | JPEG/PNG | No | Re-encoded |
| Video | MP4 | No | Re-encoded |
| Audio | Opus | No | Re-encoded |
| **Document** | **Any file** | **Yes (opaque)** | **Only type storing bytes verbatim** |
| History | Protocol sync | No | App-specific shape |
| AppState | State sync | No | App-specific shape |
| Sticker | WebP | No | Re-encoded |
| (others) | App-specific | No | Specific to WhatsApp internal protocols |

**R1-M3 fix:** The mission deliberately omits specific size limits (e.g., 100 MB for Document, 16 MB for Image/Video/Audio) from this table because wacore does not expose those as compile-time constants (`wacore/src/download.rs:33-47` defines `MediaType` as a plain enum). The 100 MiB figure used in `max_upload_bytes` is a WhatsApp server-side limit per public WhatsApp documentation as of 2026-06. The adapter's pre-flight check is the only local enforcement point; if WhatsApp raises the limit, the change is a single constant edit. The other-media-type limits are intentionally omitted from the mission AC because they are not pinned by wacore.

`Document` is the only type where WhatsApp stores the bytes verbatim and redelivers them unmodified. The 100 MiB ceiling exceeds the 4 GB max for `DeterministicEnvelope` defined in `crates/octo-network/src/dot/envelope.rs:54` by 40x — there is no realistic DOT envelope that won't fit.

### Why base64url, not standard base64 (with R2-M2 fix)

`DOT/2/{msg_id}` sits inside a text message body (the `conversation` field of a `waproto::message::Message`). Standard base64 uses `+` and `/` which are not URL-safe and require escaping in some text contexts (specifically WhatsApp's text-message parser, which treats `+` as a literal char in some code paths). Base64url (`-_` instead of `+/`) avoids the issue and matches the existing `DOT/1/{base64}` convention at `crates/octo-adapter-whatsapp/src/adapter.rs::decode_envelope` (the existing `decode_envelope` helper). **R2-M2 fix:** the R1 cite of `set_subject` function range was wrong; the actual `decode_envelope` (and its `base64::engine::general_purpose::URL_SAFE_NO_PAD` usage) is the R1-confirmed helper body (per the original R1 review).

**Note on existing `decode_envelope`:** the existing text-mode decoder still uses `base64::engine::general_purpose::URL_SAFE_NO_PAD`. The new `MediaRef` encode/decode uses `octo_network::dot::transport::b64url_encode`/`b64url_decode` (per R1-M1 fix). Both produce base64url-without-padding and are interchangeable; the migration is purely about avoiding two different crate dependencies doing the same thing. A follow-up mission (post-`0850-`) should migrate the existing `decode_envelope` to the same helpers. Out of scope for this mission because it's a pure refactor with no behavior change.

### Confidentiality of `MediaRef` contents

**R1-H4 fix:** `MediaRef` contains `media_key: [u8; 32]`, which is the AES-256 key that decrypts the CDN blob. Anyone with `media_key` + `direct_path` can fetch and decrypt the encrypted payload from WhatsApp's CDN.

**Mandatory rules** (enforced by code review + the `decode_base64url_does_not_leak_input_in_error` unit test):

1. **No panic** on any input to `decode_base64url`. All error paths return `Err(MediaRefError::...)`.
2. **No leak** of the input bytes (or decoded `MediaRef` fields) in any error message, panic message, `tracing::error!`, `tracing::warn!`, or `eprintln!`.
3. **No `tracing::debug!(?media_ref)`** — the `Debug` derive on `MediaRef` would print all fields including `media_key` in plaintext. The `Debug` impl MUST be redacted (e.g., `impl Debug for MediaRef { fn fmt(...) -> ... { write!(f, "MediaRef {{ <redacted 144 bytes> }}") } }`).
4. **No `serde_json::to_string(&media_ref)` outside `encode_base64url`**. The serialized form contains `media_key` in plaintext.
5. **Fallback `client.download` errors** are logged at `tracing::warn!` with a redacted reason (e.g., `"download failed"`), never including the `direct_path` or any `MediaRef` field.

If any future maintainer needs to debug `MediaRef` contents, they MUST use a redacted logger or an opt-in `unsafe { ... }` debug block, not the standard `tracing` macros.

### Why opaque `MediaRef` (base64url JSON) instead of returning the `UploadResponse.url` directly?

WhatsApp's `UploadResponse.url` is a CDN URL (`https://mmg.whatsapp.net/v/t62.7117-24/...`) that does not encode the encryption key — to download the bytes, the receiver needs `media_key` in addition to the URL. Three options were considered:

1. **Return `url` only, look up `media_key` server-side** — requires the adapter to maintain a per-upload database keyed by URL. Brittle (DB lost = envelope unrecoverable), expensive (per-upload write), and adds a new failure mode (DB write succeeds but DB read fails later on a different node).
2. **Return the full `DocumentMessage` as a protobuf blob** — efficient but couples the wire format to the wacore protobuf schema. A future wacore version that adds fields to `DocumentMessage` would silently break the wire format on receivers pinned to the old version.
3. **Return the full `UploadResponse` as base64url JSON (this mission's choice)** — self-contained (the receiver has every field needed to reconstruct `DocumentMessage`), version-stable (JSON ignores unknown fields by default with `serde_json`, so a wacore upgrade that adds fields doesn't break old receivers), human-debuggable (`tracing::debug!` dumps are readable). The 2x size overhead is bounded to ~120 bytes regardless of envelope size.

Option 3 mirrors the existing `DOT/1/{base64}` convention (which also base64-encodes structured envelope bytes) and keeps the wire format in the adapter's hands, not wacore's.

### Why pre-flight payload size check (the `100 MiB` validation in `upload_media`)?

`Client::upload` will accept arbitrary sizes and let WhatsApp's CDN reject with a server-side error (`wacore::Error::UploadFailed`), which the adapter maps to `PlatformAdapterError::Unreachable` — the gateway would then attempt a fallback to `DOT/1/{base64}` text mode and fail again (because the envelope is also over the 65 KB text threshold), producing two retry storms. The pre-flight check short-circuits with `PlatformAdapterError::PayloadTooLarge { size, max, platform }`, which the gateway can detect at the router layer (`crates/octo-network/src/dot/transport.rs`) and refuse the envelope outright (no retry). The variant already exists at `crates/octo-network/src/dot/error.rs:61-66`; this mission consumes it, no new error type is added.

### Why `mime_type` is ignored

WhatsApp's `Document` channel hardcodes `application/octet-stream` regardless of the upload MIME. Sending `image/png` bytes through the Document channel with `mime_type = "image/png"` would not be re-encoded (whatsapp-rust treats Document as opaque), but the CDN would store the bytes with the wrong MIME in its metadata, which can confuse downstream consumers (e.g., WhatsApp Web's UI trying to render a "PNG" that isn't). Ignoring the caller's MIME and always storing `application/octet-stream` is the safe default. The argument is preserved in the signature for future extension (e.g., if a future wacore version adds a `RawDocument` type that preserves the caller's MIME) and logged at `tracing::debug!` for operator visibility.

### Why adapter owns mode selection (not the gateway)?

**R1-C2 fix:** RFC-0850's `select_mode_with_max_text` function is deterministic and well-tested, but as of `next` it has **zero production callers** — `grep -rn "select_mode" crates/` returns only the definition in `crates/octo-network/src/dot/transport.rs::select_mode` (one-line wrapper) and `crates/octo-network/src/dot/transport.rs::select_mode_with_max_text` (the actual algorithm body), plus unit tests in the same file (`test_select_mode_*`). **R3-M3 fix:** the R1 cite "66-104" mixed both functions; the actual algorithm body the mission uses is in `select_mode_with_max_text`. No gateway code dispatches on the result.

The mission makes the **adapter own mode selection** to avoid creating a mission whose implementation is permanently inert. RFC-0850's mode-selection text-mode branch specifies `If payload.len() <= max_text_bytes → DOT/1/{base64} (text mode)` and the native-mode branch specifies `If payload.len() > max_text_bytes && capabilities.supports_upload → DOT/2/{msg_id} (native mode)`. The adapter-local dispatch implements both branches of this decision tree in `WhatsAppWebAdapter::send_message`. The 65 KB text-mode threshold is WhatsApp-specific per RFC-0850's per-platform capability tables — using the RFC default of 4 KB would route too many envelopes to native mode unnecessarily. The gateway is unaware of per-adapter quirks; the dispatch is fully encapsulated per RFC-0850's capability-driven rule.

A future mission `0850p-c` (or a `crates/octo-gateway` sub-task) can extract this dispatch into a shared helper once the gateway layer is built. For now, the adapter-local dispatch satisfies RFC-0850 Platform Translation Layer without requiring a cross-crate refactor.

### Why the integration test is `live-whatsapp`-gated, not in CI

WhatsApp-rust requires an authenticated session (a `.session.db` file from `octo-whatsapp-onboard qr-link` / `pair-link`) to talk to the CDN. CI does not have such a session, and standing one up would require real WhatsApp phone numbers and would interact with Meta's rate limiters. The existing live tests at `crates/octo-adapter-whatsapp/tests/live_session_test.rs` follow the same pattern — gated on `--features live-whatsapp`, run manually by the operator against a real session, not as a CI gate. The always-on unit tests + the new capability-report test provide sufficient CI coverage for the wire-format logic.

### Persistence convention

No new on-disk state. The `MediaRef` is fully derived from the `UploadResponse` returned by `Client::upload` and round-trips through the wire format. No additional storage is required — neither the upload side nor the download side needs to persist anything beyond the existing `stoolap` session DB (which the wacore client manages internally for the Signal Protocol state).

### SDK risk

`whatsapp-rust` + `wacore` + `wacore-binary` + `waproto` are pinned to rev `9734fb2ec544e22b7055147aa3e73b6889e3ff0d` per `crates/octo-adapter-whatsapp/Cargo.toml:38-43`. The `UploadResponse` shape and `Downloadable` impl for `DocumentMessage` are stable in this rev. A future rev that:
- Adds fields to `UploadResponse` → `serde_json`'s default behavior ignores unknown fields on deserialize, so old receivers keep working
- Removes fields from `UploadResponse` → `MediaRef` decode fails with `MediaRefError::Json` (missing field), mapped to `PlatformAdapterError::ApiError`. Acceptable failure mode (the gateway refuses the envelope, the sender can re-send)
- Renames `DocumentMessage` fields → `to_document_message` fails to compile. Caught at `cargo build`, not at runtime

No SDK version bump is required for this mission.

### RFC status

RFC-0850 is in `rfcs/accepted/networking/`. The Platform Translation Layer spec is already accepted. This mission implements the WhatsApp-side override; no RFC amendment is needed.

### Open questions

None blocking. Three items the implementor should sanity-check during the first hour of work (not gating the design):

- **R1-H1 fallback test stub-ability:** The `send_message_falls_back_to_text_when_native_fails` test requires either a trait-object refactor of the `Client` type (so a stub can be injected) or a test-only constructor that bypasses `start_bot`. The current `Client` is a concrete type, not a trait. If the stub approach is infeasible, the fallback logic can still be verified via a separate unit test on a smaller extracted helper (e.g., `fn dispatch_with_fallback(payload, primary_send_fn, fallback_send_fn) -> Result`). Decide during the first hour of work and document the choice in the PR description. Verified during mission drafting that the fallback logic is testable in principle; the exact mechanism is open.

- **R2-C1 design decision (RESOLVED in R2, refined in R3, corrected in R4):** the receive-path extension uses the **download-request channel pattern with `WhatsAppHandlerHandle` clone** (Option A of R2 review, picked over Option B/C in R3-C1). `WhatsAppWebAdapter` gets a new private struct `WhatsAppHandlerHandle { client, inbound_tx }` + a new `pub(crate) fn clone_for_handler(&self) -> WhatsAppHandlerHandle` method (R3-C1 fix). **R4-C1 correction:** `WhatsAppWebAdapter` gets a new field `download_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<DownloadRequest>>>>` (mirrors the existing `client` field in `WhatsAppWebAdapter::client`), initialized to `None` in `new` (NOT created in `new` — the R3-M1 attempt failed because `download_rx` had no owner). The `start_bot` function creates the channel `(download_tx, download_rx)` via `tokio::sync::mpsc::channel::<DownloadRequest>(64)`, stores the `Sender` in the `Option` via `*self.download_tx.lock().await = Some(download_tx);`, and spawns a consumer task that captures `let handle = self.clone_for_handler();` and `let mut download_rx = download_rx;`. The consumer task loops on `download_rx.recv()`, calls `handle.client.lock().await.as_ref().unwrap().download(&req.msg_id).await` (per R4-L1 — direct wacore call, not via `WhatsAppWebAdapter::download_media`), and pushes the wire bytes to `handle.inbound_tx` (cloned separately as `inbound_tx_for_consumer`) with `metadata["dot_mode"] = "native"`. The on_event closure (which does NOT have `self` in scope) captures `Arc::clone(&self.download_tx)` and calls `self.download_tx.lock().await.as_ref().and_then(|tx| tx.try_send(DownloadRequest { msg_id, chat, sender }).ok())` when text starts with `DOT/2/`. The closure is registered BEFORE `start_bot` populates the `Option`, but `try_send` on `None` is a no-op (returns `Err(TrySendError::Closed)`), and the `or_else` semantics ensure no message is lost — `DOT/2/` messages that arrive before the consumer task starts are silently dropped (the same fall-back semantics as messages arriving during `start_bot` auth). This is the locked design — see Phase 1.4 AC in the Acceptance Criteria section.

- **R1-M3 (informational):** wacore does not expose a compile-time constant for the `MediaType::Document` ceiling — the `100 MiB` figure comes from WhatsApp's public server-side limit, not from a wacore type. The pre-flight check at 100 MiB is the right enforcement point because `Client::upload` will accept any size and let the server reject (producing a less actionable `wacore::Error::UploadFailed` that maps to `PlatformAdapterError::Unreachable` and triggers a retry storm). The implementor should not "verify the 100 MiB limit against the live CDN" — that's exactly what the pre-flight check prevents. If WhatsApp later raises the limit, the change is a single constant edit in this mission.

- **R2-M2 follow-up (out of scope):** the existing `decode_envelope` at `crates/octo-adapter-whatsapp/src/adapter.rs:348-365` still uses `base64::engine::general_purpose::URL_SAFE_NO_PAD`. The new `MediaRef` encode/decode (per R1-M1) uses `octo_network::dot::transport::b64url_encode/decode`. Both produce base64url-without-padding and are functionally interchangeable, but the duplicate implementations are a latent refactoring hazard. A follow-up mission (post-`0850-`) should migrate the existing `decode_envelope` to the same helpers. Pure refactor with zero behavior change — not blocking this mission.

---

**Mirrors:** `missions/open/0850p-a-whatsapp-auth-onboarding.md` (sibling WhatsApp sub-mission, additive adapter change), `missions/archived/0850v-dot-dual-binary-transport.md` (consumed trait surface)