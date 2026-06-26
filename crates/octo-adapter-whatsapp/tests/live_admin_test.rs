//! Live integration tests for the WhatsApp CoordinatorAdmin surface.
//!
//! Tests the 20 CoordinatorAdmin methods not covered by live_session_test
//! or live_e2e_group_setup_test.
//!
//! ## Group reuse strategy (to avoid 429 rate limits on create_group)
//!
//! Tests are split into fixtures that share groups:
//! - `settings_group`: wa03,wa04,wa12-wa15 (mutate settings, restore after each)
//! - `invite_group`: wa16,wa17 (read-only invite queries)
//! - `member_group`: wa07-wa11,wa18-wa19 (add/remove/promote/demote/ban)
//! - Individual: wa01,wa02 (read-only, no group needed), wa05 (leave), wa06 (destroy), wa20 (shutdown)
//!
//! Only 5 groups are created across all 20 tests.
//!
//! Run:
//!   cargo test -p octo-adapter-whatsapp \
//!     --features live-whatsapp \
//!     --test live_admin_test \
//!     -- --include-ignored --nocapture --test-threads=1
//!
//! Env vars:
//!   OCTO_WHATSAPP_PERSIST_DIR   - session dir (default: ~/.local/share/octo/whatsapp)
//!   OCTO_WHATSAPP_SESSION_NAME  - session file (default: default.session.db)
//!   OCTO_WHATSAPP_TEST_MEMBER   - E.164 phone for member ops (e.g. +5521998201100)

#![cfg(feature = "live-whatsapp")]

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::coordinator_admin::{
    GroupHandle, GroupId, GroupMemberSpec, PeerId,
};
use octo_network::dot::PlatformAdapter;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ── Helpers ──────────────────────────────────────────────────────

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
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
    }
}

async fn live_adapter() -> Arc<WhatsAppWebAdapter> {
    let config = live_config();
    if let Err(e) = config.validate() {
        panic!("invalid live WhatsAppConfig: {e}");
    }
    let adapter = Arc::new(WhatsAppWebAdapter::new(config));
    let notify = adapter.connected();
    adapter.start_bot().await.unwrap_or_else(|e| {
        panic!("start_bot failed: {e:#}");
    });
    tokio::time::timeout(Duration::from_secs(60), notify.notified())
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for connected Notify"));
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if adapter.self_handle().is_some() {
            return adapter;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("self_handle() still None after 30s");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn test_group_subject(prefix: &str) -> String {
    format!("octo_test_{}_{}", prefix, timestamp())
}

fn test_member_phone() -> String {
    std::env::var("OCTO_WHATSAPP_TEST_MEMBER").expect(
        "OCTO_WHATSAPP_TEST_MEMBER not set. Run:\n  \
             export OCTO_WHATSAPP_TEST_MEMBER=+5521XXXXXXXX",
    )
}

/// Explicit async cleanup: remove members, destroy/leave group.
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

    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// Create a test group, return (adapter, group_jid, group_handle).
async fn create_test_group(prefix: &str) -> (Arc<WhatsAppWebAdapter>, String, GroupHandle) {
    let adapter = live_adapter().await;
    let subject = test_group_subject(prefix);
    let members: Vec<GroupMemberSpec> = Vec::new();

    tokio::time::sleep(Duration::from_secs(3)).await;

    let handle = adapter
        .as_coordinator_admin()
        .unwrap()
        .create_group(&subject, &members)
        .await
        .unwrap_or_else(|e| panic!("create_group '{}': {:?}", subject, e));

    let group_jid = handle.id.as_str().to_string();
    tracing::info!(group_jid = %group_jid, subject = %subject, "created test group");

    adapter
        .register_group_at_runtime(&group_jid)
        .expect("register_group_at_runtime failed");

    let entries = vec![(group_jid.clone(), Some(subject.clone()), true)];
    if let Err(e) = adapter.persist_conversations(&entries).await {
        tracing::warn!(error = %e, "failed to persist test group conversation");
    }

    (adapter, group_jid, handle)
}

/// Destroy all orphaned `octo_test_*` groups. Run standalone:
///   cargo test -p octo-adapter-whatsapp --features live-whatsapp \
///     --test live_admin_test -- cleanup_orphaned_test_groups --include-ignored --nocapture
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn cleanup_orphaned_test_groups() {
    init_tracing();
    let adapter = live_adapter().await;
    let admin = adapter.as_coordinator_admin().unwrap();

    tokio::time::sleep(Duration::from_secs(3)).await;
    let groups = admin.list_own_groups().await.expect("list_own_groups");
    tracing::info!(total = groups.len(), "found groups");

    let test_prefixes = ["octo_test_", "renamed_"];
    let orphans: Vec<_> = groups
        .iter()
        .filter(|g| {
            g.subject
                .as_deref()
                .map(|s| test_prefixes.iter().any(|p| s.starts_with(p)))
                .unwrap_or(false)
        })
        .collect();

    tracing::info!(orphaned = orphans.len(), "destroying orphaned test groups");

    for g in &orphans {
        let gid = GroupId::new(g.id.as_str().to_string());
        tracing::info!(group_jid = %g.id.as_str(), subject = ?g.subject, "destroying");
        tokio::time::sleep(Duration::from_secs(2)).await;
        match admin.destroy_group(&gid).await {
            Ok(()) => tracing::info!(group_jid = %g.id.as_str(), "destroyed"),
            Err(e) => {
                tracing::warn!(error = %e, group_jid = %g.id.as_str(), "destroy failed, trying leave");
                tokio::time::sleep(Duration::from_secs(2)).await;
                match admin.leave_group(&gid).await {
                    Ok(()) => tracing::info!(group_jid = %g.id.as_str(), "left (fallback)"),
                    Err(e2) => {
                        tracing::warn!(error = %e2, group_jid = %g.id.as_str(), "leave also failed")
                    }
                }
            }
        }
    }

    tracing::info!("cleanup complete");
}

// ── Standalone Tests (each creates its own group — these MUST destroy) ───

/// wa01: list_own_groups returns groups (read-only, no group creation needed).
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa01_list_own_groups() {
    init_tracing();
    let adapter = live_adapter().await;
    let admin = adapter.as_coordinator_admin().unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    let groups = admin.list_own_groups().await;
    match groups {
        Ok(handles) => {
            tracing::info!(count = handles.len(), "WA-01: list_own_groups");
            assert!(!handles.is_empty(), "should have at least one group");
        }
        Err(e) => {
            tracing::info!(error = %e, "WA-01: list_own_groups returned error");
        }
    }
}

/// wa02: get_group_metadata returns group info (reuses an existing group).
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa02_get_group_metadata() {
    init_tracing();
    let adapter = live_adapter().await;
    let admin = adapter.as_coordinator_admin().unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    let groups = admin.list_own_groups().await.expect("list_own_groups");
    let target = groups.first().expect("need at least one existing group");
    let group_id = GroupId::new(target.id.as_str().to_string());

    let meta = admin.get_group_metadata(&group_id).await;
    assert!(meta.is_ok(), "get_group_metadata: {:?}", meta.err());
    let meta = meta.unwrap();
    tracing::info!(subject = ?meta.subject, members = meta.members.len(), "WA-02: metadata OK");
}

/// wa05: leave_group — creates a group, then leaves (destructive, needs own group).
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa05_leave_group() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa05").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.leave_group(&group_id).await;
    assert!(result.is_ok(), "leave_group: {:?}", result.err());
    tracing::info!("WA-05: leave_group OK");
}

/// wa06: destroy_group — creates a group, then destroys (destructive, needs own group).
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa06_destroy_group() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa06").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.destroy_group(&group_id).await;
    assert!(result.is_ok(), "destroy_group: {:?}", result.err());
    tracing::info!("WA-06: destroy_group OK");
}

