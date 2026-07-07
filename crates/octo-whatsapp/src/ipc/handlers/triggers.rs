//! Phase 4 RPC handlers for the triggers registry.
//!
//! Exposes 6 methods:
//! - `triggers.list` / `triggers.get` (Phase 1 read-only, now
//!   backed by the live `TriggerStore`).
//! - `triggers.create` / `triggers.update` / `triggers.delete` —
//!   full CRUD with optimistic concurrency via etag.
//! - `triggers.run` — invoke a trigger and return a `RunRecord`.

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::events::InboundEvent;
use crate::triggers::{TriggerError, TriggerPatch};

fn err_to_rpc(e: TriggerError) -> RpcError {
    match e {
        TriggerError::NotFound { id } => {
            RpcError::method_not_found(format!("trigger {id} not found"))
        }
        TriggerError::AlreadyExists { id } => {
            RpcError::invalid_params(format!("trigger {id} already exists"))
        }
        TriggerError::Conflict {
            id,
            current_etag,
            current_version,
        } => RpcError::conflict_with_etag(id, current_etag, current_version),
        TriggerError::InvalidId { reason } => {
            RpcError::invalid_params(format!("invalid id: {reason}"))
        }
        TriggerError::Disabled { id } => RpcError::invalid_params(format!("trigger {id} disabled")),
        TriggerError::ExecFailed(why) => RpcError::exec_failed(why),
        TriggerError::NotSupported(why) => RpcError::not_supported(why),
    }
}

fn snapshot(r: &crate::triggers::Trigger) -> Value {
    serde_json::json!({
        "id": r.id,
        "version": r.version,
        "enabled": r.enabled,
        "runner": serde_json::to_value(&r.runner).unwrap_or(Value::Null),
        "rate_limit": r.rate_limit.as_ref().map(|rl| serde_json::to_value(rl).unwrap_or(Value::Null)),
        "timeout_ms": r.timeout_ms,
        "retries": r.retries,
        "history_cap": r.history_cap,
        "last_run": r.last_run.as_ref().map(|lr| serde_json::to_value(lr).unwrap_or(Value::Null)),
        "created_by": r.created_by,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "etag": r.etag,
    })
}

#[derive(Debug)]
pub struct TriggersList;

#[async_trait::async_trait]
impl RpcHandler for TriggersList {
    fn name(&self) -> &'static str {
        "triggers.list"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let triggers = h.triggers().list();
        let arr: Vec<Value> = triggers.iter().map(|t| snapshot(t)).collect();
        Ok(serde_json::json!({
            "triggers": arr,
            "count": arr.len(),
            "phase": "phase4",
        }))
    }
}

#[derive(Debug)]
pub struct TriggersGet;

#[async_trait::async_trait]
impl RpcHandler for TriggersGet {
    fn name(&self) -> &'static str {
        "triggers.get"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        match h.triggers().get(id) {
            Some(t) => {
                let mut s = snapshot(&t);
                s.as_object_mut()
                    .unwrap()
                    .insert("found".into(), Value::Bool(true));
                Ok(s)
            }
            None => Ok(serde_json::json!({"id": id, "found": false})),
        }
    }
}

#[derive(Debug)]
pub struct TriggersCreate;

#[async_trait::async_trait]
impl RpcHandler for TriggersCreate {
    fn name(&self) -> &'static str {
        "triggers.create"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing id".to_string()))?
            .to_string();
        let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        let runner: crate::triggers::RunnerSpec = serde_json::from_value(
            p.get("runner")
                .cloned()
                .ok_or_else(|| RpcError::invalid_params("missing runner".to_string()))?,
        )
        .map_err(|e| RpcError::invalid_params(format!("bad runner: {e}")))?;
        let rate_limit = p
            .get("rate_limit")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let timeout_ms = p
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(60_000);
        let retries = p.get("retries").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let history_cap = p.get("history_cap").and_then(|v| v.as_u64()).unwrap_or(10) as u32;
        let draft = crate::triggers::TriggerDraft {
            id,
            enabled,
            runner,
            rate_limit,
            timeout_ms,
            retries,
            history_cap,
            created_by: p
                .get("caller_uid")
                .and_then(|v| v.as_str())
                .unwrap_or("test")
                .to_string(),
            now_ms,
        };
        let t = h.triggers().create(draft).map_err(err_to_rpc)?;
        Ok(snapshot(&t))
    }
}

#[derive(Debug)]
pub struct TriggersUpdate;

#[async_trait::async_trait]
impl RpcHandler for TriggersUpdate {
    fn name(&self) -> &'static str {
        "triggers.update"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing id".to_string()))?
            .to_string();
        let etag = p
            .get("etag")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing etag".to_string()))?
            .to_string();
        let patch = TriggerPatch {
            runner: p
                .get("runner")
                .map(|v| serde_json::from_value(v.clone()).unwrap()),
            rate_limit: p.get("rate_limit").map(|v| {
                if v.is_null() {
                    None
                } else {
                    serde_json::from_value(v.clone()).ok()
                }
            }),
            timeout_ms: p.get("timeout_ms").and_then(|v| v.as_u64()),
            retries: p.get("retries").and_then(|v| v.as_u64()).map(|n| n as u32),
            history_cap: p
                .get("history_cap")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32),
            enabled: p.get("enabled").and_then(|v| v.as_bool()),
        };
        let t = h
            .triggers()
            .update(&id, &etag, patch, now_ms)
            .map_err(err_to_rpc)?;
        Ok(snapshot(&t))
    }
}

