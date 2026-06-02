//! OIDC / SSO login flows.
//!
//! Mission 0850h-a §Acceptance Criteria:
//! - `octo-matrix-onboard login oidc` — OAuth 2.0 Authorization Code
//!   flow via `OAuth::login_with_authorization_code()`; localhost
//!   callback listener on `127.0.0.1:port`; `--no-listener` mode for
//!   headless servers.
//! - `octo-matrix-onboard login sso` — modern Matrix SSO via
//!   `OAuth::login_sso()` (MSC 2964 / MSC 3861); same listener pattern.
//!
//! Implementation note: matrix-rust-sdk 0.17.0's `OAuth` module
//! provides a single `OAuth::login()` that takes a redirect URI and
//! drives the Authorization Code flow. Modern Matrix SSO (MSC 3861)
//! IS OIDC with a different prompt; the legacy `MatrixAuth::login_sso`
//! is a separate flow that's behind the `sso-login` feature. We
//! implement OIDC and SSO on the same code path (`OAuth::login()`),
//! matching what the SDK ships and avoiding the extra `sso-login`
//! feature dependency on `axum`/`rand`/`tower`.

use crate::cli::OidcArgs;
use crate::error::{OnboardError, Result};
use crate::output;
use language_tags::LanguageTag;
use matrix_sdk::authentication::oauth::registration::{
    ApplicationType, ClientMetadata, Localized, OAuthGrantType,
};
use matrix_sdk::authentication::oauth::ClientRegistrationData;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::OwnedDeviceId;
use matrix_sdk::utils::UrlOrQuery;
use matrix_sdk::Client;
use octo_matrix_onboard_core::oauth_listener::{self, CallbackResult};
use octo_matrix_onboard_core::session;
use std::io::{self, BufRead, Write};
use tracing::info;
use url::Url;

const CLI_CLIENT_URI: &str = "https://github.com/cipherocto/octo-matrix-onboard";

pub async fn run(args: OidcArgs, _sso: bool) -> Result<()> {
    let client = build_client(&args.homeserver).await?;
    let redirect_uri = redirect_uri(args.port);
    let device_id = args
        .device_name
        .as_ref()
        .map(|name| OwnedDeviceId::from(name.as_str()));

    let registration_data = build_registration_data(&redirect_uri)?;
    let auth_data = client
        .oauth()
        .login(
            redirect_uri.clone(),
            device_id,
            Some(registration_data),
            None,
        )
        .build()
        .await
        .map_err(|e| map_oauth_err("OAuth::login.build()", e))?;

    eprintln!("Open this URL in a browser to authenticate:");
    eprintln!("  {}", auth_data.url);
    eprintln!();
    eprintln!("After approving, the homeserver will redirect to:");
    eprintln!("  {}", redirect_uri);
    eprintln!();

    let callback = if args.no_listener {
        let q = wait_for_pasted_redirect()?;
        CallbackResult::Code {
            raw_query: query_string_from_url_or_query(q),
        }
    } else {
        match oauth_listener::listen_once(args.port).await {
            Ok(c) => c,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("already in use") || msg.contains("Address already in use") {
                    return Err(OnboardError::BadConfig(format!(
                        "port {} already in use; pass --port to override",
                        args.port
                    )));
                }
                return Err(OnboardError::Generic(e));
            }
        }
    };

    let query_or_url = match callback {
        CallbackResult::Code { raw_query } => UrlOrQuery::Query(raw_query),
        CallbackResult::IdpError { code, description } => {
            return Err(OnboardError::AuthRejected(format!(
                "IdP error: {} — {}",
                code, description
            )));
        }
    };

    client
        .oauth()
        .finish_login(query_or_url)
        .await
        .map_err(|e| map_sdk_err("OAuth::finish_login", &e))?;

    let sess = session::extract(&client, &args.homeserver)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("session extract after OIDC: {}", e)))?;
    info!(
        homeserver = %sess.homeserver_url,
        user_id = %sess.user_id,
        device_id = %sess.device_id,
        has_refresh = sess.refresh_token.is_some(),
        sso = _sso,
        "OIDC login complete"
    );
    output::write(&args.output, &sess)
}

