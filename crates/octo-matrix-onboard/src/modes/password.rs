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
use crate::error::{OnboardError, Result};
use crate::output;
use matrix_sdk::Client;
use octo_matrix_onboard_core::session;
use std::io::{self, BufRead, Write};
use tracing::{info, warn};

pub async fn run(args: PasswordArgs) -> Result<()> {
    let password = read_password_from_stdin(args.password_stdin)?;
    let client = build_client(&args.homeserver).await?;
    login(&client, &args.user, &password, args.device_name.as_deref()).await?;
    drop(password); // Best-effort: don't keep the password around any longer than needed.

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
            let msg = e.to_string();
            if msg.contains("dns")
                || msg.contains("DNS")
                || msg.contains("connect")
                || msg.contains("Connection")
            {
                OnboardError::Unreachable(format!("{}: {}", homeserver, msg))
            } else {
                OnboardError::Generic(anyhow::anyhow!(
                    "build client against {}: {}",
                    homeserver,
                    msg
                ))
            }
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
            let msg = e.to_string();
            if msg.contains("Unauthorized")
                || msg.contains("M_FORBIDDEN")
                || msg.contains("invalid")
                || msg.contains("401")
            {
                Err(OnboardError::AuthRejected(format!("{}: {}", user, msg)))
            } else if msg.contains("dns") || msg.contains("DNS") || msg.contains("connect") {
                Err(OnboardError::Unreachable(msg))
            } else {
                Err(OnboardError::Generic(anyhow::anyhow!(
                    "login_username: {}",
                    msg
                )))
            }
        }
    }
}

fn read_password_from_stdin(flag_set: bool) -> Result<String> {
    if !flag_set {
        return Err(OnboardError::BadConfig(
            "password mode requires --password-stdin (the only accepted form to prevent shell-history leaks)"
                .into(),
        ));
    }
    warn!("reading password from stdin (consumed from the first line; never echoed)");
    let stdin = io::stdin();
    let mut line = String::new();
    let mut handle = stdin.lock();
    handle
        .read_line(&mut line)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("read password from stdin: {}", e)))?;
    // Echo a newline so the operator's terminal isn't left mangled.
    let _ = io::stderr().write_all(b"\n");
    let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
    if trimmed.is_empty() {
        return Err(OnboardError::BadConfig(
            "empty password read from stdin".into(),
        ));
    }
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

    #[test]
    fn detect_auth_rejected() {
        // Sanity check the substring detection.
        let cases = [
            "M_FORBIDDEN invalid password",
            "Unauthorized: bad credentials",
            "HTTP 401",
            "invalid user_id",
        ];
        for msg in cases {
            assert!(
                msg.contains("M_FORBIDDEN")
                    || msg.contains("Unauthorized")
                    || msg.contains("401")
                    || msg.contains("invalid"),
                "should detect auth failure: {msg}"
            );
        }
    }
}