#[derive(Debug)]
pub struct TriggersDelete;

#[async_trait::async_trait]
impl RpcHandler for TriggersDelete {
    fn name(&self) -> &'static str {
        "triggers.delete"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing id".to_string()))?
            .to_string();
        let etag = p
            .get("etag")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing etag".to_string()))?
            .to_string();
        h.triggers().delete(&id, &etag).map_err(err_to_rpc)?;
        Ok(serde_json::json!({"deleted": id}))
    }
}

#[derive(Debug)]
pub struct TriggersRun;

#[async_trait::async_trait]
impl RpcHandler for TriggersRun {
    fn name(&self) -> &'static str {
        "triggers.run"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing id".to_string()))?
            .to_string();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        // For Phase 4: synthesize an inbound event from the supplied
        // payload, or use a stub Message.
        let ev: InboundEvent = p
            .get("event")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_else(|| InboundEvent::Unknown {
                raw: "trigger.run".into(),
                ts_unix_ms: now_ms,
                ts_mono_ns: 0,
                untrusted: false,
            });
        let rec = h
            .triggers()
            .run(&id, &ev, now_ms)
            .await
            .map_err(err_to_rpc)?;
        Ok(serde_json::to_value(&rec).unwrap_or(Value::Null))
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
        let cfg = WhatsAppRuntimeConfig {
            name: "p4".into(),
            data_dir: Default::default(),
            log_dir: Default::default(),
            socket_dir: Default::default(),
            media_buffer: MediaBufferConfig::default(),
            events: EventsConfig::default(),
            security: SecurityConfig::default(),
            observability: Default::default(),
            rules: RulesConfig::default(),
        };
        Daemon::new(cfg).handle()
    }

    fn basic_trigger(id: &str) -> Value {
        serde_json::json!({
            "id": id,
            "enabled": true,
            "runner": {"kind": "agent", "agent_id": "a1", "input_template": "t"},
            "timeout_ms": 1000,
            "retries": 0,
            "history_cap": 10,
        })
    }

    #[tokio::test]
    async fn list_initially_empty() {
        let h = handle();
        let v = TriggersList.call(h, Value::Null).await.unwrap();
        assert_eq!(v["count"], 0);
    }

    #[tokio::test]
    async fn create_then_get_round_trip() {
        let h = handle();
        let v = TriggersCreate
            .call(h.clone(), basic_trigger("t1"))
            .await
            .unwrap();
        assert_eq!(v["id"], "t1");
        let got = TriggersGet
            .call(h.clone(), serde_json::json!({"id": "t1"}))
            .await
            .unwrap();
        assert_eq!(got["found"], true);
        assert!(!got["etag"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_rejects_duplicate() {
        let h = handle();
        TriggersCreate
            .call(h.clone(), basic_trigger("t1"))
            .await
            .unwrap();
        let err = TriggersCreate
            .call(h, basic_trigger("t1"))
            .await
            .unwrap_err();
        assert!(format!("{err:?}").contains("already"));
    }

    #[tokio::test]
    async fn update_with_correct_etag() {
        let h = handle();
        let v = TriggersCreate
            .call(h.clone(), basic_trigger("t1"))
            .await
            .unwrap();
        let etag = v["etag"].as_str().unwrap().to_string();
        let r = TriggersUpdate
            .call(
                h.clone(),
                serde_json::json!({
                    "id": "t1",
                    "etag": etag,
                    "timeout_ms": 5000,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["timeout_ms"], 5000);
    }

    #[tokio::test]
    async fn delete_with_correct_etag() {
        let h = handle();
        let v = TriggersCreate
            .call(h.clone(), basic_trigger("t1"))
            .await
            .unwrap();
        let etag = v["etag"].as_str().unwrap().to_string();
        TriggersDelete
            .call(h.clone(), serde_json::json!({"id": "t1", "etag": etag}))
            .await
            .unwrap();
        assert_eq!(TriggersList.call(h, Value::Null).await.unwrap()["count"], 0);
    }

    #[tokio::test]
    async fn run_records_synthetic() {
        let h = handle();
        TriggersCreate
            .call(h.clone(), basic_trigger("t1"))
            .await
            .unwrap();
        let r = TriggersRun
            .call(h, serde_json::json!({"id": "t1"}))
            .await
            .unwrap();
        assert_eq!(r["exit_code"], 0);
    }

    #[tokio::test]
    async fn run_unknown_errors() {
        let h = handle();
        let err = TriggersRun
            .call(h, serde_json::json!({"id": "missing"}))
            .await
            .unwrap_err();
        assert!(format!("{err:?}").contains("not found"));
    }
}
