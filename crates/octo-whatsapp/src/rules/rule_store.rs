//! RuleStore — ArcSwap-backed rules engine with optimistic
//! concurrency and per-rule cooldown. Phase 4 of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;
use parking_lot::Mutex;

use super::etag::canonical_etag;
use super::predicate::Predicate;
use super::rule::{ActionSpec, Rule, RuleState};
use crate::events::InboundEvent;

/// `Ruleset` is the immutable snapshot stored inside the `ArcSwap`.
/// Every mutation produces a new `Arc<Ruleset>` and swaps it
/// atomically; readers hold a `Guard<Arc<Ruleset>>` for the duration
/// of one match evaluation and drop the guard before any await.
#[derive(Debug, Default)]
pub struct Ruleset {
    pub rules: Vec<Arc<Rule>>,
    pub by_id: HashMap<String, usize>,
    pub version: u64,
}

impl Ruleset {
    pub fn empty() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

/// `RuleStore` wraps an `ArcSwap<Ruleset>` and exposes the CRUD +
/// match surface that handlers call. Mutations never block the
/// matcher hot path: the matcher only reads.
#[derive(Debug)]
pub struct RuleStore {
    state: ArcSwap<Ruleset>,
    last_swap_generation: AtomicU64,
    swap_skipped: AtomicU64,
    last_fire_ms: Mutex<HashMap<String, i64>>,
    auto_approve_rules: bool,
}

impl RuleStore {
    pub fn new(auto_approve_rules: bool) -> Self {
        Self {
            state: ArcSwap::from_pointee(Ruleset::default()),
            last_swap_generation: AtomicU64::new(0),
            swap_skipped: AtomicU64::new(0),
            last_fire_ms: Mutex::new(HashMap::new()),
            auto_approve_rules,
        }
    }

    /// Loads the current `Arc<Ruleset>` snapshot for read-side use.
    /// Hold the returned guard only long enough to clone `Arc<Rule>` —
    /// drop it before any await (per design §Hot mutation safety).
    pub fn load(&self) -> arc_swap::Guard<Arc<Ruleset>> {
        self.state.load()
    }

    pub fn swap_skipped_total(&self) -> u64 {
        self.swap_skipped.load(Ordering::Relaxed)
    }

    pub fn swap_generation(&self) -> u64 {
        self.last_swap_generation.load(Ordering::Relaxed)
    }

    /// Lists every rule (cloned `Arc`s — cheap).
    pub fn list(&self) -> Vec<Arc<Rule>> {
        self.state.load().rules.clone()
    }

    /// Returns a cloned `Arc<Rule>` if present.
    pub fn get(&self, id: &str) -> Option<Arc<Rule>> {
        let s = self.state.load();
        s.by_id.get(id).and_then(|&i| s.rules.get(i).cloned())
    }

    /// Creates a new rule. The supplied `RuleDraft` is validated
    /// (regex ReDoS classification, slug format), assigned version
    /// 1, hashed for etag, and inserted. The new rule is returned
    /// (cloned). Returns `Err(RuleError)` if validation fails.
    pub fn create(&self, draft: RuleDraft) -> Result<Arc<Rule>, RuleError> {
        validate_id(&draft.id)?;
        validate_predicate(&draft.predicate)?;
        let now = draft.now_ms;
        let state = if self.auto_approve_rules
            && !draft.actions.iter().any(|a| a.requires_manual_approval())
        {
            RuleState::Approved
        } else {
            RuleState::Draft
        };
        let mut rule = Rule {
            id: draft.id,
            version: 1,
            enabled: draft.enabled,
            priority: draft.priority,
            predicate: draft.predicate,
            actions: draft.actions,
            cooldown_ms: draft.cooldown_ms,
            ttl_until: draft.ttl_until,
            created_by: draft.created_by,
            created_at: now,
            updated_at: now,
            etag: String::new(),
            state,
        };
        rule.etag = compute_etag(&rule);
        let rule = Arc::new(rule);
        self.insert(rule.clone())?;
        Ok(rule)
    }

