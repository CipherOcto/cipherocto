//! Standalone test to verify cleanup (leave + clearChat + deleteChat).
//!
//! Lists existing groups, picks a test group (octo_test_* or media-test_*),
//! runs the full cleanup chain, then the operator verifies on WhatsApp Web
//! that the chat entry is gone.
//!
//! Run:
//!   cargo test -p octo-adapter-whatsapp \
//!     --features live-whatsapp \
//!     --test cleanup_verify_test \
//!     -- --include-ignored --nocapture --test-threads=1

#![cfg(feature = "live-whatsapp")]

use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::coordinator_admin::GroupId;
use octo_network::dot::PlatformAdapter;
use std::time::Duration;

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
    WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: vec![],
        sender_allowlist: Default::default(),
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

/// Step 1: dry-run — list all groups and their subjects.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn dry_run_list_groups() {
    init_tracing();
    let config = live_config();
    let adapter = WhatsAppWebAdapter::new(config);
    let notify = adapter.connected();
    adapter.start_bot().await.expect("start_bot");
    tokio::time::timeout(Duration::from_secs(60), notify.notified())
        .await
        .expect("connect timeout");
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if adapter.self_handle().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let admin = adapter.as_coordinator_admin().unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    let groups = admin.list_own_groups().await.expect("list_own_groups");
    tracing::info!(total = groups.len(), "=== All groups ===");
    for g in &groups {
        let group_id = GroupId::new(g.id.as_str().to_string());
        let subject = match admin.get_group_metadata(&group_id).await {
            Ok(meta) => meta.subject.unwrap_or_default(),
            Err(_) => "<unknown>".to_string(),
        };
        tracing::info!(
            jid = %g.id.as_str(),
            subject = %subject,
            "group"
        );
    }

    adapter.shutdown().await.expect("shutdown");
}

/// Step 2: pick a test group (octo_test_* or media-test_*) and clean it up.
/// The operator must confirm on WhatsApp Web that the chat entry disappeared.
#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn cleanup_test_group_verify() {
    init_tracing();
    let config = live_config();
    let adapter = WhatsAppWebAdapter::new(config);
    let notify = adapter.connected();
    adapter.start_bot().await.expect("start_bot");
    tokio::time::timeout(Duration::from_secs(60), notify.notified())
        .await
        .expect("connect timeout");
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if adapter.self_handle().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let admin = adapter.as_coordinator_admin().unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    let groups = admin.list_own_groups().await.expect("list_own_groups");
    tracing::info!(total = groups.len(), "listing groups");

    // Find a test group.
    let test_prefixes = ["octo_test_", "media-test-", "renamed_", "DOT-e2e-"];

    if groups.is_empty() {
        tracing::info!("no groups found");
        adapter.shutdown().await.expect("shutdown");
        return;
    }

    // Fetch metadata for all groups to find a test group.
    let mut test_group_jid: Option<String> = None;
    for g in &groups {
        let group_id = GroupId::new(g.id.as_str().to_string());
        if let Ok(meta) = admin.get_group_metadata(&group_id).await {
            let subject = meta.subject.unwrap_or_default();
            let is_test = test_prefixes.iter().any(|p| subject.starts_with(p));
            tracing::info!(jid = %g.id.as_str(), subject = %subject, is_test, "group");
            if is_test && test_group_jid.is_none() {
                test_group_jid = Some(g.id.as_str().to_string());
            }
        }
    }

    let group_jid = match test_group_jid {
        Some(j) => j,
        None => {
            tracing::info!("no test groups found to clean up");
            adapter.shutdown().await.expect("shutdown");
            return;
        }
    };

    tracing::info!(group_jid = %group_jid, "=== Cleaning up test group ===");

    let group_id = GroupId::new(group_jid.clone());

    // Remove non-bot members.
    tokio::time::sleep(Duration::from_secs(2)).await;
    if let Ok(meta) = admin.get_group_metadata(&group_id).await {
        let self_phone = adapter.self_handle().unwrap_or_default();
        for p in &meta.members {
            if p.0.contains(&self_phone) || p.0 == "80836284174444@lid" {
                continue;
            }
            tracing::info!(member = %p.0, "removing member");
            match admin.remove_member(&group_id, p).await {
                Ok(()) => tracing::info!(member = %p.0, "removed"),
                Err(e) => tracing::warn!(error = %e, member = %p.0, "remove failed"),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // Leave + clearChat + deleteChat (the destroy_group path).
    tokio::time::sleep(Duration::from_secs(2)).await;
    tracing::info!("destroying group (revoke invite + leave + clearChat + deleteChat)");
    match admin.destroy_group(&group_id).await {
        Ok(()) => {
            tracing::info!("destroy_group returned Ok");
            tracing::info!("=== CHECK WHATSAPP WEB: is the chat entry gone? ===");
        }
        Err(e) => {
            tracing::warn!(error = %e, "destroy_group failed, trying leave_group");
            match admin.leave_group(&group_id).await {
                Ok(()) => {
                    tracing::info!("leave_group returned Ok");
                    tracing::info!("=== CHECK WHATSAPP WEB: is the chat entry gone? ===");
                }
                Err(e2) => {
                    tracing::warn!(error = %e2, "leave_group also failed");
                }
            }
        }
    }

    adapter.shutdown().await.expect("shutdown");
}
