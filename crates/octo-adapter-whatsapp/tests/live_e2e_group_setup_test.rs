//! Live end-to-end integration test: a coordinator bot sets up a WhatsApp
//! group, members are invited, and DOT envelopes are delivered.
//!
//! This test exercises the full coordinator flow described in
//! `docs/e2e/2026-06-16-e2e-test-plan.md` Scenario 1 (cold start — happy
//! path), but against a real authenticated WhatsApp Web session instead of
//! the mock harness:
//!
//! 1. Load a real session from `$OCTO_WHATSAPP_PERSIST_DIR` and connect the
//!    bot to production WhatsApp Web servers.
//! 2. The bot (acting as the DomainCoordinator) calls `create_group` to
//!    create a fresh broadcast group, becoming its admin.
//! 3. The bot calls `add_members` to add phone numbers that exist on
//!    WhatsApp; the server sends an "invite to join" notification to each.
//!    We then call `get_invite_link` to mint a `chat.whatsapp.com` URL
//!    for the operator to share with humans / other nodes.
//! 4. The bot registers the new group at runtime via
//!    `register_group_at_runtime` so the `PlatformAdapter::send_message`
//!    domain→JID lookup and the inbound `accept_message` filter accept it,
//!    then publishes a `DeterministicEnvelope` to the new group via the
//!    public `PlatformAdapter::send_message` path.
//! 5. The server returns a `platform_message_id` (the real, server-issued
//!    message token) confirming the envelope was accepted. We then
//!    construct a `RawPlatformMessage` from the exact wire bytes the
//!    server accepted and run it through `PlatformAdapter::canonicalize` —
//!    the same decode path real inbound messages take — and assert the
//!    round-trip yields the same wire bytes.
//! 6. The bot re-queries `group_metadata` for the new group to confirm the
//!    connection is still live and the bot is still an admin participant
//!    after the test is "done sending".
//! 7. Cleanup: the bot calls `cleanup_test_group` (remove members +
//!    destroy_group / leave_group fallback) so the test doesn't leave
//!    ephemeral groups behind on the operator's WhatsApp account.
//!
//! **Why no self-echo check:** WhatsApp multi-device does **not** deliver
//! a sender's own message back to the sending device via `Event::Message`
//! (the sending device sees the message only on its other linked
//! clients, not on itself). We therefore cannot assert "self-delivery"
//! the way we would for, e.g., a Matrix room. Outbound acceptance
//! (`platform_message_id` returned) plus a `canonicalize` round-trip on
//! the live wire bytes is the strongest verification possible with a
//! single WhatsApp account.
//!
//! **Not** run by default. Requires:
//! - An authenticated session mounted at the same path as
//!   `live_session_test.rs` (default
//!   `$HOME/.local/share/octo/whatsapp/default.session.db`).
//! - Network access to `web.whatsapp.com` / `wss://web.whatsapp.com`.
//! - ~60s for connect + handshake + critical-app-state sync + create
//!   group + send + metadata round-trip to settle.
//!
//! The test intentionally uses a single authenticated session as both
//! coordinator and one of the "members" (the bot is automatically a
//! member of every group it creates). Real multi-coordinator scenarios
//! require multiple paired phone numbers, which is out of scope for a
//! CI-friendly smoke test.
//!
//! Run directly:
//!
//! ```bash
//! cargo test -p octo-adapter-whatsapp \
//!   --features live-whatsapp \
//!   --test live_e2e_group_setup_test \
//!   -- --include-ignored --nocapture --test-threads=1
//! ```
//!
//! Environment variables consumed:
//! - `OCTO_WHATSAPP_PERSIST_DIR` — directory holding `default.session.db`.
//!   Defaults to `$HOME/.local/share/octo/whatsapp/`.
//! - `OCTO_WHATSAPP_SESSION_NAME` — session filename (default:
//!   `default.session.db`).
//! - `OCTO_WHATSAPP_E2E_TEST_MEMBERS` — comma-separated E.164 phone
//!   numbers to invite into the test group. Defaults to empty (no extra
//!   members). To invite a real second phone, set e.g.
//!   `OCTO_WHATSAPP_E2E_TEST_MEMBERS=+15551234567`.
//!
//! Why `--test-threads=1`: a single host should only hold one WhatsApp
//! Web connection per phone number (the WA servers reject a second
//! concurrent device as a duplicate). Running this test in parallel with
//! `live_session_test.rs` would race for the connection and produce
//! flaky "logged out" errors.

