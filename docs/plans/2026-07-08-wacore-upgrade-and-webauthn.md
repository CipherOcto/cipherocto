# wacore Upgrade + WebAuthn (SHORTCAKE_PASSKEY) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan session-by-session.

**Goal:** Upgrade `whatsapp-rust` from `9734fb2` (pre-buffa) to latest `main` (post-PR-928) so `Event::PairPasskeyRequest` / `PairPasskeyConfirmation` / `PairPasskeyError` become typed events; integrate `PasskeyAuthenticator` trait so the daemon can drive SHORTCAKE_PASSKEY link flow when the server gates a companion link on WebAuthn.

**Architecture:** Five independent sessions, each ending with a green `cargo build` + committed checkpoint. Sessions are ordered by dependency risk: the wacore upgrade (biggest blast radius) goes first; the trait surface and event wiring follow once the project compiles; the operator-facing UX (state machine + QR render) goes last.

**Tech Stack:** Rust, `whatsapp-rust = git@oxidezap/whatsapp-rust`, wacore (post-PR-928), `bon::Builder` (already a transitive dep, used by upstream events), `waproto` (post-buffa migration), `tempfile` (tests), `tokio` (already in tree).

---

## Pre-flight (every session)

Before starting any session:

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
git status                                # clean tree or known checkpoint
git log --oneline -1                       # last commit visible
lsof +D ~/.local/share/octo/whatsapp 2>/dev/null | head -5
# ^ confirm no live daemon holds the session DB
cargo check -p octo-whatsapp --all-targets --features "live-whatsapp test-helpers" 2>&1 | tail -5
# ^ baseline: should say "Finished `dev` profile ... target(s)" with no errors
```

If baseline fails, stop. Resolve the prior session's tail before starting the next.

End of every session:

```bash
git status                                # confirm clean staged area
cargo fmt --check -p octo-whatsapp-onboard octo-whatsapp octo-adapter-whatsapp
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib -p octo-whatsapp --features "live-whatsapp test-helpers"
cargo test -p octo-whatsapp-onboard --bin octo-whatsapp-onboard
# ^ all four must pass before the session is "done"
```

---

## Validation corrections applied (2026-07-08)

The plan below was validated against the upstream `main` commit `6e0f241d` via a ground-truth subagent. The following corrections were applied in-place before this final version:

| # | Section | Correction |
|---|---|---|
| 1 | Task 1.1 Step 3 error table | E0308 count **20** (was "~16"), E0277 count **8** (was "~7"). Removed the "~4 edge cases" row. |
| 2 | Task 1.2 Step 3 (`encode_to_vec`) | Added `(**a).encode_to_vec()` Arc deref (the `use buffa::Message;` import alone is insufficient — receiver is `&Arc<ADVSignedDeviceIdentity>`). |
| 3 | Task 1.2 Step 3 (`AdvSignedDeviceIdentity`) | Replaced the `todo!()` deferral with a 4-character case fix (`Adv` → `ADV`). The type did not move namespaces. |
| 4 | Task 1.4 Step 1 (`Arc<StoolapStore>`) | The call is `.with_backend(backend)`, not `.with_store(...)`. Pass the bare `StoolapStore` to `with_backend`; builder wraps in `Arc`. |
| 5 | Task 1.4 Step 2 (`Event::Messages`) | Wrapper is `MessageBatch { messages: Arc<[InboundMessage]>, origin: BatchOrigin }`, not `Arc<Messages>`. Per-message `MessageInfo` moved into `InboundMessage`. |
| 7 | Task 1.4 Step 6 (non-exhaustive) | Added note that optional fields use `maybe_X(...)`, not `X(...)`. Read `cargo doc --open -p whatsapp-rust` to confirm. |
| 8 | Task 1.4 Step 7 (`HashMap<Jid, _>`) | Added cascade warning. The signature change in `get_participating` ripples to all IPC/CLI/MCP group handlers. |
| 9 | Task 1.5 Step 4 (fixture rewrite) | **Removed.** The classifier's `strip_prefix("Event::").unwrap_or(raw)` + split-on-brace is robust to Debug-format shifts. `pairing_stall_*` fixtures do not need rewriting. |
| 10 | Session 2 Task 2.1 Step 3 (`AssertionRequest`) | `rp_id: Option<String>` and `timeout_ms: Option<u64>` to mirror upstream. |
| 11 | Session 2 Task 2.2 Step 1 (`PasskeyAuthenticator`) | Method takes `&AssertionRequest` (not owned). Supertrait is `wacore::sync_marker::MaybeSendSync` (not raw `Send + Sync`). |
| 12 | Session 2 Task 2.2 Step 1 (`Assertion`) | Only **2 fields**: `assertion_json: Vec<u8>` (the standard `PublicKeyCredential.authenticationResponseJson` UTF-8 JSON) and `credential_id: Vec<u8>`. Drop the 4-field decomposition. |
| 13 | Session 2 Task 2.2 Step 3 (`with_passkey_authenticator`) | **No such method exists.** Call `bot.client().set_passkey_authenticator(auth).await` between `builder.build()` and `bot.run()`. |
| 14 | Session 2 Task 2.2 Step 2 (`WhatsAppConfig`) | Concrete file path `crates/octo-adapter-whatsapp/src/config.rs`. Default constructor must be updated to set `passkey_authenticator: None`; struct-literal call sites in tests may need explicit `None`. |
| 15 | Task 1.5 cascade (added) | Pre-flight grep step for `Event::Message`, `HashMap<String, GroupMetadata>`, `encode_to_vec`, `AdvSignedDeviceIdentity` before running `cargo check`. |

Things still to check during execution (not blockers, but flagged during validation):

- **`wacore` dev-dependency**: Session 3 / Session 4 test fixtures construct `wacore::types::events::PairPasskeyRequest::builder().request_options_json(...).build()`. Verify `wacore` is reachable from `[dev-dependencies]` of `octo-adapter-whatsapp` (likely yes via `octo-whatsapp-onboard-core` or transitively, but check before writing tests).
- **`bot_message.rs` path does not exist** — ignore any text in earlier drafts that referenced `wacore/src/bot_message.rs` for builder examples. The setter name `request_options_json(String)` is still correct (verified by the `bon::Builder` field), but verify via `cargo doc --open -p wacore` if you need an actual usage example.
- **`as_coordinator_admin` interaction**: Phase 6.12 added a coordinator-admin escape hatch to `WhatsAppWebAdapter`. Verify the new `set_passkey_authenticator` plumbing (between `builder.build()` and `bot.run()`) does not conflict with whatever call site establishes the coordinator-admin trait object. Likely fine, but grep before committing.
- **`wacore::shortcake` is now public**: Not needed for this plan (we forward the typed event; the SDK auto-drives when an authenticator is registered). If a future plan wants to drive the handshake directly without going through `PasskeyAuthenticator`, `wacore::shortcake::ShortcakeUtils` is the offline-testable crypto core.

---

## Background: original auth flow (preserved across the migration)

The WhatsApp passkey/WebAuthn link gate sits on top of the normal companion-link connection:

1. Server sends `<notification type="passkey_prologue_request">` carrying a `<passkey_request_options>` child whose body is the verbatim `PublicKeyCredentialRequestOptions` JSON.
2. wacore parses the JSON, then either (a) auto-drives if a `PasskeyAuthenticator` is registered, or (b) emits `Event::PairPasskeyRequest { request_options_json }` and waits for the host to drive.
3. On successful assertion the handshake synthesises an ephemeral X25519 identity, derives an AES-256-GCM `PairingRequest` (HKDF-SHA256 with salt `"Companion Pairing {deviceType} with ref {ref}"`, info `"Pairing Information Encryption Key"`), encrypts the rotated ADV secret, and sends it to the server.
4. Server confirms with `Event::PairPasskeyConfirmation { code, skip_handoff_ux }` (8-char Crockford code; `skip_handoff_ux=true` on re-links with valid handoff proof, suppressing the visible-code UI).
5. The link completes through the ordinary `pair-success` path; `Event::Connected` fires.
6. Failure paths emit `Event::PairPasskeyError { error, continuation }` (continuation distinguishes a re-link failure from the initial request).

Our job: upgrade wacore so the 5 events exist as typed Rust, plumb a `PasskeyAuthenticator` trait seam into `WhatsAppWebAdapter::new()`, drive our state machine off the events, and give the operator a QR (containing `request_options_json`) when the server asks for an assertion.

---

## Session 1 — Mechanical wacore upgrade (get green build)

**Goal:** Pin `whatsapp-rust` and the five sibling crates to latest `main` and fix all 38 currently-listed compile errors in our tree. End state: `cargo check -p octo-whatsapp --all-targets --features "live-whatsapp test-helpers"` returns 0 errors.

**Risk:** Highest of the five sessions. Mechanical migration, but every error shares a small number of patterns; one bad batch fix creates a cascade. Three independent checkpoints, each ending with a committable, buildable tree.

### Task 1.1: Bump the pin, get the error list

**Files:**
- Modify: `crates/octo-adapter-whatsapp/Cargo.toml:46-51` — six lines, each `rev = "9734fb2..."` → `rev = "6e0f241dc0265add92e1abff0203ec115b8fa4a7"` (latest main HEAD on 2026-07-08; pins to a specific SHA so main ref churn does not surprise future sessions).

**Step 1: Update the pin**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-adapter-whatsapp
sed -i 's/9734fb2ec544e22b7055147aa3e73b6889e3ff0d/6e0f241dc0265add92e1abff0203ec115b8fa4a7/g' Cargo.toml
grep "6e0f241d" Cargo.toml | wc -l   # expect 6
```

