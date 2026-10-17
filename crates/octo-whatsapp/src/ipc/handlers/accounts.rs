//! Multi-account RPC surface (Phase 6.1).
//!
//! Three handlers round-trip through the daemon's `MultiAccountStore`:
//! - `daemon.accounts.list`  — enumerate all linked accounts. The result
//!   is the union of the persisted index AND a fresh on-disk scan
//!   (see `MultiAccountStore::discover_from_disk`), so newly-linked
//!   sessions appear immediately without a daemon restart.
//! - `daemon.accounts.use`   — set the active account (writes `<base>/active` symlink
//!   AND atomically rebinds the running adapter to the new account's session path).
//!   Operators may follow up with `reconnect.now` to establish a fresh connection.
//! - `daemon.accounts.info`  — fetch details for one account.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use octo_whatsapp_onboard_core::{
    default_index_base_dir, AccountEntry, CoreError, MultiAccountStore,
};

#[derive(Debug)]
pub struct AccountsList;
#[derive(Debug)]
pub struct AccountsUse;
#[derive(Debug)]
pub struct AccountsInfo;

#[derive(Deserialize)]
struct UseParams {
    account_id: String,
}

#[derive(Deserialize)]
struct InfoParams {
    account_id: String,
}

fn core_err_to_rpc(e: CoreError) -> RpcError {
    RpcError {
        code: RpcErrorCode::Internal.as_i32(),
        message: format!("MultiAccountStore error: {e:?}"),
        data: None,
    }
}

fn invalid_params(msg: impl Into<String>) -> RpcError {
    RpcError {
        code: RpcErrorCode::InvalidParams.as_i32(),
        message: msg.into(),
        data: None,
    }
}

fn entry_to_json(e: &AccountEntry) -> Value {
    json!({
        "account_id": e.account_id,
        "session_path": e.session_path.to_string_lossy(),
        "config_path": e.config_path.to_string_lossy(),
        "linked_at": e.linked_at,
        "last_used_at": e.last_used_at,
    })
}

#[async_trait::async_trait]
impl RpcHandler for AccountsList {
    fn name(&self) -> &'static str {
        "daemon.accounts.list"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        // 1. Resolve base_dir while holding the lock briefly. Drop the
        //    guard before doing the read_dir scan so we don't block
        //    other handlers on a slow filesystem during the merge.
        //    Fall back to the env-derived default if the store never
        //    initialised at boot — operators still see on-disk accounts.
        let base = {
            let store = h.accounts();
            store.base_dir().unwrap_or_else(default_index_base_dir)
        };

        // 2. Fresh on-disk scan: finds accounts that exist as `<id>.session.db/`
        //    + `<id>.session.db.meta.json` but are not yet in index.json.
        //    Best-effort: silently skips broken entries.
        let discovered = MultiAccountStore::discover_from_disk(&base);

        // 3. Merge with cached index. Cached entries win on conflicts
        //    so we preserve `last_used_at` and any operator-edited
        //    fields. Discovered entries fill in accounts that exist
        //    on disk but were never imported into the index.
        let cached = { h.accounts().list() };
        let mut by_id: std::collections::BTreeMap<String, AccountEntry> = cached
            .into_iter()
            .map(|e| (e.account_id.clone(), e))
            .collect();
        for d in discovered {
            by_id.entry(d.account_id.clone()).or_insert(d);
        }
        let arr: Vec<Value> = by_id.values().map(entry_to_json).collect();
        Ok(json!({ "accounts": arr }))
    }
}

#[async_trait::async_trait]
impl RpcHandler for AccountsUse {
    fn name(&self) -> &'static str {
        "daemon.accounts.use"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: UseParams = serde_json::from_value(params)
            .map_err(|e| invalid_params(format!("missing/invalid account_id: {e}")))?;

        // Step 1: write the symlink + update the JSON index.
        let mut store = h.accounts();
        let entry = store.use_account(&p.account_id).map_err(|e| match e {
            CoreError::InvalidSessionPath { reason, .. } => {
                invalid_params(format!("account_id {:?} not found: {reason}", p.account_id))
            }
            other => core_err_to_rpc(other),
        })?;

        // Step 2: atomically rebind the running adapter to the new session path.
        // (live_chain_j_accounts exercises this path against a real account.)
        h.rebind_adapter_for(&p.account_id, &entry.session_path);

        Ok(json!({
            "active": entry.account_id,
            "session_path": entry.session_path.to_string_lossy(),
        }))
    }
}

#[async_trait::async_trait]
impl RpcHandler for AccountsInfo {
    fn name(&self) -> &'static str {
        "daemon.accounts.info"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: InfoParams = serde_json::from_value(params)
            .map_err(|e| invalid_params(format!("missing/invalid account_id: {e}")))?;

        let entry = h
            .accounts()
            .info(&p.account_id)
            .ok_or_else(|| invalid_params(format!("account_id {:?} not found", p.account_id)))?;
        Ok(entry_to_json(&entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use serde_json::json;

    fn empty_handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accounts_list_returns_empty_array_when_no_index() {
        let h = empty_handle();
        let result = AccountsList.call(h, json!({})).await.unwrap();
        let arr = result.get("accounts").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 0, "no index.json => empty accounts list");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accounts_list_picks_up_on_disk_account_without_restart() {
        // Daemon::new_for_tests opens `tmpdir/data/index.json` and
        // seeds it with `{"accounts":{}}`. Drop a `Pattern A` pair
        // (dir + meta.json) into the same `data/` dir WITHOUT
        // touching the index — list should still surface it.
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().join("data");
        std::fs::create_dir_all(base.join("lia.session.db")).unwrap();
        std::fs::write(
            base.join("lia.session.db.meta.json"),
            br#"{"self_phone":"5521998469965","linked_at":"2026-07-23T19:04:11Z","mode":"qr-link","groups":[]}"#,
        )
        .unwrap();

        let (_daemon, h) = Daemon::new_for_tests(tmp.path());
        let result = AccountsList.call(h, json!({})).await.unwrap();
        let arr = result.get("accounts").unwrap().as_array().unwrap();
        let ids: Vec<&str> = arr
            .iter()
            .map(|v| v.get("account_id").unwrap().as_str().unwrap())
            .collect();
        assert!(
            ids.contains(&"lia"),
            "expected list to discover lia from disk; got {ids:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accounts_info_unknown_account_returns_invalid_params() {
        let h = empty_handle();
        let err = AccountsInfo
            .call(h, json!({ "account_id": "nonexistent-12345" }))
            .await
            .expect_err("should error on unknown account");
        assert_eq!(err.code, -32602, "expected InvalidParams (-32602)");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accounts_use_unknown_account_returns_invalid_params() {
        let h = empty_handle();
        let err = AccountsUse
            .call(h, json!({ "account_id": "nonexistent-12345" }))
            .await
            .expect_err("should error on unknown account");
        assert_eq!(err.code, -32602, "expected InvalidParams (-32602)");
    }
}
