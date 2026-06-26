//! On-disk config serialization for the WhatsApp session JSON.
//!
//! R1-C2 / R1-M1: `WhatsAppSession` does NOT derive `Serialize` /
//! `Deserialize`. The custom pair code (operator-typed) is not a
//! field on the struct — it lives only in `pair_link::run`'s local
//! scope. This mirrors `octo-matrix-onboard-core::Session` making
//! `access_token` private and exposing it only via `to_disk_json`.
//!
//! R2-C1: `to_disk_json` is a method on `WhatsAppSession` (`&self`),
//! matching the matrix-onboard pattern.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Captured session after a successful `Event::Connected`.
///
/// R1-C2: does NOT derive `Serialize` / `Deserialize`. The custom
/// pair code (operator-typed) is intentionally NOT a field here — it
/// is passed only to `pair_link::run()` and dropped on success. The
/// on-disk `session_meta.json` and `WhatsAppConfig` never see it.
/// This mirrors `octo-matrix-onboard-core::Session` making
/// `access_token` private and exposing it only via `to_disk_json()`.
#[derive(Debug, Clone)]
pub struct WhatsAppSession {
    /// Bot's own phone number, resolved from `device.pn` on
    /// `Event::Connected`. E.164 digits-only, e.g., `"15551234567"`.
    /// `None` if the device snapshot wasn't yet persisted when `whoami` ran.
    pub self_phone: Option<String>,
    /// Path to stoolap session database.
    pub session_path: PathBuf,
    /// Group JIDs the operator configured at link time. Mirrored into
    /// `WhatsAppConfig::groups` so the adapter picks them up unchanged.
    pub groups: Vec<String>,
    /// Pair phone (only populated by `pair-link`, omitted from `qr-link` output).
    pub pair_phone: Option<String>,
}

impl WhatsAppSession {
    /// Build the on-disk JSON shape (mission AC §OutputArgs).
    ///
    /// The on-disk shape is built field-by-field in a `serde_json::Map`
    /// (mirroring `octo-matrix-onboard-core/src/lib.rs:161-187`) so
    /// the field set is exact:
    /// - `session_path` is always present.
    /// - `groups` is present only when non-empty (matches adapter's
    ///   `#[serde(default)]` behavior; empty Vec deserializes correctly).
    /// - `pair_phone` is present only when `Some` (matches adapter).
    /// - `pair_code` is **NEVER** written (R1-C2: not a field here;
    ///   defense-in-depth: even if a future maintainer adds it, the
    ///   on-disk JSON never includes it).
    /// - `ws_url` is omitted (None) — adapter default is None.
    pub fn to_disk_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::with_capacity(4);
        map.insert(
            "session_path".to_string(),
            serde_json::Value::String(format!("{}", self.session_path.display())),
        );
        if !self.groups.is_empty() {
            map.insert(
                "groups".to_string(),
                serde_json::Value::Array(
                    self.groups
                        .iter()
                        .map(|g| serde_json::Value::String(g.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(ref pp) = self.pair_phone {
            map.insert(
                "pair_phone".to_string(),
                serde_json::Value::String(pp.clone()),
            );
        }
        serde_json::Value::Object(map)
    }
}

/// Session info for `session list` / `verify` output.
///
/// R10-L1: `last_linked_at` is `Option<String>` (the sidecar JSON's
/// `linked_at` is a String, mirroring the on-disk shape; avoids the
/// `chrono` dep and the parse-from-RFC-3339 complexity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_path: PathBuf,
    pub self_phone: Option<String>,
    pub is_valid: bool,
    /// RFC 3339 UTC `YYYY-MM-DDTHH:MM:SSZ`, or `"<unknown>"` if unset.
    pub last_linked_at: Option<String>,
}

/// CLI input for `qr-link` (subset of `WhatsAppConfig`; `pair_code`
/// and `pair_phone` are omitted).
#[derive(Clone, Debug)]
pub struct QrLinkArgs {
    pub session_path: PathBuf,
    pub groups: Vec<String>,
    pub ws_url: Option<String>,
    pub timeout_secs: u64,
    pub wait_sync: bool,
}

/// CLI input for `pair-link`.
#[derive(Clone, Debug)]
pub struct PairLinkArgs {
    pub session_path: PathBuf,
    pub phone: String,
    pub custom_code: Option<String>,
    pub groups: Vec<String>,
    pub ws_url: Option<String>,
    pub timeout_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_session() -> WhatsAppSession {
        WhatsAppSession {
            self_phone: Some("15551234567".to_string()),
            session_path: PathBuf::from("/home/user/.local/share/octo/whatsapp/default.session.db"),
            groups: vec!["120363012345678901@g.us".to_string()],
            pair_phone: None,
        }
    }

    #[test]
    fn to_disk_json_qr_link_shape() {
        let s = sample_session();
        let v = s.to_disk_json();
        assert_eq!(
            v["session_path"],
            "/home/user/.local/share/octo/whatsapp/default.session.db"
        );
        assert_eq!(v["groups"][0], "120363012345678901@g.us");
        assert!(v.get("pair_phone").is_none());
    }

    #[test]
    fn to_disk_json_pair_link_shape() {
        let mut s = sample_session();
        s.pair_phone = Some("15551234567".to_string());
        let v = s.to_disk_json();
        assert_eq!(v["pair_phone"], "15551234567");
    }

    #[test]
    fn to_disk_json_empty_groups_omits_field() {
        let mut s = sample_session();
        s.groups = vec![];
        let v = s.to_disk_json();
        let obj = v.as_object().unwrap();
        assert!(obj.get("groups").is_none());
    }

    #[test]
    fn to_disk_json_never_includes_pair_code() {
        // R1-C2 / R1-M1: defense-in-depth — even if a future maintainer
        // adds a `pair_code` field to the in-memory `WhatsAppSession`,
        // `to_disk_json` must NOT include it.
        let s = sample_session();
        let v = s.to_disk_json();
        let obj = v.as_object().unwrap();
        assert!(obj.get("pair_code").is_none());
    }

    #[test]
    fn to_disk_json_round_trip_to_whatsapp_config() {
        // R5-M1: round-trip via adapter instantiation. The on-disk JSON
        // must deserialize into WhatsAppConfig (or, where the field is
        // missing, default to None).
        use octo_adapter_whatsapp::WhatsAppConfig;

        let s = sample_session();
        let v = s.to_disk_json();
        let json_str = serde_json::to_string(&v).unwrap();
        let cfg: WhatsAppConfig =
            serde_json::from_str(&json_str).expect("must deserialize into WhatsAppConfig");
        assert_eq!(
            cfg.session_path,
            "/home/user/.local/share/octo/whatsapp/default.session.db"
        );
        assert_eq!(cfg.groups, vec!["120363012345678901@g.us".to_string()]);
    }
}
