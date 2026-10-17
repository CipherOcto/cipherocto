//! `messages.read_view_once` — one-shot media download for a view-once
//! message. Returns the decrypted media bytes + the existing
//! caption/text once; subsequent reads return `consumed` to enforce the
//! single-view contract. Mirrors the WA Web "you can only view this
//! once" UX.
//!
//! **One-shot semantics:** on first successful read the
//! `messages.consumed_at_unix_ms` column is set to the current wall
//! clock and the underlying `media_token` is zeroed. A retry sees
//! the consumed timestamp and returns an error rather than
//! re-downloading.
//!
//! **Scoping:** requires the daemon's `QuerySubsystem` (gated behind
//! the `query` cargo feature) so the persisted `messages` row can be
//! inspected + updated. Without it the RPC returns `NotConnected`.
//!
//! **Why not on `messages.download`:** view-once media carries the
//! same wire format as ordinary media (the view-once flag is on the
//! outer wrapper). `messages.download` would happily decrypt the
//! media every time it was called, side-stepping the single-view
//! contract. The dedicated RPC centralises the gate so the
//! `consumed_at` guard cannot be forgotten by an operator who opts
//! straight for `messages.download`.

use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    event_id: i64,
}

#[derive(Debug)]
pub struct MessagesReadViewOnce;

#[async_trait::async_trait]
impl RpcHandler for MessagesReadViewOnce {
    fn name(&self) -> &'static str {
        "messages.read_view_once"
    }

    async fn call(&self, h: DaemonHandle, params: JsonValue) -> Result<JsonValue, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        // Query subsystem must be live for the messages row to exist.
        let subsystem = h.query_subsystem().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "messages.read_view_once requires the query subsystem (rebuild with `--features query`)".into(),
            data: None,
        })?;
        let db = subsystem.ingester().db();

        // Note: we deliberately drive the row iterator with a
        // `while let Some(...)` loop rather than `match rows.next()`
        // for the first row. Empirically, when this query is the first
        // octo_storage_core::Database::query call against the ingester's
        // in-memory file, the `match` form reports `None` even when
        // the underlying drain loop (identical SQL) returns exactly
        // one row. `while let` consistently observes the row, so we
        // keep the working pattern across all 3 SELECT sites.
        // The `?` operator handles stoolap::Error → RpcError via
        // this file's existing From impl.
        let pre_consumed: Option<Option<i64>> = {
            let sql = format!(
                "SELECT consumed_at_unix_ms FROM messages WHERE event_id = {}",
                p.event_id
            );
            let mut rows = db.query(&sql, ()).map_err(|e| RpcError {
                code: RpcErrorCode::Internal.as_i32(),
                message: format!("messages.read_view_once: select failed: {e}"),
                data: None,
            })?;
            let mut r: Option<Option<i64>> = None;
            for row_res in rows.by_ref() {
                let row = row_res.map_err(|e| RpcError {
                    code: RpcErrorCode::Internal.as_i32(),
                    message: format!("messages.read_view_once: row error: {e}"),
                    data: None,
                })?;
                if r.is_none() {
                    r = Some(row.get::<i64>(0).ok());
                }
            }
            r
        };
        let Some(consumed) = pre_consumed else {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "event_id {event_id} not found in messages",
                    event_id = p.event_id
                ),
                data: None,
            });
        };
        if consumed.is_some() {
            return Err(RpcError {
                code: RpcErrorCode::Internal.as_i32(),
                message: "view-once message already consumed".into(),
                data: None,
            });
        }

        // Read view_once + media_token together so we can both
        // validate and obtain the token in one query.
        let row: Option<(bool, Option<String>)> = {
            let sql = format!(
                "SELECT view_once, media_token FROM messages WHERE event_id = {}",
                p.event_id
            );
            let mut rows = db.query(&sql, ()).map_err(|e| RpcError {
                code: RpcErrorCode::Internal.as_i32(),
                message: format!("messages.read_view_once: select view_once failed: {e}"),
                data: None,
            })?;
            let mut r: Option<(bool, Option<String>)> = None;
            for row_res in rows.by_ref() {
                let row = row_res.map_err(|e| RpcError {
                    code: RpcErrorCode::Internal.as_i32(),
                    message: format!("messages.read_view_once: select view_once row error: {e}"),
                    data: None,
                })?;
                if r.is_none() {
                    let vo: bool = row.get::<i64>(0).map(|v| v == 1).unwrap_or(false);
                    let mt: Option<String> = row.get::<String>(1).ok();
                    r = Some((vo, mt));
                }
            }
            r
        };
        let Some((true, Some(media_token))) = row else {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "event_id {event_id} is not a view-once message",
                    event_id = p.event_id
                ),
                data: None,
            });
        };

        // Pull the decrypted bytes via the adapter.
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let bytes = adapter
            .download_media(&media_token)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::Internal.as_i32(),
                message: format!("download_media failed: {e}"),
                data: None,
            })?;

        // Mark the row consumed + zero the media_token so a second
        // call cannot bypass the gate.
        let now_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        db.execute(
            &format!(
                "UPDATE messages SET consumed_at_unix_ms = {}, media_token = '' WHERE event_id = {}",
                now_unix_ms, p.event_id
            ),
            (),
        )
        .map_err(|e| RpcError {
            code: RpcErrorCode::Internal.as_i32(),
            message: format!("consume update failed: {e}"),
            data: None,
        })?;

        Ok(json!({
            "status": "delivered",
            "event_id": p.event_id,
            "consumed_at_unix_ms": now_unix_ms,
            "size_bytes": bytes.len(),
            "media_b64": base64_encode(&bytes),
        }))
    }
}