    /// Updates an existing rule. The caller's `etag` must match the
    /// current rule's etag; mismatch returns
    /// `RuleError::Conflict { current_etag, current_version }`.
    pub fn update(
        &self,
        id: &str,
        caller_etag: &str,
        patch: RulePatch,
        now_ms: i64,
    ) -> Result<Arc<Rule>, RuleError> {
        let mut new_rule = {
            let current = self
                .get(id)
                .ok_or(RuleError::NotFound { id: id.to_string() })?;
            if current.etag != caller_etag {
                return Err(RuleError::Conflict {
                    id: id.to_string(),
                    current_etag: current.etag.clone(),
                    current_version: current.version,
                });
            }
            let mut r: Rule = (*current).clone();
            r.version += 1;
            r.updated_at = now_ms;
            if let Some(p) = patch.predicate {
                validate_predicate(&p)?;
                r.predicate = p;
            }
            if let Some(a) = patch.actions {
                r.actions = a;
            }
            if let Some(p) = patch.priority {
                r.priority = p;
            }
            if let Some(c) = patch.cooldown_ms {
                r.cooldown_ms = c;
            }
            if let Some(t) = patch.tttl_until {
                r.ttl_until = t;
            }
            if let Some(e) = patch.enabled {
                r.enabled = e;
            }
            r
        };
        new_rule.etag = compute_etag(&new_rule);
        self.replace(new_rule)
    }

    /// Deletes a rule. Optimistic concurrency: `caller_etag` must
    /// match the current rule's etag.
    pub fn delete(&self, id: &str, caller_etag: &str) -> Result<(), RuleError> {
        let current = self
            .get(id)
            .ok_or(RuleError::NotFound { id: id.to_string() })?;
        if current.etag != caller_etag {
            return Err(RuleError::Conflict {
                id: id.to_string(),
                current_etag: current.etag.clone(),
                current_version: current.version,
            });
        }
        let new_snapshot = {
            let s = self.state.load();
            let idx = *s.by_id.get(id).expect("present");
            let mut rules: Vec<Arc<Rule>> = Vec::with_capacity(s.rules.len() - 1);
            for (i, r) in s.rules.iter().enumerate() {
                if i != idx {
                    rules.push(r.clone());
                }
            }
            let by_id = rebuild_by_id(&rules);
            Arc::new(Ruleset {
                rules,
                by_id,
                version: s.version + 1,
            })
        };
        self.state.store(new_snapshot);
        self.last_swap_generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Flips `enabled` on an existing rule without otherwise
    /// changing it. Returns the new rule. Etag is recomputed.
    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
        now_ms: i64,
    ) -> Result<Arc<Rule>, RuleError> {
        let mut new_rule = {
            let current = self
                .get(id)
                .ok_or(RuleError::NotFound { id: id.to_string() })?;
            let mut r: Rule = (*current).clone();
            r.enabled = enabled;
            r.updated_at = now_ms;
            r.version += 1;
            r
        };
        new_rule.etag = compute_etag(&new_rule);
        self.replace(new_rule)
    }

    /// Approves a draft rule (Draft → Approved). Returns the new
    /// rule. Errors on `NotDraft` or `NotFound`.
    pub fn approve(&self, id: &str, now_ms: i64) -> Result<Arc<Rule>, RuleError> {
        let mut new_rule = {
            let current = self
                .get(id)
                .ok_or(RuleError::NotFound { id: id.to_string() })?;
            if current.state != RuleState::Draft {
                return Err(RuleError::NotDraft {
                    id: id.to_string(),
                    current_state: current.state,
                });
            }
            let mut r: Rule = (*current).clone();
            r.state = RuleState::Approved;
            r.updated_at = now_ms;
            r.version += 1;
            r
        };
        new_rule.etag = compute_etag(&new_rule);
        self.replace(new_rule)
    }

