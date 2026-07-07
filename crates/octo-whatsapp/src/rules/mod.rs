//! Rules engine. Phase 4 of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`.
//!
//! Submodules:
//! - [`predicate`] — recursive predicate tree + ReDoS classifier.
//! - [`etag`] — canonical etag (RFC 8785 subset) for optimistic
//!   concurrency.
//! - [`rule`] — `Rule` struct, `RuleState`, `ActionSpec`.
//! - [`rule_store`] — `RuleStore` with `ArcSwap<Ruleset>`, CRUD,
//!   match_event with cooldown + priority sort.

pub mod etag;
pub mod persister;
pub mod predicate;
pub mod rule;
pub mod rule_store;

// Public re-exports — keep the surface narrow.
pub use etag::canonical_etag;
pub use persister::{
    resolve_storage_path, validate_persisted_rule, PersistError, PersistOp, PersistedRule,
    PersistedRuleset, RulesPersister,
};
pub use predicate::{classify_regex, event_kind, glob_match, Predicate, ReDoSError};
pub use rule::{ActionSpec, Rule, RuleState};
pub use rule_store::{MutationRateLimiter, RuleDraft, RuleError, RulePatch, RuleStore, Ruleset};

// Backwards-compat: the Phase 1 stub used `RulesView` for the
// read-only `rules.list|get` RPC. Phase 4 keeps the name but routes
// the handlers to `RuleStore` instead. We re-export a thin alias so
// external imports of `crate::rules::RulesView` continue to compile.
//
// (External imports are not expected in this crate — it's all
// internal — but we keep the type so old tests don't break.)
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RulesView {
    pub rules: Vec<Rule>,
}

impl RulesView {
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }
    pub fn list(&self) -> &[Rule] {
        &self.rules
    }
    pub fn get(&self, _id: &str) -> Option<&Rule> {
        None
    }
}
