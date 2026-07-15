//! `groups.participants_lid_to_phone` — group-scoped LID → phone-number
//! resolution.
//!
//! Wire shape: a single `w:g2` GroupQueryIq `<query request="interactive"/>`
//! (already wrapped by `client.groups().get_metadata()`), to the operator's
//! group JID. For LID-addressed groups, the server populates a
//! `phone_number=NN@s.whatsapp.net` attribute on every `<participant>`
//! element. WA Web uses exactly this mechanism to populate the
//! phone-number column in its group-member panel for all participants,
//! including non-business accounts.
//!
//! Direction: LID → PN, full-group coverage. **Orthogonal** to
//! `contacts.get_lid_pn_mappings` (which uses usync
//! `IsOnWhatsAppSpec::Lid` and only returns pn_jid for business
//! accounts; see that handler's doc for the wire difference).
//!
//! Request: `{ "group_jid": "120363411021224818@g.us" }`
//! Response:
//! ```json
//! {
//!   "mappings": [
//!     {"lid": "142670844444773", "phone_number": "5521959473159"},
//!     ...
//!   ],
//!   "not_resolved": ["168642025119945@lid", ...],
//!   "resolved_count": 947,
//!   "requested_count": 948,
//!   "group_jid": "120363411021224818@g.us"
//! }
//! ```
//!
//! **Tier 7.K of the live coverage matrix.** Single-group scope,
//! requires the operator to already be a participant. For a
//! not-a-member group, the server returns `Forbidden` and the
//! adapter propagates the error.

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::adapter_trait::OctoWhatsAppAdapter;
use crate::daemon::DaemonHandle;
use octo_network::dot::GroupId;

fn require_adapter(h: &DaemonHandle) -> Result<Arc<dyn OctoWhatsAppAdapter>, RpcError> {
    h.adapter().ok_or(RpcError {
        code: RpcErrorCode::NotConnected.as_i32(),
        message: "no adapter bound to daemon".into(),
        data: None,
    })
}

fn invalid_params(e: serde_json::Error) -> RpcError {
    RpcError {
        code: RpcErrorCode::InvalidParams.as_i32(),
        message: format!("invalid params: {e}"),
        data: None,
    }
}

#[derive(Deserialize)]
struct Params {
    /// Group JID the operator is a member of (e.g. `120363411021224818@g.us`).
    /// Server returns `Forbidden` for non-members.
    group_jid: String,
}

#[derive(Debug)]
pub struct GroupsParticipantsLidToPhone;

#[async_trait::async_trait]
impl RpcHandler for GroupsParticipantsLidToPhone {
    fn name(&self) -> &'static str {
        "groups.participants_lid_to_phone"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(invalid_params)?;

        if p.group_jid.is_empty() {
            return Err(invalid_params(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "group_jid required",
            ))));
        }

        let adapter = require_adapter(&h)?;
        let coord = adapter.as_coordinator_admin().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "adapter does not implement CoordinatorAdmin".into(),
            data: None,
        })?;

        let gid = GroupId::new(p.group_jid.clone());
        let meta = coord
            .get_group_metadata(&gid)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("groups.participants_lid_to_phone failed: {e}"),
                data: Some(json!({"group_jid": p.group_jid})),
            })?;

        // Build mapping list from `phone_for_peer` (populated by the
        // WA adapter's `extract_group_metadata` from
        // `whatsapp_rust::GroupParticipant::phone_number`). For each
        // entry, strip the JID decoration to bare digits so the
        // shape matches `contacts.get_lid_pn_mappings`.
        let mut mappings: Vec<Value> = Vec::with_capacity(meta.phone_for_peer.len());
        let mut resolved_keys: HashSet<String> = HashSet::with_capacity(meta.phone_for_peer.len());
        for (lid, phone) in &meta.phone_for_peer {
            let lid_bare = strip_jid_decoration(lid.as_str());
            let phone_bare = strip_jid_decoration(phone.as_str());
            resolved_keys.insert(lid.as_str().to_string());
            mappings.push(json!({ "lid": lid_bare, "phone_number": phone_bare }));
        }

        // Anything in `meta.members` that didn't appear in
        // `phone_for_peer` is privacy-withheld server-side (or the
        // server's `<participant>` node didn't carry the attr).
        let not_resolved: Vec<String> = meta
            .members
            .iter()
            .filter(|p| !resolved_keys.contains(p.as_str()))
            .map(|p| p.to_string())
            .collect();

        Ok(json!({
            "mappings": mappings,
            "not_resolved": not_resolved,
            "resolved_count": mappings.len(),
            "requested_count": meta.members.len(),
            "group_jid": p.group_jid,
        }))
    }
}

