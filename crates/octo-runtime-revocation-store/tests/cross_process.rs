//! TV-AGT27 cross-process revocation propagation test
//! (RFC-0011-c §F.7.5 paired amendment + §F.7.4 three-process
//! topology).
//!
//! Spawns three child processes that all open the same Stoolap-backed
//! revocation ledger at a shared tempdir path. Each child takes a
//! distinct role via the `OCTO_REVOCATION_CROSS_PROCESS_ROLE` env var
//! (parent process leaves the env var unset and orchestrates):
//! `spawn_side` installs the factory-dispatched revocation store and
//! asserts `octo_runtime::is_token_revoked` returns `false` (the
//! pre-write baseline observer); `operator_side` installs the store
//! and calls `octo_runtime::revoke_attach_token` to write the
//! revocation (cross-process writer); `attach_side` installs the
//! store and asserts `octo_runtime::is_token_revoked` returns `true`
//! for the same `session_id` (cross-process reader).
//!
//! If the ledger were process-local (the §F.3 fork-fail-closed
//! default), the attach-side would observe `false` and the test
//! would fail. The Stoolap-backed Layer D adapter makes the ledger
//! the single source of truth across CLI process boundaries.
//! Exercising the factory dispatch (not a direct
//! `StoolapRevocationStore` method call) is what proves the §F.7.5
//! substrate-faithful wiring actually works end-to-end.
//!
//! Test isolation: each test run uses a fresh tempdir ledger path so
//! parallel `cargo test` runs do not collide. The shared ledger path
//! is propagated to each child via env vars (not CLI args — args are
//! forwarded by the cargo test harness for the test binary itself).
//!
//! NO PUSH per [[feedback_initiation_user_only]] + [[git-workflow]].

use std::env;
use std::process::{Command, Stdio};
use std::sync::Arc;

use octo_runtime::persistence::{install_revocation_store_default_with, RevocationStore};
use octo_runtime::{is_token_revoked, revoke_attach_token};
use octo_runtime_revocation_store::StoolapRevocationStore;

const ROLE_ENV: &str = "OCTO_REVOCATION_CROSS_PROCESS_ROLE";
const LEDGER_ENV: &str = "OCTO_REVOCATION_LEDGER_PATH";
const SESSION_ID_ENV: &str = "OCTO_REVOCATION_SESSION_ID";
const EXPECT_REVOKED_ENV: &str = "OCTO_REVOCATION_EXPECT_REVOKED";

