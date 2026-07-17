//! `contacts.get_lid_pn_mappings` — batch LID → phone-number resolution.
//!
//! Inverse of `contacts.get_pn_lid_mappings` (which is one-way PN → LID).
//! Wire shape: wacore's `Client::contacts().is_on_whatsapp(&[Jid])` —
//! when given LID-form JIDs, the server returns
//! `<user jid="NN@lid" pn_jid="MM@s.whatsapp.net">`. The fork's
//! `IsOnWhatsAppSpec::Lid` sends `context="interactive"` +
//! `<query><lid/><business><verified_name/></business></query>` (see
//! `wacore::iq::usync::IsOnWhatsAppQueryType::Lid`) and the parser
//! reads `pn_jid` from the response (see `IsOnWhatsAppSpec::parse_response`
//! + `test_is_on_whatsapp_spec_parse_pn_jid` unit test).
//!
//! **Direction: LID → PN.** This is the direction the recent
//! `get_pn_lid_mappings` rename explicitly cannot service — its
//! `LidQuerySpec` is hardwired to send PN-form `<user>` nodes
//! (`build_user_nodes` branches on `user.jid.is_pn()`).
//!
//! Request: `{ "lids": ["108074580897808", "108074580897808@lid", ...] }`
//! Response:
//! ```json
//! {
//!   "mappings": [
//!     {"lid": "108074580897808", "phone_number": "5521995544743"},
//!     ...
//!   ],
//!   "not_resolved": ["...", ...],    // lids the server couldn't map
//!   "requested_count": 39,
//!   "resolved_count": 22
//! }
//! ```
//!
//! Privacy-hidden LIDs land in `not_resolved` — the WA server
//! refuses to disclose a phone number for a contact whose privacy
//! settings gate it from the operator. This is not an error —
//! the caller treats `mappings + not_resolved` as a complete answer.
//!
//! **Tier 7.J.2 of the live coverage matrix.**

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// List of LID user parts (`108074580897808`) or full
    /// `@lid` JIDs. The handler normalizes both forms via
    /// `wacore_binary::Jid::parse`. Unparseable entries are
    /// logged and skipped.
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
                "expected_format": r#"{"lids": ["108074580897808", "108074580897808@lid", ...]}"#
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

        // Mirror the WA server's usync batch cap (~100 users per
        // IQ). Larger requests still succeed on the wire but risk
        // server-side truncation.
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
            })?;
        }

        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;

        // Snapshot input so we can compute `not_resolved`.
        let requested: Vec<String> = p.lids.clone();

        // Adapter returns (input_jid, Option<pn_jid>, is_registered)
        // triples derived from the WA server's LID-form `<user>`
        // responses carrying `pn_jid=...@s.whatsapp.net`.
        let results = adapter
            .is_on_whatsapp_batch(p.lids)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("contacts.get_lid_pn_mappings failed: {e}"),
                data: Some(json!({"requested_count": requested.len()})),
            })?;

        // Build a set of LIDs the server actually resolved, then diff.
        // Match against the bare digits the server would echo back:
        // strip `:device`, `@lid`, any trailing device suffix so e.g.
        // `108074580897808:42` matches `108074580897808`.
        let resolved_lids: std::collections::HashSet<String> = results
            .iter()
            .filter_map(|(jid, pn_jid, is_registered)| {
                if *is_registered && pn_jid.is_some() {
                    Some(strip_jid_decoration(jid))
                } else {
                    None
                }
            })
            .collect();

        let mut mappings = Vec::with_capacity(resolved_lids.len());
        for (jid, pn_jid, is_registered) in &results {
            if !is_registered || pn_jid.is_none() {
                continue;
            }
            // The pn_jid is always a `@s.whatsapp.net` form per
            // wacore's parser; strip the server-side decoration to
            // give the caller a bare digits phone number.
            let lid = strip_jid_decoration(jid);
            let phone_number = strip_jid_decoration(pn_jid.as_ref().unwrap());
            mappings.push(json!({"lid": lid, "phone_number": phone_number}));
        }

        let not_resolved: Vec<String> = requested
            .into_iter()
            .filter(|raw| {
                let stripped = strip_jid_decoration(raw);
                !resolved_lids.contains(&stripped)
            })
            .collect();

        Ok(json!({
            "mappings": mappings,
            "not_resolved": not_resolved,
            "requested_count": requested_count,
            "resolved_count": resolved_lids.len(),
        }))
    }
}

/// Strip `@s.whatsapp.net` / `@lid` server and any `:device` suffix
/// from a JID string, leaving just the user part (bare digits for
/// both LID and PN). Accepts `@c.us` (group / broadcast forms) and
/// `+` prefixes without panicking.
fn strip_jid_decoration(raw: &str) -> String {
    raw.split('@')
        .next()
        .unwrap_or(raw)
        .split(':')
        .next()
        .unwrap_or(raw)
        .trim_start_matches('+')
        .to_string()
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
        let mut lids: Vec<String> = (0..150).map(|i| format!("1080{i:010}")).collect();
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
            .call(handle(), serde_json::json!({ "lids": ["108074580897808"] }))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock_dedupes_mappings() {
        // Mock adapter's is_on_whatsapp_batch returns an empty Vec
        // — verify the handler correctly bucket-sorts requested vs
        // resolved: every requested lid lands in `not_resolved`,
        // `mappings` is empty.
        let r = ContactsGetLidPnMappings
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "lids": ["108074580897808", "108074580897809@lid"]
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["mappings"], serde_json::json!([]));
        assert_eq!(
            r["not_resolved"],
            serde_json::json!(["108074580897808", "108074580897809@lid"])
        );
        assert_eq!(r["requested_count"], serde_json::json!(2));
        assert_eq!(r["resolved_count"], serde_json::json!(0));
    }

    #[test]
    fn strip_jid_decoration_handles_known_forms() {
        assert_eq!(
            strip_jid_decoration("108074580897808@lid"),
            "108074580897808"
        );
        assert_eq!(
            strip_jid_decoration("108074580897808:42"),
            "108074580897808"
        );
        assert_eq!(
            strip_jid_decoration("108074580897808@lid:42"),
            "108074580897808"
        );
        assert_eq!(strip_jid_decoration("+5521995544743"), "5521995544743");
        assert_eq!(
            strip_jid_decoration("5521995544743@s.whatsapp.net"),
            "5521995544743"
        );
        assert_eq!(strip_jid_decoration("108074580897808"), "108074580897808");
    }
}