fn build_registration_data(redirect_uri: &Url) -> Result<ClientRegistrationData> {
    let url = Url::parse(CLI_CLIENT_URI)
        .map_err(|e| OnboardError::BadConfig(format!("invalid client URI: {}", e)))?;
    let metadata = ClientMetadata::new(
        ApplicationType::Native,
        vec![OAuthGrantType::AuthorizationCode {
            redirect_uris: vec![redirect_uri.clone()],
        }],
        Localized::new(url, std::iter::empty::<(LanguageTag, Url)>()),
    );
    let raw = Raw::new(&metadata)
        .map_err(|e| OnboardError::BadConfig(format!("serialize ClientMetadata: {}", e)))?;
    Ok(ClientRegistrationData::new(raw))
}

fn redirect_uri(port: u16) -> Url {
    Url::parse(&format!("http://127.0.0.1:{}/callback", port)).expect("hard-coded URL is valid")
}

async fn build_client(homeserver: &str) -> Result<Client> {
    Client::builder()
        .homeserver_url(homeserver)
        .build()
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("dns") || msg.contains("DNS") || msg.contains("connect") {
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

/// Wait for the operator to paste the final redirect URL on stdin.
/// Used by `--no-listener` mode (headless servers).
fn wait_for_pasted_redirect() -> Result<UrlOrQuery> {
    eprintln!("Paste the full redirect URL (or just the query string `code=...&state=...`) and press Enter:");
    let stdin = io::stdin();
    let mut line = String::new();
    let mut handle = stdin.lock();
    handle
        .read_line(&mut line)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("read pasted redirect: {}", e)))?;
    let _ = io::stderr().write_all(b"\n");
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(OnboardError::Cancelled(
            "empty stdin; user cancelled (no URL pasted)".into(),
        ));
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let url = Url::parse(trimmed)
            .map_err(|e| OnboardError::BadConfig(format!("invalid redirect URL: {}", e)))?;
        Ok(UrlOrQuery::Url(url))
    } else {
        Ok(UrlOrQuery::Query(trimmed.to_string()))
    }
}

fn query_string_from_url_or_query(q: UrlOrQuery) -> String {
    match q {
        UrlOrQuery::Query(s) => s,
        UrlOrQuery::Url(u) => u.query().unwrap_or("").to_string(),
    }
}

fn map_oauth_err(where_: &str, e: matrix_sdk::authentication::oauth::OAuthError) -> OnboardError {
    let msg = e.to_string();
    if msg.contains("dns") || msg.contains("DNS") || msg.contains("connect") {
        OnboardError::Unreachable(format!("{}: {}", where_, msg))
    } else if msg.contains("access_denied")
        || msg.contains("Unauthorized")
        || msg.contains("rejected")
        || msg.contains("denied")
    {
        OnboardError::AuthRejected(format!("{}: {}", where_, msg))
    } else {
        OnboardError::Generic(anyhow::anyhow!("{}: {}", where_, msg))
    }
}

fn map_sdk_err(where_: &str, e: &matrix_sdk::Error) -> OnboardError {
    let msg = e.to_string();
    if msg.contains("dns") || msg.contains("DNS") || msg.contains("connect") {
        OnboardError::Unreachable(format!("{}: {}", where_, msg))
    } else if msg.contains("access_denied")
        || msg.contains("Unauthorized")
        || msg.contains("rejected")
        || msg.contains("denied")
    {
        OnboardError::AuthRejected(format!("{}: {}", where_, msg))
    } else {
        OnboardError::Generic(anyhow::anyhow!("{}: {}", where_, msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_uri_uses_loopback() {
        let url = redirect_uri(8080);
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(8080));
        assert_eq!(url.path(), "/callback");
    }
}
