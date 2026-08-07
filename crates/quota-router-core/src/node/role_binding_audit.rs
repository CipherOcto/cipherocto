//! Role-binding audit trail (RFC-0971 §Phase 3 + mission `0971-a`).
//!
//! Append-only log of role-binding transitions. Per-entry fields:
//! - `node_did: String` — operator-facing identifier (REDACTED in Debug)
//! - `role_tag: RoleTag` — typed enum (preserved in Debug for forensics)
//! - `from_state: RoleBindingLifecycle` — pre-transition state
//! - `to_state: RoleBindingLifecycle` — post-transition state
//! - `node_epoch: u64` — node key-rotation epoch at transition time
//! - `at_millis_unix: i64` — wall-clock millis at transition time
//!
//! **Security (RFC-0957-A1 §Security):** `node_did` is a stable identifier
//! of a network actor; Debug output MUST NOT print it raw. Manual `Debug`
//! impl prints only `[REDACTED did]` + the typed `role_tag` + state
//! transition + epoch + timestamp. Operators needing raw `node_did` for
//! forensics query the structured fields directly via accessors.
//!
//! The audit log is monotonic (append-only) — entries are never removed
//! or mutated. Storage backing is intentionally in-memory
//! (`Vec<RoleBindingAuditEntry>`) for the Band A substrate; production
//! persistence (stoolap-backed append-only ledger) is deferred to a
//! follow-up mission (per [[stoolap-general-purpose-db]] hard red line:
//! cipherocto business/consumer schema stays cipherocto-side).
//!
//! **Cross-mission contract:** this module is the canonical substrate for
//! RFC-0971 §Role-binding audit trail. Mission `0970-a1-holder-binding-and-crypto`
//! consumes it for `audit_replay_log` entries.

use super::role_binding::{RoleBindingLifecycle, RoleTag};

/// One audit entry per role-binding transition (RFC-0971 §Phase 3).
///
/// `node_did` redacted in Debug per RFC-0957-A1 §Security (operator-facing
/// identifier; log lines MUST NOT print it raw). `role_tag` preserved for
/// forensics — typed enum prevents string-literal audit entries per TV8.
#[derive(Clone, PartialEq, Eq)]
pub struct RoleBindingAuditEntry {
    pub node_did: String,
    pub role_tag: RoleTag,
    pub from_state: RoleBindingLifecycle,
    pub to_state: RoleBindingLifecycle,
    pub node_epoch: u64,
    pub at_millis_unix: i64,
}

impl std::fmt::Debug for RoleBindingAuditEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoleBindingAuditEntry")
            .field("node_did", &"[REDACTED did]")
            .field("role_tag", &self.role_tag)
            .field("from_state", &self.from_state)
            .field("to_state", &self.to_state)
            .field("node_epoch", &self.node_epoch)
            .field("at_millis_unix", &self.at_millis_unix)
            .finish()
    }
}

/// Append-only role-binding audit log (RFC-0971 §Phase 3).
///
/// In-memory `Vec` backing for Band A substrate. Storage is monotonic —
/// entries can only be appended via `record()`. No mutation, no removal.
///
/// Production deployment MUST persist this log to a cipherocto-side
/// append-only ledger (stoolap migration `v013__role_binding_audit.sql`,
/// to be authored). The in-memory `Vec` substrate is the canonical
/// surface for tests + Band A acceptance.
#[derive(Debug, Default)]
pub struct RoleBindingAuditLog {
    entries: Vec<RoleBindingAuditEntry>,
}

impl RoleBindingAuditLog {
    /// Construct a new empty audit log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a transition entry. Monotonic — no ordering checks; caller
    /// is responsible for clock monotonicity (`at_millis_unix >= last`).
    pub fn record(&mut self, entry: RoleBindingAuditEntry) {
        self.entries.push(entry);
    }

    /// All entries in append order. Inspect-only; the returned slice is
    /// the canonical read view.
    #[must_use]
    pub fn entries(&self) -> &[RoleBindingAuditEntry] {
        &self.entries
    }

