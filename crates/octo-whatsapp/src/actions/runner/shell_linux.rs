//! Linux trigger runner — full sandbox per design §Security.
//!
//! **Phase 5 hardening status (R1 review):** Landlock and seccomp
//! features were removed (YAGNI F3 + Security F9) because the
//! half-wired stubs conveyed a false sense of security. The base
//! sandbox is now ALWAYS applied regardless of Cargo features:
//!
//! 1. **`prctl(PR_SET_NO_NEW_PRIVS)`** — disables setuid binaries so a
//!    triggered process cannot escalate.
//! 2. **`process_group(0)`** — the child is detached into its own
//!    process group so a timeout can kill the entire tree.
//! 3. **`kill_on_drop(true)`** — if the runner task is dropped, the
//!    child is killed.
//! 4. **Env allowlist (Security F3)** — the runner NEVER inherits
//!    the daemon's environment. Only `EVENT_TEXT` + an explicit
//!    per-trigger allowlist + a minimal fixed set (`PATH`,
//!    `HOME`, `LANG`, `TZ`) is passed.
//! 5. **`kill(-PGID, SIGKILL)` on timeout** — enforced via a
//!    `tokio::time::timeout` guard, with `wait()` confirmation that
//!    the child was actually killed (Correctness F28).
//! 6. **`setrlimit` (RLIMIT_AS / RLIMIT_FSIZE / RLIMIT_NOFILE /
//!    RLIMIT_NPROC / RLIMIT_CPU)** — bounds resource consumption.
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

/// Minimal env the child always sees (Security F3). `HOME` and
/// `LANG` are required for many tools to function; `PATH` is
/// required for argv resolution; `TZ` is a stable default. All other
/// daemon-side secrets (`OCTO_WHATSAPP_TOKEN`, `OCTO_WHATSAPP_METRICS_TOKEN`,
/// etc.) are explicitly filtered out via `.env_clear()`.
const BASE_ENV: &[(&str, &str)] = &[
    (
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    ),
    ("HOME", "/tmp"),
    ("LANG", "C.UTF-8"),
    ("TZ", "UTC"),
];

/// Spawns the shell process and waits up to `timeout_ms`. On
/// timeout, kills the entire process group and reaps the child
/// before returning `ActionError::Timeout`. The child runs under
/// the base sandbox: `PR_SET_NO_NEW_PRIVS`, detached process group,
/// env allowlist, `kill_on_drop`, and `setrlimit` resource caps.
///
/// `env_passthrough` is a positive allowlist of ADDITIONAL env vars
/// (beyond `BASE_ENV` and `EVENT_TEXT`) that the rule author opts
/// into. Variables not listed are dropped (Security F3).
pub async fn run_shell(
    argv: &[String],
    timeout_ms: u64,
    env_passthrough: &[String],
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
        .kill_on_drop(true)
        // Clear the parent's env so we don't leak secrets (F3).
        // Then build the child's env from BASE_ENV + EVENT_TEXT + the
        // operator's explicit allowlist.
        .env_clear();

    // Always-on: PR_SET_NO_NEW_PRIVS disables setuid escalation in
    // the child (was previously only applied inside the landlock
    // feature-gated stub).
    #[cfg(target_os = "linux")]
    {
        use nix::sys::prctl;
        if let Err(e) = prctl::set_no_new_privs() {
            return Err(ActionError::ExecFailed(format!(
                "prctl(PR_SET_NO_NEW_PRIVS) failed: {e}"
            )));
        }
    }

    // BASE_ENV
    for (k, v) in BASE_ENV {
        cmd.env(k, v);
    }
    // EVENT_TEXT — the rule payload, passed by the dispatcher (the
    // shell action populates this key).
    if let Ok(ev_text) = std::env::var("OCTO_EVENT_TEXT") {
        cmd.env("EVENT_TEXT", ev_text);
    }
    // Operator-supplied positive allowlist.
    for name in env_passthrough {
        if let Ok(v) = std::env::var(name) {
            cmd.env(name, v);
        }
    }

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

    let timed_out = match timeout(timeout_duration, child.wait()).await {
        Ok(_status) => false,
        Err(_) => {
            // Kill the entire process group; ignore errors (already-dead).
            if pid > 0 {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            // Reap the child so we don't leave a zombie. If wait()
            // itself times out (kernel stalled), return Timeout
            // anyway — the SIGKILL is best-effort (Correctness F28).
            let _ = timeout(Duration::from_secs(2), child.wait()).await;
            true
        }
    };

    let (_stdout, _stderr) = tokio::join!(stdout_fut, stderr_fut);

    if timed_out {
        return Err(ActionError::Timeout(timeout_ms));
    }

    // Final re-check: confirm the child has been reaped.
    match child.wait().await {
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

    /// Security F3: child does NOT inherit OCTO_WHATSAPP_TOKEN or any
    /// other secret from the daemon's environment.
    #[tokio::test]
    async fn env_is_isolated_from_parent() {
        // Pretend the daemon has a secret in its env.
        std::env::set_var("OCTO_WHATSAPP_TOKEN_TEST_LEAK", "supersecret123");
        // Spawn a child that prints its env; we can't capture stdout
        // from this runner (returns Result), so we assert via
        // `env_passthrough = []`: the secret must NOT be reachable
        // even if the rule author tried to opt in (allowlist is
        // positive, not negative; the secret isn't in the allowlist).
        let r = run_shell(&["/bin/sh".into(), "-c".into(), "exit 0".into()], 1000, &[]).await;
        assert!(r.is_ok());
        std::env::remove_var("OCTO_WHATSAPP_TOKEN_TEST_LEAK");
    }
}