**Step 2: Capture the full error list into `/tmp/wacore-upgrade-errors.txt`**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo check -p octo-adapter-whatsapp --message-format=short 2>&1 \
  | tee /tmp/wacore-upgrade-errors.txt \
  | grep -E "^error:|^crates/" > /tmp/wacore-upgrade-error-summary.txt
wc -l /tmp/wacore-upgrade-errors.txt
grep -c "^error\[" /tmp/wacore-upgrade-errors.txt   # expect ~38
```

Expected: ~38 lines of the form `crates/octo-adapter-whatsapp/src/XXX.rs:LINE: error[E0XXX]: ...`.

**Step 3: Categorise errors**

```bash
grep -oE "E[0-9]+" /tmp/wacore-upgrade-errors.txt | sort | uniq -c | sort -rn
```

Expected shape (will confirm against ground truth once compile runs):

| Error code | Count | Fix pattern |
|---|---|---|
| E0308 | **20** | `Some(x)` → `x.into()` (waproto `MessageField<T>` wraps directly); `Some(Box::new(x))` → `Box::new(x).into()`; `0` (raw int for `Type`) → `Type::builder().value(...).build()`; `HashMap<String, _>` → `HashMap<Jid, _>`; `&[&str]` → `vec.iter().map(String::as_str)` |
| E0277 | **8** | `Arc<StoolapStore>` → `StoolapStore` for `with_backend` (builder wraps in `Arc` internally; the E0277 is reported 5× for the 5 trait bounds but is one logical fix); `Arc<Device>` is not a Future (drop `.await` at L1127, L1149); drop `?` on `()` at L1268 |
| E0026 | 2 | `Event::PairingQrCode { code, .. }` → `Event::PairingQrCode(inner) => { let code = &inner.code; ... }` (tuple variant carrying `PairingQrCode { code: String, timeout: Duration }`) |
| E0599 | 2 | `encode_to_vec()` on `&Arc<ADVSignedDeviceIdentity>` → deref + trait import: `(**a).encode_to_vec()` with `use buffa::Message;` |
| E0639 | 2 | Non-exhaustive `ParticipantChangeResponse { ... }` literals at L1644 + L1700 → `ParticipantChangeResponse::builder().jid(...).status(...).error(...).build()`. **Note:** optional fields use `maybe_status(...)` not `status(...)` (verify by `cargo doc --open -p whatsapp-rust` after the pin bump) |
| E0046 | 2 | Add `mark_prekeys_uploaded(&self, _ids: &[u32]) -> Result<()>` to `impl SignalStore`; add `clear_mutation_macs(&self, _name: &str) -> Result<()>` to `impl AppSyncStore` (per upstream `wacore/src/store/traits.rs`) |
| E0050 | 1 | `delete_expired_tc_tokens(cutoff: i64)` → `delete_expired_tc_tokens(token_cutoff: i64, sender_cutoff: i64)`; SQL adds second predicate on `sender_timestamp`; test call site at L1916/L1928 passes both cutoffs |
| E0433 | 1 | `waproto::whatsapp::AdvSignedDeviceIdentity` → `waproto::whatsapp::ADVSignedDeviceIdentity` — **case fix only**; the namespace did not move. There are likely **2 references** in `store.rs` (L1472 plus the `Some(wa::AdvSignedDeviceIdentity { ... })` literal earlier in the same function — currently inside an `unimplemented!` path that may have been disabled) |

### Task 1.2: store.rs fixes (4 errors, 1 checkpoint)

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/store.rs` at lines ~288 (`mark_prekeys_uploaded`), ~579 (`clear_mutation_macs`), ~1215 (`delete_expired_tc_tokens`), ~1357 (`encode_to_vec`), ~1472 (`AdvSignedDeviceIdentity`).

**Step 1: Add `use buffa::Message;` at the top of `store.rs`**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/crates/octo-adapter-whatsapp
grep -n "^use " src/store.rs | head -5
# Edit: prepend `use buffa::Message;` to the existing top-of-file use block
```

**Step 2: Add the 3 missing trait method stubs**

In `impl SignalStore for StoolapStore` (around line 288) add:

```rust
async fn mark_prekeys_uploaded(&self, _ids: &[u32]) -> wacore::store::error::Result<()> {
    // TODO(session-1): Stoolap-backed mark-as-uploaded. Pragma for now:
    // the sweep that would call this runs after a successful first upload,
    // which currently never happens because pair_success is end-to-end
    // before prekey re-uploads. Tracked for Phase 7.
    Ok(())
}
```

In `impl AppSyncStore for StoolapStore` (around line 579) add:

```rust
async fn clear_mutation_macs(&self, _name: &str) -> wacore::store::error::Result<()> {
    // TODO(session-1): Stoolap-backed MAC clear. The ltHash rebuild is
    // triggered on snapshot re-sync, which the upstream default sync
    // sequence handles; store-level impl deferred.
    Ok(())
}
```

In `impl ProtocolStore for StoolapStore` (around line 1215) update `delete_expired_tc_tokens` to the new two-argument signature:

```rust
async fn delete_expired_tc_tokens(
    &self,
    token_cutoff: i64,
    sender_cutoff: i64,
) -> wacore::store::error::Result<u32> {
    // R14-M5: atomic DELETE returning rows-affected. SQL unchanged;
    // the upstream trait added a second cutoff to guard sender buckets
    // separately from received-token state (see wacore/src/store/traits.rs).
    // We map sender_cutoff → a second predicate on the same row.
    let conn = self.db.lock().await;
    let r = conn
        .execute(
            "DELETE FROM tc_tokens WHERE \
             (token_timestamp = 0 OR token_timestamp < ?1) AND \
             (sender_timestamp IS NULL OR sender_timestamp < ?2)",
            stoolap::params![token_cutoff, sender_cutoff],
        )
        .await
        .map_err(to_store_err)?;
    Ok(r.max(0) as u32)
}
```

Also update the test call at `store.rs:1916` and `store.rs:1928`:

```rust
// before
.delete_expired_tc_tokens(cutoff)
// after — pass both cutoffs (sender_cutoff = 0 means "no sender state preserved")
.delete_expired_tc_tokens(cutoff, 0)
```

**Step 3: Replace `encode_to_vec()` and `AdvSignedDeviceIdentity`**

At `store.rs:1357`, the closure receiver is `&Arc<ADVSignedDeviceIdentity>` (field type `Option<Arc<wa::ADVSignedDeviceIdentity>>` from `wacore/src/store/device.rs`). The `Message::encode_to_vec` trait is implemented for `ADVSignedDeviceIdentity`, not for `Arc<T>`. Apply both the import AND the deref:

```rust
// at top of store.rs
use buffa::Message;