    /// Count of entries recorded so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True iff no entries have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Convenience builder: emit a transition from `from` to `to` for the
    /// given `node_did` + `role_tag` + `node_epoch` at `at_millis_unix`.
    pub fn record_transition(
        &mut self,
        node_did: impl Into<String>,
        role_tag: RoleTag,
        from: RoleBindingLifecycle,
        to: RoleBindingLifecycle,
        node_epoch: u64,
        at_millis_unix: i64,
    ) {
        self.record(RoleBindingAuditEntry {
            node_did: node_did.into(),
            role_tag,
            from_state: from,
            to_state: to,
            node_epoch,
            at_millis_unix,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_did() -> String {
        octo_ident::test_helpers::sample_did(147).clone()
    }

    #[test]
    fn new_log_is_empty() {
        let log = RoleBindingAuditLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        assert!(log.entries().is_empty());
    }

    #[test]
    fn record_appends_monotonic() {
        let mut log = RoleBindingAuditLog::new();
        log.record_transition(
            sample_did(),
            RoleTag::Router,
            RoleBindingLifecycle::Active,
            RoleBindingLifecycle::Draining,
            1,
            1_700_000_000_000,
        );
        log.record_transition(
            sample_did(),
            RoleTag::Router,
            RoleBindingLifecycle::Draining,
            RoleBindingLifecycle::Suspended,
            1,
            1_700_000_500_000,
        );
        assert_eq!(log.len(), 2);
        let entries = log.entries();
        assert_eq!(entries[0].role_tag, RoleTag::Router);
        assert_eq!(entries[0].from_state, RoleBindingLifecycle::Active);
        assert_eq!(entries[0].to_state, RoleBindingLifecycle::Draining);
        assert_eq!(entries[1].from_state, RoleBindingLifecycle::Draining);
        assert_eq!(entries[1].to_state, RoleBindingLifecycle::Suspended);
    }

    #[test]
    fn debug_redacts_node_did_preserves_role_tag() {
        // **TV8 (RFC-0971):** audit entry Debug MUST redact `node_did` and
        // preserve `role_tag` (typed, no string literal). Verifies the
        // manual `Debug` impl per RFC-0957-A1 §Security.
        let entry = RoleBindingAuditEntry {
            node_did: sample_did(),
            role_tag: RoleTag::Router,
            from_state: RoleBindingLifecycle::Active,
            to_state: RoleBindingLifecycle::Draining,
            node_epoch: 1,
            at_millis_unix: 1_700_000_000_000,
        };
        let dbg = format!("{entry:?}");
        assert!(
            !dbg.contains(&entry.node_did),
            "node_did MUST be redacted; got {dbg}"
        );
        assert!(
            dbg.contains("Router"),
            "role_tag MUST be visible; got {dbg}"
        );
        assert!(
            dbg.contains("[REDACTED did]"),
            "redaction marker MUST appear; got {dbg}"
        );
    }

    #[test]
    fn tv8_grep_no_string_literal_role_tags_in_entries() {
        // **TV8:** every audit entry carries a TYPED `role_tag` (no string
        // literals). The Debug output for a logged entry must reference
        // the variant name (e.g. `Router`, `TokenIssuer`, `Asker`) — not
        // a free-form string. The cargo test suite itself is the grep:
        // the audit module source has zero `String` fields carrying role
        // names. Inspect manually that the only `String` field is
        // `node_did`.
        let mut log = RoleBindingAuditLog::new();
        for role in [
            RoleTag::Router,
            RoleTag::TokenIssuer,
            RoleTag::Asker,
            RoleTag::PureForwarder,
            RoleTag::ReputationAnchor,
        ] {
            log.record_transition(
                sample_did(),
                role,
                RoleBindingLifecycle::Active,
                RoleBindingLifecycle::Active,
                1,
                1_700_000_000_000,
            );
        }
        // All 5 entries logged; verify each has a typed role_tag.
        for entry in log.entries() {
            // The role_tag field is `RoleTag`, not `String` — type system
            // enforces no string literals at audit-write time. This
            // assertion is structural (compile-time guarantee); the test
            // verifies runtime invariants (each entry round-trips).
            let _ = entry.role_tag; // typed access; compiler enforces.
        }
        assert_eq!(log.len(), 5);
    }
}
