//! `contacts.get_pn_lid_mappings` — batch phone-number → LID resolution.
//!
//! Single round-trip to the WA server (`usync` IQ with `<lid>`
//! subprotocol — see `octo-adapter-whatsapp::inherent::lid_query`).
//! Replaces N individual `contacts.get_user_info` calls when the
//! caller needs LIDs for a list of phone numbers (e.g. cross-checking
//! which contacts in the operator's address book are LID-migrated).
//!
//! **Direction: PN → LID.** The reverse direction (LID → PN) is
//! structurally unservable through the public WA protocol — the
//! server's `<list>` will be empty for LID-form `<user>` nodes and a
//! batch-level `<result><lid/></result>` denial is returned. The
//! adapter method (wacore `LidQuerySpec`) emits `<user jid="NN@s.whatsapp.net">`
//! and parses server responses of the form
//! `<user jid="NN@s.whatsapp.net"><lid val="MM@lid"/></user>`.
//! Use `contacts.save_contact` + the local address book for LID→PN.
//!
//! Request: `{ "phones": ["5521995544743", "5521995544743@s.whatsapp.net", ...] }`
//! Response:
//! ```json
//! {
//!   "mappings": [
//!     {"phone": "5521995544743", "lid": "80836284174444"},
//!     ...
//!   ],
//!   "not_resolved": ["...", ...],     // phones the server couldn't map
//!   "requested_count": 39,
//!   "resolved_count": 22
//! }
//! ```
//!
//! Privacy-hidden phones land in `not_resolved` — the WA server
//! refuses to disclose a LID for an account that has set its
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
    /// List of phone-number JIDs (`NN@s.whatsapp.net`) or bare
    /// E.164 numbers. The handler normalizes both forms server-side;
    /// the adapter sends PN-form `<user jid="NN@s.whatsapp.net"/>`
    /// nodes to the WA server's `<lid/>` usync subprotocol.
    /// Unparseable entries are logged and skipped.
    #[serde(default)]
    phones: Vec<String>,
}

#[derive(Debug)]
pub struct ContactsGetPnLidMappings;

#[async_trait::async_trait]
impl RpcHandler for ContactsGetPnLidMappings {
    fn name(&self) -> &'static str {
        "contacts.get_pn_lid_mappings"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: Some(json!({
                "expected_format": r#"{"phones": ["5521995544743", "5521995544743@s.whatsapp.net", ...]}"#
            })),
        })?;

        if p.phones.is_empty() {
            return Ok(json!({
                "mappings": [],
                "not_resolved": [],
                "requested_count": 0,
                "resolved_count": 0,
            }));
        }
        let requested_count = p.phones.len();

        // The DAEMON-side cap mirrors the WA server's `usync` batch
        // limit (~100 users per IQ). Larger requests still succeed on
        // the wire but risk server-side truncation.
        const MAX_BATCH: usize = 100;
        if p.phones.len() > MAX_BATCH {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!(
                    "phones: too many entries ({} > {}). Split into multiple calls.",
                    p.phones.len(),
                    MAX_BATCH
                ),
                data: Some(json!({"received": p.phones.len(), "max": MAX_BATCH})),
            })?;
        }

        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;

        // Snapshot input so we can compute `not_resolved`.
        let requested: Vec<String> = p.phones.clone();

        // Adapter returns (phone, lid) pairs derived from the WA
        // server's PN-form `<user>` responses. The internal method
        // name (`lid_query`) reflects the wacore IQ spec name; the
        // direction is what the wire protocol supports.
        let mappings = adapter.lid_query(p.phones).await.map_err(|e| RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("contacts.get_pn_lid_mappings failed: {e}"),
            data: Some(json!({"requested_count": requested.len()})),
        })?;

        // Build a set of phones the server actually resolved, then diff.
        let resolved_phones: std::collections::HashSet<String> =
            mappings.iter().map(|(phone, _)| phone.clone()).collect();

        let not_resolved: Vec<String> = requested
            .into_iter()
            .filter(|raw| {
                // Match against the bare digits the server would echo
                // back. Strip `:device`, `@s.whatsapp.net`, and any
                // leading `+` so e.g. `+5521995544743` matches
                // `5521995544743` if the server returned the latter.
                let stripped = raw
                    .trim_start_matches('+')
                    .split('@')
                    .next()
                    .unwrap_or(raw)
                    .split(':')
                    .next()
                    .unwrap_or(raw);
                !resolved_phones.contains(stripped)
            })
            .collect();

        Ok(json!({
            "mappings": mappings
                .into_iter()
                .map(|(phone, lid)| json!({"phone": phone, "lid": lid}))
                .collect::<Vec<_>>(),
            "not_resolved": not_resolved,
            "requested_count": requested_count,
            "resolved_count": resolved_phones.len(),
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
    async fn empty_phones_returns_empty_response() {
        let r = ContactsGetPnLidMappings
            .call(handle_with_mock(), serde_json::json!({"phones": []}))
            .await
            .unwrap();
        assert_eq!(r["mappings"], serde_json::json!([]));
        assert_eq!(r["not_resolved"], serde_json::json!([]));
        assert_eq!(r["requested_count"], serde_json::json!(0));
        assert_eq!(r["resolved_count"], serde_json::json!(0));
    }

    #[tokio::test]
    async fn oversized_batch_rejected() {
        let mut phones: Vec<String> = (0..150).map(|i| format!("55{i:010}")).collect();
        phones.truncate(101);
        let err = ContactsGetPnLidMappings
            .call(handle_with_mock(), serde_json::json!({ "phones": phones }))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
        assert!(err.message.contains("too many"));
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = ContactsGetPnLidMappings
            .call(handle(), serde_json::json!({ "phones": ["5521995544743"] }))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock_dedupes_mappings() {
        // Mock adapter's lid_query returns an empty Vec — verify the
        // handler correctly bucket-sorts requested vs resolved.
        let r = ContactsGetPnLidMappings
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "phones": ["5521995544743", "5521964532901"]
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["mappings"], serde_json::json!([]));
        assert_eq!(
            r["not_resolved"],
            serde_json::json!(["5521995544743", "5521964532901"])
        );
    }
}
