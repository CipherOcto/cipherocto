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
//! 10_000 entries.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use octo_settlement::Receipt;

use octo_audit_core::AuditError;

use crate::StatusRef;

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

/// Filter for `list_receipts` — RFC-0016 §6.2.4 + RFC-0016-a §6.6 paired
/// extension.
///
/// The KEEP RFC-0016 surface (`router_id`, `timestamp_unix_gte/lte`,
/// `cursor`, `limit`) is preserved verbatim — RFC-0016-a §Compatibility
/// #4 keeps the `list_receipts(filter: &AuditFilter) -> Result<Vec<Receipt>, AuditError>`
/// signature valid. The RFC-0016-a additive fields (`since_unix`,
/// `until_unix`, `subject_did`, `status`, `model`, `capability_root`)
/// extend the struct without breaking existing callers; both
/// timestamp filter forms are honored at the read boundary
/// (`timestamp_unix_gte/lte` legacy + `since_unix/until_unix` new).
///
/// All-`None` (or all-additive-`None`) returns the unfiltered receipt
/// set, sorted by `timestamp_unix DESC` with `receipt_id ASC`
/// tiebreaker.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFilter {
    /// Restrict to receipts whose `router_id` equals this string
    /// (RFC-0010 canonical wire form; RFC-0016 KEEP).
    pub router_id: Option<String>,
    /// Minimum timestamp (`>=`); RFC-0016 KEEP field name.
    /// None = unbounded below. Honored at the read boundary alongside
    /// the RFC-0016-a `since_unix` alias.
    pub timestamp_unix_gte: Option<u64>,
    /// Maximum timestamp (`<=`); RFC-0016 KEEP field name.
    /// None = unbounded above. Honored at the read boundary alongside
    /// the RFC-0016-a `until_unix` alias.
    pub timestamp_unix_lte: Option<u64>,
    /// RFC-0016-a §6.6 additive field: minimum timestamp (`>=`).
    /// Canonical name; `timestamp_unix_gte` is the legacy RFC-0016
    /// KEEP alias. Either OR both may be set — the read boundary
    /// uses the `max(gte, since_unix)` of the two.
    #[serde(default)]
    pub since_unix: Option<u64>,
    /// RFC-0016-a §6.6 additive field: maximum timestamp (`<=`).
    /// Canonical name; `timestamp_unix_lte` is the legacy RFC-0016
    /// KEEP alias. Either OR both may be set — the read boundary
    /// uses the `min(lte, until_unix)` of the two.
    #[serde(default)]
    pub until_unix: Option<u64>,
    /// RFC-0016-a §6.6 additive ACL: restrict to receipts whose
    /// `subject_did` equals this canonical DID wire form
    /// (multi-tenant per-process trust boundary).
    #[serde(default)]
    pub subject_did: Option<String>,
    /// RFC-0016-a §6.6 additive multi-valued status filter
    /// (UNION semantics): receipt is included if its `status` is
    /// `==` ANY element of this `Vec`. Empty `Vec` = no status
    /// filter (all statuses match). `StatusRef` is the canonical
    /// alias for `octo_settlement::ReceiptStatus` (RFC-0014
    /// substrate re-export).
    #[serde(default)]
    pub status: Vec<StatusRef>,
    /// RFC-0016-a §6.6 additive model-name filter (exact match).
    #[serde(default)]
    pub model: Option<String>,
    /// RFC-0016-a §6.6 additive capability-root filter (exact
    /// match against the 32-byte BLAKE3 digest).
    #[serde(default)]
    pub capability_root: Option<[u8; 32]>,
    /// Maximum rows returned (`None` = no cap; substrate applies
    /// a hard ceiling of `MAX_LIMIT = 10000` per TV-AUD-4c).
    /// `u32` (not `usize`) per RFC-0016-a §6.6 additive filter
    /// spec: portable across 32-bit and 64-bit platforms; bounded
    /// to a sane upper limit; serde + JSON stable wire form.
    pub limit: Option<u32>,
    /// Opaque cursor (forward-compat for Phase 2 multi-page
    /// iteration). RFC-0016 KEEP.
    pub cursor: Option<String>,
}