/// Strip JID decoration (`@lid`, `:device`, `+`, `@s.whatsapp.net`)
/// to bare digits. Mirrors `contacts.get_lid_pn_mappings` so the two
/// RPCs return the same shape on the wire.
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
    use std::collections::HashMap;
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
    async fn empty_group_jid_rejected() {
        let h = handle_with_mock();
        let r = GroupsParticipantsLidToPhone
            .call(h, json!({ "group_jid": "" }))
            .await;
        assert!(r.is_err(), "empty group_jid must reject");
    }

    #[tokio::test]
    async fn mocked_group_with_phones_produces_mappings() {
        // Pre-canned metadata with two LID participants both
        // carrying `phone_number` (mimics a server response).
        let mut phones = HashMap::new();
        phones.insert("142670844444773@lid".to_string(), "5521959473159@s.whatsapp.net".to_string());
        phones.insert("245169735610485@lid".to_string(), "5521979958315@s.whatsapp.net".to_string());

        let canned_meta = octo_network::dot::adapters::coordinator_admin::GroupMetadata {
            id: octo_network::dot::GroupId::new(String::from("120363411021224818@g.us")),
            subject: Some("mock".into()),
            description: None,
            members: phones.keys().cloned().map(octo_network::dot::PeerId::new).collect(),
            admins: vec![],
            invite_url: None,
            mode_flags: octo_network::dot::adapters::coordinator_admin::GroupModeFlags::default(),
            phone_for_peer: phones
                .iter()
                .map(|(k, v)| {
                    (
                        octo_network::dot::PeerId::new(k.clone()),
                        octo_network::dot::PeerId::new(v.clone()),
                    )
                })
                .collect(),
        };

        let h = handle();
        let mock = Arc::new(MockAdapter::new());
        mock.coord_admin.set_canned_metadata("120363411021224818@g.us", canned_meta);
        h.bind_adapter(mock);

        let r = GroupsParticipantsLidToPhone
            .call(
                h,
                json!({ "group_jid": "120363411021224818@g.us" }),
            )
            .await
            .expect("ok");

        assert_eq!(r["group_jid"], "120363411021224818@g.us");
        assert_eq!(r["resolved_count"], 2);
        assert_eq!(r["requested_count"], 2);
        let empty = r["not_resolved"].as_array().unwrap();
        assert_eq!(empty.len(), 0, "expected no not_resolved entries, got {empty:?}");

        let mappings = r["mappings"].as_array().unwrap();
        assert!(mappings
            .iter()
            .any(|m| m["lid"] == "142670844444773" && m["phone_number"] == "5521959473159"));
        assert!(mappings
            .iter()
            .any(|m| m["lid"] == "245169735610485" && m["phone_number"] == "5521979958315"));
    }

    #[tokio::test]
    async fn lid_without_phone_lands_in_not_resolved() {
        // Two members: one carries phone_number, one doesn't.
        let mut phones = HashMap::new();
        phones.insert("142670844444773@lid".to_string(), "5521959473159@s.whatsapp.net".to_string());

        let mut members = vec![
            octo_network::dot::PeerId::new("142670844444773@lid".to_string()),
            octo_network::dot::PeerId::new("99999999999999@lid".to_string()),
        ];
        members.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        let canned_meta = octo_network::dot::adapters::coordinator_admin::GroupMetadata {
            id: octo_network::dot::GroupId::new(String::from("120363411021224818@g.us")),
            subject: Some("mock".into()),
            description: None,
            members,
            admins: vec![],
            invite_url: None,
            mode_flags: octo_network::dot::adapters::coordinator_admin::GroupModeFlags::default(),
            phone_for_peer: phones
                .iter()
                .map(|(k, v)| {
                    (
                        octo_network::dot::PeerId::new(k.clone()),
                        octo_network::dot::PeerId::new(v.clone()),
                    )
                })
                .collect(),
        };

        let h = handle();
        let mock = Arc::new(MockAdapter::new());
        mock.coord_admin.set_canned_metadata("120363411021224818@g.us", canned_meta);
        h.bind_adapter(mock);

        let r = GroupsParticipantsLidToPhone
            .call(
                h,
                json!({ "group_jid": "120363411021224818@g.us" }),
            )
            .await
            .expect("ok");

        assert_eq!(r["resolved_count"], 1);
        assert_eq!(r["requested_count"], 2);
        let not_resolved = r["not_resolved"].as_array().unwrap();
        assert!(not_resolved
            .iter()
            .any(|v| v == "99999999999999@lid"));
    }

    #[test]
    fn strip_jid_decoration_matches_lid_pn_handler() {
        assert_eq!(strip_jid_decoration("142670844444773@lid"), "142670844444773");
        assert_eq!(strip_jid_decoration("5521959473159@s.whatsapp.net"), "5521959473159");
        assert_eq!(strip_jid_decoration("+5521995544743"), "5521995544743");
        assert_eq!(strip_jid_decoration("108074580897808:42"), "108074580897808");
    }
}