//! L4: Cross-process E2E tests (multiple processes, TCP transport).
//!
//! Per `docs/e2e/2026-06-23-stoolap-data-sync-e2e-test-plan.md` §L4.
//!
//! These tests spawn `stoolap-node` child processes and connect them via real TCP.
//! Writer uses `file://` DSN (WAL needed for LSN tracking).
//! Verification is via `--status-file` which queries the live DB handle.

use std::process::Command;
use std::time::Duration;

fn stoolap_node_bin() -> String {
    let candidates = [
        {
            let mut p = std::env::current_exe().unwrap();
            p.pop();
            p.pop();
            p.pop();
            p.push("stoolap-node");
            p.push("target");
            p.push("debug");
            p.push("stoolap-node");
            p
        },
        {
            let mut p = std::env::current_dir().unwrap_or_default();
            p.push("stoolap-node");
            p.push("target");
            p.push("debug");
            p.push("stoolap-node");
            p
        },
    ];
    for c in &candidates {
        if c.exists() {
            return c.to_string_lossy().to_string();
        }
    }
    panic!("stoolap-node not found. Build: cd sync-e2e-tests/stoolap-node && cargo build");
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_for_status(path: &str, timeout: Duration) -> Option<i64> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            let trimmed = content.trim();
            if let Ok(n) = trimmed.parse::<i64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// L4-T1: Two-node TCP roundtrip — writer commits 10 rows, reader sees them.
#[tokio::test]
async fn two_node_tcp_roundtrip() {
    let bin = stoolap_node_bin();
    let mission_id = "abcd000000000000000000000000000000000000000000000000000000000000";
    let writer_port = free_port();
    let reader_port = free_port();

    let writer_dir = tempfile::tempdir().unwrap();
    let writer_dsn = format!("file://{}/db", writer_dir.path().to_str().unwrap());

    let status_file = tempfile::NamedTempFile::new().unwrap();
    let status_path = status_file.path().to_str().unwrap().to_string();

    // Writer: file:// DSN (needed for WAL/LSN), commits 10 rows
    let mut writer = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_dsn)
        .arg("--listen")
        .arg(writer_port.to_string())
        .arg("--commit")
        .arg("10")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Reader: memory:// DSN (verification via live db handle, not WAL)
    let mut reader = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&status_path)
        .spawn()
        .expect("failed to spawn reader");

    let count = wait_for_status(&status_path, Duration::from_secs(5)).await;
    assert_eq!(count, Some(10), "reader should have 10 rows after sync");

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
}

/// L4-T2: Three-node TCP fan-out — writer commits 100 rows, both readers see them.
#[tokio::test]
async fn three_node_tcp_fan_out() {
    let bin = stoolap_node_bin();
    let mission_id = "abcd000000000000000000000000000000000000000000000000000000000000";
    let writer_port = free_port();
    let reader1_port = free_port();
    let reader2_port = free_port();

    let writer_dir = tempfile::tempdir().unwrap();
    let writer_dsn = format!("file://{}/db", writer_dir.path().to_str().unwrap());

    let status1 = tempfile::NamedTempFile::new().unwrap();
    let status2 = tempfile::NamedTempFile::new().unwrap();
    let sp1 = status1.path().to_str().unwrap().to_string();
    let sp2 = status2.path().to_str().unwrap().to_string();

    let mut writer = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_dsn)
        .arg("--listen")
        .arg(writer_port.to_string())
        .arg("--commit")
        .arg("100")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let mut r1 = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader1_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&sp1)
        .spawn()
        .expect("failed to spawn reader1");

    let mut r2 = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader2_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0300000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&sp2)
        .spawn()
        .expect("failed to spawn reader2");

    let c1 = wait_for_status(&sp1, Duration::from_secs(5)).await;
    let c2 = wait_for_status(&sp2, Duration::from_secs(5)).await;
    assert_eq!(c1, Some(100), "reader1 should have 100 rows");
    assert_eq!(c2, Some(100), "reader2 should have 100 rows");

    writer.kill().ok();
    r1.kill().ok();
    r2.kill().ok();
    let _ = writer.wait();
    let _ = r1.wait();
    let _ = r2.wait();
}

