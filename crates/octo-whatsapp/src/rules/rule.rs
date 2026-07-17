//! Rule + ActionSpec definitions. Phase 4.

use serde::{Deserialize, Serialize};

use super::predicate::Predicate;

/// State machine for a rule. New rules enter as `Draft` unless
/// `[security] auto_approve_rules = true`. `Approved` rules fire;
/// `Disabled` rules never fire even if matched. There is no
/// `Approved → Draft` transition — once approved, a rule stays
/// approved until deleted or explicitly disabled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleState {
    Draft,
    Approved,
    Disabled,
}

/// A named rule. The `etag` field is computed from
/// `{version, predicate, actions, cooldown_ms, ttl_until}` via the
/// canonical etag function; callers present it on update/delete for
/// optimistic concurrency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    pub id: String,
    pub version: u64,
    pub enabled: bool,
    pub priority: i32,
    pub predicate: Predicate,
    pub actions: Vec<ActionSpec>,
    pub cooldown_ms: u64,
    pub ttl_until: Option<i64>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub etag: String,
    pub state: RuleState,
}

impl Rule {
    /// Returns the fields that participate in the canonical etag.
    /// Adding a field to the etag input is a **breaking change** to
    /// the optimistic-concurrency contract — every persisted rule
    /// would need to be re-hashed. Keep this set narrow.
    pub fn etag_payload(&self) -> ETagPayload<'_> {
        ETagPayload {
            version: self.version,
            predicate: &self.predicate,
            actions: &self.actions,
            cooldown_ms: self.cooldown_ms,
            ttl_until: self.ttl_until,
            priority: self.priority,
            state: self.state,
        }
    }

    /// Returns true if the rule is currently eligible to fire:
    /// - `enabled` AND
    /// - `state == Approved` AND
    /// - `ttl_until` is `None` or in the future.
    pub fn is_fireable(&self, now_ms: i64) -> bool {
        if !self.enabled {
            return false;
        }
        if self.state != RuleState::Approved {
            return false;
        }
        match self.ttl_until {
            None => true,
            Some(until) => now_ms < until,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ETagPayload<'a> {
    pub version: u64,
    pub predicate: &'a Predicate,
    pub actions: &'a [ActionSpec],
    pub cooldown_ms: u64,
    pub ttl_until: Option<i64>,
    pub priority: i32,
    pub state: RuleState,
}

/// The action side of a rule. Each rule carries an ordered list of
/// actions; all listed actions execute (in order) when the rule
/// fires, unless the dispatcher rejects them.
///
/// `AgentRun` / non-allowlist `Webhook` / `Shell` actions always
/// require manual `rules.approve` even when
/// `[security] auto_approve_rules = true` (design §Security).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionSpec {
    /// HTTP POST to a webhook URL with HMAC signature.
    Webhook {
        url: String,
        signing_secret_env: Option<String>,
        allowed_domains: Vec<String>,
    },
    /// Invoke a registered trigger.
    AgentRun { trigger_id: String },
    /// Spawn a sandboxed shell process with the event payload as
    /// argv / stdin / `EVENT_TEXT` env var.
    Shell {
        argv: Vec<String>,
        timeout_ms: u64,
        env_passthrough: Vec<String>,
    },
    /// Push the event to MCP clients subscribed to the rule's
    /// notification template.
    McpNotify { template: String },
    /// Escalate to a named target (operator / oncall / etc.).
    Escalate { target: String, reason: String },
}

impl ActionSpec {
    /// True if the action requires manual `rules.approve` even when
    /// `auto_approve_rules = true`. Per design §Security:
    /// "even with `[security] auto_approve_rules = true`, rules
    ///  with `AgentRun` or non-allowlist Webhook or Shell actions
    ///  require manual `rules.approve`."
    pub fn requires_manual_approval(&self) -> bool {
        match self {
            ActionSpec::AgentRun { .. } => true,
            ActionSpec::Shell { .. } => true,
            ActionSpec::Webhook {
                allowed_domains, ..
            } => allowed_domains.is_empty(),
            ActionSpec::McpNotify { .. } | ActionSpec::Escalate { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_state_serde_round_trip() {
        for s in [RuleState::Draft, RuleState::Approved, RuleState::Disabled] {
            let j = serde_json::to_string(&s).unwrap();
            let back: RuleState = serde_json::from_str(&j).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn action_spec_serde_round_trip() {
        let a = ActionSpec::Webhook {
            url: "https://example.com/hook".into(),
            signing_secret_env: Some("HOOK_SECRET".into()),
            allowed_domains: vec!["example.com".into()],
        };
        let j = serde_json::to_string(&a).unwrap();
        let back: ActionSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn requires_manual_approval_rules() {
        assert!(ActionSpec::AgentRun {
            trigger_id: "t".into()
        }
        .requires_manual_approval());
        assert!(ActionSpec::Shell {
            argv: vec!["echo".into()],
            timeout_ms: 1000,
            env_passthrough: vec![],
        }
        .requires_manual_approval());
        assert!(ActionSpec::Webhook {
            url: "https://x.com/h".into(),
            signing_secret_env: None,
            allowed_domains: vec![],
        }
        .requires_manual_approval());
        assert!(!ActionSpec::Webhook {
            url: "https://x.com/h".into(),
            signing_secret_env: None,
            allowed_domains: vec!["x.com".into()],
        }
        .requires_manual_approval());
        assert!(!ActionSpec::McpNotify {
            template: "t".into()
        }
        .requires_manual_approval());
        assert!(!ActionSpec::Escalate {
            target: "oncall".into(),
            reason: "x".into(),
        }
        .requires_manual_approval());
    }

    #[test]
    fn is_fireable_checks_enabled_approved_and_ttl() {
        let mut r = Rule {
            id: "r".into(),
            version: 1,
            enabled: true,
            priority: 0,
            predicate: Predicate::True,
            actions: vec![],
            cooldown_ms: 0,
            ttl_until: None,
            created_by: "test".into(),
            created_at: 0,
            updated_at: 0,
            etag: String::new(),
            state: RuleState::Approved,
        };
        assert!(r.is_fireable(0));
        r.enabled = false;
        assert!(!r.is_fireable(0));
        r.enabled = true;
        r.state = RuleState::Draft;
        assert!(!r.is_fireable(0));
        r.state = RuleState::Approved;
        r.ttl_until = Some(100);
        assert!(r.is_fireable(50));
        assert!(!r.is_fireable(150));
    }
}