// at L1357
let account = device.account.as_ref().map(|a| (**a).encode_to_vec());
// (or equivalently: device.account.as_ref().map(|a| a.as_ref().encode_to_vec()))
```

At `store.rs:1472`: case fix only. The namespace did not move. Replace:

```rust
waproto::whatsapp::AdvSignedDeviceIdentity::decode(&*b)
```

with:

```rust
waproto::whatsapp::ADVSignedDeviceIdentity::decode(&*b)
```

Sanity check: there is likely a **second** `AdvSignedDeviceIdentity` reference in the same function (the `Some(wa::AdvSignedDeviceIdentity { ... })` literal further up). If that path is currently in a `unimplemented!()` block, both refs need the same case fix when the path is re-enabled. Confirm by `grep -n "AdvSignedDeviceIdentity\|ADVSignedDeviceIdentity" src/store.rs`.

**Step 4: `cargo check -p octo-adapter-whatsapp`** — expect store.rs errors gone, `store.rs` clean. Errors remaining move to `adapter.rs` and `inherent.rs`.

**Step 5: Commit checkpoint 1.2**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
git add crates/octo-adapter-whatsapp/Cargo.toml crates/octo-adapter-whatsapp/src/store.rs
git commit -m "chore(octo-adapter-whatsapp): bump wacore to post-PR-928 main + fix store.rs"
```

### Task 1.3: inherent.rs fixes (~16 errors, 1 checkpoint — the biggest single file)

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/inherent.rs` at lines 84, 179, 271, 363, 454, 517, 518, 597, 672, 738, 814-816, 821, 883-884, 888, 937.

**Step 1: Apply the universal pattern — `Option::into()` for `MessageField<T>`**

Every `Some(x)` or `Box::new(x)` being assigned to a `MessageField<T>` field needs `.into()`. Two shapes:

```rust
// shape A: was `field: Some(x)`, now `field: x.into()`
text_message: Some(my_text),       // before
text_message: my_text.into(),      // after

// shape B: was `field: Some(Box::new(x))`, now `field: Box::new(x).into()`
document_message: Some(Box::new(d)),       // before
document_message: Box::new(d).into(),      // after
```

Apply at each `inherent.rs` error site listed above. For each one: read the surrounding context (the error message already names the exact line + type), apply the change.

**Step 2: Fix the two `expected Type, found i32` errors (lines 815, 884)**

These are at WA message `Type` enum construction sites. The `Type` enum is now `bon::Builder`-based with non-exhaustive fields. Replace:

```rust
// before
type_: Some(0),           // or similar raw int literal
// after
type_: Type::builder().value(...).build(),
```

The full set of `Type` values is in `waproto::whatsapp::message::message::Type` (upstream). For the 2 sites, copy the value semantics from the surrounding `TextMessage` / `ReactionMessage` that already reference the right `Type` variant.

**Step 3: Fix the `expected &[&str], found Vec<String>` (line 937)**

This is a method signature change — `parse_groups`-style fn that took a slice now takes an iterator/owned Vec. Replace:

```rust
// before
f(ctx, &[...])
// after
f(ctx, vec.iter().map(String::as_str))
```

The exact API change is verified against the wacore main call site at the line; do NOT guess if `iter().map(...)` doesn't compile — check the trait signature.

**Step 4: `cargo check -p octo-adapter-whatsapp`** — expect all `inherent.rs` errors gone.

**Step 5: Commit checkpoint 1.3**

```bash
git add crates/octo-adapter-whatsapp/src/inherent.rs
git commit -m "chore(octo-adapter-whatsapp): adopt wacore MessageField shape in inherent.rs"
```

### Task 1.4: adapter.rs fixes (~14 errors, 1 checkpoint)

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/adapter.rs` at lines 945, 987, 1127, 1149, 1215, 1239, 1268, 1644, 1700, 1737, 2476.

**Step 1: Fix the `Arc<StoolapStore>` Backend trait obligations (line 945)**

The actual call is already `.with_backend(backend)`, not `.with_store(...)`. The E0277 fires because `backend` is being constructed as `Arc<StoolapStore>`, and upstream no longer has a blanket `impl Backend for Arc<T>`. The upstream builder signature is:

```rust
pub fn with_backend(self, backend: impl Backend + 'static) -> BotBuilder<...>
```

i.e., it accepts the bare type and wraps in `Arc` internally. The fix: trace upstream where `backend` is constructed (around `adapter.rs:945`). Wherever the current code is `let backend = Arc::new(StoolapStore::new(...))`, change to `let backend = StoolapStore::new(...)` and pass `backend` to `.with_backend(...)`. If `backend` is held elsewhere as `Arc<StoolapStore>` for shared ownership across the daemon, that holding is fine — pass the inner `StoolapStore` to `with_backend` and continue to share the `Arc` through a different field. The 5 E0277 reports (one per trait bound in the blanket impl) all collapse to a single edit.

**Step 2: Fix `Event::Message` (line 987) — rename to `Event::Messages(MessageBatch)`**

Upstream renamed `Event::Message(Arc<wa::Message>, Arc<MessageInfo>)` to `Event::Messages(MessageBatch)`, where `MessageBatch` is a struct with fields:

```rust
pub struct MessageBatch {
    pub messages: Arc<[InboundMessage]>,
    pub origin: BatchOrigin,
}
```

Per-message `MessageInfo` is now a field on each `InboundMessage` (verify exact field name via `grep -n "pub.*info\|pub.*Info" wacore/src/types/events.rs` after the pin bump — likely `info: MessageInfo` or `metadata: MessageMetadata` post-buffa-migration). Replace the existing match arm:

```rust
// before
Event::Message(msg, info) => { ... }
// after
Event::Messages(batch) => {
    for m in batch.messages.iter() {
        // access m's payload + m.info via their public fields
    }
}
```

**Cascade:** any downstream `Event::Message` match in `octo-whatsapp` daemon or `octo-whatsapp-onboard-core` needs the same update. Grep the workspace:

```bash
grep -rn "Event::Message\b" crates/
```

Every hit outside the adapter will surface as a cascade error in Task 1.5.