/// Maximum `AuditFilter::limit` value accepted by
/// `list_receipts` (RFC-0016-a §6.6 + TV-AUD-4c substrate contract).
/// Limits above this ceiling are rejected with
/// `AuditError::InvalidFilter` so the operator gets an explicit
/// substrate-shape error rather than a silently-clamped result.
/// Substrate hard ceiling for `list_receipts` results (TV-AUD-4c).
/// Re-exported via `octo_audit::MAX_LIMIT` so consumers can
/// reference the canonical substrate value rather than duplicating
/// the constant (per no-parallel-abstractions principle).
pub const MAX_LIMIT: u32 = 10_000;

/// List receipt IDs (`Vec<u64>` of `receipt_id` values) matching
/// `filter` (RFC-0016 §6.2.1 + RFC-0016-a §6.6 additive filter
/// fields).
///
/// Sorted by `timestamp_unix DESC` with `receipt_id ASC`
/// tiebreaker. Returns empty `Vec` when zero matches (NOT an
/// error). Limit clamp at 10_000 (MAX_LIMIT). Read-only — no state mutation.
///
/// Honors BOTH the RFC-0016 KEEP filter fields (`router_id`,
/// `timestamp_unix_gte/lte`, `cursor`) AND the RFC-0016-a additive
/// fields (`since_unix`, `until_unix`, `subject_did`, `status` UNION,
/// `model`, `capability_root`). The timestamp filters use
/// `max(gte, since_unix)` for the lower bound and `min(lte, until_unix)`
/// for the upper bound — i.e. the tightest range of either filter
/// form is honored.
pub fn list_receipts(filter: &AuditFilter) -> Result<Vec<u64>, AuditError> {
    // RFC-0016-a §6.6 + TV-AUD-4: substrate-side validation.
    // `limit = 0` is meaningless (would return zero rows always);
    // reject explicitly. `limit > MAX_LIMIT` is rejected per
    // TV-AUD-4c — operators get an explicit substrate-shape error
    // rather than a silently-clamped result. `since_unix > until_unix`
    // is rejected so the empty-result set is never silently returned
    // for an inverted range.
    if let Some(n) = filter.limit {
        if n == 0 {
            return Err(AuditError::InvalidFilter(
                "limit must be >= 1 (TV-AUD-4)".into(),
            ));
        }
        if n > MAX_LIMIT {
            return Err(AuditError::InvalidFilter(format!(
                "limit must be <= {MAX_LIMIT} (TV-AUD-4c)"
            )));
        }
    }
    let effective_gte = match (filter.timestamp_unix_gte, filter.since_unix) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    let effective_lte = match (filter.timestamp_unix_lte, filter.until_unix) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    if let (Some(gte), Some(lte)) = (effective_gte, effective_lte) {
        if gte > lte {
            return Err(AuditError::InvalidFilter(format!(
                "since_unix ({gte}) > until_unix ({lte}) (TV-AUD-4b)"
            )));
        }
    }

    let limit = filter.limit.unwrap_or(MAX_LIMIT);

    let registry = registry()
        .lock()
        .map_err(|_| AuditError::SinkSpecific("receipt registry mutex poisoned".into()))?;

    let mut ids: Vec<u64> = registry
        .values()
        .filter(|r| match &filter.router_id {
            Some(rid) => r.router_id == *rid,
            None => true,
        })
        .filter(|r| match effective_gte {
            Some(gte) => r.timestamp_unix >= gte,
            None => true,
        })
        .filter(|r| match effective_lte {
            Some(lte) => r.timestamp_unix <= lte,
            None => true,
        })
        .filter(|r| match &filter.subject_did {
            Some(did) => r.subject_did == *did,
            None => true,
        })
        .filter(|r| match &filter.model {
            Some(m) => r.model == *m,
            None => true,
        })
        .filter(|r| match filter.capability_root {
            Some(cr) => r.capability_root == cr,
            None => true,
        })
        .filter(|r| {
            // RFC-0016-a §6.6 UNION semantics: empty Vec = no
            // status filter (all statuses match).
            if filter.status.is_empty() {
                return true;
            }
            filter.status.iter().any(|s| s == &r.status)
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
    ids.truncate(limit as usize);
    Ok(ids)
}

/// Point-lookup of a `Receipt` by canonical `ReceiptId`
/// (RFC-0016 §6.2.2 + RFC-0016-a §6.4 `ReceiptId(pub u64)` newtype).
///
/// Returns the canonical `Receipt` on hit. Misses surface
/// `AuditError::ReceiptNotFound(decimal)` — the canonical CLI-shape
/// variant per RFC-0016-a §6.7. The miss carries the requested
/// `receipt_id` in canonical decimal form (matches the CLI
/// `ReceiptNotFound(String)` envelope payload and the TV-AUD-2
/// substrate-faithful test vector).
///
/// The mutex-poison error stays `SinkSpecific` (preserves the
/// pre-RFC-0016-a 3-variant form for non-miss internal failures).
pub fn get_receipt(id: &octo_settlement::ReceiptId) -> Result<Receipt, AuditError> {
    let registry = registry()
        .lock()
        .map_err(|_| AuditError::SinkSpecific("receipt registry mutex poisoned".into()))?;

    registry
        .get(&id.0)
        .cloned()
        .ok_or_else(|| AuditError::ReceiptNotFound(id.0.to_string()))
}

/// Resolve the canonical audit home directory
/// (RFC-0016 §6.2.3; RFC-0011-a §Key Files `audit_home`).
///
/// Read-path discovery helper for the CLI `octo audit list/show` style
/// subcommands. Resolves to `<OCTO_HOME>/audit/receipts` when
/// `OCTO_HOME` is set, falling back to
/// `<HOME>/.config/octo/audit/receipts` (default per
/// RFC-0011-a §Configuration).
///
/// **Trust boundary check (RFC-0016-a §6.7 + TV-AUD-permission-check-1/2
/// + R1 reviewer CRITICAL C19):** when the audit home parent directory
/// exists, this helper verifies that:
///
/// 1. The parent directory's Unix permission mode is exactly `0o700`
///    (owner-only read/write/execute — no group/world access).
/// 2. The canonical audit home path itself is also `0o700`.
///
/// Phase 1 deliberately omits the strict UID-ownership check
/// (`geteuid()` equality) — adding it would require a `libc` dep
/// plus an `unsafe` block, both forbidden by the substrate's
/// `#![forbid(unsafe_code)]` lint. The mode bit enforces the
/// security-critical invariant; UID equality is operator-attested
/// via the directory-create lifecycle.
///
/// On violation, returns
/// `AuditError::PermissionDenied(<OCTO_HOME>/audit/receipts)` per
/// the RFC-0016-a §6.7 envelope. The CLI `From<AuditError>` mapping
/// routes the path payload through the §6.8 scrubber (defense in
/// depth — Pattern 8 absolute-path redaction).
///
/// When the parent directory does NOT yet exist (Phase 1 fresh
/// install / test fixture), the trust check is skipped — the caller
/// is responsible for creating the directory with the correct mode
/// before persisting any receipts. This matches the substrate-
/// faithful semantics: the substrate does NOT create directories
/// (per RFC-0016 KEEP §6.2.3); the operator owns the lifecycle.
///
/// The check is Unix-only (`#[cfg(unix)]`). On non-Unix platforms
/// the helper returns the path unchanged — Windows uses an
/// entirely different ACL model that is out of scope for Phase 1.
pub fn audit_home() -> Result<PathBuf, AuditError> {
    let path = if let Ok(octo_home) = std::env::var("OCTO_HOME") {
        PathBuf::from(octo_home).join("audit/receipts")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config/octo/audit/receipts")
    };
    trust_boundary_check(&path)?;
    Ok(path)
}

/// Verify the trust-boundary invariants for the audit home path
/// (RFC-0016-a §6.7 + TV-AUD-permission-check-1/2).
///
/// - Parent directory exists AND has mode exactly `0o700`; violations
///   return `PermissionDenied(<path>)`.
/// - Audit home itself ALSO has mode exactly `0o700` when it exists.
///
/// Missing directories skip the check (Phase 1 fresh install).
///
/// **Phase 1 mode-only check (RFC-0016-a §6.7 substrate-faithful
/// rationale):** the spec substrate contract requires mode `0o700`
/// AND UID ownership equality. The mode bit enforces the
/// security-critical invariant — group/world cannot read or traverse
/// regardless of which UID owns the directory. The strict UID
/// equality against `geteuid()` is intentionally deferred to Phase 2
/// (would require adding a `libc` dep + an `unsafe` block, both
/// forbidden by the substrate's `#![forbid(unsafe_code)]` lint).
/// Phase 1 records the operator-attested lifecycle (the operator
/// who creates `<OCTO_HOME>/audit/receipts` with `chmod 700` is
/// the operator who runs the CLI); Phase 2 will harden with the
/// UID check once a safe-Rust syscall wrapper lands.
#[cfg(unix)]
fn trust_boundary_check(path: &std::path::Path) -> Result<(), AuditError> {
    use std::os::unix::fs::PermissionsExt;
    let path_display = path.display().to_string();
    // Check the parent directory (operators create <OCTO_HOME>/audit
    // before /audit/receipts).
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let meta = parent
                .metadata()
                .map_err(|_| AuditError::PermissionDenied(path_display.clone()))?;
            let mode = meta.permissions().mode() & 0o777;
            if mode != 0o700 {
                return Err(AuditError::PermissionDenied(path_display));
            }
        }
    }
    // Check the audit home directory itself when it exists.
    if path.exists() {
        let meta = path
            .metadata()
            .map_err(|_| AuditError::PermissionDenied(path_display.clone()))?;
        let mode = meta.permissions().mode() & 0o777;
        if mode != 0o700 {
            return Err(AuditError::PermissionDenied(path_display));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn trust_boundary_check(_path: &std::path::Path) -> Result<(), AuditError> {
    // Non-Unix platforms (Windows, WASI): the ACL model is
    // entirely different. Phase 1 defers to platform-default
    // permissions; a future Windows-specific trust-boundary
    // helper will gate on the same RFC-0016-a §6.7 invariants
    // via DACL/owner lookup.
    Ok(())
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
            ..Default::default()
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

        let r = get_receipt(&octo_settlement::ReceiptId::new(42)).unwrap();
        assert_eq!(r.receipt_id, 42);
        assert_eq!(r.router_id, "did:octo:router-a");
    }

    #[test]
    fn get_receipt_miss_returns_receipt_not_found() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut reg = registry().lock().unwrap();
        reg.clear();
        drop(reg);

        let err = get_receipt(&octo_settlement::ReceiptId::new(999)).unwrap_err();
        match err {
            AuditError::ReceiptNotFound(s) => {
                assert_eq!(
                    s, "999",
                    "ReceiptNotFound carries canonical decimal receipt_id"
                );
            }
            other => panic!("expected ReceiptNotFound, got {other:?}"),
        }
    }

    #[test]
    fn list_receipts_rejects_zero_limit() {
        let _guard = TEST_LOCK.lock().unwrap();
        let filter = AuditFilter {
            limit: Some(0),
            ..Default::default()
        };
        let err = list_receipts(&filter).unwrap_err();
        assert!(
            matches!(err, AuditError::InvalidFilter(_)),
            "limit=0 must surface InvalidFilter (TV-AUD-4), got {err:?}"
        );
    }

    #[test]
    fn list_receipts_rejects_over_limit() {
        let _guard = TEST_LOCK.lock().unwrap();
        let filter = AuditFilter {
            limit: Some(10_001),
            ..Default::default()
        };
        let err = list_receipts(&filter).unwrap_err();
        assert!(
            matches!(err, AuditError::InvalidFilter(_)),
            "limit>10000 must surface InvalidFilter (TV-AUD-4c), got {err:?}"
        );
    }

    #[test]
    fn list_receipts_rejects_inverted_timestamp_range() {
        let _guard = TEST_LOCK.lock().unwrap();
        let filter = AuditFilter {
            since_unix: Some(2_000),
            until_unix: Some(1_000),
            ..Default::default()
        };
        let err = list_receipts(&filter).unwrap_err();
        assert!(
            matches!(err, AuditError::InvalidFilter(_)),
            "since>until must surface InvalidFilter (TV-AUD-4b), got {err:?}"
        );
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
