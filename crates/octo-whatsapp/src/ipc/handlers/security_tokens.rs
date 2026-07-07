//! Phase 5 Part A: security RPC handlers.
//!
//! Three methods, mirroring the operator-driven token lifecycle:
//! - `security.rotate_token` — install a new active token, register a
//!   grace entry so the old token continues to verify, and persist the
//!   grace file.
//! - `security.revoke_all_tokens` — incident response: revoke every
//!   active token and clear the grace list.
//! - `security.list_tokens` — snapshot of active + grace state.
//!
//! These handlers DO NOT consult the bearer-auth middleware: by
//! design, an operator with shell access can rotate the daemon's own
//! tokens without first presenting a valid bearer. (The unix socket
//! is `0600`; the operator who can connect is already authorized.)

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct SecurityRotateToken;

#[async_trait::async_trait]
impl RpcHandler for SecurityRotateToken {
    fn name(&self) -> &'static str {
        "security.rotate_token"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let old_token_id = p
            .get("old_token_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing old_token_id"))?;
        let new_secret_hex = p
            .get("new_secret_hex")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing new_secret_hex"))?;
        let grace_ms = p.get("grace_ms").and_then(|v| v.as_i64()).unwrap_or(60_000);
        let label = p.get("label").and_then(|v| v.as_str()).unwrap_or("rotated");

        let entry = h
            .tokens()
            .rotate(old_token_id, new_secret_hex, grace_ms, label)
            .map_err(|e| RpcError::exec_failed(e.to_string()))?;
        // Persist the grace file best-effort; surface storage errors.
        h.tokens()
            .persist_grace()
            .map_err(|e| RpcError::exec_failed(e.to_string()))?;

        Ok(serde_json::json!({
            "old_token_id": entry.old_token_id,
            "new_token_id": entry.new_token_id,
            "grace_expires_at_unix_ms": entry.expires_at_unix_ms,
            "new_bearer": format!("{}.{}", entry.new_token_id, new_secret_hex),
        }))
    }
}

#[derive(Debug)]
pub struct SecurityRevokeAllTokens;

#[async_trait::async_trait]
impl RpcHandler for SecurityRevokeAllTokens {
    fn name(&self) -> &'static str {
        "security.revoke_all_tokens"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let n = h.tokens().revoke_all();
        // Persist (empty) grace file so a restart does not surface
        // stale entries.
        h.tokens()
            .persist_grace()
            .map_err(|e| RpcError::exec_failed(e.to_string()))?;
        Ok(serde_json::json!({
            "revoked_count": n,
        }))
    }
}

#[derive(Debug)]
pub struct SecurityListTokens;