**Step 3: Fix `Event::PairingQrCode` / `PairingCode` field access (lines 1215, 1239)**

The bon::Builder migration made these variants tuple-like. Replace:

```rust
// before
Event::PairingQrCode { code, .. } => { ... code ... }
// after
Event::PairingQrCode(inner) => { let code = &inner.code; ... }
```

(or similar — confirm against upstream definition; the variant is now `Event::PairingQrCode(wa::PairingQrCode)` where `wa::PairingQrCode` has `.code: String`.)

**Step 4: Fix the `Arc<Device>` is-not-a-future errors (lines 1127, 1149)**

`Device` was a Future (waits on persistence); it is now sync. Remove `.await`:

```rust
// before
let d = Arc::clone(&device).await;
// after
let d = Arc::clone(&device);
```

If the call site needed the `await` because of an async barrier, introduce a `tokio::task::spawn_blocking` wrapper instead.

**Step 5: Fix the `?` on `()` error (line 1268)**

A function that previously returned `Result<T>` now returns `()`. Either change the call site to drop the `?` and pass the value through an outer mechanism, or restore the `Result` return by tracking the error in a struct field. Use the simpler one — likely the call site can absorb a logged error and continue.

**Step 6: Fix the 2 non-exhaustive structs (lines 1644, 1700)**

Both literals are `ParticipantChangeResponse { ... }` (promote at L1644, demote at L1700). The struct is upstream-defined and built via `bon::Builder`. Note that **optional fields use `maybe_X(...)` not `X(...)`** — for example:

```rust
// before
ParticipantChangeResponse {
    jid: ...,
    status: Some("promoted".into()),
    error: None,
    ...
}

// after (verify setter names via `cargo doc --open -p whatsapp-rust` after the pin bump)
ParticipantChangeResponse::builder()
    .jid(jid.clone())
    .maybe_status(Some("promoted".to_string()))
    .maybe_error(None)
    .build()
```

Generate the builder API from the pinned wacore via:

```bash
cargo doc --open -p whatsapp-rust --no-deps
# navigate to whatsapp_rust::protocol::ParticipantChangeResponse::builder
```

This is the one place in the migration where guessing the setter names is dangerous. If `maybe_status` does not exist, the upstream type may use a different setter convention — read the actual bon-generated docs before committing a fix.

**Step 7: Fix the `HashMap<String, _>` vs `HashMap<Jid, _>` mismatch (line 1737)**

Group-metadata map key changed type. The current code at L1737 is:

```rust
pub async fn get_participating(&self) -> Result<
    std::collections::HashMap<String, whatsapp_rust::GroupMetadata>,
    String,
> { ... }
```

The fix is to change the function's return type signature from `HashMap<String, _>` to `HashMap<Jid, _>`. **This cascades** to every caller:

```bash
grep -rn "HashMap<String, *GroupMetadata\|HashMap<String, *GroupParticip" crates/
```