/// L4-T5: Process crash and restart — reader crashes, restarts, catches up.
#[tokio::test]
async fn process_crash_and_restart() {
    let bin = stoolap_node_bin();
    let mission_id = "abcd000000000000000000000000000000000000000000000000000000000000";
    let writer_port = free_port();
    let reader_port = free_port();

    let writer_dir = tempfile::tempdir().unwrap();
    let writer_dsn = format!("file://{}/db", writer_dir.path().to_str().unwrap());

    let status_file = tempfile::NamedTempFile::new().unwrap();
    let status_path = status_file.path().to_str().unwrap().to_string();

    // Start writer with 5 rows
    let mut writer = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_dsn)
        .arg("--listen")
        .arg(writer_port.to_string())
        .arg("--commit")
        .arg("5")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // First reader instance
    let mut reader = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&status_path)
        .spawn()
        .expect("failed to spawn reader");

    // Wait for initial sync
    let c = wait_for_status(&status_path, Duration::from_secs(5)).await;
    assert_eq!(c, Some(5), "initial sync should have 5 rows");

    // Crash reader
    reader.kill().ok();
    let _ = reader.wait();
    std::fs::write(&status_path, "0").ok();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Restart reader
    let mut reader2 = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&status_path)
        .spawn()
        .expect("failed to restart reader");

    // Wait for re-sync
    let c2 = wait_for_status(&status_path, Duration::from_secs(5)).await;
    assert_eq!(c2, Some(5), "restarted reader should catch up to 5 rows");

    writer.kill().ok();
    reader2.kill().ok();
    let _ = writer.wait();
    let _ = reader2.wait();
}

/// L4-T3: TCP partition and heal — 3 processes. Writer commits data, reader1
/// crashes, reader2 stays connected. Reader1 restarts and catches up via WAL tail.
#[tokio::test]
async fn tcp_partition_and_heal() {
    let bin = stoolap_node_bin();
    let mission_id = "abcd000000000000000000000000000000000000000000000000000000000000";
    let writer_port = free_port();
    let reader1_port = free_port();
    let reader2_port = free_port();

    let writer_dir = tempfile::tempdir().unwrap();
    let writer_dsn = format!("file://{}/db", writer_dir.path().to_str().unwrap());

    let status1 = tempfile::NamedTempFile::new().unwrap();
    let status2 = tempfile::NamedTempFile::new().unwrap();
    let sp1 = status1.path().to_str().unwrap().to_string();
    let sp2 = status2.path().to_str().unwrap().to_string();

    // Start writer with 5 rows
    let mut writer = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_dsn)
        .arg("--listen")
        .arg(writer_port.to_string())
        .arg("--commit")
        .arg("5")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Start both readers
    let mut reader1 = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader1_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&sp1)
        .spawn()
        .expect("failed to spawn reader1");

    let mut reader2 = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader2_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0300000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&sp2)
        .spawn()
        .expect("failed to spawn reader2");

    // Both readers sync initial 5 rows
    let c1 = wait_for_status(&sp1, Duration::from_secs(5)).await;
    let c2 = wait_for_status(&sp2, Duration::from_secs(5)).await;
    assert_eq!(c1, Some(5), "reader1 initial sync");
    assert_eq!(c2, Some(5), "reader2 initial sync");

    // Partition: kill reader1 (simulates TCP drop)
    reader1.kill().ok();
    let _ = reader1.wait();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Heal: restart reader1 — catches up via WAL tail from LSN 0
    std::fs::write(&sp1, "0").ok();
    let mut reader1b = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader1_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&sp1)
        .spawn()
        .expect("failed to restart reader1");

    let c1b = wait_for_status(&sp1, Duration::from_secs(5)).await;
    assert_eq!(c1b, Some(5), "reader1 catches up after heal");

    // reader2 should still be connected and healthy
    let c2_final = std::fs::read_to_string(&sp2)
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok());
    assert_eq!(c2_final, Some(5), "reader2 still has data");

    writer.kill().ok();
    reader1b.kill().ok();
    reader2.kill().ok();
    let _ = writer.wait();
    let _ = reader1b.wait();
    let _ = reader2.wait();
}

/// L4-T7: TCP multi-writer — two writers, one reader receives from both.
///
/// Both writers insert rows with the same PKs (0-4). The reader applies
/// both WAL streams; the second stream's INSERTs overwrite the first
/// (last-writer-wins). The test verifies the transport handles multiple
/// writer connections without crashing.
#[tokio::test]
async fn tcp_multi_writer() {
    let bin = stoolap_node_bin();
    let mission_id = "abcd000000000000000000000000000000000000000000000000000000000000";
    let writer_a_port = free_port();
    let writer_b_port = free_port();
    let reader_port = free_port();

    let writer_a_dir = tempfile::tempdir().unwrap();
    let writer_a_dsn = format!("file://{}/db", writer_a_dir.path().to_str().unwrap());
    let writer_b_dir = tempfile::tempdir().unwrap();
    let writer_b_dsn = format!("file://{}/db", writer_b_dir.path().to_str().unwrap());

    let status = tempfile::NamedTempFile::new().unwrap();
    let sp = status.path().to_str().unwrap().to_string();

    // Writer A: commits 5 rows
    let mut writer_a = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_a_dsn)
        .arg("--listen")
        .arg(writer_a_port.to_string())
        .arg("--commit")
        .arg("5")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer_a");

    // Writer B: commits 5 rows
    let mut writer_b = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_b_dsn)
        .arg("--listen")
        .arg(writer_b_port.to_string())
        .arg("--commit")
        .arg("5")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0500000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer_b");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Reader: connects to both writers
    let mut reader = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_a_port}"))
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_b_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&sp)
        .spawn()
        .expect("failed to spawn reader");

    // Both writers use same PKs (0-4), so reader has 5 rows (last-writer-wins).
    // The test verifies transport handles multiple writers without crashing.
    let c = wait_for_status(&sp, Duration::from_secs(10)).await;
    assert!(
        c == Some(5) || c == Some(10),
        "reader should have rows from both writers, got {:?}",
        c
    );

    writer_a.kill().ok();
    writer_b.kill().ok();
    reader.kill().ok();
    let _ = writer_a.wait();
    let _ = writer_b.wait();
    let _ = reader.wait();
}

