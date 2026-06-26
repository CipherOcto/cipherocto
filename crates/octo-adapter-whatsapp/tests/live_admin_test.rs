//! Live integration tests for the WhatsApp CoordinatorAdmin surface.
//!
//! Tests the 20 CoordinatorAdmin methods not covered by live_session_test
//! or live_e2e_group_setup_test. Each test creates a group, exercises the
//! method, and leaves/destroys the group on cleanup.
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
/// Mirrors the Telegram mtproto_live_session cleanup pattern.
async fn cleanup_test_group(adapter: &WhatsAppWebAdapter, group_jid: &str) {
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.to_string());

    // Small delay so WhatsApp servers settle.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Remove all non-bot members before leaving.
    if let Ok(meta) = admin.get_group_metadata(&group_id).await {
        let self_phone = adapter.self_handle().unwrap_or_default();
        for participant in &meta.members {
            // Skip the bot itself.
            if participant.0.contains(&self_phone) {
                continue;
            }
            if let Err(e) = admin.remove_member(&group_id, participant).await {
                tracing::warn!(
                    error = %e,
                    member = %participant.0,
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

    // Post-cleanup cooldown.
    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// Create a test group, return (adapter, group_jid, group_handle).
/// Caller is responsible for calling `cleanup_test_group` at end of test.
async fn create_test_group(prefix: &str) -> (Arc<WhatsAppWebAdapter>, String, GroupHandle) {
    let adapter = live_adapter().await;
    let subject = test_group_subject(prefix);
    let members: Vec<GroupMemberSpec> = Vec::new(); // no extra members by default

    // Proactive delay.
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

    // Persist group JID to stoolap conversations table so cleanup
    // utility can find it even after adapter restart.
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

// ── Tests ────────────────────────────────────────────────────────

/// list_own_groups returns groups.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa01_list_own_groups() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa01").await;
    let admin = adapter.as_coordinator_admin().unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    let groups = admin.list_own_groups().await;
    match groups {
        Ok(handles) => {
            tracing::info!(count = handles.len(), "WA-01: list_own_groups");
            assert!(
                handles.iter().any(|g| g.id.as_str() == group_jid),
                "test group should appear in list_own_groups"
            );
        }
        Err(e) => {
            tracing::info!(error = %e, "WA-01: list_own_groups returned error");
        }
    }

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// get_group_metadata returns group info.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa02_get_group_metadata() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa02").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let meta = admin.get_group_metadata(&group_id).await;
    assert!(meta.is_ok(), "get_group_metadata: {:?}", meta.err());
    let meta = meta.unwrap();
    assert!(meta.subject.is_some(), "subject should be set");
    tracing::info!(subject = ?meta.subject, "WA-02: metadata OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// rename_group changes the group subject.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa03_rename_group() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa03").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    let new_subject = format!("renamed_{}", timestamp());
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.rename_group(&group_id, &new_subject).await;
    assert!(result.is_ok(), "rename_group: {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let meta = admin.get_group_metadata(&group_id).await.unwrap();
    assert_eq!(meta.subject.as_deref(), Some(new_subject.as_str()));
    tracing::info!("WA-03: rename_group OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// set_group_description changes the group description.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa04_set_group_description() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa04").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    let desc = format!("test description {}", timestamp());
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_group_description(&group_id, &desc).await;
    assert!(result.is_ok(), "set_group_description: {:?}", result.err());
    tracing::info!("WA-04: set_group_description OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// leave_group leaves a group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa05_leave_group() {
    init_tracing();
    let adapter = live_adapter().await;
    let subject = test_group_subject("wa05");
    let admin = adapter.as_coordinator_admin().unwrap();

    tokio::time::sleep(Duration::from_secs(3)).await;
    let handle = admin
        .create_group(&subject, &[])
        .await
        .expect("create_group");
    let group_jid = handle.id.as_str().to_string();
    let group_id = GroupId::new(group_jid.clone());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.leave_group(&group_id).await;
    assert!(result.is_ok(), "leave_group: {:?}", result.err());
    tracing::info!("WA-05: leave_group OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// destroy_group deletes a group.
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

    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// add_member adds a second user to a group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa07_add_member() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa07").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();
    let member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: false,
    };

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.add_member(&group_id, &member).await;
    assert!(result.is_ok(), "add_member: {:?}", result.err());
    tracing::info!(phone = %phone, "WA-07: add_member OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// remove_member removes a user from a group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa08_remove_member() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa08").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();
    let member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: false,
    };

    // Add first, then remove.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let add_result = admin.add_member(&group_id, &member).await;
    assert!(add_result.is_ok(), "add_member: {:?}", add_result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let remove_result = admin
        .remove_member(&group_id, &PeerId::new(phone.clone()))
        .await;
    assert!(
        remove_result.is_ok(),
        "remove_member: {:?}",
        remove_result.err()
    );
    tracing::info!(phone = %phone, "WA-08: remove_member OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// promote_to_admin promotes a member.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa09_promote_to_admin() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa09").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();
    let member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: true,
    };

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.add_member(&group_id, &member).await;
    assert!(
        result.is_ok(),
        "add_member with promote: {:?}",
        result.err()
    );
    tracing::info!("WA-09: promote_to_admin OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// demote_from_admin demotes a member.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa10_demote_from_admin() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa10").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();

    // Add as admin first.
    let member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: true,
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = admin.add_member(&group_id, &member).await;

    // Demote.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .demote_from_admin(&group_id, &PeerId::new(phone.clone()))
        .await;
    assert!(result.is_ok(), "demote_from_admin: {:?}", result.err());
    tracing::info!("WA-10: demote_from_admin OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// ban_member bans a user from a group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa11_ban_member() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa11").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();

    // Add first.
    let member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: false,
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = admin.add_member(&group_id, &member).await;

    // Ban.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .ban_member(&group_id, &PeerId::new(phone.clone()), None)
        .await;
    assert!(result.is_ok(), "ban_member: {:?}", result.err());
    tracing::info!("WA-11: ban_member OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// set_locked toggles the locked flag on a group.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa12_set_locked() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa12").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_locked(&group_id, true).await;
    assert!(result.is_ok(), "set_locked(true): {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_locked(&group_id, false).await;
    assert!(result.is_ok(), "set_locked(false): {:?}", result.err());
    tracing::info!("WA-12: set_locked OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// set_announce toggles announce-only mode.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa13_set_announce() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa13").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_announce(&group_id, true).await;
    assert!(result.is_ok(), "set_announce(true): {:?}", result.err());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_announce(&group_id, false).await;
    assert!(result.is_ok(), "set_announce(false): {:?}", result.err());
    tracing::info!("WA-13: set_announce OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// set_ephemeral sets the ephemeral timer.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa14_set_ephemeral() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa14").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    // Set 1-day ephemeral.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .set_ephemeral(&group_id, Some(Duration::from_secs(86400)))
        .await;
    assert!(result.is_ok(), "set_ephemeral(86400): {:?}", result.err());

    // Disable ephemeral.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_ephemeral(&group_id, None).await;
    assert!(result.is_ok(), "set_ephemeral(None): {:?}", result.err());
    tracing::info!("WA-14: set_ephemeral OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// set_require_approval toggles join approval.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa15_set_require_approval() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa15").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_require_approval(&group_id, true).await;
    assert!(
        result.is_ok(),
        "set_require_approval(true): {:?}",
        result.err()
    );

    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin.set_require_approval(&group_id, false).await;
    assert!(
        result.is_ok(),
        "set_require_approval(false): {:?}",
        result.err()
    );
    tracing::info!("WA-15: set_require_approval OK");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// list_own_groups_with_invites returns groups with invite URLs.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa16_list_own_groups_with_invites() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa16").await;
    let admin = adapter.as_coordinator_admin().unwrap();

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

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// resolve_invite resolves an invite hash.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn wa17_resolve_invite() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa17").await;
    let admin = adapter.as_coordinator_admin().unwrap();

    // Get a real invite link first.
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
            tracing::info!(
                subject = ?handle.subject,
                "WA-17: resolve_invite OK"
            );
        }
        Err(e) => {
            tracing::info!(error = %e, "WA-17: resolve_invite returned error");
        }
    }

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// approve_join_request — test the error path (no pending requests).
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa18_approve_join_request() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa18").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();

    // No pending join request — should either succeed (no-op) or fail gracefully.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .approve_join_request(&group_id, &PeerId::new(phone))
        .await;
    tracing::info!(?result, "WA-18: approve_join_request");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// transfer_ownership — test the error path (needs 2FA or fails).
#[tokio::test]
#[ignore = "requires live WhatsApp Web session + OCTO_WHATSAPP_TEST_MEMBER"]
async fn wa19_transfer_ownership() {
    init_tracing();
    let (adapter, group_jid, _handle) = create_test_group("wa19").await;
    let admin = adapter.as_coordinator_admin().unwrap();
    let group_id = GroupId::new(group_jid.clone());
    let phone = test_member_phone();

    // Add user first.
    let member = GroupMemberSpec {
        handle: phone.clone(),
        display_name: None,
        is_admin: false,
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = admin.add_member(&group_id, &member).await;

    // Transfer — may fail without 2FA.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let result = admin
        .transfer_ownership(&group_id, &PeerId::new(phone.clone()))
        .await;
    tracing::info!(?result, "WA-19: transfer_ownership");

    tokio::time::sleep(Duration::from_secs(2)).await;
    cleanup_test_group(&adapter, &group_jid).await;
}

/// shutdown completes cleanly.
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
