//! `octo-matrix-onboard-core` — library half of `octo-matrix-onboard`.
//!
//! Mission 0850h-a: authenticate a human operator against any Matrix
//! homeserver in four modes (password, OIDC, SSO, QR) and write a JSON
//! config file matching the extended `MatrixConfig` schema consumed by
//! `octo-adapter-matrix-sdk`.
//!
//! The binary crate (`octo-matrix-onboard`) imports this lib to drive the
//! actual flows; the integration test also imports it directly so it can
//! run the auth code without spawning a subprocess.

pub mod oauth_listener;
pub mod qrcode_render;
pub mod session;

/// Captured session material — what the SDK returns after a successful
/// login. The on-disk JSON written by the binary is built directly from
/// this struct.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    /// Matrix homeserver URL (e.g. `https://matrix.example.com`).
    pub homeserver_url: String,
    /// Authenticated user MXID (e.g. `@bot:matrix.example.com`).
    pub user_id: String,
    /// Device ID assigned by the homeserver.
    pub device_id: String,
    /// Access token. NOT serialized into the on-disk JSON via this struct
    /// directly — the binary's `output` module emits the JSON manually so
    /// the access_token is included (the adapter's `MatrixConfig` marks
    /// it `#[serde(skip_serializing)]` to prevent the adapter from
    /// rewriting it).
    pub access_token: String,
    /// Refresh token, when the homeserver issued one.
    pub refresh_token: Option<String>,
}
