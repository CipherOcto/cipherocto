//! Phase 4 RPC handlers for the audit log.
//!
//! Exposes 2 methods:
//! - `audit.tail` — paginated audit log access (loss-recovery flow).
//! - `audit.verify` — walks the in-memory chain and asserts each
//!   row's `prev_audit_hash` matches the previous row's `this_hash`,
//!   and recomputes `this_hash` from the canonical payload.

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct AuditTail;

#[async_trait::async_trait]
impl RpcHandler for AuditTail {
    fn name(&self) -> &'static str {
        "audit.tail"
    }
    async fn call(&self, h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let since_seq = p.get("since_seq").and_then(|v| v.as_u64()).unwrap_or(0);
        let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(1000) as usize;
        let entries = h.audit_log().tail(since_seq, limit);
        let json_entries: Vec<Value> = entries
            .iter()
            .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
            .collect();
        Ok(serde_json::json!({
            "entries": json_entries,
            "count": json_entries.len(),
            "seq_no": h.audit_log().seq_no(),
            "truncated_total": h.audit_log().truncated_total(),
        }))
    }
}

#[derive(Debug)]
pub struct AuditVerify;

#[async_trait::async_trait]
impl RpcHandler for AuditVerify {
    fn name(&self) -> &'static str {
        "audit.verify"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let result = h.audit_log().verify_chain();
        Ok(serde_json::to_value(&result).unwrap_or(Value::Null))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::AuditEntryInput;
    use crate::config::{EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig};
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

    fn input(m: &str) -> AuditEntryInput {
        AuditEntryInput {
            ts_unix_ms: 1000,
            ts_mono_ns: 999,
            caller_uid: "test".into(),
            caller_pid: 1,
            method: m.into(),
            args_canonical_sha256: "abc".into(),
            result_status: "ok".into(),
            latency_ms: 1,
        }
    }

    #[tokio::test]
    async fn tail_returns_recorded_entries() {
        let h = handle();
        h.audit_log().record(input("version.get"));
        h.audit_log().record(input("status.get"));
        h.audit_log().record(input("health.get"));
        let r = AuditTail
            .call(h, serde_json::json!({"since_seq": 0, "limit": 10}))
            .await
            .unwrap();
        assert_eq!(r["count"], 3);
        assert_eq!(r["seq_no"], 3);
    }

    #[tokio::test]
    async fn tail_filters_by_since_seq() {
        let h = handle();
        for m in ["a", "b", "c", "d"] {
            h.audit_log().record(input(m));
        }
        let r = AuditTail
            .call(h, serde_json::json!({"since_seq": 2, "limit": 10}))
            .await
            .unwrap();
        assert_eq!(r["count"], 2);
    }

    #[tokio::test]
    async fn verify_empty_log_returns_ok() {
        let h = handle();
        let r = AuditVerify.call(h, Value::Null).await.unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["verified_count"], 0);
    }

    #[tokio::test]
    async fn verify_after_writes_returns_ok() {
        let h = handle();
        for _ in 0..5 {
            h.audit_log().record(input("m"));
        }
        let r = AuditVerify.call(h, Value::Null).await.unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["verified_count"], 5);
        assert_eq!(r["last_seq_no"], 5);
    }
}