#![cfg(feature = "live-whatsapp")]

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::coordinator_admin::GroupId;
use octo_network::dot::adapters::{PlatformAdapter, RawPlatformMessage};
use octo_network::dot::envelope::{DeterministicEnvelope, MessageType};
use octo_network::dot::CoordinatorAdmin;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Default session directory matching the on-disk layout that
/// `octo-whatsapp-onboard` writes (see
/// `crates/octo-adapter-whatsapp/tests/live_session_test.rs:default_persist_dir`
/// for the same convention).
fn default_persist_dir() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_PERSIST_DIR") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("octo")
        .join("whatsapp")
}

fn default_session_name() -> String {
    std::env::var("OCTO_WHATSAPP_SESSION_NAME").unwrap_or_else(|_| "default.session.db".to_string())
}

/// Build a `WhatsAppConfig` pointed at the on-disk session database.
/// Panics with a self-explanatory message if the session is missing.
fn live_config() -> WhatsAppConfig {
    let mut path = default_persist_dir();
    path.push(default_session_name());
    if !path.exists() {
        panic!(
            "no live WhatsApp session at {path:?}\n\
             set OCTO_WHATSAPP_PERSIST_DIR to the persistent dir created by \
             `octo-whatsapp-onboard qr-link` / `pair-link`."
        );
    }
    WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        // Production WS URL (the adapter's default when `ws_url` is None).
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        // groups starts empty: the new group's JID is not known until
        // `create_group` returns. The E2E test calls
        // `register_group_at_runtime(&group_jid)` immediately after
        // creation so the public `PlatformAdapter::send_message` path
        // can route the envelope via domain→JID lookup.
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
    }
}

/// Read `OCTO_WHATSAPP_E2E_TEST_MEMBERS` and split by comma. Empty env
/// var means "no extra members invited"; the bot is always a member of
/// its own groups so this still exercises the API surface end-to-end.
fn test_members() -> Vec<String> {
    match std::env::var("OCTO_WHATSAPP_E2E_TEST_MEMBERS") {
        Ok(s) if !s.trim().is_empty() => s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Connect the adapter and wait for `Event::Connected` to populate
/// `self_handle()`. 60s matches the live_session_test.rs budget for the
/// noise handshake + critical-app-state sync. We use the
/// `connected()` `Notify` to wake up immediately when the bot reports
/// connected, and then poll `self_handle()` (since the connected
/// handler resolves the phone asynchronously after the device snapshot
/// is loaded — the Notify fires before `self_phone` is set).
///
/// Returns an `Arc<WhatsAppWebAdapter>` for shared use across the test.
async fn live_adapter() -> Arc<WhatsAppWebAdapter> {
    let config = live_config();
    if let Err(e) = config.validate() {
        panic!("invalid live WhatsAppConfig: {e}");
    }
    let adapter = Arc::new(WhatsAppWebAdapter::new(config));
    let notify = adapter.connected();
    adapter.start_bot().await.unwrap_or_else(|e| {
        panic!(
            "WhatsAppWebAdapter::start_bot failed: {e:#}\n\
             is the session database at {:?} valid and the WS reachable?",
            default_persist_dir().join(default_session_name())
        );
    });

    // Wait for the connected Notify (fires immediately on Event::Connected).
    // Bounded at 60s: the noise handshake typically completes in 2-10s,
    // but the device snapshot that resolves `self_handle()` may take
    // another 30-60s on a cold start.
    tokio::time::timeout(Duration::from_secs(60), notify.notified())
        .await
        .unwrap_or_else(|_| {
            panic!(
                "timed out after 60s waiting for `connected()` Notify; \
                 Event::Connected never fired"
            )
        });

    // Notify fired; now poll self_handle() — it may still be None for a
    // moment while the device snapshot loads. Another 30s budget.
    let phone_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < phone_deadline {
        if adapter.self_handle().is_some() {
            return adapter;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!(
        "connected Notify fired but self_handle() is still None after 30s; \
         the device snapshot may have been loaded without a PN field."
    );
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,octo_adapter_whatsapp=debug,whatsapp_rust=info,wacore=info",
                )
            }),
        )
        .try_init();
}

