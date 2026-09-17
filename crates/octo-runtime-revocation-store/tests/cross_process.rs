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

/// Harness arg list forwarded to every child process. `--ignored`
/// is required so the test harness matches our test fn in the
/// subprocess (without it the child harness would skip the fn
/// entirely and the child-role branch below would never run).
const HARNESS_ARGS: &[&str] = &[
    "--ignored",
    "--test",
    "tv_agt27_cross_process_revocation_propagates",
    "--exact",
    "--nocapture",
];

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

    // 1. spawn_side: baseline observer. Asserts token is NOT revoked.
    let spawn_side = spawn_role(&exe, "spawn_side", &ledger_path, &session_id_hex, false);
    assert!(
        spawn_side.success(),
        "spawn_side child failed: status={spawn_side:?}"
    );

    // 2. operator_side: writer. Revokes the session_id in the ledger.
    let operator_side = spawn_role(&exe, "operator_side", &ledger_path, &session_id_hex, true);
    assert!(
        operator_side.success(),
        "operator_side child failed: status={operator_side:?}"
    );

    // 3. attach_side: cross-process reader. Asserts is_token_revoked
    // returns true — this is the cross-process visibility check.
    // Without the Stoolap-backed ledger, attach_side would observe
    // false (process-local default) and exit non-zero.
    let attach_side = spawn_role(&exe, "attach_side", &ledger_path, &session_id_hex, true);
    assert!(
        attach_side.success(),
        "attach_side child failed: status={attach_side:?}"
    );
}

fn run_child_role() -> ! {
    // Exit codes below (11/12/13/20/21/30/40) are test-harness
    // INTERNAL codes for child-process assertion failures. They
    // numerically overlap with OctoCliError::exit_code slots but
    // the overlap is benign: orchestrator only checks `.success()`.
    // A future reader of a failing child's status field should NOT
    // mis-attribute the cause to the CLI dispatch slot.
    // Fail-CLOSED per RFC-0011-c §F.7.5 §Failure semantics: any
    // malformed env input exits non-zero via `fail` before reaching
    // the role-specific match arm below.
    let ledger_path: std::path::PathBuf = env::var(LEDGER_ENV)
        .map(std::path::PathBuf::from)
        .expect("OCTO_REVOCATION_LEDGER_PATH");
    let session_id_hex = env::var(SESSION_ID_ENV)
        .unwrap_or_else(|e| fail(11, format!("child FAIL: missing {SESSION_ID_ENV}: {e}")));
    let session_id_bytes = hex::decode(&session_id_hex).unwrap_or_else(|e| {
        fail(
            12,
            format!("child FAIL: invalid hex in {SESSION_ID_ENV}: {e}"),
        )
    });
    let session_id_bytes_len = session_id_bytes.len();
    let session_id: [u8; 32] = session_id_bytes.try_into().unwrap_or_else(|_| {
        fail(
            13,
            format!("child FAIL: session_id wrong length {session_id_bytes_len} (expected 32)"),
        )
    });
    let expect_revoked = env::var(EXPECT_REVOKED_ENV).as_deref() == Ok("true");
    let role = env::var(ROLE_ENV).expect("OCTO_REVOCATION_CROSS_PROCESS_ROLE");

    // RFC-0011-c §F.7.5 step 4 — install via factory closure so the
    // child dispatches through `octo_runtime::current_revocation_store()`
    // (the production path) rather than calling the Layer D adapter
    // directly. This is what the test proves: a separately-spawned CLI
    // process observing another process's revocation through the
    // substrate-level free functions.
    //
    // `store_arc` is kept local so the operator-side match arm can
    // drop it before `std::process::exit`. Note that drop+exit is
    // NOT what preserves durability — see the operator-side comment
    // below.
    let store_arc: Arc<StoolapRevocationStore> =
        Arc::new(StoolapRevocationStore::open_at(&ledger_path).expect("open ledger"));
    let dyn_arc: Arc<dyn RevocationStore> = store_arc.clone();
    install_revocation_store_default_with(move || Ok(dyn_arc.clone()))
        .expect("install revocation store");

    match role.as_str() {
        "spawn_side" => {
            let revoked = is_token_revoked(&session_id);
            if revoked != expect_revoked {
                fail(
                    20,
                    format!("spawn_side FAIL: expected revoked={expect_revoked} got {revoked}"),
                );
            }
            std::process::exit(0);
        }
        "operator_side" => {
            if let Err(e) = revoke_attach_token(session_id) {
                fail(21, format!("operator_side FAIL: revoke failed: {e}"));
            }
            // drop(store_arc) is decorative: ACTIVE_REVOCATION_STORE still holds
            // an Arc<dyn> clone, and std::process::exit bypasses all destructors.
            // Durability comes from the fsync calls below.
            drop(store_arc);
            // Belt-and-suspenders fsync of the ledger file, sibling
            // SQLite-family files (DELETE-mode journal / WAL-mode -wal
            // + -shm), and the parent directory (POSIX metadata
            // durability). All best-effort: missing sibling files
            // are normal if the journal was already merged.
            best_effort_fsync(&ledger_path);
            for suffix in ["-journal", "-wal", "-shm"] {
                best_effort_fsync(format!("{}{}", ledger_path.display(), suffix));
            }
            if let Some(parent) = ledger_path.parent() {
                best_effort_fsync(parent);
            }
            std::process::exit(0);
        }
        "attach_side" => {
            let revoked = is_token_revoked(&session_id);
            if revoked != expect_revoked {
                fail(
                    30,
                    format!(
                        "attach_side FAIL: expected revoked={expect_revoked} got {revoked} \
                         (cross-process write not visible — ledger is process-local)"
                    ),
                );
            }
            std::process::exit(0);
        }
        other => fail(40, format!("unknown role: {other}")),
    }
}

/// Spawn a child process for one cross-process role and return its
/// exit status. Used for spawn_side + operator_side + attach_side;
/// only the role label + env vars differ between sites.
fn spawn_role(
    exe: &std::path::Path,
    role: &str,
    ledger_path: &std::path::Path,
    session_id_hex: &str,
    expect_revoked: bool,
) -> std::process::ExitStatus {
    Command::new(exe)
        .args(HARNESS_ARGS)
        .env(ROLE_ENV, role)
        .env(LEDGER_ENV, ledger_path)
        .env(SESSION_ID_ENV, session_id_hex)
        .env(
            EXPECT_REVOKED_ENV,
            if expect_revoked { "true" } else { "false" },
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|e| panic!("spawn {role}: {e}"))
}

/// Print a child-process failure message to stderr and exit with
/// the given code. Used for env-var parse failures (11/12/13) and
/// role-assertion failures (20/21/30/40).
fn fail(code: i32, msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(code);
}

/// Best-effort fsync of a file or directory path. Returns silently
/// on `File::open` failure (missing sibling files are normal —
/// the journal may have been merged + deleted). Used for the
/// ledger file, `-journal` / `-wal` / `-shm` siblings, and the
/// parent directory.
fn best_effort_fsync(path: impl AsRef<std::path::Path>) {
    let _ = std::fs::File::open(path.as_ref()).map(|f| f.sync_all());
}
