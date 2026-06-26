#![cfg(feature = "live-whatsapp")]

/// Captures events during group creation and destruction to compare
/// with the official app's event flow.
use octo_adapter_whatsapp::{WhatsAppConfig, WhatsAppWebAdapter};
use octo_network::dot::adapters::coordinator_admin::{GroupId, GroupMemberSpec};
use octo_network::dot::PlatformAdapter;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn live_config() -> WhatsAppConfig {
    let mut path =
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()));
    path.push(".local/share/octo/whatsapp/default.session.db");
    WhatsAppConfig {
        session_path: path.to_string_lossy().into_owned(),
        ws_url: None,
        pair_phone: None,
        pair_code: None,
        groups: vec![],
        sender_allowlist: BTreeMap::new(),
    }
}

#[tokio::test]
#[ignore = "requires live WhatsApp Web session"]
async fn capture_cleanup_events() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .try_init();

    let config = live_config();
    let adapter = Arc::new(WhatsAppWebAdapter::new(config));

    // Subscribe BEFORE starting.
    let mut raw_rx = adapter.subscribe_raw_events();
    let connected_notify = adapter.connected();
    let synced_notify = adapter.synced();
    let connected_fut = connected_notify.notified();
    let synced_fut = synced_notify.notified();

    adapter.start_bot().await.expect("start_bot");

    tokio::time::timeout(Duration::from_secs(60), connected_fut)
        .await
        .expect("connected timeout");
    tokio::time::timeout(Duration::from_secs(120), synced_fut)
        .await
        .expect("synced timeout");

    // Wait for self_handle.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if adapter.self_handle().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let admin = adapter.as_coordinator_admin().unwrap();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let subject = format!("event_capture_test_{ts}");

    println!("\n=== Creating group ===");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let handle = admin
        .create_group(&subject, &[])
        .await
        .expect("create_group");
    let group_jid = handle.id.as_str().to_string();
    println!("Created: {group_jid} (subject: {subject})");

    // Drain events during creation.
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("\n--- Events during creation ---");
    while let Ok(desc) = raw_rx.try_recv() {
        if desc.contains(&group_jid) || desc.contains("GroupUpdate") || desc.contains("Create") {
            let short = desc.chars().take(200).collect::<String>();
            println!("  {short}");
        }
    }

    // Now destroy via cleanup (leave + delete_chat).
    println!("\n=== Destroying group via cleanup_test_group ===");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Inline cleanup: remove members, destroy.
    let group_id = GroupId::new(group_jid.clone());
    let self_phone = adapter.self_handle().unwrap_or_default();

    // Remove members.
    if let Ok(meta) = admin.get_group_metadata(&group_id).await {
        for participant in &meta.members {
            if participant.0.contains(&self_phone) || participant.0 == "80836284174444@lid" {
                continue;
            }
            let _ = admin.remove_member(&group_id, participant).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // Destroy (which calls leave + delete_chat internally).
    match admin.destroy_group(&group_id).await {
        Ok(()) => println!("destroy_group OK"),
        Err(e) => println!("destroy_group failed: {e}"),
    }

    // Capture ALL events after destruction.
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("\n--- ALL events during/after destruction ---");
    let mut n = 0u32;
    while let Ok(desc) = raw_rx.try_recv() {
        n += 1;
        let event_type = desc.split('(').next().unwrap_or("?");
        let short = desc.chars().take(250).collect::<String>();
        println!("  [{n}] {event_type}: {short}");
    }
    println!("  (total: {n} events)");

    println!("\n=== Done. Check WhatsApp Web to see if chat entry persists ===");

    let _ = adapter.shutdown().await;
}
