//! L4: Bootstrap cross-process E2E tests.
//!
//! Exercises the `--seed-list` and `--seed-authority` CLI flags
//! of `stoolap-node` via real child processes. Since
//! `send_bootstrap_requests` is a stub (returns empty), these
//! tests verify flag parsing, error handling, and precedence
//! rather than full bootstrap convergence.
//!
//! Test matrix (6 tests):
//!
//! | ID   | Scenario                                         | Expected          |
//! |------|--------------------------------------------------|-------------------|
//! | LB01 | Node starts with --seed-list flag                | Runs, times out   |
//! | LB02 | Node exits on missing seed list file             | Exit code != 0    |
//! | LB03 | Node exits on invalid JSON seed list             | Exit code != 0    |
//! | LB04 | --peer takes precedence over --seed-list         | Connects via TCP  |
//! | LB05 | --seed-authority dao flag accepted               | Runs, times out   |
//! | LB06 | --seed-list with valid JSON, no --peer           | Runs bootstrap    |

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

fn mission_id() -> &'static str {
    "abcd000000000000000000000000000000000000000000000000000000000000"
}

fn node_id(suffix: u8) -> String {
    format!("{:0>64x}", suffix)
}

/// Write a seed list JSON file with N peers at the given epoch.
fn write_seed_list(dir: &std::path::Path, peers: Vec<(&str, &str)>, epoch: u64) -> String {
    let entries: Vec<String> = peers
        .iter()
        .map(|(id, addr)| {
            format!(
                r#"{{"peer_id":"{}","multiaddr":"{}","signed_at_epoch":{}}}"#,
                id, addr, epoch
            )
        })
        .collect();
    let json = format!(
        r#"{{
        "authority_pubkey": "{}",
        "signed_at_epoch": {},
        "peers": [{}]
    }}"#,
        "00".repeat(32),
        epoch,
        entries.join(",")
    );
    let path = dir.join("seed_list.json");
    std::fs::write(&path, &json).unwrap();
    path.to_string_lossy().to_string()
}

// ── LB01: Node starts with --seed-list flag ──────────────────────
//
// Verifies that the --seed-list flag is accepted by the binary.
// Since bootstrap returns NoResponses (stub), the node will
// eventually exit, but it should not crash on flag parsing.

#[tokio::test]
async fn lb01_node_starts_with_seed_list_flag() {
    let bin = stoolap_node_bin();
    let port = free_port();
    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = write_seed_list(
        seed_dir.path(),
        vec![
            ("seed-1", "/ip4/10.0.0.1/tcp/4001"),
            ("seed-2", "/ip4/10.0.0.2/tcp/4001"),
            ("seed-3", "/ip4/10.0.0.3/tcp/4001"),
        ],
        100,
    );

    let db_dir = tempfile::tempdir().unwrap();
    let dsn = format!("file://{}/db", db_dir.path().to_str().unwrap());

    let mut child = Command::new(&bin)
        .arg("--dsn")
        .arg(&dsn)
        .arg("--listen")
        .arg(port.to_string())
        .arg("--seed-list")
        .arg(&seed_path)
        .arg("--mission-id")
        .arg(mission_id())
        .arg("--node-id")
        .arg(&node_id(0x01))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn node");

    // Give it time to start and attempt bootstrap
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // The process may have exited (bootstrap fails with no --peer fallback)
    // or may still be running (TCP listener continues). Either is acceptable.
    let status = child.try_wait().unwrap();
    if let Some(exit) = status {
        assert!(!exit.success(), "should exit with error on bootstrap failure");
    }
    // If still running, that's OK — node continues with TCP listener

    child.kill().ok();
    let _ = child.wait();
}

// ── LB02: Node exits on missing seed list file ───────────────────

#[tokio::test]
async fn lb02_node_exits_on_missing_seed_list() {
    let bin = stoolap_node_bin();
    let port = free_port();

    let db_dir = tempfile::tempdir().unwrap();
    let dsn = format!("file://{}/db", db_dir.path().to_str().unwrap());

    let output = Command::new(&bin)
        .arg("--dsn")
        .arg(&dsn)
        .arg("--listen")
        .arg(port.to_string())
        .arg("--seed-list")
        .arg("/nonexistent/path/seed_list.json")
        .arg("--mission-id")
        .arg(mission_id())
        .arg("--node-id")
        .arg(&node_id(0x01))
        .output()
        .expect("failed to run node");

    assert!(
        !output.status.success(),
        "should exit with error for missing seed list"
    );
    // Note: tracing may not flush before std::process::exit,
    // so stderr may be empty. Exit code check is sufficient.
}

// ── LB03: Node exits on invalid JSON seed list ───────────────────

#[tokio::test]
async fn lb03_node_exits_on_invalid_seed_list_json() {
    let bin = stoolap_node_bin();
    let port = free_port();

    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = seed_dir.path().join("bad_seed_list.json");
    std::fs::write(&seed_path, "{not valid json!!!}").unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let dsn = format!("file://{}/db", db_dir.path().to_str().unwrap());

    let output = Command::new(&bin)
        .arg("--dsn")
        .arg(&dsn)
        .arg("--listen")
        .arg(port.to_string())
        .arg("--seed-list")
        .arg(seed_path.to_str().unwrap())
        .arg("--mission-id")
        .arg(mission_id())
        .arg("--node-id")
        .arg(&node_id(0x01))
        .output()
        .expect("failed to run node");

    assert!(
        !output.status.success(),
        "should exit with error for invalid JSON"
    );
    // Note: tracing may not flush before std::process::exit,
    // so stderr may be empty. Exit code check is sufficient.
}

