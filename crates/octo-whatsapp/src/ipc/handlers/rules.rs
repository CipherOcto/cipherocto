//! Phase 4 RPC handlers for the rules engine.
//!
//! Exposes 11 methods:
//! - `rules.list` / `rules.get` (Phase 1 read-only, now backed by
//!   the live `RuleStore`).
//! - `rules.create` / `rules.update` / `rules.patch` / `rules.delete`
//!   — full CRUD with optimistic concurrency via etag.
//! - `rules.enable` / `rules.disable` — toggle without etag.
//! - `rules.approve` — `Draft → Approved` (operator scope, deferred
//!   to auth layer; handler just gates on rule state).
//! - `rules.reload` — re-read rules.toml from disk (Phase 4 stub:
//!   noop with a `noop: true` field).
//! - `rules.test` — evaluate an event against the live ruleset
//!   without executing actions.
//! - `rules.flush` — sync debounced disk writes (Phase 4 stub: ok).

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::events::InboundEvent;
use crate::rules::{ActionSpec, Predicate, RuleError};

fn err_to_rpc(e: RuleError) -> RpcError {
    match e {
        RuleError::NotFound { id } => RpcError::method_not_found(format!("rule {id} not found")),
        RuleError::AlreadyExists { id } => {
            RpcError::invalid_params(format!("rule {id} already exists"))
        }
        RuleError::Conflict {
            id,
            current_etag,
            current_version,
        } => RpcError::conflict_with_etag(id, current_etag, current_version),
        RuleError::InvalidId { reason } => {
            RpcError::invalid_params(format!("invalid id: {reason}"))
        }
        RuleError::UnsafeRegex(why) => RpcError::invalid_params(format!("unsafe regex: {why}")),
        RuleError::NotDraft { id, current_state } => RpcError::invalid_params(format!(
            "rule {id} is in state {current_state:?}, not Draft"
        )),
        RuleError::AlreadyApproved { id } => {
            RpcError::invalid_params(format!("rule {id} already approved"))
        }
        RuleError::RateLimited { max_per_minute } => {
            RpcError::rate_limited(format!("max {max_per_minute} rule mutations per minute"))
        }
    }
}

// ---- list / get ----

#[derive(Debug)]
pub struct RulesList;

#[async_trait::async_trait]
impl RpcHandler for RulesList {
    fn name(&self) -> &'static str {
        "rules.list"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let rules = h.rules().list();
        let arr: Vec<Value> = rules
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "version": r.version,
                    "enabled": r.enabled,
                    "state": format!("{:?}", r.state).to_lowercase(),
                    "priority": r.priority,
                    "etag": r.etag,
                    "predicate": serde_json::to_value(&r.predicate).unwrap_or(Value::Null),
                    "actions": serde_json::to_value(&r.actions).unwrap_or(Value::Null),
                    "cooldown_ms": r.cooldown_ms,
                    "ttl_until": r.ttl_until,
                })
            })
            .collect();
        Ok(serde_json::json!({
            "rules": arr,
            "count": arr.len(),
            "phase": "phase4",
        }))
    }
}

#[derive(Debug)]
pub struct RulesGet;

#[async_trait::async_trait]
impl RpcHandler for RulesGet {
    fn name(&self) -> &'static str {
        "rules.get"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        match h.rules().get(id) {
            Some(r) => Ok(serde_json::json!({
                "id": r.id,
                "version": r.version,
                "enabled": r.enabled,
                "state": format!("{:?}", r.state).to_lowercase(),
                "priority": r.priority,
                "etag": r.etag,
                "predicate": serde_json::to_value(&r.predicate).unwrap_or(Value::Null),
                "actions": serde_json::to_value(&r.actions).unwrap_or(Value::Null),
                "cooldown_ms": r.cooldown_ms,
                "ttl_until": r.ttl_until,
                "created_by": r.created_by,
                "created_at": r.created_at,
                "updated_at": r.updated_at,
                "found": true,
            })),
            None => Ok(serde_json::json!({
                "id": id,
                "found": false,
            })),
        }
    }
}