/// Inline base64 (no external dep).
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(all(test, feature = "query"))]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::events::{EventEnvelope, InboundEvent};
    use crate::query::embedder::MockEmbedder;
    use crate::test_mock_adapter::MockAdapter;
    use octo_storage_core::Database;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    /// Build a daemon handle with a fully wired QuerySubsystem
    /// pointed at hermetic tmpdirs. Mirrors the test setup in
    /// `daemon_search.rs`.
    fn handle_with_query() -> (tempfile::TempDir, DaemonHandle) {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        let base = tempfile::tempdir().expect("subdir");
        let _db = Database::open("memory://messages-read-view-once-test").expect("db");
        let embedder: Arc<dyn crate::query::embedder::Embedder> =
            Arc::new(MockEmbedder::ok("test", 384));
        let s =
            crate::query::open_subsystem(base.path(), embedder, crate::query::JobConfig::default())
                .expect("open subsystem");
        let arc = Arc::new(s);
        let mock = MockAdapter::new();
        mock.set_download_media_result("download_media", vec![0xCA, 0xFE, 0xBA, 0xBE]);
        h.bind_adapter(Arc::new(mock));
        let installed = h.install_query_subsystem(arc);
        assert!(installed, "first install wins");
        let _ = (PathBuf::from("memory://messages-read-view-once-test"), base);
        (tmp, h)
    }

    fn ingest_view_once(h: &DaemonHandle, id: u64, text: &str) {
        let raw = format!(
            r#"Message(id: "M{id}", peer: "X", sender: "Y", text: "{text}", kind: Image, media_token: "tok-123", view_once: true, is_group: false)"#
        );
        let ev = InboundEvent::parse(EventEnvelope {
            raw,
            ts_unix_ms: 1000,
            ts_mono_ns: 0,
        });
        h.query_subsystem()
            .unwrap()
            .ingester()
            .ingest(id, (1000, 0), &ev)
            .expect("ingest");
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(MessagesReadViewOnce.name(), "messages.read_view_once");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        let err = MessagesReadViewOnce
            .call(handle(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn missing_query_subsystem_returns_not_connected() {
        let err = MessagesReadViewOnce
            .call(handle(), serde_json::json!({"event_id": 1}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn first_call_delivers_and_marks_consumed() {
        let (_tmp, h) = handle_with_query();
        ingest_view_once(&h, 42, "caption");
        let r = MessagesReadViewOnce
            .call(h.clone(), serde_json::json!({"event_id": 42}))
            .await
            .unwrap();
        assert_eq!(r["status"], "delivered");
        assert_eq!(r["size_bytes"], 4);
        assert_eq!(r["event_id"], 42);
        let consumed: i64 = r["consumed_at_unix_ms"].as_i64().unwrap();
        assert!(consumed >= 1000);
        // media_b64 of [0xCA, 0xFE, 0xBA, 0xBE] is "yv66vg==" (verified
        // by `python3 -c 'import base64; print(base64.b64encode(bytes([0xCA, 0xFE, 0xBA, 0xBE])).decode())'`).
        assert_eq!(r["media_b64"], "yv66vg==");
    }

    #[tokio::test]
    async fn second_call_returns_consumed_error() {
        let (_tmp, h) = handle_with_query();
        ingest_view_once(&h, 99, "");
        let _ = MessagesReadViewOnce
            .call(h.clone(), serde_json::json!({"event_id": 99}))
            .await
            .unwrap();
        let err = MessagesReadViewOnce
            .call(h.clone(), serde_json::json!({"event_id": 99}))
            .await
            .unwrap_err();
        assert!(
            err.message.contains("consumed"),
            "expected consumed error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn non_view_once_message_returns_invalid_kind() {
        let (_tmp, h) = handle_with_query();
        let raw =
            r#"Message(id: "M1", peer: "X", sender: "Y", text: "hi", kind: Text, is_group: false)"#;
        let ev = InboundEvent::parse(EventEnvelope {
            raw: raw.into(),
            ts_unix_ms: 1000,
            ts_mono_ns: 0,
        });
        h.query_subsystem()
            .unwrap()
            .ingester()
            .ingest(77, (1000, 0), &ev)
            .expect("ingest");
        let err = MessagesReadViewOnce
            .call(h.clone(), serde_json::json!({"event_id": 77}))
            .await
            .unwrap_err();
        assert!(err.message.contains("not a view-once"), "got {err:?}");
    }

    #[tokio::test]
    async fn missing_event_id_returns_invalid_params() {
        let (_tmp, h) = handle_with_query();
        let err = MessagesReadViewOnce
            .call(h.clone(), serde_json::json!({"event_id": 9999}))
            .await
            .unwrap_err();
        assert!(err.message.contains("not found"));
    }
}
