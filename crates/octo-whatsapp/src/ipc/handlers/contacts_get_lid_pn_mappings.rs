//! `contacts.get_lid_pn_mappings` — batch LID → phone-number resolution.
//!
//! Single round-trip to the WA server (`usync` IQ with `<lid>`
//! subprotocol — see `octo-adapter-whatsapp::inherent::lid_query`).
//! Replaces N individual `contacts.get_user_info` calls when the
//! caller needs phone numbers for a list of LIDs (e.g. refreshing the
//! operator's `common_members_*` membership tables after a group info
//! refresh).
//!
//! Request: `{ "lids": ["108074580897808@lid", ...] }`
//! Response:
//! ```json
//! {
//!   "mappings": [
//!     {"lid": "108074580897808", "phone_number": "5521995544743"},
//!     ...
//!   ],
//!   "not_resolved": ["...", ...],     // LIDs the server couldn't map
//!   "requested_count": 39
//! }
//! ```
//!
//! Privacy-hidden LIDs land in `not_resolved` — the WA server refuses
//! to disclose a phone number for an account that has set its
//! privacy settings to "nobody" / "contacts only" and isn't in the
//! operator's contact list. This is not an error — caller treats
//! `mappings + not_resolved` as a complete answer.
//!
//! **Tier 7.J.1 of the live coverage matrix.**

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// List of LID JIDs (any form: `@lid`, `@s.whatsapp.net`, bare
    /// user-part with `@lid` suffix). Unparseable entries are logged
    /// and skipped server-side; resolve-rate is the only ceiling.
    #[serde(default)]
    lids: Vec<String>,
}

#[derive(Debug)]
pub struct ContactsGetLidPnMappings;

#[async_trait::async_trait]
impl RpcHandler for ContactsGetLidPnMappings {
    fn name(&self) -> &'static str {
        "contacts.get_lid_pn_mappings"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: Some(json!({
                "expected_format": r#"{"lids": ["108074580897808@lid", ...]}"#
            })),
        })?;

        if p.lids.is_empty() {
            return Ok(json!({
                "mappings": [],
                "not_resolved": [],
                "requested_count": 0,
                "resolved_count": 0,
            }));
        }
        let requested_count = p.lids.len();

        // The DAEMON-side cap mirrors the WA server's `usync` batch
        // limit (~100 users per IQ). Larger requests still succeed on
        // the wire but risk server-side truncation.
        const MAX_BATCH: usize = 100;
        if p.lids.len() > MAX_BATCH {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "lids: too many entries ({} > {}). Split into multiple calls.",
                    p.lids.len(),
                    MAX_BATCH
                ),
                data: Some(json!({"received": p.lids.len(), "max": MAX_BATCH})),
            });
        }

        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;

        // Snapshot input so we can compute `not_resolved`.
        let requested: Vec<String> = p.lids.clone();

        let mappings = adapter.lid_query(p.lids).await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("contacts.get_lid_pn_mappings failed: {e}"),
            data: Some(json!({"requested_count": requested.len()})),
        })?;

        // Build a set of LIDs the server actually resolved, then diff.
        let resolved_lids: std::collections::HashSet<String> =
            mappings.iter().map(|(_, lid)| lid.clone()).collect();

        let not_resolved: Vec<String> = requested
            .into_iter()
            .filter(|raw| {
                // Match against the server's bare-user-part form.
                // `parsed` strips the `:device` suffix and `@lid`.
                let stripped = raw
                    .split('@')
                    .next()
                    .unwrap_or(raw)
                    .split(':')
                    .next()
                    .unwrap_or(raw);
                !resolved_lids.contains(stripped)
            })
            .collect();

        Ok(json!({
            "mappings": mappings
                .into_iter()
                .map(|(phone_number, lid)| json!({"lid": lid, "phone_number": phone_number}))
                .collect::<Vec<_>>(),
            "not_resolved": not_resolved,
            "requested_count": requested_count,
            "resolved_count": resolved_lids.len(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn handle_with_mock() -> DaemonHandle {
        let h = handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn empty_lids_returns_empty_response() {
        let r = ContactsGetLidPnMappings
            .call(handle_with_mock(), serde_json::json!({"lids": []}))
            .await
            .unwrap();
        assert_eq!(r["mappings"], serde_json::json!([]));
        assert_eq!(r["not_resolved"], serde_json::json!([]));
        assert_eq!(r["requested_count"], serde_json::json!(0));
        assert_eq!(r["resolved_count"], serde_json::json!(0));
    }

    #[tokio::test]
    async fn oversized_batch_rejected() {
        let mut lids: Vec<String> = (0..150).map(|i| format!("{i}@lid")).collect();
        lids.truncate(101);
        let err = ContactsGetLidPnMappings
            .call(handle_with_mock(), serde_json::json!({ "lids": lids }))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
        assert!(err.message.contains("too many"));
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = ContactsGetLidPnMappings
            .call(
                handle(),
                serde_json::json!({ "lids": ["108074580897808@lid"] }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock_dedupes_mappings() {
        // Mock adapter's lid_query returns an empty Vec — verify the
        // handler correctly bucket-sorts requested vs resolved.
        let r = ContactsGetLidPnMappings
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "lids": ["108074580897808@lid", "112382332399848@lid"]
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["mappings"], serde_json::json!([]));
        assert_eq!(
            r["not_resolved"],
            serde_json::json!(["108074580897808@lid", "112382332399848@lid"])
        );
    }
}