// ---- create ----

#[derive(Debug)]
pub struct RulesCreate;

#[async_trait::async_trait]
impl RpcHandler for RulesCreate {
    fn name(&self) -> &'static str {
        "rules.create"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let caller_uid = p
            .get("caller_uid")
            .and_then(|v| v.as_str())
            .unwrap_or("test")
            .to_string();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        // Rate limit per caller.
        let now_minute = now_ms / 60_000;
        h.mutation_rl()
            .check(&caller_uid, now_minute)
            .map_err(err_to_rpc)?;

        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing id".to_string()))?
            .to_string();
        let enabled = p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        let priority = p.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let predicate: Predicate =
            serde_json::from_value(p.get("predicate").cloned().unwrap_or(Value::Null))
                .map_err(|e| RpcError::invalid_params(format!("bad predicate: {e}")))?;
        let actions: Vec<ActionSpec> =
            serde_json::from_value(p.get("actions").cloned().unwrap_or(Value::Array(vec![])))
                .map_err(|e| RpcError::invalid_params(format!("bad actions: {e}")))?;
        let cooldown_ms = p.get("cooldown_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        let ttl_until = p.get("ttl_until").and_then(|v| v.as_i64());

        let draft = crate::rules::RuleDraft {
            id,
            enabled,
            priority,
            predicate,
            actions,
            cooldown_ms,
            ttl_until,
            created_by: caller_uid,
            now_ms,
        };
        let rule = h.rules().create(draft).map_err(err_to_rpc)?;
        Ok(serde_json::json!({
            "id": rule.id,
            "version": rule.version,
            "etag": rule.etag,
            "state": format!("{:?}", rule.state).to_lowercase(),
        }))
    }
}

// ---- update ----

#[derive(Debug)]
pub struct RulesUpdate;

#[async_trait::async_trait]
impl RpcHandler for RulesUpdate {
    fn name(&self) -> &'static str {
        "rules.update"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let caller_uid = p
            .get("caller_uid")
            .and_then(|v| v.as_str())
            .unwrap_or("test")
            .to_string();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let now_minute = now_ms / 60_000;
        h.mutation_rl()
            .check(&caller_uid, now_minute)
            .map_err(err_to_rpc)?;

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
        let patch = crate::rules::RulePatch {
            predicate: p
                .get("predicate")
                .map(|v| serde_json::from_value(v.clone()).unwrap()),
            actions: p
                .get("actions")
                .map(|v| serde_json::from_value(v.clone()).unwrap()),
            priority: p.get("priority").and_then(|v| v.as_i64()).map(|n| n as i32),
            cooldown_ms: p.get("cooldown_ms").and_then(|v| v.as_u64()),
            tttl_until: p.get("ttl_until").map(|v| v.as_i64()),
            enabled: p.get("enabled").and_then(|v| v.as_bool()),
        };
        let rule = h
            .rules()
            .update(&id, &etag, patch, now_ms)
            .map_err(err_to_rpc)?;
        Ok(serde_json::json!({
            "id": rule.id,
            "version": rule.version,
            "etag": rule.etag,
        }))
    }
}

// ---- patch (subset of update; same handler code) ----

#[derive(Debug)]
pub struct RulesPatch;

#[async_trait::async_trait]
impl RpcHandler for RulesPatch {
    fn name(&self) -> &'static str {
        "rules.patch"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        // Phase 4 stub: identical to `rules.update`. Full RFC 6902
        // JSON Patch (`add`/`remove`/`replace`) ships in Phase 5.
        let super_call = RulesUpdate;
        let mut q = p.clone();
        // Strip patch-only fields if any. None for now.
        if let Some(o) = q.as_object_mut() {
            o.remove("ops");
        }
        super_call.call(h, q).await
    }
}

// ---- delete ----

#[derive(Debug)]
pub struct RulesDelete;

