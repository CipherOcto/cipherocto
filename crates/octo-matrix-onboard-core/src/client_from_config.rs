//! Read an on-disk 0850h-a / 0850h-c config and rebuild a logged-in
//! `Client` ready for follow-up SDK calls.
//!
//! Mission 0850h-b / 0850h-d extended the original `octo-matrix-onboard
//! login` flow to keep writing the legacy JSON config. Several
//! follow-up subcommands (the e2ee flows, the `whoami` CLI command)
//! need to read that file and reconstruct an authenticated
//! `matrix_sdk::Client`. R1-M14: the reconstruction logic was
//! duplicated in `octo-matrix-onboard/src/modes/e2ee.rs` and
//! `octo-matrix-onboard/src/whoami.rs` with subtle differences
//! (serde_json::Value lookups vs. typed deserialization). This
//! module is the single canonical implementation; both call sites
//! now delegate here.
//!
//! R2-M1: errors are returned as typed `crate::CoreError` variants
//! so callers (the CLI's `e2ee` and `whoami` modules) can `match`
//! on the kind instead of substring-matching on the message.

use crate::CoreError;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::ruma::{OwnedDeviceId, OwnedUserId};
use matrix_sdk::{Client, SessionMeta, SessionTokens};
use serde::Deserialize;
use std::path::Path;

/// On-disk config shape produced by `octo-matrix-onboard login` and
/// consumed by the adapter. The set of required fields is the
/// 0850h-a contract; the on-disk JSON is otherwise a superset of
/// the adapter's `MatrixConfig` (the adapter adds
/// `use_session_store` / `session_store_path` / `force_writeback`
/// / `passphrase` knobs that the CLI does not write).
#[derive(Debug, Clone, Deserialize)]
pub struct OnboardConfig {
    pub homeserver_url: String,
    pub user_id: String,
    pub device_id: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Optional rooms list — kept for forward compatibility with
    /// the adapter's `MatrixConfig::rooms` field. The CLI's
    /// `output::write` always emits `rooms: []`; the adapter
    /// populates this from a host-process config that lists
    /// rooms to auto-join.
    #[serde(default)]
    pub rooms: Vec<String>,
}