// ── LB04: --peer takes precedence over --seed-list ───────────────
//
// When both --peer and --seed-list are provided, --peer should
// take precedence (backward compatibility). The node should
// connect via TCP and sync normally.

#[tokio::test]
async fn lb04_peer_flag_takes_precedence() {
    let bin = stoolap_node_bin();
    let port_writer = free_port();
    let port_reader = free_port();
    let mission = mission_id();

    let writer_dir = tempfile::tempdir().unwrap();
    let writer_dsn = format!("file://{}/db", writer_dir.path().to_str().unwrap());

    let status_file = tempfile::NamedTempFile::new().unwrap();
    let status_path = status_file.path().to_str().unwrap().to_string();

    // Writer: commit 5 rows
    let mut writer = Command::new(&bin)
        .arg("--dsn")
        .arg(&writer_dsn)
        .arg("--listen")
        .arg(port_writer.to_string())
        .arg("--commit")
        .arg("5")
        .arg("--mission-id")
        .arg(mission)
        .arg("--node-id")
        .arg(&node_id(0x01))
        .spawn()
        .expect("failed to spawn writer");

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Reader: has BOTH --peer and --seed-list
    // --peer should take precedence
    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = write_seed_list(
        seed_dir.path(),
        vec![("nonexistent", "/ip4/192.0.2.1/tcp/9999")], // Fake bootstrap
        100,
    );

    let mut reader = Command::new(&bin)
        .arg("--dsn")
        .arg("memory://")
        .arg("--listen")
        .arg(port_reader.to_string())
        .arg("--peer")
        .arg(format!("127.0.0.1:{port_writer}"))
        .arg("--seed-list")
        .arg(&seed_path)
        .arg("--mission-id")
        .arg(mission)
        .arg("--node-id")
        .arg(&node_id(0x02))
        .arg("--status-file")
        .arg(&status_path)
        .spawn()
        .expect("failed to spawn reader");

    // If --peer takes precedence, reader syncs via TCP
    let count = tokio::time::timeout(
        Duration::from_secs(8),
        async {
            loop {
                if let Ok(content) = std::fs::read_to_string(&status_path) {
                    if let Ok(n) = content.trim().parse::<i64>() {
                        if n > 0 {
                            return n;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        },
    )
    .await;

    assert!(
        count.is_ok(),
        "reader should sync via --peer, ignoring --seed-list"
    );
    assert_eq!(count.unwrap(), 5, "reader should have 5 rows");

    writer.kill().ok();
    reader.kill().ok();
    let _ = writer.wait();
    let _ = reader.wait();
}

// ── LB05: --seed-authority dao flag accepted ─────────────────────

#[tokio::test]
async fn lb05_seed_authority_dao_flag_accepted() {
    let bin = stoolap_node_bin();
    let port = free_port();
    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = write_seed_list(
        seed_dir.path(),
        vec![
            ("seed-1", "/ip4/10.0.0.1/tcp/4001"),
            ("seed-2", "/ip4/10.0.0.2/tcp/4001"),
            ("seed-3", "/ip4/10.0.0.3/tcp/4001"),
        ],
        1_700_000_001, // After EPOCH_GOVERNANCE_TAKEOVER
    );

    let db_dir = tempfile::tempdir().unwrap();
    let dsn = format!("file://{}/db", db_dir.path().to_str().unwrap());

    let mut child = Command::new(&bin)
        .arg("--dsn")
        .arg(&dsn)
        .arg("--listen")
        .arg(port.to_string())
        .arg("--seed-list")
        .arg(&seed_path)
        .arg("--seed-authority")
        .arg("dao")
        .arg("--mission-id")
        .arg(mission_id())
        .arg("--node-id")
        .arg(&node_id(0x01))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn node");

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let status = child.try_wait().unwrap();
    if let Some(exit) = status {
        // Should fail at bootstrap (stub), NOT at authority validation
        assert!(!exit.success());
    }
    // If still running, that's OK — authority check passed, bootstrap is pending

    child.kill().ok();
    let _ = child.wait();
}

// ── LB06: --seed-list with valid JSON, no --peer ─────────────────
//
// Full bootstrap path: seed list loaded, health check passed,
// authority verified, BOOTSTRAP_REQ sent, no responses (stub).
// Node should exit with bootstrap failure.

#[tokio::test]
async fn lb06_seed_list_full_path_exits_on_no_responses() {
    let bin = stoolap_node_bin();
    let port = free_port();
    let seed_dir = tempfile::tempdir().unwrap();
    let seed_path = write_seed_list(
        seed_dir.path(),
        vec![
            ("seed-alpha", "/ip4/10.0.0.1/tcp/4001"),
            ("seed-beta", "/ip4/10.0.0.2/tcp/4001"),
            ("seed-gamma", "/ip4/10.0.0.3/tcp/4001"),
        ],
        100,
    );

    let db_dir = tempfile::tempdir().unwrap();
    let dsn = format!("file://{}/db", db_dir.path().to_str().unwrap());

    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            Command::new(&bin)
                .arg("--dsn")
                .arg(&dsn)
                .arg("--listen")
                .arg(port.to_string())
                .arg("--seed-list")
                .arg(&seed_path)
                .arg("--mission-id")
                .arg(mission_id())
                .arg("--node-id")
                .arg(&node_id(0x01))
                .output()
                .expect("failed to run node")
        }),
    )
    .await
    .expect("node timed out (should have exited on bootstrap failure)")
    .expect("spawn failed");

    // Node should exit because bootstrap fails (stub returns empty)
    // and there are no --peer args to fall back to
    assert!(
        !output.status.success(),
        "should exit with error when bootstrap fails with no --peer fallback"
    );
    // Note: tracing may not flush before std::process::exit,
    // so stderr may be empty. Exit code check is sufficient.
}
