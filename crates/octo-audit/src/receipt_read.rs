//! Read-path surface for audit receipts (RFC-0016 §6.2.1-§6.2.4).
//!
//! Phase 1 mirrors the octo-wallet `AGENT_REGISTRY` pattern: a
//! process-global in-memory `BTreeMap<u64, Receipt>` indexed by
//! `receipt_id` (monotonic, per-stream). Read functions walk that
//! registry under a `Mutex`; Phase 2 swaps in the Stoolap-backed
//! DOMAIN adapter (RFC-0016-a §6.9 paired-acceptance unblocks).
//!
//! Filter parsing mirrors `list_owned_agents`: server-side filter
//! plus deterministic sorting (by `timestamp_unix DESC`, `receipt_id
//! ASC` tiebreaker) plus clamp at the substrate hard ceiling of
//! 1024 entries.

use std::collections::BTreeMap;
#[cfg(feature = "octo-audit-internal")]
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use octo_settlement::Receipt;

use octo_audit_core::AuditError;

/// Process-global in-memory receipt registry (Phase 1).
///
/// Indexed by `receipt_id`. The static is intentionally
/// `BTreeMap`-keyed so iteration order is deterministic across
/// runs (per RFC-0008 Class B determinism contract).
static RECEIPT_REGISTRY: OnceLock<Mutex<BTreeMap<u64, Receipt>>> = OnceLock::new();

pub(crate) fn registry() -> &'static Mutex<BTreeMap<u64, Receipt>> {
    RECEIPT_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Insert a receipt into the Phase 1 process-global registry.
///
/// Phase 1 fixture helper (parallels `register_agent`). Real
/// persistence lands with the Stoolap DOMAIN adapter gated on
/// RFC-0016-a acceptance.
///
/// Fail-closed on mutex poisoning (lens-8 ask): poisoning should
/// NOT propagate as a panic to future callers (write-path missions,
/// follow-on amendments). The lock is acquired opportunistically;
/// on poison the insert is silently dropped — the test-fixture
/// semantics (write-once registration before assertion) tolerate
/// the loss; the future Stoolap DOMAIN adapter will surface its
/// own poisoning error.
pub fn insert_receipt(receipt: Receipt) {
    if let Ok(mut registry) = registry().lock() {
        registry.insert(receipt.receipt_id, receipt);
    }
}

/// Filter for `list_receipts` — RFC-0016 §6.2.4.
///
/// All-`None` returns the unfiltered receipt set, sorted by
/// `timestamp_unix DESC` with `receipt_id ASC` tiebreaker.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFilter {
    /// Restrict to receipts whose `router_id` equals this string
    /// (RFC-0010 canonical wire form).
    pub router_id: Option<String>,
    /// Minimum timestamp (`>=`). None = unbounded below.
    pub timestamp_unix_gte: Option<u64>,
    /// Maximum timestamp (`<=`). None = unbounded above.
    pub timestamp_unix_lte: Option<u64>,
    /// Maximum rows returned (`None` = no cap; substrate applies
    /// a hard ceiling of 1024).
    pub limit: Option<usize>,
    /// Opaque cursor (forward-compat for Phase 2 multi-page
    /// iteration).
    pub cursor: Option<String>,
}

/// List receipt IDs (`Vec<u64>` of `receipt_id` values) matching
/// `filter` (RFC-0016 §6.2.1).
///
/// Sorted by `timestamp_unix DESC` with `receipt_id ASC`
/// tiebreaker. Returns empty `Vec` when zero matches (NOT an
/// error). Limit clamp at 1024. Read-only — no state mutation.
pub fn list_receipts(filter: &AuditFilter) -> Result<Vec<u64>, AuditError> {
    let limit = filter.limit.unwrap_or(1024).min(1024);

    let registry = registry()
        .lock()
        .map_err(|_| AuditError::SinkSpecific("receipt registry mutex poisoned".into()))?;

    let mut ids: Vec<u64> = registry
        .values()
        .filter(|r| match &filter.router_id {
            Some(rid) => r.router_id == *rid,
            None => true,
        })
        .filter(|r| match filter.timestamp_unix_gte {
            Some(gte) => r.timestamp_unix >= gte,
            None => true,
        })
        .filter(|r| match filter.timestamp_unix_lte {
            Some(lte) => r.timestamp_unix <= lte,
            None => true,
        })
        .map(|r| r.receipt_id)
        .collect();

    ids.sort_by(|a, b| {
        let ra = &registry[a];
        let rb = &registry[b];
        rb.timestamp_unix
            .cmp(&ra.timestamp_unix)
            .then_with(|| a.cmp(b))
    });
    ids.truncate(limit);
    Ok(ids)
}