/// Read a JSON config file and rebuild a logged-in
/// `matrix_sdk::Client`. Returns the original `Client` (the caller
/// owns the handle) ready for SDK calls like `client.whoami()` or
/// `client.encryption().bootstrap_cross_signing(...)`.
pub async fn client_from_config(path: &Path) -> Result<Client, CoreError> {
    let bytes = std::fs::read(path).map_err(|source| CoreError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let cfg: OnboardConfig = serde_json::from_slice(&bytes).map_err(|source| CoreError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    cfg.build_client().await
}

impl OnboardConfig {
    /// Build a `MatrixSession` from the in-memory config. The caller
    /// supplies a `Client::builder()` so the call site can wire
    /// additional knobs (e.g. `handle_refresh_tokens()` for the
    /// `whoami` path). R1-M14: this replaces the duplicated
    /// `MatrixSession { meta, tokens }` construction that used to
    /// live in both `whoami.rs` and `e2ee.rs`.
    ///
    /// R1-L6: `user_id` is validated via `OwnedUserId::try_from`
    /// (the Matrix MXID grammar is well-defined; an invalid one
    /// would surface as a server-side error). `device_id` uses the
    /// infallible `From<&str>` because ruma's
    /// `OwnedDeviceId::validate` is a no-op (device IDs are opaque
    /// per the Matrix spec — any string is valid). The asymmetry
    /// is intentional; we surface it here so future readers don't
    /// try to "fix" it by adding a redundant `try_from` for the
    /// device_id.
    pub fn build_session(&self) -> Result<MatrixSession, CoreError> {
        let user_id = OwnedUserId::try_from(self.user_id.as_str()).map_err(|source| {
            CoreError::InvalidUserId {
                value: self.user_id.clone(),
                source,
            }
        })?;
        let device_id = OwnedDeviceId::from(self.device_id.as_str());
        Ok(MatrixSession {
            meta: SessionMeta { user_id, device_id },
            tokens: SessionTokens {
                access_token: self.access_token.clone(),
                refresh_token: self.refresh_token.clone(),
            },
        })
    }

    /// Build a logged-in `matrix_sdk::Client` from in-memory config
    /// fields. Convenience wrapper over `build_session` +
    /// `Client::builder().build()` + `restore_session`. Callers that
    /// need to add `handle_refresh_tokens()` or other builder
    /// methods should use `build_session` directly.
    pub async fn build_client(&self) -> Result<Client, CoreError> {
        let session = self.build_session()?;
        let client = Client::builder()
            .homeserver_url(&self.homeserver_url)
            .build()
            .await
            .map_err(|source| CoreError::ClientBuild {
                homeserver: self.homeserver_url.clone(),
                source: Box::new(source),
            })?;
        client
            .restore_session(session)
            .await
            .map_err(|e| CoreError::RestoreSession(Box::new(e)))?;
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_config(dir: &TempDir, json: &str) -> std::path::PathBuf {
        let path = dir.path().join("matrix.json");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        f.write_all(json.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parse_minimal_config() {
        let j = r#"{
            "homeserver_url": "https://matrix.example.com",
            "user_id": "@bot:matrix.example.com",
            "device_id": "ABCDEFGHIJ",
            "access_token": "syt_abcdefgh"
        }"#;
        let parsed: OnboardConfig = serde_json::from_str(j).unwrap();
        assert_eq!(parsed.user_id, "@bot:matrix.example.com");
        assert!(parsed.refresh_token.is_none());
        assert!(parsed.rooms.is_empty());
    }

    #[test]
    fn parse_with_refresh_token_and_rooms() {
        let j = r#"{
            "homeserver_url": "https://matrix.example.com",
            "user_id": "@bot:matrix.example.com",
            "device_id": "ABCDEFGHIJ",
            "access_token": "syt_x",
            "refresh_token": "syr_y",
            "rooms": ["!abc:matrix.example.com"]
        }"#;
        let parsed: OnboardConfig = serde_json::from_str(j).unwrap();
        assert_eq!(parsed.refresh_token.as_deref(), Some("syr_y"));
        assert_eq!(parsed.rooms, vec!["!abc:matrix.example.com"]);
    }

    #[test]
    fn reject_missing_required_field() {
        let j = r#"{
            "homeserver_url": "https://matrix.example.com",
            "user_id": "@bot:matrix.example.com",
            "device_id": "ABCDEFGHIJ"
        }"#;
        let parsed: std::result::Result<OnboardConfig, _> = serde_json::from_str(j);
        assert!(parsed.is_err());
    }

    /// Network round-trip: building a `Client` against a real
    /// homeserver is not exercised here (the integration test in
    /// `octo-adapter-matrix-sdk/tests/integration_matrix.rs` does
    /// that). The test that `client_from_config` calls into
    /// `Client::builder` and `restore_session` is covered by the
    /// `whoami` end-to-end test on a running synapse.

    #[test]
    fn write_to_disk_and_read_back_parses() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"{
                "homeserver_url": "https://matrix.example.com",
                "user_id": "@bot:matrix.example.com",
                "device_id": "ABCDEFGHIJ",
                "access_token": "syt_x",
                "refresh_token": "syr_y"
            }"#,
        );
        let bytes = std::fs::read(&path).unwrap();
        let parsed: OnboardConfig = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.user_id, "@bot:matrix.example.com");
        assert_eq!(parsed.refresh_token.as_deref(), Some("syr_y"));
    }

    /// R2-M1: the error variants from `client_from_config` are
    /// typed. A missing file surfaces as `CoreError::Read`, a bad
    /// JSON shape as `CoreError::Parse`, and a malformed user_id
    /// as `CoreError::InvalidUserId`. The CLI uses these to route
    /// to the right exit code (read/parse/invalid → `BadConfig`,
    /// exit 5; SDK build / restore → `Generic`).
    #[tokio::test]
    async fn client_from_config_missing_file_returns_read_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let err = client_from_config(&path).await.expect_err("must error");
        match err {
            CoreError::Read { path: p, .. } => {
                assert_eq!(p, path);
            }
            other => panic!("expected CoreError::Read, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn client_from_config_bad_json_returns_parse_error() {
        let dir = TempDir::new().unwrap();
        let path = write_config(&dir, "{ this is not valid json");
        let err = client_from_config(&path).await.expect_err("must error");
        match err {
            CoreError::Parse { path: p, .. } => {
                assert_eq!(p, path);
            }
            other => panic!("expected CoreError::Parse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn client_from_config_bad_user_id_returns_invalid_user_id_error() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"{
                "homeserver_url": "https://matrix.example.com",
                "user_id": "not-an-mxid",
                "device_id": "ABCDEFGHIJ",
                "access_token": "syt_x"
            }"#,
        );
        let err = client_from_config(&path).await.expect_err("must error");
        match err {
            CoreError::InvalidUserId { value, .. } => {
                assert_eq!(value, "not-an-mxid");
            }
            other => panic!("expected CoreError::InvalidUserId, got {other:?}"),
        }
    }
}