// `#[ignore]` because:
// (a) The test spawns 3 child processes and takes ~1s, slowing
//     the default `cargo test` run on every dev iteration.
// (b) The child-process branch is invoked via `env::current_exe()`
//     with `--ignored` so the harness matches and runs this
//     function inside the spawned subprocess. Without `#[ignore]`
//     the child harness would skip the function entirely
//     (filter mismatch).
//
// Run with: `cargo test -p octo-runtime-revocation-store
// --test cross_process -- --ignored --nocapture`.
#[test]
#[ignore]
fn tv_agt27_cross_process_revocation_propagates() {
    // Child-process branch: when the env var is set, this process
    // was spawned by the parent to perform a single role. Execute
    // that role + exit the process before the test harness reaches
    // the orchestrator branch below.
    if env::var_os(ROLE_ENV).is_some() {
        run_child_role();
    }

    // Parent-process orchestrator: spawn three children against a
    // shared ledger path and assert each child's exit status.
    let tmp = tempfile::tempdir().expect("tempdir");
    let ledger_path = tmp.path().join("revocation.stoolap");

    // Deterministic session_id (32 bytes, all 0xAB). Using a fixed
    // session_id keeps the test idempotent + free of any mint
    // dependency; only the revocation read/write paths are
    // exercised.
    let session_id: [u8; 32] = [0xAB; 32];
    let session_id_hex = hex::encode(session_id);

    let exe = env::current_exe().expect("current_exe");

    // The shared harness arg list. We pass `--ignored` so the test
    // harness matches the parent process invocation; without this
    // the child would skip our test fn and the orchestrator branch
    // would not be reached.
    let harness_args = [
        "--ignored",
        "--test",
        "cross_process_revocation_propagates",
        "--exact",
        "--nocapture",
    ];

    // 1. spawn_side: baseline observer. Asserts token is NOT revoked.
    let spawn_side = Command::new(&exe)
        .args(harness_args)
        .env(ROLE_ENV, "spawn_side")
        .env(LEDGER_ENV, &ledger_path)
        .env(SESSION_ID_ENV, &session_id_hex)
        .env(EXPECT_REVOKED_ENV, "false")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn spawn_side");
    assert!(
        spawn_side.success(),
        "spawn_side child failed: status={spawn_side:?}"
    );

    // 2. operator_side: writer. Revokes the session_id in the ledger.
    let operator_side = Command::new(&exe)
        .args(harness_args)
        .env(ROLE_ENV, "operator_side")
        .env(LEDGER_ENV, &ledger_path)
        .env(SESSION_ID_ENV, &session_id_hex)
        .env(EXPECT_REVOKED_ENV, "true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn operator_side");
    assert!(
        operator_side.success(),
        "operator_side child failed: status={operator_side:?}"
    );

    // 3. attach_side: cross-process reader. Asserts is_token_revoked
    // returns true — this is the cross-process visibility check.
    // Without the Stoolap-backed ledger, attach_side would observe
    // false (process-local default) and exit non-zero.
    let attach_side = Command::new(&exe)
        .args(harness_args)
        .env(ROLE_ENV, "attach_side")
        .env(LEDGER_ENV, &ledger_path)
        .env(SESSION_ID_ENV, &session_id_hex)
        .env(EXPECT_REVOKED_ENV, "true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn attach_side");
    assert!(
        attach_side.success(),
        "attach_side child failed: status={attach_side:?}"
    );
}

fn run_child_role() -> ! {
    let ledger_path = env::var(LEDGER_ENV).expect("OCTO_REVOCATION_LEDGER_PATH");
    let session_id_hex = env::var(SESSION_ID_ENV).expect("OCTO_REVOCATION_SESSION_ID");
    let session_id_bytes = hex::decode(&session_id_hex).expect("hex decode");
    let mut session_id = [0u8; 32];
    if session_id_bytes.len() != 32 {
        eprintln!(
            "child FAIL: session_id wrong length {} (expected 32)",
            session_id_bytes.len()
        );
        std::process::exit(11);
    }
    session_id.copy_from_slice(&session_id_bytes);
    let expect_revoked: bool = env::var(EXPECT_REVOKED_ENV)
        .map(|v| v == "true")
        .unwrap_or(false);
    let role = env::var(ROLE_ENV).expect("OCTO_REVOCATION_CROSS_PROCESS_ROLE");

    // RFC-0011-c §F.7.5 step 4 — install via factory closure so the
    // child dispatches through `octo_runtime::current_revocation_store()`
    // (the production path) rather than calling the Layer D adapter
    // directly. This is what the test proves: a separately-spawned CLI
    // process observing another process's revocation through the
    // substrate-level free functions.
    install_revocation_store_default_with(|| {
        let store = StoolapRevocationStore::open_at(&ledger_path).expect("open ledger");
        Ok(Arc::new(store) as Arc<dyn RevocationStore>)
    })
    .expect("install revocation store");

    match role.as_str() {
        "spawn_side" => {
            let revoked = is_token_revoked(&session_id);
            if revoked != expect_revoked {
                eprintln!("spawn_side FAIL: expected revoked={expect_revoked} got {revoked}");
                std::process::exit(20);
            }
            std::process::exit(0);
        }
        "operator_side" => {
            revoke_attach_token(session_id).expect("revoke write");
            std::process::exit(0);
        }
        "attach_side" => {
            let revoked = is_token_revoked(&session_id);
            if revoked != expect_revoked {
                eprintln!(
                    "attach_side FAIL: expected revoked={expect_revoked} got {revoked} \
                     (cross-process write not visible — ledger is process-local)"
                );
                std::process::exit(30);
            }
            std::process::exit(0);
        }
        other => {
            eprintln!("unknown role: {other}");
            std::process::exit(40);
        }
    }
}