/// Point-lookup of a `Receipt` by canonical `receipt_id`
/// (RFC-0016 §6.2.2).
///
/// Returns the canonical `Receipt` on hit. The substrate does not
/// leak existence — misses surface `AuditError::SinkSpecific` per
/// the canonical 3-variant form (no parallel abstraction per
/// [[cipherocto-design-principles]]).
pub fn get_receipt(id: &u64) -> Result<Receipt, AuditError> {
    let registry = registry()
        .lock()
        .map_err(|_| AuditError::SinkSpecific("receipt registry mutex poisoned".into()))?;

    registry
        .get(id)
        .cloned()
        .ok_or_else(|| AuditError::SinkSpecific(format!("receipt_id {id} not found")))
}

/// Discover the canonical audit home directory
/// (RFC-0016 §6.2.3; RFC-0011-a §Key Files `audit_home`).
///
/// Resolves to `$OCTO_HOME/audit/receipts` when `OCTO_HOME` is
/// set; otherwise `~/.config/octo/audit/receipts` per parent RFC
/// §Implicit Assumptions Audit "Operator config dir" row.
///
/// `pub(crate)` + `#[cfg(feature = "octo-audit-internal")]` per
/// RFC-0016 §6.2.3 + §Adversary Analysis row "canonical-path info
/// leak" — only the `octo-audit` Layer B façade can call this
/// function (with the internal feature flag enabled), preventing
/// accidental path leakage to downstream consumers (e.g., `octo-cli`
/// in default builds). The Result return is RESERVED for a future
/// Phase 2 IO-error path (filesystem stat failures on the canonical
/// audit-receipts directory); Phase 1 env-var resolution is
/// infallible (both branches construct `Ok(...)`).
///
/// `#[allow(dead_code)]` is required because the lib target's
/// `dead_code` lint does not see the `#[cfg(test)]` test call sites
/// (test target is a separate compilation unit); the function is
/// exercised by the internal-feature-gated tests at the bottom of
/// this module.
#[cfg(feature = "octo-audit-internal")]
#[allow(dead_code)]
pub(crate) fn audit_home() -> Result<PathBuf, AuditError> {
    if let Ok(octo_home) = std::env::var("OCTO_HOME") {
        Ok(PathBuf::from(octo_home).join("audit/receipts"))
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Ok(PathBuf::from(home).join(".config/octo/audit/receipts"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Test fixture: a fresh Receipt; tests seed the registry
    // directly to keep the lock tight around assertion blocks.
    fn sample_receipt(id: u64, ts: u64) -> Receipt {
        Receipt {
            receipt_id: id,
            ask_id: [0xAA; 32],
            settlement_hash: [0xBB; 32],
            router_id: "did:octo:router-a".to_string(),
            router_sig: vec![0xCC; 64],
            timestamp_unix: ts,
        }
    }

    // Tests in this module share a single registry (global
    // OnceLock). Run with `cargo test -p octo-audit --lib
    // receipt_read::tests -- --test-threads=1` to keep assertions
    // deterministic.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    #[cfg(feature = "octo-audit-internal")]
    fn audit_home_returns_octo_home_path() {
        // No mutation needed; lock guards test ordering only.
        let _guard = TEST_LOCK.lock().unwrap();

        std::env::set_var("OCTO_HOME", "/tmp/octo-test");
        let p = audit_home().expect("audit_home resolves when OCTO_HOME is set");
        assert_eq!(p, PathBuf::from("/tmp/octo-test/audit/receipts"));
        std::env::remove_var("OCTO_HOME");
    }

    #[test]
    #[cfg(feature = "octo-audit-internal")]
    fn audit_home_returns_default_when_no_octo_home() {
        let _guard = TEST_LOCK.lock().unwrap();
        std::env::remove_var("OCTO_HOME");
        let p = audit_home().expect("audit_home resolves with default fallback");
        assert!(
            p.ends_with(".config/octo/audit/receipts"),
            "default path should end in .config/octo/audit/receipts, got {p:?}",
        );
    }

    #[test]
    fn list_receipts_empty_registry() {
        let _guard = TEST_LOCK.lock().unwrap();
        // Wipe registry so this test sees an empty substrate.
        let mut reg = registry().lock().unwrap();
        reg.clear();
        drop(reg);

        let filter = AuditFilter::default();
        let ids = list_receipts(&filter).unwrap();
        assert!(ids.is_empty(), "empty registry yields empty Vec");
    }

    #[test]
    fn list_receipts_returns_ids_in_deterministic_order() {
        let _guard = TEST_LOCK.lock().unwrap();
        {
            let mut reg = registry().lock().unwrap();
            reg.clear();
            reg.insert(1, sample_receipt(1, 1_700_000_001));
            reg.insert(2, sample_receipt(2, 1_700_000_002));
            reg.insert(3, sample_receipt(3, 1_700_000_000));
        }
        let ids = list_receipts(&AuditFilter::default()).unwrap();
        // Sort: ts DESC then id ASC. ts=2 has id=2, ts=1 has id=1,
        // ts=0 has id=3.
        assert_eq!(ids, vec![2, 1, 3], "expected ts DESC, id ASC order");
    }

    #[test]
    fn list_receipts_filters_by_router_id() {
        let _guard = TEST_LOCK.lock().unwrap();
        {
            let mut reg = registry().lock().unwrap();
            reg.clear();
            reg.insert(1, sample_receipt(1, 1_700_000_001));
            reg.insert(
                2,
                Receipt {
                    router_id: "did:octo:router-b".to_string(),
                    ..sample_receipt(2, 1_700_000_002)
                },
            );
        }
        let filter = AuditFilter {
            router_id: Some("did:octo:router-a".to_string()),
            ..Default::default()
        };
        let ids = list_receipts(&filter).unwrap();
        assert_eq!(ids, vec![1], "filter keeps only router-a rows");
    }

    #[test]
    fn list_receipts_clamps_limit() {
        let _guard = TEST_LOCK.lock().unwrap();
        {
            let mut reg = registry().lock().unwrap();
            reg.clear();
            for i in 0..5 {
                reg.insert(i, sample_receipt(i, 1_700_000_000 + i));
            }
        }
        let filter = AuditFilter {
            limit: Some(2),
            ..Default::default()
        };
        let ids = list_receipts(&filter).unwrap();
        assert_eq!(ids.len(), 2, "limit clamps to requested ceiling");
    }

    #[test]
    fn get_receipt_returns_seeded_receipt() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut reg = registry().lock().unwrap();
        reg.clear();
        reg.insert(42, sample_receipt(42, 1_700_000_042));
        drop(reg);

        let r = get_receipt(&42u64).unwrap();
        assert_eq!(r.receipt_id, 42);
        assert_eq!(r.router_id, "did:octo:router-a");
    }

    #[test]
    fn get_receipt_miss_returns_sink_specific() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut reg = registry().lock().unwrap();
        reg.clear();
        drop(reg);

        let err = get_receipt(&999u64).unwrap_err();
        match err {
            AuditError::SinkSpecific(s) => {
                assert!(
                    s.contains("999"),
                    "error carries the missed receipt_id: {s}"
                );
            }
            other => panic!("expected SinkSpecific, got {other:?}"),
        }
    }

    #[test]
    fn audit_filter_default_is_all_none() {
        let f = AuditFilter::default();
        assert!(f.router_id.is_none());
        assert!(f.timestamp_unix_gte.is_none());
        assert!(f.timestamp_unix_lte.is_none());
        assert!(f.limit.is_none());
        assert!(f.cursor.is_none());
    }
}