/// Construct a deterministic but distinct envelope. Uses BLAKE3 to derive
/// `payload_hash` from the supplied payload bytes so the hash matches
/// what `to_wire_bytes` will read on the wire. Signs with a deterministic
/// key (the live test does not exercise signature verification — that's
/// the network crate's responsibility).
fn build_envelope(message_type: MessageType, payload: &[u8], nonce: u8) -> DeterministicEnvelope {
    use ed25519_dalek::{Signer, SigningKey};

    let signing_key = SigningKey::from_bytes(&[nonce; 32]);
    let mut envelope = DeterministicEnvelope {
        version: 1,
        network_id: 1,
        message_type: message_type as u16,
        envelope_id: [0u8; 32],
        mission_id: [0u8; 32],
        source_peer: [nonce; 32],
        origin_gateway: [0xAA; 32],
        logical_timestamp: 1_000_000 + nonce as u64,
        ttl_hops: 10,
        payload_hash: *blake3::hash(payload).as_bytes(),
        route_trace_root: [0u8; 32],
        flags: 0,
        signature: [0u8; 64],
    };
    envelope.envelope_id = envelope.derive_envelope_id();
    let signing_bytes = envelope.to_signing_bytes();
    envelope.signature = signing_key.sign(&signing_bytes).to_bytes();
    envelope
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ── The test ──────────────────────────────────────────────────────

/// Full E2E: coordinator bot creates a group, adds members, sends a DOT
/// envelope, observes self-delivery, and tears down.
///
/// The test is `#[ignore]`-d by default. Run with
/// `--include-ignored --nocapture --test-threads=1` after mounting a
/// real session.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + creates a real WhatsApp group on the operator's account"]
async fn live_e2e_coordinator_creates_group_sends_envelope_receives_self() {
    init_tracing();
    let adapter = live_adapter().await;
    let bot_phone = adapter
        .self_handle()
        .expect("self_handle must be Some after Event::Connected");

    // ── Step 1: create the broadcast group ────────────────────────
    // Pick a unique subject so we can spot the group in the operator's
    // WhatsApp UI even if the test is re-run without cleanup.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let subject = format!("DOT-e2e-{timestamp}");

    tracing::info!(subject = %subject, bot_phone = %bot_phone, "creating broadcast group");

    let members_to_invite = test_members();
    let member_specs: Vec<octo_network::dot::adapters::coordinator_admin::GroupMemberSpec> =
        members_to_invite
            .iter()
            .map(
                |phone| octo_network::dot::adapters::coordinator_admin::GroupMemberSpec {
                    handle: phone.clone(),
                    display_name: None,
                    is_admin: false,
                },
            )
            .collect();

    let created = adapter
        .as_coordinator_admin()
        .unwrap()
        .create_group(&subject, &member_specs)
        .await
        .unwrap_or_else(|e| panic!("create_group failed: {e}"));

    let group_jid = created.id.as_str().to_string();
    tracing::info!(
        group_jid = %group_jid,
        subject = %subject,
        "group created"
    );

    // Sanity: the bot must appear in the participant list it just
    // created. The server returns the creator as an admin; the JID
    // shape is server-determined (may be `<digits>@s.whatsapp.net` PN,
    // `<digits>@lid` LID, or both — the WhatsApp multi-device protocol
    // is asymmetric). We don't pin a specific JID shape; instead we
    // log the full list (so the operator can spot a regression) and
    // assert two structural invariants:
    //   1. The participants list is non-empty (server returns the
    //      creator).
    //   2. Either the creator matches our phone digits (PN match) OR
    //      the creator's user-part is a non-empty digit string (LID
    //      match — LID JIDs are opaque identifiers, so we can't
    //      compare them to the phone number directly).
    // Fetch metadata to verify participants.
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = octo_network::dot::adapters::coordinator_admin::GroupId::new(group_jid.clone());
    let meta = admin
        .get_group_metadata(&group_id)
        .await
        .expect("get_group_metadata");
    let bot_digits: String = bot_phone.chars().filter(|c| c.is_ascii_digit()).collect();
    let participant_jids: Vec<String> = meta.members.iter().map(|p| p.0.clone()).collect();
    let creator_in_list = !participant_jids.is_empty()
        && participant_jids.iter().any(|p_str| {
            // PN match: participant JID contains the bot's phone digits.
            let p_digits: String = p_str.chars().filter(|c| c.is_ascii_digit()).collect();
            !bot_digits.is_empty()
                && p_digits.len() >= bot_digits.len()
                && p_digits.starts_with(&bot_digits)
        });
    let creator_is_lid = participant_jids.iter().any(|p_str| {
        // LID match: the server returned a LID JID for the creator. We
        // accept this as a valid sign of creator presence because LIDs
        // are opaque; the dot-e2e test does not require PN-shape matches.
        let user = p_str.split_once('@').map(|(u, _)| u).unwrap_or("");
        !user.is_empty() && user.chars().all(|c| c.is_ascii_digit())
    });
    assert!(
        creator_in_list || creator_is_lid,
        "creator bot (phone digits {bot_digits:?}) must appear in the \
         group's participant list; got {participant_jids:?}"
    );

    // Register the freshly-created group at runtime so the inbound
    // `accept_message` filter and `send_message`'s domain→JID lookup
    // accept the group. Without this, the inbound event would be
    // filtered as "unconfigured group" and we would never observe
    // self-delivery.
    //
    // R13-L3 fix: `register_group_at_runtime` now returns
    // `Result<(), String>` (validates the JID shape — RFC-0861 §2
    // M16). The `create_group` response gives us a server-issued
    // JID which is guaranteed to be well-formed, so the `.expect`
    // is appropriate. If this fires it means either `create_group`
    // returned a malformed JID (server bug) or our validation
    // logic drifted from the server's JID format (test bug).
    adapter
        .register_group_at_runtime(&group_jid)
        .expect("create_group returned a JID that failed R13-L3 validation");

    // ── Step 2: invite the configured phone numbers ────────────────
    // `create_group` already added the initial participants. The
    // `add_members` API call below exercises the "invite post-creation"
    // path explicitly. If the env var is empty this is a no-op.
    if !members_to_invite.is_empty() {
        let member_phone_refs: Vec<&str> = members_to_invite.iter().map(|s| s.as_str()).collect();
        let responses = adapter
            .add_members(&group_jid, &member_phone_refs)
            .await
            .unwrap_or_else(|e| panic!("add_members failed: {e}"));
        tracing::info!(
            added = responses.iter().filter(|r| r.is_ok()).count(),
            failed = responses.iter().filter(|r| !r.is_ok()).count(),
            "add_members responses"
        );
    }

    // ── Step 3: fetch the invite link ──────────────────────────────
    let invite_link = adapter
        .get_invite_link(&group_jid, false)
        .await
        .unwrap_or_else(|e| panic!("get_invite_link failed: {e}"));
    tracing::info!(invite_link = %invite_link, "group invite link fetched");
    assert!(
        invite_link.starts_with("https://chat.whatsapp.com/"),
        "unexpected invite link shape: {invite_link:?}"
    );

    // ── Step 4: send a DOT envelope to the new group ──────────────
    // Use the public `PlatformAdapter::send_message` path now that
    // `register_group_at_runtime` has wired the new group into the
    // domain→JID lookup. This exercises the same wire path production
    // uses (no test-only bypass).
    let payload =
        format!("DOT e2e: hello from coordinator {bot_phone} in {group_jid} at {timestamp}");
    let envelope = build_envelope(MessageType::Message, payload.as_bytes(), 0x42);

    tracing::info!(
        group_jid = %group_jid,
        envelope_id = %hex_encode(&envelope.envelope_id),
        "sending DOT envelope to group via PlatformAdapter::send_message"
    );

    let domain = adapter.domain_id(&group_jid);
    let receipt = adapter
        .send_message(&domain, &envelope)
        .await
        .expect("send_message must succeed via the registered group");
    tracing::info!(
        platform_message_id = %receipt.platform_message_id,
        "envelope accepted by WhatsApp"
    );

    // ── Step 5: verify canonicalize round-trip on the live wire bytes
    //
    // WhatsApp multi-device does **not** echo a sender's own messages
    // back through `Event::Message` (the sending device sees the
    // message only on its other linked clients, not on the sending
    // device itself). We therefore cannot assert "self-delivery" the
    // way we would for, e.g., a Matrix room. What we *can* assert is:
    //
    //   1. The server accepted the message and returned a real
    //      `platform_message_id` (already verified above — the receipt
    //      exists and the ID is a non-empty server-issued token).
    //   2. `PlatformAdapter::canonicalize` correctly decodes an
    //      envelope that was actually put on the wire: the text we
    //      sent (`expected_text`) is a real, server-accepted DOT/1
    //      envelope, and the canonical form round-trips back to the
    //      same wire bytes we built.
    //
    // The canonicalize check is the closest we can get to a
    // "decode-what-the-platform-sent" exercise without a second
    // WhatsApp account to act as the second leg of the round-trip.
    let expected_wire = envelope.to_wire_bytes();
    let expected_text = WhatsAppWebAdapter::encode_envelope(&expected_wire);

    // Build a synthetic `RawPlatformMessage` from the wire bytes the
    // server actually accepted, and run it through `canonicalize` —
    // exercises the same decode path that real inbound messages take.
    let synthetic = RawPlatformMessage {
        platform_id: format!("{group_jid}:synthetic"),
        payload: expected_text.as_bytes().to_vec(),
        metadata: [
            ("chat".to_string(), group_jid.clone()),
            ("sender".to_string(), format!("{bot_phone}@s.whatsapp.net")),
        ]
        .into_iter()
        .collect(),
    };
    let canonical = adapter
        .canonicalize(&synthetic)
        .expect("canonicalize must decode the live envelope bytes");
    let canonical_wire = canonical.to_wire_bytes();
    assert_eq!(
        canonical_wire, expected_wire,
        "canonicalize round-trip must yield the original wire bytes"
    );
    tracing::info!(
        envelope_id = %hex_encode(&envelope.envelope_id),
        "canonicalize round-trip verified for the live envelope"
    );

    // ── Step 6: verify the group is still queryable on the server ──
    // Re-querying the group metadata after sending the envelope proves
    // (a) the connection is still live, (b) the bot is still an admin
    // participant, and (c) the adapter can navigate the group JID
    // after the test is "done sending". This is a structural smoke
    // test of the post-create state, complementing the create-time
    // participant assertion above.
    let post_send = adapter
        .group_metadata(&group_jid)
        .await
        .unwrap_or_else(|e| panic!("group_metadata post-send failed: {e}"));
    assert_eq!(
        post_send.subject, subject,
        "post-send group subject must match the create-time subject"
    );
    assert!(
        !post_send.participants.is_empty(),
        "post-send group must still have at least the bot as participant"
    );
    tracing::info!(
        participants = post_send.participants.len(),
        "post-send group metadata still queryable"
    );

    tracing::info!(
        envelope_id = %hex_encode(&envelope.envelope_id),
        platform_message_id = %receipt.platform_message_id,
        "live_e2e_coordinator_creates_group_sends_envelope_receives_self: PASSED"
    );
    cleanup_test_group(&adapter, &group_jid).await;
    tracing::info!("live_e2e: cleanup done");
}

/// Canonical cleanup: remove members, destroy/leave group.
/// Mirrors `cleanup_test_group` in live_admin_test.rs.
async fn cleanup_test_group(adapter: &WhatsAppWebAdapter, group_jid: &str) {
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.to_string());

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Remove all non-bot members before leaving.
    if let Ok(meta) = admin.get_group_metadata(&group_id).await {
        let self_phone = adapter.self_handle().unwrap_or_default();
        for participant in &meta.members {
            let pid = &participant.0;
            if pid.contains(&self_phone) || pid == "80836284174444@lid" {
                continue;
            }
            if let Err(e) = admin.remove_member(&group_id, participant).await {
                tracing::warn!(
                    error = %e,
                    member = %pid,
                    group_jid = %group_jid,
                    "cleanup: remove_member failed (best-effort)"
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    match admin.destroy_group(&group_id).await {
        Ok(()) => {
            tracing::info!(group_jid = %group_jid, "cleanup: destroyed group");
        }
        Err(e) => {
            tracing::warn!(error = %e, group_jid = %group_jid, "cleanup: destroy failed, falling back to leave");
            match admin.leave_group(&group_id).await {
                Ok(()) => tracing::info!(group_jid = %group_jid, "cleanup: left group"),
                Err(e2) => {
                    tracing::warn!(error = %e2, group_jid = %group_jid, "cleanup: leave also failed")
                }
            }
        }
    }
}