#[async_trait::async_trait]
impl RpcHandler for SecurityListTokens {
    fn name(&self) -> &'static str {
        "security.list_tokens"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let active = h.tokens().list_active();
        let all = h.tokens().list_all();
        let grace = h.tokens().list_grace();
        Ok(serde_json::json!({
            "active": active,
            "all": all,
            "grace": grace,
            "counts": {
                "active": active.len(),
                "all": all.len(),
                "grace": grace.len(),
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig,
    };
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        // Each test gets its own temp data_dir so the on-disk grace
        // file does not bleed across parallel test runs.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let cfg = WhatsAppRuntimeConfig {
            name: "p5a".into(),
            data_dir: path.clone(),
            log_dir: Default::default(),
            socket_dir: Default::default(),
            media_buffer: MediaBufferConfig::default(),
            events: EventsConfig::default(),
            security: SecurityConfig {
                grace_path: Some(path.join("grace.json")),
                ..SecurityConfig::default()
            },
            observability: Default::default(),
            rules: RulesConfig::default(),
        };
        let handle = Daemon::new(cfg).handle();
        // Pin the tempdir to the test's end-of-scope via a leak. The
        // tests are short-lived; leaked TempDirs are reclaimed at
        // process exit.
        std::mem::forget(dir);
        handle
    }

    fn strong_hex(seed: u8) -> String {
        let mut s = String::with_capacity(64);
        for i in 0..32u8 {
            s.push_str(&format!("{:02x}", seed.wrapping_add(i)));
        }
        s
    }

    #[tokio::test]
    async fn rotate_produces_grace_entry_and_persists() {
        let h = handle();
        let old_secret = strong_hex(0x10);
        let old_id = crate::security::tokens::derive_token_id(&old_secret);
        h.tokens()
            .load_from_value(&format!("{old_id}.{old_secret}"), Some("seed"))
            .unwrap();

        let new_secret = strong_hex(0x20);
        let r = SecurityRotateToken
            .call(
                h.clone(),
                serde_json::json!({
                    "old_token_id": old_id,
                    "new_secret_hex": new_secret,
                    "grace_ms": 5000,
                    "label": "rotated-v2",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["old_token_id"], old_id);
        assert!(r["new_token_id"].is_string());
        assert!(r["grace_expires_at_unix_ms"].as_i64().unwrap() > 0);

        // Grace entry visible via list.
        let list = SecurityListTokens
            .call(h.clone(), Value::Null)
            .await
            .unwrap();
        assert_eq!(list["grace"].as_array().unwrap().len(), 1);
        assert_eq!(list["counts"]["grace"], 1);
    }

    #[tokio::test]
    async fn rotate_rejects_weak_new_secret() {
        let h = handle();
        let old_secret = strong_hex(0x30);
        let old_id = crate::security::tokens::derive_token_id(&old_secret);
        h.tokens()
            .load_from_value(&format!("{old_id}.{old_secret}"), None)
            .unwrap();

        let r = SecurityRotateToken
            .call(
                h.clone(),
                serde_json::json!({
                    "old_token_id": old_id,
                    "new_secret_hex": "deadbeef",
                    "grace_ms": 60_000,
                    "label": "x",
                }),
            )
            .await;
        assert!(r.is_err(), "expected error for weak secret");
    }

    #[tokio::test]
    async fn revoke_all_clears_active_and_grace() {
        let h = handle();
        let old_secret = strong_hex(0x40);
        let old_id = crate::security::tokens::derive_token_id(&old_secret);
        h.tokens()
            .load_from_value(&format!("{old_id}.{old_secret}"), None)
            .unwrap();
        h.tokens()
            .rotate(&old_id, &strong_hex(0x41), 60_000, "v2")
            .unwrap();
        let r = SecurityRevokeAllTokens
            .call(h.clone(), Value::Null)
            .await
            .unwrap();
        assert!(r["revoked_count"].as_u64().unwrap() >= 1);

        let list = SecurityListTokens.call(h, Value::Null).await.unwrap();
        assert_eq!(list["counts"]["active"], 0);
        assert_eq!(list["counts"]["grace"], 0);
    }

    #[tokio::test]
    async fn list_returns_active_tokens() {
        let h = handle();
        let secret = strong_hex(0x50);
        let id = crate::security::tokens::derive_token_id(&secret);
        h.tokens()
            .load_from_value(&format!("{id}.{secret}"), Some("primary"))
            .unwrap();
        let r = SecurityListTokens.call(h, Value::Null).await.unwrap();
        let active = r["active"].as_array().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0]["token_id"], id);
        assert_eq!(active[0]["label"], "primary");
    }

    #[tokio::test]
    async fn verify_works_during_grace_for_both_old_and_new() {
        let h = handle();
        let old_secret = strong_hex(0x60);
        let old_id = crate::security::tokens::derive_token_id(&old_secret);
        h.tokens()
            .load_from_value(&format!("{old_id}.{old_secret}"), None)
            .unwrap();
        let new_secret = strong_hex(0x61);
        let r = SecurityRotateToken
            .call(
                h.clone(),
                serde_json::json!({
                    "old_token_id": old_id,
                    "new_secret_hex": new_secret,
                    "grace_ms": 60_000,
                    "label": "v2",
                }),
            )
            .await
            .unwrap();
        let new_id = r["new_token_id"].as_str().unwrap();

        // Old token verifies.
        let d_old = h
            .tokens()
            .verify(&format!("{old_id}.{old_secret}"))
            .unwrap();
        assert_eq!(d_old.token_id, old_id);
        // New token verifies.
        let d_new = h
            .tokens()
            .verify(&format!("{new_id}.{new_secret}"))
            .unwrap();
        assert_eq!(d_new.token_id, new_id);
    }
}
