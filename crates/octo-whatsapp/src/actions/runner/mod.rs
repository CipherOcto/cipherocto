//! Trigger runner dispatch. Cross-platform entry point that
//! routes to a Linux sandbox impl on Linux and refuses on other
//! platforms.

use crate::actions::ActionError;

/// Spawns a shell process with the supplied argv. `timeout_ms`
/// bounds the entire run; on expiry the process group is killed.
/// `env_passthrough` is the allowlist of env-var names (default
/// non-empty allowlist: HOME, PATH, LANG, TZ).
///
/// **Linux:** full Landlock + seccomp + rlimit + pidfd sandbox.
/// **Other platforms:** `NotSupported` (fail closed, design
/// §Security).
pub async fn run_shell(
    argv: &[String],
    timeout_ms: u64,
    env_passthrough: &[String],
) -> Result<(), ActionError> {
    #[cfg(target_os = "linux")]
    {
        crate::actions::runner::shell_linux::run_shell(argv, timeout_ms, env_passthrough).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (argv, timeout_ms, env_passthrough);
        Err(ActionError::NotSupported(
            "shell runner is Linux-only (Landlock+seccomp+pidfd)".into(),
        ))
    }
}

#[cfg(target_os = "linux")]
pub mod shell_linux;