    /// Replaces the entire ruleset with a new one (used by
    /// `rules.reload` to read rules.toml from disk). The supplied
    /// `Vec<Arc<Rule>>` is the new full set; existing rules not in
    /// the new set are dropped.
    pub fn replace_all(&self, new_rules: Vec<Arc<Rule>>) {
        let by_id = rebuild_by_id(&new_rules);
        let snap = Arc::new(Ruleset {
            rules: new_rules,
            by_id,
            version: 0,
        });
        self.state.store(snap);
        self.last_swap_generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns rules that:
    /// - match the event per their predicate,
    /// - are `is_fireable(now)` (enabled + approved + not TTL-expired),
    /// - are not in cooldown for the supplied `now_ms`,
    ///
    /// Sorted by descending priority. Each matched rule has its
    /// `last_fire_ms` updated so that subsequent calls within the
    /// cooldown window will not re-fire it. This is the only
    /// mutation `match_event` performs.
    pub fn match_event(&self, ev: &InboundEvent, now_ms: i64) -> Vec<Arc<Rule>> {
        let snapshot = self.state.load();
        let mut matched: Vec<Arc<Rule>> = Vec::new();
        for r in snapshot.rules.iter() {
            if !r.is_fireable(now_ms) {
                continue;
            }
            if !r.predicate.matches(ev, now_ms) {
                continue;
            }
            matched.push(r.clone());
        }
        // Drop the snapshot guard BEFORE taking the cooldown lock.
        drop(snapshot);
        matched.sort_by_key(|r| std::cmp::Reverse(r.priority));
        let mut cooldown_map = self.last_fire_ms.lock();
        let mut filtered = Vec::with_capacity(matched.len());
        for r in matched {
            let last = cooldown_map.get(&r.id).copied().unwrap_or(i64::MIN);
            if now_ms.saturating_sub(last) < r.cooldown_ms as i64 {
                continue;
            }
            cooldown_map.insert(r.id.clone(), now_ms);
            filtered.push(r);
        }
        filtered
    }

    // ---- private helpers ----

    fn insert(&self, rule: Arc<Rule>) -> Result<(), RuleError> {
        let new_snapshot = {
            let s = self.state.load();
            if s.by_id.contains_key(&rule.id) {
                return Err(RuleError::AlreadyExists {
                    id: rule.id.clone(),
                });
            }
            let mut rules = s.rules.clone();
            rules.push(rule.clone());
            let by_id = rebuild_by_id(&rules);
            Arc::new(Ruleset {
                rules,
                by_id,
                version: s.version + 1,
            })
        };
        self.state.store(new_snapshot);
        self.last_swap_generation.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn replace(&self, new_rule: Rule) -> Result<Arc<Rule>, RuleError> {
        let new_arc = Arc::new(new_rule);
        let new_snapshot = {
            let s = self.state.load();
            if !s.by_id.contains_key(&new_arc.id) {
                return Err(RuleError::NotFound {
                    id: new_arc.id.clone(),
                });
            }
            let mut rules = s.rules.clone();
            let idx = *s.by_id.get(&new_arc.id).expect("present");
            rules[idx] = new_arc.clone();
            Arc::new(Ruleset {
                rules,
                by_id: s.by_id.clone(),
                version: s.version + 1,
            })
        };
        self.state.store(new_snapshot);
        self.last_swap_generation.fetch_add(1, Ordering::Relaxed);
        Ok(new_arc)
    }
}

fn rebuild_by_id(rules: &[Arc<Rule>]) -> HashMap<String, usize> {
    let mut m = HashMap::with_capacity(rules.len());
    for (i, r) in rules.iter().enumerate() {
        m.insert(r.id.clone(), i);
    }
    m
}

fn compute_etag(rule: &Rule) -> String {
    canonical_etag(&rule.etag_payload())
}

fn validate_id(id: &str) -> Result<(), RuleError> {
    if id.is_empty() {
        return Err(RuleError::InvalidId {
            reason: "empty".into(),
        });
    }
    if id.len() > 64 {
        return Err(RuleError::InvalidId {
            reason: "longer than 64 chars".into(),
        });
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(RuleError::InvalidId {
            reason: "must be [A-Za-z0-9_-] only".into(),
        });
    }
    Ok(())
}

fn validate_predicate(p: &Predicate) -> Result<(), RuleError> {
    // ReDoS check on every TextRegex leaf.
    let mut stack = vec![p];
    while let Some(node) = stack.pop() {
        match node {
            Predicate::TextRegex { pattern } => {
                super::predicate::classify_regex(pattern)
                    .map_err(|e| RuleError::UnsafeRegex(e.to_string()))?;
            }
            Predicate::And(children) | Predicate::Or(children) => {
                stack.extend(children.iter());
            }
            Predicate::Not(inner) => stack.push(inner),
            _ => {}
        }
    }
    Ok(())
}

// ---- types shared with handlers ----

#[derive(Debug, Clone)]
pub struct RuleDraft {
    pub id: String,
    pub enabled: bool,
    pub priority: i32,
    pub predicate: Predicate,
    pub actions: Vec<ActionSpec>,
    pub cooldown_ms: u64,
    pub ttl_until: Option<i64>,
    pub created_by: String,
    pub now_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct RulePatch {
    pub predicate: Option<Predicate>,
    pub actions: Option<Vec<ActionSpec>>,
    pub priority: Option<i32>,
    pub cooldown_ms: Option<u64>,
    pub tttl_until: Option<Option<i64>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    #[error("rule not found: {id}")]
    NotFound { id: String },
    #[error("rule already exists: {id}")]
    AlreadyExists { id: String },
    #[error(
        "etag conflict on {id} (current_version={current_version}, current_etag={current_etag})"
    )]
    Conflict {
        id: String,
        current_etag: String,
        current_version: u64,
    },
    #[error("invalid rule id: {reason}")]
    InvalidId { reason: String },
    #[error("unsafe regex: {0}")]
    UnsafeRegex(String),
    #[error("rule {id} is in state {current_state:?}, not Draft")]
    NotDraft {
        id: String,
        current_state: RuleState,
    },
    #[error("rule {id} already approved")]
    AlreadyApproved { id: String },
    #[error("rate limited: max {max_per_minute} rule mutations per minute")]
    RateLimited { max_per_minute: u64 },
}

// ---- Cooldown / rate-limit bookkeeping for `rules.create|update` ----

/// Per-caller rate limiter for rule mutations. Design §Hot mutation
/// safety: "10/min per caller_uid".
#[derive(Debug)]
pub struct MutationRateLimiter {
    max_per_minute: u64,
    bucket: Mutex<HashMap<String, (i64, u64)>>,
}

impl MutationRateLimiter {
    pub fn new(max_per_minute: u64) -> Self {
        Self {
            max_per_minute,
            bucket: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `Ok(())` if the call is permitted; `Err(RateLimited)`
    /// otherwise. `now_minute` is the floor of `now_ms / 60_000`.
    pub fn check(&self, caller_uid: &str, now_minute: i64) -> Result<(), RuleError> {
        let mut g = self.bucket.lock();
        let entry = g.entry(caller_uid.to_string()).or_insert((now_minute, 0));
        if entry.0 != now_minute {
            *entry = (now_minute, 0);
        }
        entry.1 += 1;
        if entry.1 > self.max_per_minute {
            return Err(RuleError::RateLimited {
                max_per_minute: self.max_per_minute,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_pred() -> Predicate {
        Predicate::EventKind {
            kinds: vec!["message".into()],
        }
    }

    fn dummy_draft(id: &str) -> RuleDraft {
        RuleDraft {
            id: id.into(),
            enabled: true,
            priority: 0,
            predicate: dummy_pred(),
            actions: vec![],
            cooldown_ms: 0,
            ttl_until: None,
            created_by: "test".into(),
            now_ms: 1000,
        }
    }

    fn msg_event() -> InboundEvent {
        use crate::events::MessageKind;
        InboundEvent::Message {
            id: "M".into(),
            mentions_truncated: false,
            peer: "p".into(),
            sender: "s".into(),
            ts_unix_ms: 0,
            ts_mono_ns: 0,
            kind: MessageKind::Text,
            text: "hello".into(),
            media_token: None,
            reply_to: None,
            mentions: Vec::new(),
            is_group: false,
        }
    }

    #[test]
    fn new_store_is_empty() {
        let s = RuleStore::new(false);
        assert!(s.list().is_empty());
        assert_eq!(s.swap_generation(), 0);
    }

    #[test]
    fn create_inserts_rule_and_assigns_etag() {
        let s = RuleStore::new(true);
        let r = s.create(dummy_draft("foo")).unwrap();
        assert_eq!(r.id, "foo");
        assert_eq!(r.version, 1);
        assert_eq!(r.state, RuleState::Approved); // auto-approve on
        assert!(!r.etag.is_empty());
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.swap_generation(), 1);
    }

    #[test]
    fn create_in_draft_state_without_auto_approve() {
        let s = RuleStore::new(false);
        let r = s.create(dummy_draft("foo")).unwrap();
        assert_eq!(r.state, RuleState::Draft);
    }

    #[test]
    fn create_rejects_duplicate_id() {
        let s = RuleStore::new(true);
        s.create(dummy_draft("dup")).unwrap();
        let err = s.create(dummy_draft("dup")).unwrap_err();
        assert!(matches!(err, RuleError::AlreadyExists { .. }));
    }

    #[test]
    fn create_rejects_empty_id() {
        let s = RuleStore::new(true);
        let err = s.create(dummy_draft("")).unwrap_err();
        assert!(matches!(err, RuleError::InvalidId { .. }));
    }

    #[test]
    fn create_rejects_invalid_id_chars() {
        let s = RuleStore::new(true);
        let err = s.create(dummy_draft("bad id!")).unwrap_err();
        assert!(matches!(err, RuleError::InvalidId { .. }));
    }

    #[test]
    fn create_rejects_unsafe_regex() {
        let s = RuleStore::new(true);
        let draft = RuleDraft {
            predicate: Predicate::TextRegex {
                pattern: "(a+)+".into(),
            },
            ..dummy_draft("bad")
        };
        let err = s.create(draft).unwrap_err();
        assert!(matches!(err, RuleError::UnsafeRegex(_)));
    }

    #[test]
    fn update_increments_version_and_changes_etag() {
        let s = RuleStore::new(true);
        let r = s.create(dummy_draft("foo")).unwrap();
        let old_etag = r.etag.clone();
        let new = s
            .update(
                "foo",
                &old_etag,
                RulePatch {
                    priority: Some(99),
                    ..Default::default()
                },
                2000,
            )
            .unwrap();
        assert_eq!(new.version, 2);
        assert_eq!(new.priority, 99);
        assert_ne!(new.etag, old_etag);
    }

    #[test]
    fn update_with_stale_etag_returns_conflict() {
        let s = RuleStore::new(true);
        let r = s.create(dummy_draft("foo")).unwrap();
        // First update succeeds and bumps version.
        let _ = s
            .update(
                "foo",
                &r.etag,
                RulePatch {
                    priority: Some(1),
                    ..Default::default()
                },
                2000,
            )
            .unwrap();
        // Replaying with the old etag must fail.
        let err = s
            .update(
                "foo",
                &r.etag,
                RulePatch {
                    priority: Some(2),
                    ..Default::default()
                },
                3000,
            )
            .unwrap_err();
        match err {
            RuleError::Conflict {
                current_version, ..
            } => assert_eq!(current_version, 2),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn delete_with_correct_etag_succeeds() {
        let s = RuleStore::new(true);
        let r = s.create(dummy_draft("foo")).unwrap();
        s.delete("foo", &r.etag).unwrap();
        assert!(s.list().is_empty());
    }

    #[test]
    fn delete_with_wrong_etag_returns_conflict() {
        let s = RuleStore::new(true);
        let _r = s.create(dummy_draft("foo")).unwrap();
        let err = s.delete("foo", "stale-etag").unwrap_err();
        assert!(matches!(err, RuleError::Conflict { .. }));
    }

    #[test]
    fn delete_missing_returns_not_found() {
        let s = RuleStore::new(true);
        let err = s.delete("nope", "x").unwrap_err();
        assert!(matches!(err, RuleError::NotFound { .. }));
    }

    #[test]
    fn approve_draft_transitions_to_approved() {
        let s = RuleStore::new(false); // draft mode
        let r = s.create(dummy_draft("foo")).unwrap();
        assert_eq!(r.state, RuleState::Draft);
        let approved = s.approve("foo", 5000).unwrap();
        assert_eq!(approved.state, RuleState::Approved);
    }

    #[test]
    fn approve_non_draft_returns_error() {
        let s = RuleStore::new(true);
        let r = s.create(dummy_draft("foo")).unwrap();
        assert_eq!(r.state, RuleState::Approved);
        let err = s.approve("foo", 5000).unwrap_err();
        assert!(matches!(err, RuleError::NotDraft { .. }));
    }

    #[test]
    fn set_enabled_toggles_flag() {
        let s = RuleStore::new(true);
        let r = s.create(dummy_draft("foo")).unwrap();
        assert!(r.enabled);
        let disabled = s.set_enabled("foo", false, 5000).unwrap();
        assert!(!disabled.enabled);
    }

    #[test]
    fn match_event_sorts_by_priority_descending() {
        let s = RuleStore::new(true);
        s.create(RuleDraft {
            id: "low".into(),
            priority: 1,
            ..dummy_draft("low")
        })
        .unwrap();
        s.create(RuleDraft {
            id: "high".into(),
            priority: 100,
            ..dummy_draft("high")
        })
        .unwrap();
        s.create(RuleDraft {
            id: "mid".into(),
            priority: 50,
            ..dummy_draft("mid")
        })
        .unwrap();
        let matched = s.match_event(&msg_event(), 0);
        let ids: Vec<&str> = matched.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["high", "mid", "low"]);
    }

    #[test]
    fn match_event_respects_cooldown() {
        let s = RuleStore::new(true);
        s.create(RuleDraft {
            id: "r".into(),
            cooldown_ms: 5_000,
            ..dummy_draft("r")
        })
        .unwrap();
        let first = s.match_event(&msg_event(), 1000);
        assert_eq!(first.len(), 1);
        // Within cooldown: no match.
        let second = s.match_event(&msg_event(), 2000);
        assert_eq!(second.len(), 0);
        // After cooldown: re-match.
        let third = s.match_event(&msg_event(), 7000);
        assert_eq!(third.len(), 1);
    }

    #[test]
    fn match_event_skips_disabled_and_draft() {
        let s = RuleStore::new(false);
        let r = s.create(dummy_draft("foo")).unwrap();
        assert_eq!(r.state, RuleState::Draft);
        // Draft state: not fireable.
        let matched = s.match_event(&msg_event(), 0);
        assert!(matched.is_empty());
    }

    #[test]
    fn replace_all_drops_unknown_rules() {
        let s = RuleStore::new(true);
        s.create(dummy_draft("a")).unwrap();
        s.create(dummy_draft("b")).unwrap();
        // Build a fresh rule with the new id `c`.
        let c = Arc::new(Rule {
            id: "c".into(),
            version: 1,
            enabled: true,
            priority: 0,
            predicate: dummy_pred(),
            actions: vec![],
            cooldown_ms: 0,
            ttl_until: None,
            created_by: "test".into(),
            created_at: 0,
            updated_at: 0,
            etag: "x".into(),
            state: RuleState::Approved,
        });
        s.replace_all(vec![c]);
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.list()[0].id, "c");
    }

    #[test]
    fn rate_limiter_allows_until_limit_then_rejects() {
        let rl = MutationRateLimiter::new(3);
        assert!(rl.check("alice", 0).is_ok());
        assert!(rl.check("alice", 0).is_ok());
        assert!(rl.check("alice", 0).is_ok());
        let err = rl.check("alice", 0).unwrap_err();
        assert!(matches!(err, RuleError::RateLimited { .. }));
        // Next minute: bucket resets.
        assert!(rl.check("alice", 1).is_ok());
        // Different caller: separate bucket.
        assert!(rl.check("bob", 0).is_ok());
    }
}