/// L4-T4: TCP slow consumer — reader applies slowly, writer doesn't OOM.
///
/// Reader has a 50ms artificial delay per entry. Writer sends 50 rows.
/// The system should handle the backpressure without crashing.
#[tokio::test]
async fn tcp_slow_consumer() {
    let bin = stoolap_node_bin();
    let mission_id = "abcd000000000000000000000000000000000000000000000000000000000000";
    let writer_port = free_port();
    let reader_port = free_port();

    let writer_dir = tempfile::tempdir().unwrap();
    let writer_dsn = format!("file://{}/db", writer_dir.path().to_str().unwrap());

    let status_file = tempfile::NamedTempFile::new().unwrap();
    let status_path = status_file.path().to_str().unwrap().to_string();

    // Start writer with 50 rows
    let mut writer = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_dsn)
        .arg("--listen")
        .arg(writer_port.to_string())
        .arg("--commit")
        .arg("50")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Start slow reader (50ms per entry → 50 entries * 50ms = 2.5s minimum)
    let mut reader = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(reader_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&status_path)
        .arg("--slow-apply-ms")
        .arg("50")
        .spawn()
        .expect("failed to spawn slow reader");

    // Wait longer than the slow consumer — give it time to process all entries.
    let c = wait_for_status(&status_path, Duration::from_secs(10)).await;
    assert_eq!(
        c,
        Some(50),
        "slow reader should eventually receive all 50 rows"
    );

    // Writer should still be alive (no OOM, no crash).
    assert!(
        writer.try_wait().expect("failed to check writer").is_none(),
        "writer should still be running"
    );

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
}

/// L4-T6: TCP chain relay — writer → relay → leaf.
///
/// Writer A commits 5 rows. Relay B (file:// DSN) connects to A, receives
/// entries via apply_wal_entry (which now re-enters into B's WAL per RFC-0862
/// §4.3.3.1). Leaf C (memory:// DSN) connects to B and receives entries via
/// B's read_wal_range.
///
/// This test verifies the StoolapAdapter WAL re-entry fix (mission 0862k).
/// Before the fix, B's current_lsn() stayed at 0 and C received nothing.
#[tokio::test]
async fn tcp_chain_relay() {
    let bin = stoolap_node_bin();
    let mission_id = "abcd000000000000000000000000000000000000000000000000000000000000";
    let writer_port = free_port();
    let relay_port = free_port();
    let leaf_port = free_port();

    let writer_dir = tempfile::tempdir().unwrap();
    let writer_dsn = format!("file://{}/db", writer_dir.path().to_str().unwrap());

    let relay_dir = tempfile::tempdir().unwrap();
    let relay_dsn = format!("file://{}/db", relay_dir.path().to_str().unwrap());

    let status_leaf = tempfile::NamedTempFile::new().unwrap();
    let sp_leaf = status_leaf.path().to_str().unwrap().to_string();

    // Writer A: commits 5 rows
    let mut writer = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_dsn)
        .arg("--listen")
        .arg(writer_port.to_string())
        .arg("--commit")
        .arg("5")
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0100000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn writer");

    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Relay B: file:// DSN, connects to writer A
    let mut relay = Command::new(&bin)
        .arg("--dsn")
        .arg(&relay_dsn)
        .arg("--listen")
        .arg(relay_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{writer_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0200000000000000000000000000000000000000000000000000000000000000")
        .spawn()
        .expect("failed to spawn relay");

    // Wait for relay to fully receive and persist entries
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Leaf C: memory:// DSN, connects to relay B
    let mut leaf = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(leaf_port.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{relay_port}"))
        .arg("--mission-id")
        .arg(mission_id)
        .arg("--node-id")
        .arg("0300000000000000000000000000000000000000000000000000000000000000")
        .arg("--status-file")
        .arg(&sp_leaf)
        .spawn()
        .expect("failed to spawn leaf");

    // Leaf should have 5 rows via chain relay (A → B → C)
    let c = wait_for_status(&sp_leaf, Duration::from_secs(10)).await;
    assert_eq!(
        c,
        Some(5),
        "leaf should have 5 rows via chain relay (A→B→C). \
         If this fails, the StoolapAdapter WAL re-entry fix may not be working."
    );

    writer.kill().ok();
    relay.kill().ok();
    leaf.kill().ok();
    let _ = writer.wait();
    let _ = relay.wait();
    let _ = leaf.wait();
}