#[async_trait::async_trait]
impl RpcHandler for RulesDelete {
    fn name(&self) -> &'static str {
        "rules.delete"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let caller_uid = p
            .get("caller_uid")
            .and_then(|v| v.as_str())
            .unwrap_or("test")
            .to_string();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let now_minute = now_ms / 60_000;
        h.mutation_rl()
            .check(&caller_uid, now_minute)
            .map_err(err_to_rpc)?;
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
        h.rules().delete(&id, &etag).map_err(err_to_rpc)?;
        Ok(serde_json::json!({"deleted": id}))
    }
}

// ---- enable / disable ----

#[derive(Debug)]
pub struct RulesEnable;

#[async_trait::async_trait]
impl RpcHandler for RulesEnable {
    fn name(&self) -> &'static str {
        "rules.enable"
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
        let r = h
            .rules()
            .set_enabled(&id, true, now_ms)
            .map_err(err_to_rpc)?;
        Ok(serde_json::json!({"id": r.id, "enabled": r.enabled, "etag": r.etag}))
    }
}

#[derive(Debug)]
pub struct RulesDisable;

#[async_trait::async_trait]
impl RpcHandler for RulesDisable {
    fn name(&self) -> &'static str {
        "rules.disable"
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
        let r = h
            .rules()
            .set_enabled(&id, false, now_ms)
            .map_err(err_to_rpc)?;
        Ok(serde_json::json!({"id": r.id, "enabled": r.enabled, "etag": r.etag}))
    }
}

// ---- approve ----

#[derive(Debug)]
pub struct RulesApprove;

#[async_trait::async_trait]
impl RpcHandler for RulesApprove {
    fn name(&self) -> &'static str {
        "rules.approve"
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
        let r = h.rules().approve(&id, now_ms).map_err(err_to_rpc)?;
        Ok(
            serde_json::json!({"id": r.id, "state": format!("{:?}", r.state).to_lowercase(), "etag": r.etag}),
        )
    }
}

// ---- reload ----

#[derive(Debug)]
pub struct RulesReload;

#[async_trait::async_trait]
impl RpcHandler for RulesReload {
    fn name(&self) -> &'static str {
        "rules.reload"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        // Phase 5 Part C: re-read rules.toml from the configured
        // storage path, parse + ReDoS-classify the predicates,
        // then call `replace_all` on the RuleStore. The diff is
        // computed against the previous snapshot for observability.
        let storage_path = h.config().rules.resolved_storage_path();
        let previous = h.rules().list();
        let bytes = match tokio::fs::read(&storage_path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(serde_json::json!({
                    "loaded_count": 0,
                    "previous_count": previous.len(),
                    "diff": [],
                    "noop_reason": "rules.toml not found",
                }));
            }
            Err(e) => {
                return Err(RpcError::exec_failed(format!("read rules.toml: {e}")));
            }
        };
        let text = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(_) => {
                return Err(RpcError::invalid_params(
                    "rules.toml is not UTF-8".to_string(),
                ));
            }
        };
        let set: crate::rules::PersistedRuleset = toml::from_str(text)
            .map_err(|e| RpcError::invalid_params(format!("rules.toml parse: {e}")))?;
        let rules = set.into_rules();
        // Validate each rule (id + ReDoS). Drop any invalid ones
        // with a tracing warning — reload should never crash the
        // daemon because of one bad row.
        let mut valid: Vec<std::sync::Arc<crate::rules::Rule>> = Vec::new();
        let mut dropped: u64 = 0;
        for r in rules {
            if crate::rules::validate_persisted_rule(&r) {
                valid.push(std::sync::Arc::new(r));
            } else {
                dropped += 1;
                tracing::warn!(id = %r.id, "rules.reload: rejected invalid rule row");
            }
        }
        // Compute diff (added/modified/removed) before swap.
        let mut diff: Vec<Value> = Vec::new();
        // Build a map by id for both sides.
        let prev: std::collections::HashMap<String, std::sync::Arc<crate::rules::Rule>> =
            previous.into_iter().map(|r| (r.id.clone(), r)).collect();
        let new_ids: std::collections::HashSet<String> =
            valid.iter().map(|r| r.id.clone()).collect();
        // Removed: in prev but not new.
        for (id, _) in prev.iter() {
            if !new_ids.contains(id) {
                diff.push(serde_json::json!({"id": id, "change": "removed"}));
            }
        }
        // Added or modified: walk new list.
        for nr in valid.iter() {
            match prev.get(&nr.id) {
                None => diff.push(serde_json::json!({"id": nr.id, "change": "added"})),
                Some(pr) if pr.etag != nr.etag || pr.version != nr.version => {
                    diff.push(serde_json::json!({"id": nr.id, "change": "modified"}));
                }
                _ => {}
            }
        }
        // Apply.
        h.rules().replace_all(valid.clone());
        Ok(serde_json::json!({
            "loaded_count": valid.len(),
            "previous_count": prev.len(),
            "dropped_invalid": dropped,
            "diff": diff,
        }))
    }
}

