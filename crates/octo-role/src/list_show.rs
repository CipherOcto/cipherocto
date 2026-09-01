//! `octo_role::list` + `octo_role::show` — read paths per RFC-0011-d §7.4.
//!
//! Substrate-authoritative read-side. Returns role summaries (list) or
//! full records (show) from the role registry. Filter parser follows
//! the `capability list` pattern from RFC-0011 §Subcommand Taxonomy.

use crate::error::RoleError;
use crate::registry;
use crate::types::{RoleFilter, RoleRecord, RoleSummary};

/// Enumerate roles in the registry, optionally filtered.
///
/// Matches TV-RL-1, TV-RL-2, TV-RL-3 (RFC-0011-d §11).
pub fn list(filter: &RoleFilter) -> Vec<RoleSummary> {
    registry::all_roles()
        .into_iter()
        .map(|r| r.summary)
        .filter(|s| filter.matches(s))
        .collect()
}

/// Fetch a single role record by slug.
///
/// Returns `RoleError::RoleNotFound` if no role with `role_id` exists
/// in the registry. Matches TV-RS-1, TV-RS-2, TV-RS-3.
pub fn show(role_id: &str) -> Result<RoleRecord, RoleError> {
    registry::all_roles()
        .into_iter()
        .find(|r| r.summary.name == role_id)
        .ok_or_else(|| RoleError::RoleNotFound {
            role_id: role_id.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tv_rl_1_list_all() {
        let summaries = list(&RoleFilter::default());
        assert_eq!(summaries.len(), 7);
    }

    #[test]
    fn tv_rl_2_list_filter_kind() {
        let filter = RoleFilter {
            kind: Some("provider".into()),
            ..Default::default()
        };
        let summaries = list(&filter);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].name, "provider");
    }

    #[test]
    fn tv_rl_3_list_filter_no_match() {
        let filter = RoleFilter {
            kind: Some("nonexistent".into()),
            ..Default::default()
        };
        let summaries = list(&filter);
        assert_eq!(summaries.len(), 0);
    }

    #[test]
    fn tv_rs_1_show_existing() {
        let record = show("builder").expect("builder exists");
        assert_eq!(record.summary.name, "builder");
        assert_eq!(
            record.summary.role_token_ticker.as_deref(),
            Some("OCTO-A")
        );
        assert!(!record.slashing_rules.is_empty());
        assert!(!record.allowed_actions.is_empty());
    }

    #[test]
    fn tv_rs_2_show_unknown_returns_role_not_found() {
        let err = show("nonexistent-role").unwrap_err();
        assert!(matches!(err, RoleError::RoleNotFound { .. }));
    }

    #[test]
    fn list_filter_by_min_stake() {
        let filter = RoleFilter {
            requires_octo_min: Some(1000_000_000),
            ..Default::default()
        };
        let summaries = list(&filter);
        // builder + provider + orchestrator at >= 1000 OCTO minimum
        assert!(summaries.iter().any(|s| s.name == "builder"));
        assert!(summaries.iter().any(|s| s.name == "orchestrator"));
        // recorder (100 OCTO) and wallet (None) excluded
        assert!(!summaries.iter().any(|s| s.name == "recorder"));
    }
}