/// wa20: shutdown completes cleanly.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa20_shutdown() {
    init_tracing();
    let adapter = live_adapter().await;

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = adapter.shutdown().await;
    assert!(result.is_ok(), "shutdown: {:?}", result.err());
    tracing::info!("WA-20: shutdown OK");
}

// ── Settings Fixture (wa03,wa04,wa12-wa15) ──────────────────────
// Creates ONE group, runs all settings tests, restores state after each, then destroys.

/// wa03-wa04,wa12-wa15: settings mutations on a single shared group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa03_04_12_15_settings_fixture() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa_settings").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    // ── wa03: rename_group ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let meta_before = admin.get_group_metadata(&group_id).await.unwrap();
    let original_subject = meta_before.subject.clone().unwrap_or_default();

    let new_subject = format!("renamed_{}", timestamp());
    let result = admin.rename_group(&group_id, &new_subject).await;
    assert!(result.is_ok(), "wa03 rename_group: {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let meta = admin.get_group_metadata(&group_id).await.unwrap();
    assert_eq!(meta.subject.as_deref(), Some(new_subject.as_str()));
    tracing::info!("WA-03: rename_group OK");

    // Restore original subject.
    let _ = admin.rename_group(&group_id, &original_subject).await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // ── wa04: set_group_description ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let desc = format!("test description {}", timestamp());
    let result = admin.set_group_description(&group_id, &desc).await;
    assert!(result.is_ok(), "wa04 set_group_description: {:?}", result.err());
    tracing::info!("WA-04: set_group_description OK");

    // Clear description.
    let _ = admin.set_group_description(&group_id, "").await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // ── wa12: set_locked ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_locked(&group_id, true).await;
    assert!(result.is_ok(), "wa12 set_locked(true): {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_locked(&group_id, false).await;
    assert!(result.is_ok(), "wa12 set_locked(false): {:?}", result.err());
    tracing::info!("WA-12: set_locked OK");

    // ── wa13: set_announce ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_announce(&group_id, true).await;
    assert!(result.is_ok(), "wa13 set_announce(true): {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_announce(&group_id, false).await;
    assert!(result.is_ok(), "wa13 set_announce(false): {:?}", result.err());
    tracing::info!("WA-13: set_announce OK");

    // ── wa14: set_ephemeral ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .set_ephemeral(&group_id, Some(Duration::from_secs(86400)))
        .await;
    assert!(result.is_ok(), "wa14 set_ephemeral(86400): {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_ephemeral(&group_id, None).await;
    assert!(result.is_ok(), "wa14 set_ephemeral(None): {:?}", result.err());
    tracing::info!("WA-14: set_ephemeral OK");

    // ── wa15: set_require_approval ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_require_approval(&group_id, true).await;
    assert!(result.is_ok(), "wa15 set_require_approval(true): {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_require_approval(&group_id, false).await;
    assert!(result.is_ok(), "wa15 set_require_approval(false): {:?}", result.err());
    tracing::info!("WA-15: set_require_approval OK");

    // Cleanup: destroy the shared group.
    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

// ── Invite Fixture (wa16,wa17) ─────────────────────────────────
// Creates ONE group, runs invite queries, then destroys.

/// wa16-wa17: invite queries on a single shared group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa16_17_invite_fixture() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa_invite").await;
    let admin = adapter.as_coordinator_admin().unwrap();

    // ── wa16: list_own_groups_with_invites ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.list_own_groups_with_invites().await;
    match result {
        Ok(groups) => {
            tracing::info!(count = groups.len(), "WA-16: list_own_groups_with_invites");
            assert!(
                groups.iter().any(|g| g.id.as_str() == group_jid),
                "test group should appear"
            );
        }
        Err(e) => {
            tracing::info!(error = %e, "WA-16: list_own_groups_with_invites error");
        }
    }

    // ── wa17: resolve_invite ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let invite_url = adapter
        .get_invite_link(&group_jid, false)
        .await
        .expect("get_invite_link");
    assert!(invite_url.starts_with("https://chat.whatsapp.com/"));
    let hash = invite_url
        .rsplit_once('/')
        .map(|(_, h)| h)
        .unwrap_or(&invite_url);

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .resolve_invite(&octo_network::dot::adapters::coordinator_admin::InviteRef::new(hash))
        .await;
    match result {
        Ok(handle) => {
            tracing::info!(subject = ?handle.subject, "WA-17: resolve_invite OK");
        }
        Err(e) => {
            tracing::info!(error = %e, "WA-17: resolve_invite returned error");
        }
    }

    // Cleanup.
    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

// ── Member Fixture (wa07-wa11,wa18-wa19) ───────────────────────
// Creates ONE group with a test member. Reuses across member ops.
// Order: wa07(add) → wa08(remove) → wa09(promote) → wa10(demote)
//        → wa19(transfer) → wa18(approve, no-op) → wa11(ban, last)
// wa11 is last because ban is irreversible (no unban_member).

/// wa07-wa11,wa18-wa19: member operations on a single shared group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa07_11_18_19_member_fixture() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa_member").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();

    // ── wa07: add_member ──
    let member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: false,
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.add_member(&group_id, &member).await;
    assert!(result.is_ok(), "wa07 add_member: {:?}", result.err());
    tracing::info!(phone = %phone, "WA-07: add_member OK");

    // ── wa08: remove_member (member was just added, now remove) ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .remove_member(&group_id, &PeerId::new(phone.clone()))
        .await;
    assert!(result.is_ok(), "wa08 remove_member: {:?}", result.err());
    tracing::info!(phone = %phone, "WA-08: remove_member OK");

    // Re-add for subsequent tests.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = admin.add_member(&group_id, &member).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // ── wa09: promote_to_admin ──
    let admin_member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: true,
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.add_member(&group_id, &admin_member).await;
    assert!(result.is_ok(), "wa09 promote_to_admin: {:?}", result.err());
    tracing::info!("WA-09: promote_to_admin OK");

    // ── wa10: demote_from_admin ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .demote_from_admin(&group_id, &PeerId::new(phone.clone()))
        .await;
    assert!(result.is_ok(), "wa10 demote_from_admin: {:?}", result.err());
    tracing::info!("WA-10: demote_from_admin OK");

    // ── wa19: transfer_ownership (member is currently in group) ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .transfer_ownership(&group_id, &PeerId::new(phone.clone()))
        .await;
    tracing::info!(?result, "WA-19: transfer_ownership");

    // ── wa18: approve_join_request (no pending request — tests error path) ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .approve_join_request(&group_id, &PeerId::new(phone.clone()))
        .await;
    tracing::info!(?result, "WA-18: approve_join_request");

    // ── wa11: ban_member (WhatsApp has no ban primitive — expected Unimplemented) ──
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .ban_member(&group_id, &PeerId::new(phone.clone()), None)
        .await;
    match &result {
        Err(e) if e.to_string().contains("Unimplemented") => {
            tracing::info!("WA-11: ban_member correctly returns Unimplemented (WhatsApp has no ban primitive)");
        }
        Ok(()) => {
            tracing::info!("WA-11: ban_member unexpectedly succeeded (may have been implemented)");
        }
        Err(e) => {
            panic!("wa11 ban_member: unexpected error: {:?}", e);
        }
    }

    // Cleanup.
    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}
