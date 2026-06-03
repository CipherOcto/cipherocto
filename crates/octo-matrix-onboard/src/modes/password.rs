//! Password login (`m.login.password`).
//!
//! Mission 0850h-a §Acceptance Criteria:
//! - `m.login.password` via `Client::login_username`
//! - password via `--password-stdin` only; clap-level rejection of
//!   `--password <value>` flag form
//!
//! Flow:
//! 1. Build a `Client` against the homeserver URL.
//! 2. Read password from stdin (a single line, no echo).
//! 3. `client.matrix_auth().login_username(user, password).send()`.
//! 4. Extract session via `octo_matrix_onboard_core::session::extract`.
//! 5. Write to disk via `output::write`.
//!
//! SDK returns `Http(Unauthorized)` for bad credentials → mapped to
//! `OnboardError::AuthRejected` (exit 2). Other transport errors map
//! to `OnboardError::Unreachable` (exit 3).

use crate::cli::PasswordArgs;
use crate::error::{classify_sdk_err, OnboardError, Result};
use crate::output;
use matrix_sdk::Client;
use octo_matrix_onboard_core::session;
use std::io::{self, BufRead, Write};
use tracing::{info, warn};
use zeroize::Zeroizing;

pub async fn run(args: PasswordArgs) -> Result<()> {
    let password: Zeroizing<String> = read_password_from_stdin(args.password_stdin)?;
    let client = build_client(&args.homeserver).await?;
    login(&client, &args.user, &password, args.device_name.as_deref()).await?;
    // R1-M11: `Zeroizing<String>` zeros its heap allocation on drop.
    // The explicit `drop` is no longer needed (the binding goes out
    // of scope at end of fn), but kept here as a code-review hint
    // that the lifetime is intentionally short.
    drop(password);

    let sess = session::extract(&client, &args.homeserver).map_err(|e| {
        OnboardError::Generic(anyhow::anyhow!(
            "session extract after login_username: {}",
            e
        ))
    })?;
    info!(
        homeserver = %sess.homeserver_url,
        user_id = %sess.user_id,
        device_id = %sess.device_id,
        has_refresh = sess.refresh_token.is_some(),
        "Password login complete"
    );
    output::write(&args.output, &sess)
}

async fn build_client(homeserver: &str) -> Result<Client> {
    Client::builder()
        .homeserver_url(homeserver)
        .build()
        .await
        .map_err(|e| {
            // R14-L1: same fix as `login` — the previous shape
            // substring-matched on `"dns"` / `"DNS"` / `"connect"` /
            // `"Connection"`, all of which can appear in unrelated
            // SDK error bodies (e.g. `"connection pool error: dns
            // resolved but TLS handshake failed"` would misclassify
            // as `Unreachable`). `classify_sdk_err` inspects the
            // leading `[NNN / errcode]` ruma prefix first, falling
            // back to a narrower DNS/connect substring heuristic
            // only when no status code is present. The `where_` arg
            // namespaces the homeserver so the log line keeps the
            // operator context.
            let where_ = format!("build client against {homeserver}");
            classify_sdk_err(&where_, &e)
        })
}

async fn login(
    client: &Client,
    user: &str,
    password: &str,
    device_name: Option<&str>,
) -> Result<()> {
    let mut builder = client
        .matrix_auth()
        .login_username(user, password)
        .request_refresh_token();
    if let Some(name) = device_name {
        builder = builder.initial_device_display_name(name);
    }
    match builder.send().await {
        Ok(_) => Ok(()),
        Err(e) => {
            // R14-L1: replaced the previous substring-based
            // classification with `classify_sdk_err` from
            // `error.rs`. The old code matched on
            // `"Unauthorized"` / `"M_FORBIDDEN"` / `"invalid"` /
            // `"401"`, which are also substrings of unrelated
            // error bodies (e.g. an SDK internal error like
            // `"internal: invalid utf-8 in M_FORBIDDEN check"` would
            // misclassify as `AuthRejected`). R1-M10 / R1-M12 added
            // `classify_sdk_err` specifically to inspect the leading
            // `[NNN / errcode]` ruma prefix; `oidc.rs::run` and
            // `whoami.rs::run` already use it. Password login was
            // the missed call site. The `where_` arg is namespaced
            // with the user (when present) so the log line keeps
            // the operator context that the previous substring
            // match also surfaced.
            let where_ = format!("login_username({user})");
            match classify_sdk_err(&where_, &e) {
                classified @ OnboardError::AuthRejected(_) => Err(classified),
                other => Err(other),
            }
        }
    }
}

fn read_password_from_stdin(flag_set: bool) -> Result<Zeroizing<String>> {
    if !flag_set {
        return Err(OnboardError::BadConfig(
            "password mode requires --password-stdin (the only accepted form to prevent shell-history leaks)"
                .into(),
        ));
    }
    warn!("reading password from stdin (consumed from the first line; never echoed)");
    let stdin = io::stdin();
    let mut line = Zeroizing::new(String::new());
    let mut handle = stdin.lock();
    handle
        .read_line(&mut line)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("read password from stdin: {}", e)))?;
    // Copy the trimmed bytes into a fresh Zeroizing buffer so the
    // input buffer can be zeroed on drop (the `to_string` returns a
    // new heap allocation that is owned by the returned `Zeroizing`).
    let trimmed = Zeroizing::new(line.trim_end_matches(['\n', '\r']).to_string());
    if trimmed.is_empty() {
        // R1-L7: don't echo a trailing newline when the input is
        // empty — the operator didn't produce one (stdin was at
        // EOF, e.g. Ctrl-D). Echoing a newline here would write
        // a stray blank line into the terminal.
        return Err(OnboardError::BadConfig(
            "empty password read from stdin".into(),
        ));
    }
    // Echo a newline so the operator's terminal isn't left mangled
    // (the read_line consumed the operator's trailing newline).
    let _ = io::stderr().write_all(b"\n");
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_password_rejects_when_flag_not_set() {
        let result = read_password_from_stdin(false);
        match result {
            Err(OnboardError::BadConfig(msg)) => {
                assert!(msg.contains("--password-stdin"));
            }
            other => panic!("expected BadConfig, got {:?}", other),
        }
    }
}