// ---- flush ----

#[derive(Debug)]
pub struct RulesFlush;

#[async_trait::async_trait]
impl RpcHandler for RulesFlush {
    fn name(&self) -> &'static str {
        "rules.flush"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let pending_before = h.rules().persister().map(|p| p.pending_len()).unwrap_or(0);
        let flushed = match h.rules().persister() {
            Some(p) => p.flush_sync().await.is_ok(),
            None => true, // no-op
        };
        Ok(serde_json::json!({
            "flushed": flushed,
            "had_pending": pending_before > 0,
        }))
    }
}

// ---- test (no-execute) ----

#[derive(Debug)]
pub struct RulesTest;

#[async_trait::async_trait]
impl RpcHandler for RulesTest {
    fn name(&self) -> &'static str {
        "rules.test"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let ev_json = p
            .get("event")
            .cloned()
            .ok_or_else(|| RpcError::invalid_params("missing event".to_string()))?;
        let ev: InboundEvent = serde_json::from_value(ev_json)
            .map_err(|e| RpcError::invalid_params(format!("bad event: {e}")))?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let matched = h.rules().match_event(&ev, now_ms);
        let arr: Vec<Value> = matched
            .iter()
            .map(|r| {
                serde_json::json!({
                    "rule_id": r.id,
                    "priority": r.priority,
                    "actions": r.actions.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(serde_json::json!({
            "matched": arr,
            "count": arr.len(),
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
            ..Default::default()
        };
        Daemon::new(cfg).handle()
    }

    fn basic_rule(id: &str) -> Value {
        serde_json::json!({
            "id": id,
            "enabled": true,
            "priority": 0,
            "predicate": {"kind": "event_kind", "kinds": ["message"]},
            "actions": [],
            "cooldown_ms": 0,
        })
    }

    #[tokio::test]
    async fn list_initially_empty() {
        let h = handle();
        let v = RulesList.call(h, Value::Null).await.unwrap();
        assert_eq!(v["count"], 0);
    }

    #[tokio::test]
    async fn create_then_get_round_trip() {
        let h = handle();
        let v = RulesCreate.call(h.clone(), basic_rule("r1")).await.unwrap();
        assert_eq!(v["id"], "r1");
        assert_eq!(v["state"], "draft");
        let etag = v["etag"].as_str().unwrap().to_string();

        let got = RulesGet
            .call(h.clone(), serde_json::json!({"id": "r1"}))
            .await
            .unwrap();
        assert_eq!(got["found"], true);
        assert_eq!(got["version"], 1);
        assert_eq!(got["etag"].as_str().unwrap(), etag);
    }

    #[tokio::test]
    async fn create_rejects_unsafe_regex() {
        let h = handle();
        let bad = serde_json::json!({
            "id": "r",
            "predicate": {"kind": "text_regex", "pattern": "(a+)+"},
            "actions": [],
        });
        let err = RulesCreate.call(h, bad).await.unwrap_err();
        assert!(format!("{err:?}").contains("unsafe"));
    }

    #[tokio::test]
    async fn create_rejects_invalid_id() {
        let h = handle();
        let bad = basic_rule("bad id!");
        let err = RulesCreate.call(h, bad).await.unwrap_err();
        assert!(format!("{err:?}").contains("invalid"));
    }

    #[tokio::test]
    async fn create_rejects_duplicate() {
        let h = handle();
        RulesCreate.call(h.clone(), basic_rule("r1")).await.unwrap();
        let err = RulesCreate.call(h, basic_rule("r1")).await.unwrap_err();
        assert!(format!("{err:?}").contains("already"));
    }

    #[tokio::test]
    async fn update_etag_conflict() {
        let h = handle();
        let v = RulesCreate.call(h.clone(), basic_rule("r1")).await.unwrap();
        let stale = v["etag"].as_str().unwrap().to_string();
        // First update succeeds.
        let _ = RulesUpdate
            .call(
                h.clone(),
                serde_json::json!({
                    "id": "r1",
                    "etag": stale,
                    "priority": 99,
                }),
            )
            .await
            .unwrap();
        // Second update with the now-stale etag fails.
        let err = RulesUpdate
            .call(
                h,
                serde_json::json!({
                    "id": "r1",
                    "etag": stale,
                    "priority": 1,
                }),
            )
            .await
            .unwrap_err();
        assert!(format!("{err:?}").contains("conflict"));
    }

    #[tokio::test]
    async fn delete_with_correct_etag() {
        let h = handle();
        let v = RulesCreate.call(h.clone(), basic_rule("r1")).await.unwrap();
        let etag = v["etag"].as_str().unwrap().to_string();
        let r = RulesDelete
            .call(h.clone(), serde_json::json!({"id": "r1", "etag": etag}))
            .await
            .unwrap();
        assert_eq!(r["deleted"], "r1");
        assert_eq!(RulesList.call(h, Value::Null).await.unwrap()["count"], 0);
    }

    #[tokio::test]
    async fn enable_disable_toggle() {
        let h = handle();
        let _ = RulesCreate.call(h.clone(), basic_rule("r1")).await.unwrap();
        let r = RulesEnable
            .call(h.clone(), serde_json::json!({"id": "r1"}))
            .await
            .unwrap();
        assert_eq!(r["enabled"], true);
        let r = RulesDisable
            .call(h.clone(), serde_json::json!({"id": "r1"}))
            .await
            .unwrap();
        assert_eq!(r["enabled"], false);
    }

    #[tokio::test]
    async fn approve_drafts_transition() {
        let h = handle();
        let _ = RulesCreate.call(h.clone(), basic_rule("r1")).await.unwrap();
        let r = RulesApprove
            .call(h.clone(), serde_json::json!({"id": "r1"}))
            .await
            .unwrap();
        assert_eq!(r["state"], "approved");
    }

    #[tokio::test]
    async fn reload_missing_file_is_noop() {
        let h = handle();
        let r = RulesReload.call(h, Value::Null).await.unwrap();
        assert_eq!(r["loaded_count"], 0);
    }

    #[tokio::test]
    async fn flush_returns_ok() {
        let h = handle();
        let r = RulesFlush.call(h, Value::Null).await.unwrap();
        assert_eq!(r["flushed"], true);
    }

    #[tokio::test]
    async fn test_returns_matched_rules() {
        let h = handle();
        let _ = RulesCreate.call(h.clone(), basic_rule("r1")).await.unwrap();
        // Approve so it's fireable.
        let _ = RulesApprove
            .call(h.clone(), serde_json::json!({"id": "r1"}))
            .await
            .unwrap();
        let event = serde_json::json!({
            "event": {
                "event": "message",
                "id": "M1",
                "peer": "p",
                "sender": "s",
                "ts_unix_ms": 0,
                "ts_mono_ns": 0,
                "kind": "text",
                "text": "hi",
                "is_group": false,
                "mentions_truncated": false,
            }
        });
        let r = RulesTest.call(h, event).await.unwrap();
        assert!(r["count"].as_u64().unwrap() >= 1);
    }
}
