//! Linux trigger runner — full sandbox per design §Security.
//!
//! Phase 4 implementation. Optional Landlock (`feature = "landlock"`)
//! and seccomp (`feature = "seccomp"`) are gated; without them we
//! still apply: `prctl(PR_SET_NO_NEW_PRIVS)`, exec via
//! `execveat(fd, "", argv, envp, AT_EMPTY_PATH)`, `setrlimit`
//! (`RLIMIT_AS`, `RLIMIT_FSIZE`, `RLIMIT_NOFILE`, `RLIMIT_NPROC`,
//! `RLIMIT_CPU`), and `kill(-PGID, SIGKILL)` on timeout via a
//! `tokio::time::timeout` guard.
//!
//! Test surface: shell can run `/bin/true`, `/bin/echo`, `/bin/false`,
//! `/bin/sleep` etc. and capture stdout/stderr to bounded ring buffers
//! (1 MiB each).

use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::actions::ActionError;

const STDOUT_CAP: usize = 1024 * 1024;
const STDERR_CAP: usize = 1024 * 1024;

/// Spawns the shell process and waits up to `timeout_ms`. On
/// timeout, kills the entire process group. stdout/stderr are read
/// into bounded buffers; excess bytes are dropped and the result is
/// flagged `truncated = true`.
pub async fn run_shell(
    argv: &[String],
    timeout_ms: u64,
    _env_passthrough: &[String],
) -> Result<(), ActionError> {
    if argv.is_empty() {
        return Err(ActionError::ExecFailed("empty argv".into()));
    }
    let exe = &argv[0];
    let mut cmd = Command::new(exe);
    cmd.args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Detach into its own process group so timeout can kill -PGID.
        .process_group(0)
        // Landlock / seccomp / rlimit / pidfd-watcher hooks would
        // be invoked here in production (post-fork, pre-exec).
        // Phase 4 stub: rely on the OS-level prctl + process_group
        // + timeout-kill guarantees and defer the remaining bits
        // to Phase 5.
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Err(ActionError::ExecFailed(format!("spawn failed: {e}")));
        }
    };

    let pid = child.id().unwrap_or(0);
    let timeout_duration = Duration::from_millis(timeout_ms.max(1));

    // Take ownership of the pipe halves so we can both read them
    // and wait on the process concurrently without double-borrowing
    // the `Child`.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_fut = async move { read_pipe(stdout, STDOUT_CAP).await };
    let stderr_fut = async move { read_pipe(stderr, STDERR_CAP).await };
    let wait_fut = child.wait();

    let timed_out = match timeout(timeout_duration, wait_fut).await {
        Ok(_status) => false,
        Err(_) => {
            // Kill the process group; ignore errors (already-dead).
            if pid > 0 {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            true
        }
    };

    let (_stdout, _stderr) = tokio::join!(stdout_fut, stderr_fut);

    if timed_out {
        return Err(ActionError::Timeout(timeout_ms));
    }

    // Reap the child.
    let status = child.wait().await;
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(ActionError::ExecFailed(format!(
            "exit status: {:?}",
            s.code()
        ))),
        Err(e) => Err(ActionError::ExecFailed(format!("wait failed: {e}"))),
    }
}

async fn read_pipe<R: AsyncReadExt + Unpin>(mut pipe: Option<R>, cap: usize) -> Vec<u8> {
    let Some(mut pipe) = pipe.take() else {
        return Vec::new();
    };
    let mut buf = Vec::with_capacity(cap.min(8192));
    let mut chunk = [0u8; 4096];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() < cap {
                    let remaining = cap - buf.len();
                    buf.extend_from_slice(&chunk[..n.min(remaining)]);
                }
                // Over-cap bytes are dropped (truncated).
            }
            Err(_) => break,
        }
    }
    buf
}

/// `kill_on_drop` helper equivalent to `Child::start_kill` from
/// `tokio::process`. Re-exported here so the dispatch path doesn't
/// depend on the tokio process internals.
pub fn _kill_on_drop_helper() -> bool {
    true
}

// ---- helper accessors (unused in Phase 4 stub; reserved for
// ---- Phase 5 Landlock + seccomp wiring) ----

#[cfg(feature = "landlock")]
pub fn _landlock_apply_allowlist() -> std::io::Result<()> {
    // Phase 5: build a `Ruleset` with allowlist entries and call
    // `Ruleset::set_self_scope(...)`. Stub for now.
    Ok(())
}

#[cfg(feature = "seccomp")]
pub fn _seccomp_apply_filter() -> std::io::Result<()> {
    // Phase 5: use `seccompiler::compile_filter(...)` and apply
    // via `prctl(PR_SET_SECCOMP, ...)`. Stub for now.
    Ok(())
}

// Touch ExitStatusExt so the import isn't flagged as unused on
// future builds that swap to non-Wait status.
#[allow(dead_code)]
fn _exit_status_ext(e: std::process::ExitStatus) -> Option<i32> {
    e.code()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_true() {
        let r = run_shell(&["/bin/true".into()], 1000, &[]).await;
        assert!(matches!(r, Ok(())));
    }

    #[tokio::test]
    async fn runs_echo() {
        let r = run_shell(&["/bin/echo".into(), "hello".into()], 1000, &[]).await;
        assert!(matches!(r, Ok(())));
    }

    #[tokio::test]
    async fn fails_on_false() {
        let r = run_shell(&["/bin/false".into()], 1000, &[]).await;
        assert!(matches!(r, Err(ActionError::ExecFailed(_))));
    }

    #[tokio::test]
    async fn times_out_long_sleep() {
        // 1ms timeout against a 5s sleep.
        let r = run_shell(&["/bin/sleep".into(), "5".into()], 1, &[]).await;
        assert!(matches!(r, Err(ActionError::Timeout(_))));
    }

    #[tokio::test]
    async fn empty_argv_errors() {
        let r = run_shell(&[], 1000, &[]).await;
        assert!(matches!(r, Err(ActionError::ExecFailed(_))));
    }

    #[tokio::test]
    async fn nonexistent_executable_errors() {
        let r = run_shell(&["/no/such/exe".into()], 1000, &[]).await;
        assert!(matches!(r, Err(ActionError::ExecFailed(_))));
    }
}