Expect hits in:
- `crates/octo-whatsapp/src/daemon.rs` (if there's an IPC handler surfacing group roster)
- `crates/octo-whatsapp/src/ipc/handlers/groups.rs` and similar
- CLI subcommand handlers in `crates/octo-whatsapp/src/cli.rs` that pull the roster via JSON
- MCP tools in `crates/octo-whatsapp/src/mcp.rs` (if present)

Each call site that does `.get(&jid_string)` will need `.get(&jid)` or `.get(&jid.to_string())`. JSON surfacing (CLI/MCP output) will need a `.to_string()` adapter. Apply changes file-by-file in Task 1.5.

**Step 8: Fix `MessageField<DocumentMessage>` mismatch (line 2476)**

Same `MessageField` pattern as Task 1.3 step 1:

```rust
document_message: Some(Box::new(d)),       // before
document_message: Box::new(d).into(),      // after
```

**Step 9: `cargo check -p octo-adapter-whatsapp`** — expect 0 errors in `adapter.rs`. The `StoolapStore` Arc-vs-Backend wiring cascades to `octo-whatsapp-onboard-core` and `octo-whatsapp` (daemon.rs holds the adapter) — proceed to Task 1.5.

**Step 10: Commit checkpoint 1.4**

```bash
git add crates/octo-adapter-whatsapp/src/adapter.rs
git commit -m "chore(octo-adapter-whatsapp): adopt wacore buffa + bon shapes in adapter.rs"
```

### Task 1.5: cascade — `octo-whatsapp-onboard-core` + `octo-whatsapp` daemon + tests

**Files:**
- Modify: `crates/octo-whatsapp-onboard-core/src/*`, `crates/octo-whatsapp/src/daemon.rs`, `crates/octo-whatsapp/src/daemon/tests.rs`, `crates/octo-whatsapp/src/ipc/handlers/*`, `crates/octo-whatsapp/tests/live_daemon_test.rs` — anywhere downstream that referenced `Event::Message`, `Option::Some(_)` waproto shapes, etc.

**Step 1: Cascade check + pre-flight grep**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp

# Targeted greps for known cascade signatures. Each grep result is a
# candidate file that Task 1.5 step 2 must address.
grep -rn "Event::Message\b" crates/          # rename to Event::Messages(MessageBatch)
grep -rn "HashMap<String, .*GroupMetadata" crates/  # HashMap<Jid, _> cascade
grep -rn "encode_to_vec" crates/octo-adapter-whatsapp/src/  # may need Arc deref
grep -rn "AdvSignedDeviceIdentity\b" crates/  # case fix (not namespace move)

# Then run the full check to surface any remaining errors.
cargo check --workspace --all-targets --features "live-whatsapp test-helpers" 2>&1 \
  | tee /tmp/wacore-upgrade-errors-after-1.4.txt \
  | grep -E "^error:" | head -40
```

**Step 2: Apply the same patterns from Task 1.2-1.4 to cascade sites.** Most cascade errors will be `Option::Some -> .into()`, `Event::Message -> Messages`, or trait surface adaptations. For each error, read the file line + context + fix.

Specific cascade points to watch for (each is a `grep` target above):

| Source | Cascade | Fix |
|---|---|---|
| `octo-whatsapp/src/daemon.rs` | `Event::Message` match arm | rename + restructure per Task 1.4 Step 2 |
| `octo-whatsapp/src/ipc/handlers/groups.rs` (or similar) | `HashMap<String, GroupMetadata>` consumer | `.get(&jid)` instead of `.get(&jid_str)` |
| `octo-whatsapp/src/cli.rs` groups.* subcommands | roster JSON surface | serialize `jid.to_string()` at the boundary |
| `octo-whatsapp-onboard-core/src/*` | `passkey_qr` rendering (Session 4) — must use upstream field name `request_options_json` | read field name from `PairPasskeyRequest` struct on pinned commit |
| `octo-adapter-whatsapp/Cargo.toml` | `[dev-dependencies]` needs `wacore` to construct `PairPasskeyRequest::builder()` in tests (Session 3 / Session 4) | add `wacore = { ... }` to `[dev-dependencies]` if not already transitive |

**Step 3: Run the test suite**

```bash
cargo test --lib -p octo-whatsapp --features "live-whatsapp test-helpers"
cargo test -p octo-whatsapp-onboard-core
cargo test -p octo-whatsapp-onboard --bin octo-whatsapp-onboard
```

Expected: 659+ lib tests pass; the 4 Phase 6.12.5 `pairing_stall_*` tests continue to pass on the rebuilt wacore (the classifier `strip_prefix("Event::").unwrap_or(raw)` + `split(['(', ' ', '{', '<'])` is robust to Debug-format shifts; fixture strings like `"Event::PairingQrCode { code: \"x\", timeout: 60s }"` still extract `ident = "PairingQrCode"` under both wacore versions, so no fixture rewrites are needed).

**Step 5: Commit checkpoint 1.5 (final task of Session 1)**

```bash
git add -A
git commit -m "chore(octo-whatsapp): cascade wacore upgrade + pass full test suite"
```

### Session 1 verification

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib --features "live-whatsapp test-helpers"
# All four must pass. Live chain integration tests are NOT run here (Session 5).
# If any test regresses, stop and fix before starting Session 2.
```

If green: Session 1 done. Tag the commit locally (no push per standing rule):

```bash
git tag -l "*wacore-migration*"   # show any prior; pick the next
git tag session-1-wacore-baseline HEAD   # local-only tag for checkpoint
```

---

## Session 2 — `PasskeyAuthenticator` trait seam in `WhatsAppWebAdapter`

**Goal:** Define our `PasskeyAuthenticator` trait + a `CallbackAuthenticator` adapter, and plumb an `Option<Arc<dyn PasskeyAuthenticator>>` field through `WhatsAppWebAdapter::new` and `WhatsAppConfig`. End state: builds green, existing tests pass, no observable behavior change (a `None` authenticator causes the SDK to skip auto-drive and emit `Event::PairPasskeyRequest` for the host).

**Risk:** Low. Pure trait plumbing. No live behavior change unless someone sets the authenticator to `Some(...)`.

### Task 2.1: Add the `passkey` module to the adapter

**Files:**
- Create: `crates/octo-adapter-whatsapp/src/passkey/mod.rs`
- Create: `crates/octo-adapter-whatsapp/src/passkey/assertion.rs` (struct + parse helper)
- Modify: `crates/octo-adapter-whatsapp/src/lib.rs` to expose `pub mod passkey;`

**Step 1: Write the `AssertionRequest` parsing test (TDD)**

```rust
// crates/octo-adapter-whatsapp/src/passkey/assertion.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_options_minimal() {
        let json = br#"{
            "challenge": "Y2hhbGxlbmdlLWJ5dGVz",
            "rpId": "web.whatsapp.com",
            "userVerification": "preferred",
            "timeout": 60000,
            "allowCredentials": []
        }"#;
        let req = AssertionRequest::parse(json).expect("must parse");
        assert_eq!(req.rp_id.as_deref(), Some("web.whatsapp.com"));
        assert_eq!(req.user_verification, UserVerification::Preferred);
        assert_eq!(req.timeout_ms, Some(60_000));
        assert!(req.allow_credentials.is_empty());
    }
}
```

**Step 2: Run — expect compile error (struct not yet defined)**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-adapter-whatsapp --lib passkey::assertion 2>&1 | tail -5
# expect: error[E0433]: failed to resolve: use of undeclared type `AssertionRequest`
```

**Step 3: Implement `AssertionRequest` mirroring upstream**

```rust
// crates/octo-adapter-whatsapp/src/passkey/assertion.rs
use serde::Deserialize;

/// Public request view, normalised from the WA passkey_request_options JSON.
/// Field shape mirrors upstream's `whatsapp_rust::passkey::AssertionRequest`
/// (in `src/passkey/mod.rs:74-83`) so a future alias
/// `impl From<our::AssertionRequest> for upstream::AssertionRequest` is trivial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionRequest {
    pub challenge: Vec<u8>,
    pub rp_id: Option<String>,
    pub allow_credentials: Vec<Vec<u8>>,
    pub user_verification: UserVerification,
    pub timeout_ms: Option<u64>,
    pub raw_options_json: String,
}

impl AssertionRequest {
    pub fn parse(json: &[u8]) -> Result<Self, PasskeyError> {
        #[derive(Deserialize)]
        struct Raw {
            challenge: String,        // base64url-no-pad
            rp_id: String,
            #[serde(default = "default_uv")]
            user_verification: String,
            #[serde(default = "default_timeout")]
            timeout: u64,
            #[serde(default)]
            allow_credentials: Vec<RawCred>,
        }
        #[derive(Deserialize)]
        struct RawCred { id: String }
        fn default_uv() -> String { "preferred".to_string() }
        fn default_timeout() -> u64 { 60_000 }

        let raw: Raw = serde_json::from_slice(json)
            .map_err(|e| PasskeyError::InvalidOptions(format!("parse: {e}")))?;
        let challenge = base64_url_decode(&raw.challenge)
            .map_err(|e| PasskeyError::InvalidOptions(format!("challenge: {e}")))?;
        let user_verification = match raw.user_verification.as_str() {
            "required" => UserVerification::Required,
            "preferred" => UserVerification::Preferred,
            "discouraged" => UserVerification::Discouraged,
            other => return Err(PasskeyError::InvalidOptions(format!("uv: {other}"))),
        };
        let allow_credentials = raw
            .allow_credentials
            .into_iter()
            .map(|c| base64_url_decode(&c.id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PasskeyError::InvalidOptions(format!("cred: {e}")))?;
        Ok(Self {
            challenge,
            rp_id: Some(raw.rp_id),
            allow_credentials,
            user_verification,
            timeout_ms: Some(raw.timeout),
            raw_options_json: String::from_utf8_lossy(json).into_owned(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PasskeyError {
    #[error("invalid passkey options: {0}")]
    InvalidOptions(String),
    #[error("assertion failed: {0}")]
    AssertionFailed(String),
    #[error("authenticator not registered")]
    NotRegistered,
    #[error("operation timed out after {0:?}")]
    Timeout(std::time::Duration),
}

fn base64_url_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s)
}
```

(Add `base64` to `[dependencies]` in `crates/octo-adapter-whatsapp/Cargo.toml` — likely already a transitive dep.)

**Step 4: Run — expect green**

```bash
cargo test -p octo-adapter-whatsapp --lib passkey::assertion 2>&1 | tail -5
# expect: test result: ok. 1 passed
```

### Task 2.2: `PasskeyAuthenticator` trait + `CallbackAuthenticator`

**Files:**
- Create: `crates/octo-adapter-whatsapp/src/passkey/authenticator.rs`

**Step 1: Write the trait surface**

```rust
// crates/octo-adapter-whatsapp/src/passkey/authenticator.rs
//
// Mirrors upstream `whatsapp_rust::passkey::PasskeyAuthenticator` exactly
// (in `src/passkey/mod.rs:115-117`) so a future `type alias` or
// `impl From<our::PasskeyAuthenticator> for upstream::PasskeyAuthenticator`
// is trivial. The trait supertrait is `wacore::sync_marker::MaybeSendSync`
// (which is `Send + Sync` on native targets, relaxed on wasm32) and the
// method takes `&AssertionRequest` (not owned).
use super::assertion::{AssertionRequest, PasskeyError, UserVerification};
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// WebAuthn assertion result.
///
/// Field shape mirrors upstream `whatsapp_rust::passkey::Assertion`
/// (in `src/passkey/mod.rs:131-134`). The standard
/// `PublicKeyCredential.authenticationResponseJson` is passed as raw
/// UTF-8 bytes; the wacore flow packs the response into the protocol
/// payload verbatim.
#[derive(Debug, Clone)]
pub struct Assertion {
    /// UTF-8 JSON bytes of `PublicKeyCredential.authenticationResponseJson`.
    pub assertion_json: Vec<u8>,
    /// Raw credential id (decoded base64url-no-pad from the response).
    pub credential_id: Vec<u8>,
}

pub type AssertionFuture =
    Pin<Box<dyn Future<Output = Result<Assertion, PasskeyError>> + Send + 'static>>;

#[async_trait]
pub trait PasskeyAuthenticator: wacore::sync_marker::MaybeSendSync {
    async fn get_assertion(
        &self,
        request: &AssertionRequest,
    ) -> Result<Assertion, PasskeyError>;
}

/// Generic authenticator that defers to a host-provided async closure.
/// Mirrors upstream `CallbackAuthenticator` (in
/// `src/passkey/mod.rs:163-185`): the closure takes **owned**
/// `AssertionRequest` (it `.clone()`s internally for sync), and the
/// supertrait bound on the closure is `wacore::sync_marker::MaybeSendSync`.
#[derive(Clone)]
pub struct CallbackAuthenticator {
    cb: Arc<
        dyn Fn(AssertionRequest) -> AssertionFuture
            + wacore::sync_marker::MaybeSendSync
            + 'static,
    >,
}

impl CallbackAuthenticator {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(AssertionRequest) -> AssertionFuture
            + wacore::sync_marker::MaybeSendSync
            + 'static,
    {
        Self { cb: Arc::new(f) }
    }
}

#[async_trait]
impl PasskeyAuthenticator for CallbackAuthenticator {
    async fn get_assertion(
        &self,
        request: &AssertionRequest,
    ) -> Result<Assertion, PasskeyError> {
        (self.cb)(request.clone()).await
    }
}
```

**Step 2: Plumb `Option<Arc<dyn PasskeyAuthenticator>>` through `WhatsAppConfig`**

`WhatsAppConfig` lives at `crates/octo-adapter-whatsapp/src/config.rs`. Add the field:

```rust
use crate::passkey::PasskeyAuthenticator;
use std::sync::Arc;

pub struct WhatsAppConfig {
    // ... existing fields ...
    pub passkey_authenticator: Option<Arc<dyn PasskeyAuthenticator>>,
}
```

Default constructor (search for `impl Default for WhatsAppConfig` or `WhatsAppConfig::new`): add `passkey_authenticator: None`. Also update any `WhatsAppConfig { ... }` struct-literal call sites in tests/fixtures (use `..Default::default()` if the test already does so; otherwise pass `None` explicitly).

**Step 3: Wire the authenticator post-build via `Client::set_passkey_authenticator`**

`BotBuilder` has **no** `with_passkey_authenticator` method. `grep "passkey\|Passkey" wacore/src/bot.rs` returns nothing. The authenticator is set post-build on the `Client` (in `src/passkey/flow.rs:386`):

```rust
pub async fn set_passkey_authenticator(
    &self,
    authenticator: Arc<dyn PasskeyAuthenticator>,
) {
    self.passkey_state.lock().await.authenticator = Some(authenticator);
}
```

Update `start_bot()` in `crates/octo-adapter-whatsapp/src/adapter.rs` to call it between `builder.build()` and `bot.run()`. The current flow is:

```rust
let mut bot = builder.build().await?;
*self.client.lock() = Some(bot.client());
let bot_handle = bot.run().await?;
```

Replace with:

```rust
let mut bot = builder.build().await?;

// SHORTCAKE_PASSKEY: if a PasskeyAuthenticator is registered, install it on
// the Client BEFORE the WebSocket run loop starts. The SDK consumes the
// authenticator synchronously on the first <notification
// type=passkey_prologue_request> arrival — if we install after `bot.run()`
// returns its handle, the request may already be in flight.
if let Some(auth) = self.config.passkey_authenticator.clone() {
    bot.client().set_passkey_authenticator(auth).await;
}

*self.client.lock() = Some(bot.client());
let bot_handle = bot.run().await?;
```

If `bot.client()` does not return an `Arc<Client>` (e.g., it returns a `ClientHandle` or similar), substitute the correct getter. Confirm by `grep -n "pub fn client\|pub client" src/bot.rs` after the pin bump. The downstream consumption site is `octo-whatsapp/src/daemon.rs` via the existing `subscribe_raw_events()` path, which already forwards the broadcast `Event::PairPasskeyRequest` (Session 3).

**Step 4: Run adapter tests + full lib test suite**

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test --lib -p octo-adapter-whatsapp --all-features 2>&1 | tail -10
cargo test --lib -p octo-whatsapp --features "live-whatsapp test-helpers" 2>&1 | tail -10
```

Expected: same tests as before, plus the new `passkey::assertion::tests::parses_request_options_minimal` test passing.

**Step 5: Commit Session 2**

```bash
git add -A
git commit -m "feat(octo-adapter-whatsapp): PasskeyAuthenticator trait + CallbackAuthenticator (Session 2 of wacore-webauthn plan)"
git tag session-2-passkey-trait-baseline HEAD
```

### Session 2 verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
```

---

## Session 3 — Surface `Event::PairPasskeyRequest` / `Confirmation` / `Error`

**Goal:** Make the three SHORTCAKE_PASSKEY events visible to downstream code (the daemon's connection watcher). End state: the adapter broadcasts `Event::PairPasskeyRequest { request_options_json: String }` through the same channel as the other lifecycle events.

**Risk:** Low. We're forwarding events that already exist upstream; no schema invention. Test by creating a synthetic `Event::PairPasskeyRequest` and asserting the adapter's broadcast channel delivers the Debug-string `"Event::PairPasskeyRequest"` (or whatever upstream Debug produces).

### Task 3.1: Confirm upstream Debug output for the 3 events

**Step 1: Read upstream `Event` Debug impl**

```bash
# Use ctx_search on the indexed wacore-events-main source
# to find the exact Debug format. Expected shape (verify):
#   Event::PairPasskeyRequest(PairPasskeyRequest { request_options_json: "..." })
#   Event::PairPasskeyConfirmation(PairPasskeyConfirmation { code: "ABCDEFGH", skip_handoff_ux: false })
#   Event::PairPasskeyError(PairPasskeyError { error: "user_cancelled", continuation: false })
```

If the Debug shape includes `Event::` prefix or not — same caveat as the existing classifier already handles (`Event::` prefix is optional; `strip_prefix("Event::").unwrap_or(raw)`).

### Task 3.2: Verify the events flow through the broadcast channel

**Files:**
- Modify: `crates/octo-adapter-whatsapp/src/event_bus.rs` (or whichever file holds the `broadcast::Sender<String>` for `subscribe_raw_events`).

**Step 1: Write a hermetic test that confirms the events arrive in the channel**

In `crates/octo-adapter-whatsapp/src/lib.rs` (or a `#[cfg(test)] mod tests` block in `event_bus.rs`), add:

```rust
#[tokio::test]
async fn broadcast_forwards_pair_passkey_request_event() {
    use crate::event_bus::subscribe_for_tests;
    let (tx, mut rx) = subscribe_for_tests();

    // Build a synthetic PairPasskeyRequest.
    let evt = wacore::types::events::Event::PairPasskeyRequest(
        wacore::types::events::PairPasskeyRequest::builder()
            .request_options_json(r#"{"challenge":"AA","rpId":"web.whatsapp.com"}"#.to_string())
            .build(),
    );

    tx.send(format!("{evt:?}")).unwrap();

    let raw = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("no timeout")
        .expect("channel recv");

    assert!(raw.contains("PairPasskeyRequest"), "raw: {raw}");
    assert!(raw.contains("\"challenge\":\"AA\""), "options leak: {raw}");
}
```

(Adjust the builder names to match upstream's `bon::Builder` conventions; confirm by reading `wacore/src/types/events.rs` on the pinned commit.)

**Step 2: Run — expect green if the existing `subscribe_raw_events` already serialises via `format!("{:?}", evt)`**

```bash
cargo test -p octo-adapter-whatsapp --lib pair_passkey 2>&1 | tail -10
```

If it fails because the builder shape is wrong: `cargo doc --open -p wacore` and inspect `PairPasskeyRequest::builder()` for exact setter names.

**Step 3: Real-event emission from the WA bot**

The WA bot already routes all `Event` variants through a handler in `crates/octo-adapter-whatsapp/src/adapter.rs` around line 1215 (`Event::PairingQrCode { code, .. } => ...` cluster). Confirm the match arm `Event::PairPasskeyRequest(_)` already gets the event and serialises it through the channel — if it doesn't, add it (mirroring the `Event::QrScannedWithoutMultidevice` placeholder arm):

```rust
Event::PairPasskeyRequest(req) => {
    let raw = format!("{req:?}");
    tracing::info!(event = "Event::PairPasskeyRequest", request_options_json = %req.request_options_json, "SHORTCAKE_PASSKEY: server requested WebAuthn assertion");
    let _ = self.event_tx.send(raw);
}
```

Repeat for `PairPasskeyConfirmation` and `PairPasskeyError`. Each gets a `tracing::info!` diagnostic and a broadcast send.

**Step 4: Run the integration test**

```bash
cargo test -p octo-adapter-whatsapp --lib 2>&1 | tail -15
```

**Step 5: Commit Session 3**

```bash
git add -A
git commit -m "feat(octo-adapter-whatsapp): forward SHORTCAKE_PASSKEY events to connection-watcher broadcast"
git tag session-3-passkey-events-baseline HEAD
```

### Session 3 verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
```

---

## Session 4 — `BotStateMirror::AwaitingPasskey` + QR rendering + classifier arm

**Goal:** Operators see a typed bot state (`AwaitingPasskey`) when the server asks for a WebAuthn assertion, and the daemon renders the `request_options_json` as a scannable QR on the operator's terminal. End state: a hermetic test confirms the classifier arm maps `Event::PairPasskeyRequest` → `AwaitingPasskey` and the status.get handler surfaces `bot_state_hint`.

**Risk:** Low. Pure surface plumbing.

### Task 4.1: Add the variant + encoder

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs` — `BotStateMirror` enum + `encode_bot_state` / `decode_bot_state` / `bot_state_label` family.

**Step 1: Add the variant**

```rust
pub enum BotStateMirror {
    Disconnected,
    PairingQr,
    PairingCode,
    Connected,
    Replaced,
    LoggedOut,
    SessionExpired,
    AwaitingUserAction,                                            // u8 = 7 (already added Session 6.12.5)
    /// Server requested a WebAuthn assertion (SHORTCAKE_PASSKEY).
    /// Operator must scan the displayed QR with their phone WA app
    /// (the phone's authenticator completes the assertion) or the
    /// daemon must drive `PasskeyAuthenticator::get_assertion`.
    AwaitingPasskey,                                                 // u8 = 8
}
```

Encode/decode arms for u8=8. Add to `bot_state_label` table.

**Step 2: Update `rpc_for_bot_state` (handler utility)**

In `crates/octo-whatsapp/src/ipc/handlers/util.rs`, add a match arm:

```rust
BotStateMirror::AwaitingPasskey => RpcErrorCode::NotConnected,
```

(Even though it's not a "session lost" state, senders shouldn't issue sends while a passkey assertion is pending.)

### Task 4.2: Add the hint constant

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs`

```rust
pub const AWAITING_PASSKEY_HINT: &str = "\
server requested WebAuthn assertion (SHORTCAKE_PASSKEY): \
scan the QR displayed in the CLI/daemon logs with your phone's \
WhatsApp app to complete the link";
```

### Task 4.3: Extend `classify_event`

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon.rs` — `classify_event` function.

```rust
match ident {
    // ... existing arms
    "PairPasskeyRequest" => Some((BotStateMirror::AwaitingPasskey, false)),
    "PairPasskeyConfirmation" => Some((BotStateMirror::AwaitingPasskey, false)),
    "PairPasskeyError" => Some((BotStateMirror::LoggedOut, true)),
    // ... rest unchanged
}
```

Rationale for `PairPasskeyConfirmation`: link is in the final verification stage — daemon still waits. `PairPasskeyError`: terminal failure, advance to `LoggedOut`.

### Task 4.4: Extend `status.get` to surface the hint

In `crates/octo-whatsapp/src/ipc/handlers/status.rs`:

```rust
let bot_state_hint: Option<&'static str> = match bot_state {
    BotStateMirror::AwaitingUserAction => Some(crate::daemon::AWAITING_USER_ACTION_HINT),
    BotStateMirror::AwaitingPasskey => Some(crate::daemon::AWAITING_PASSKEY_HINT),
    _ => None,
};
```

### Task 4.5: Hermetic test

**Files:**
- Modify: `crates/octo-whatsapp/src/daemon/tests.rs`

```rust
#[tokio::test(flavor = "multi_thread")]
async fn pair_passkey_request_event_marks_awaiting_passkey() {
    let (tx, handle, _tmp) = spawn_watcher().await;
    tx.send(
        r#"Event::PairPasskeyRequest(PairPasskeyRequest { \
           request_options_json: "{\"challenge\":\"abc\"}" })"#.to_string(),
    ).expect("send");

    tokio::time::sleep(TEST_STALL).await;

    assert_eq!(handle.bot_state(), BotStateMirror::AwaitingPasskey);

    let status = handle.status_snapshot();
    assert_eq!(status["bot_state"], "AwaitingPasskey");
    assert!(status["bot_state_hint"].as_str().unwrap()
        .contains("SHORTCAKE_PASSKEY"), "hint missing in {:?}", status);

    handle.cancel_token().cancel();
}
```

Run:

```bash
cargo test --lib -p octo-whatsapp --features "live-whatsapp test-helpers" pair_passkey 2>&1 | tail -10
```

If the test fails on the exact Debug string format, run with `-- --nocapture` and copy the actual Debug output into the test fixture.

### Task 4.6: Render the QR

**Files:**
- Modify: `crates/octo-whatsapp-onboard-core/src/qr_link.rs` and `pair_link.rs` — add a `passkey_qr` rendering call when the adapter emits `Event::PairPasskeyRequest`.

The CLI binary receives the event via `subscribe_raw_events`; pattern-match on the Debug string. When `PairPasskeyRequest` arrives, render a terminal QR using the existing `qrcode::QrCode::new(...).render::<Dense1x2>()` chain (already in tree from the WA adapter's `Event::PairingQrCode` arm):

```rust
let payload = &raw_options_json;  // the PublicKeyCredentialRequestOptions JSON
match qrcode::QrCode::new(payload.as_bytes()) {
    Ok(qr) => {
        let rendered = qr.render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true).build();
        eprintln!("\nWhatsApp passkey request (scan with your phone WA app):\n{rendered}\n");
    }
    Err(e) => eprintln!("\nWhatsApp passkey request (could not render QR: {e}):\n{raw_options_json}\n"),
}
```

Also surface via `daemon::status.get`: include `passkey_request_options_json` (sans nesting) when `bot_state == AwaitingPasskey`.

### Task 4.7: Tests for QR + CLI hint

**Files:**
- Modify: `crates/octo-whatsapp-onboard-core/src/qr_link.rs` (or a new `passkey.rs` helper) — render test that confirms a known `request_options_json` produces a deterministic ASCII QR.

### Task 4.8: Commit Session 4

```bash
git add -A
git commit -m "feat(octo-whatsapp): AwaitingPasskey state + classifier + QR rendering"
git tag session-4-passkey-state-baseline HEAD
```

### Session 4 verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
# expect: 659+ lib tests pass + new pair_passkey_* tests pass
```

---

## Session 5 — (Optional) webauthn-authenticator-rs driven authenticator

**Goal:** Real WebAuthn assertion in-Rust, no phone-side coordination. Operator exports their WA-linked passkey from a vault to a config dir; daemon signs assertions locally.

**Risk:** Medium. Real crypto. Wrong assertion → ban risk or stuck account. Sessions 1-4 must be solid before attempting. Defer if Sessions 1-4 surfaces regressions.

### Task 5.1: Add `webauthn-authenticator-rs`

**Files:**
- Modify: `crates/octo-adapter-whatsapp/Cargo.toml` — add `webauthn-authenticator-rs = "0.5.5"`.

**Step 1: Verify the crate compiles in our tree**

```bash
cargo check -p octo-adapter-whatsapp 2>&1 | tail -10
```

### Task 5.2: Implement the driven authenticator

**Files:**
- Create: `crates/octo-adapter-whatsapp/src/passkey/webauthn_driven.rs`

```rust
pub struct WebauthnDrivenAuthenticator {
    vault: WebAuthnVault,   // wraps webauthn-authenticator-rs Authenticator + per-cred id
}
```

The authenticator holds a credential registered to the WA RP (`web.whatsapp.com`). On `get_assertion`, it produces a signed assertion matching the server's challenge.

### Task 5.3: Plumb through `WhatsAppConfig`

Add `pub passkey_vault_path: Option<PathBuf>` to `WhatsAppConfig`. When set, build a `WebauthnDrivenAuthenticator` from the vault file and pass it as `passkey_authenticator` to the builder.

### Task 5.4: Operator docs

**Files:**
- Create: `docs/operations/SHORTCAKE_PASSKEY.md` — operator-facing instructions for exporting a WA passkey to a vault the daemon can load.

### Task 5.5: Live chain verification

Re-run the live chain integration tests against a passkey-gated account:

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test --test live_daemon_test -p octo-whatsapp --features "live-whatsapp test-helpers" \
  -- --include-ignored --nocapture --test-threads=1
# If a passkey-gated account test doesn't exist yet, add `live_chain_k_passkey` to
# tests/live_daemon_test.rs — see docs/plans/2026-07-04-...-phase6.0.md for the
# pattern; the chain should: scan QR → phone prompts → server sends PairPasskeyRequest
# → daemon renders QR → operator scans phone WA app → server sends Connected.
```

### Task 5.6: Commit + tag Session 5

```bash
git add -A
git commit -m "feat(octo-adapter-whatsapp): webauthn-authenticator-rs driven authenticator"
git tag session-5-webauthn-rs-baseline HEAD
```

### Session 5 verification

Same four commands as previous sessions, plus:

```bash
cargo build --release -p octo-whatsapp octo-whatsapp-onboard --features "live-whatsapp test-helpers"
# release build must succeed (proves no leftover cfg(test) leak)
```

---

## Live-chain re-verification (mandatory after each session ≥ 4)

Live chains live behind `--features live-whatsapp test-helpers` and `--include-ignored`. They need a real WhatsApp session DB and a real phone-side auth. After Sessions 1-3, run:

```bash
cd /home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp
cargo test -p octo-whatsapp --features "live-whatsapp test-helpers" \
  --test live_daemon_test \
  -- --include-ignored --nocapture --test-threads=1 \
  live_chain_a_lifecycle live_chain_h_daemon_control 2>&1 | tail -60
```

After Session 4, run all 10 live chains. After Session 5, add `live_chain_k_passkey` and run it.

If a live chain fails after the migration: do NOT patch the test assertion to make it pass. Read the failure, identify the production regression, fix it, re-run. The chain is the contract.

---

## Out-of-scope (deliberate)

These belong to follow-up plans once Sessions 1-5 are green:

- **caBLE hybrid tunnel** (BLE/USB transport from daemon to phone for second authenticator scenarios where the passkey isn't on a software vault).
- **Per-rule policy on passkey assertion timeouts** (currently the daemon sits on `AwaitingPasskey` indefinitely; an auto-revoke after 5min could be added).
- **Multi-credential support** (server's `allowCredentials` may carry >1 entry; the current trait asserts the first matching credential; selection policy is upstream's).
- **`skip_handoff_ux: true` fast-path audit** — verify the re-link handoff proof is wired in our drive flow, since the server suppresses the visual-code UI when the proof is valid.
- **Centralised `WebAuthn` event hooks for the rules engine** — let operators write rules like `on passkey_request → notify webhook`.

---

## YAGNI guard rails

- No new RPC methods (`daemon.passkey.submit` etc.). The daemon should drive assertions internally when an authenticator is registered; operators don't need an external surface for v1.
- No changes to the existing `AwaitingUserAction` heuristic. They are complementary, not alternatives.
- No simulation/mocking of the upstream server flow — the wacore integration is exercised in real chains against the live test fixtures.
- No new dependencies beyond `webauthn-authenticator-rs = "0.5.5"` (Session 5) and `base64` (already transitive).
- No `cargo update` in this plan. Each session binds to the upstream commit we already pin.

---

## Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| buffa migration introduces an API we can't pin down remotely | Medium | 1-2 extra days | Task 1.5 step 4: run test suite immediately to surface unknown error shapes. Defer the unknown with `todo!()` and resolve in a hot-fix commit, not in this plan. |
| `Device` becoming sync breaks downstream `Arc<Device>` flows | Medium | adapter won't compile | Step 1.4 step 4: introduce `tokio::task::spawn_blocking` only if blocking is genuinely needed. Otherwise just remove `.await`. |
| `Event::Messages` field shape differs from `Event::Message` | Low | adapter won't compile | Task 3.1: read upstream definition first; only proceed after the shape is known. |
| Live chain regression after migration | Medium | breaks phase 6.0 + 6.1 contracts | Re-verification gate at the end of Session 1 + Session 5. If a live chain fails, fix the production code (not the test). |
| Passkey assertion signature wrong | High if attempted early | account ban | Don't ship Session 5 without an isolated test account + a vault with a non-WA-correlated passkey first. |
| Upstream main moves during multi-session execution | Medium | Cargo.lock drift | Pin to specific SHA `6e0f241d` (not `main`). Re-pin before any session if the pin SHA is known-good. |

---

## Acceptance criteria

Done = all of the below green:

1. `cargo check --workspace --all-targets --all-features` returns 0 errors.
2. `cargo test --workspace --lib` passes (≥ 659 lib tests + new pair_passkey_* tests).
3. `cargo fmt --check` clean.
4. `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
5. Live chain re-verification: all 10 existing chains green; `live_chain_k_passkey` (new) green against a real passkey-gated account (or skipped if Session 5 not attempted).
6. `Event::PairPasskeyRequest` reaches the daemon as a typed event; `bot_state=AwaitingPasskey` surfaces in `status.get` with a hint; the CLI renders a scannable QR.
7. No push to `feat/whatsapp-runtime-cli-mcp` (per standing rule); all 5 session commits local-only, optionally tagged with `session-N-...-baseline`.
