//! Session capture from the SDK after a successful login.
//!
//! The SDK exposes `client.session()` which returns
//! `Option<AuthSession>` (Matrix-auth or OAuth variants). We extract the
//! four fields the adapter needs: user_id, device_id, access_token,
//! refresh_token.

use crate::Session;
use matrix_sdk::Client;

/// Extract a [`Session`] from an authenticated [`Client`].
///
/// Returns an error if the client has no session restored yet (i.e. the
/// login call hasn't completed or failed silently). The error message is
/// human-readable and intended to surface to the CLI operator.
pub fn extract(client: &Client, homeserver_url: &str) -> anyhow::Result<Session> {
    // matrix-sdk exposes session metadata via session_meta() and tokens
    // via session_tokens(). Both are Option, and present together only
    // after a successful login or restore_session call.
    let meta = client
        .session_meta()
        .ok_or_else(|| anyhow::anyhow!("client has no session_meta — login did not complete"))?;
    let tokens = client
        .session_tokens()
        .ok_or_else(|| anyhow::anyhow!("client has no session_tokens — login did not complete"))?;

    Ok(Session {
        homeserver_url: homeserver_url.to_string(),
        user_id: meta.user_id.to_string(),
        device_id: meta.device_id.to_string(),
        access_token: tokens.access_token.clone(),
        refresh_token: tokens.refresh_token.clone(),
    })
}
