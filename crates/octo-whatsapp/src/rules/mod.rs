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
