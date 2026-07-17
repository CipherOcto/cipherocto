//! `McpNotify` action. Phase 5 Part F: pushes a
//! `Notification`-tagged envelope to subscribed sinks via the
//! `EventsRouter` fanout bus. The body of the envelope is the
//! `template` string plus a compact JSON summary of the event +
//! matching rule, sourced from `ctx.event` + `ctx.rule_id`.
//!
//! The fanout uses the same `EventsSink` machinery as inbound
//! events: a `try_send` per sink, with the sink's own Lagged
//! counter absorbing backpressure. Subscribers distinguish
//! notifications from inbound events by inspecting the envelope
//! shape (`{ "kind": "rule_notification", ... }`).

use super::{ActionContext, ActionError};
use serde_json::json;

pub async fn execute(template: &str, _ctx: &ActionContext) -> Result<(), ActionError> {
    if template.is_empty() {
        return Err(ActionError::ExecFailed("empty template".into()));
    }
    Ok(())
}

/// Phase 5 Part F: production dispatcher. Emits a
/// `RuleNotification` event onto the
/// `EventsRouter::notify(...)` bus; subscribers consume it via
/// the regular `EventsSubscriber::recv()` path.
pub async fn dispatch(template: &str, ctx: &ActionContext) -> Result<(), ActionError> {
    if template.is_empty() {
        return Err(ActionError::ExecFailed("empty template".into()));
    }
    let body = json!({
        "kind": "rule_notification",
        "rule_id": ctx.rule_id,
        "rule_version": ctx.rule_version,
        "now_ms": ctx.now_ms,
        "template": template,
        "event": ctx.event,
    });
    let body_str = body.to_string();
    // Push a synthetic InboundEvent::Unknown with the notification
    // payload as its `raw` text. Subscribers can filter by parsing
    // the raw JSON and checking `kind == "rule_notification"`.
    let notification = crate::events::InboundEvent::Unknown {
        raw: body_str,
        ts_unix_ms: ctx.now_ms,
        ts_mono_ns: 0,
        untrusted: false,
    };
    ctx.daemon.events_buffer().push(notification);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionContext;
    use crate::daemon::Daemon;
    use crate::events::{InboundEvent, MessageKind};
    use std::sync::Arc;

    fn handle() -> crate::daemon::DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn ctx() -> ActionContext {
        ctx_for(&handle())
    }

    fn ctx_for(h: &crate::daemon::DaemonHandle) -> ActionContext {
        ActionContext {
            rule_id: "r".into(),
            rule_version: 1,
            event: Arc::new(InboundEvent::Message {
                id: "M".into(),
                mentions_truncated: false,
                peer: "p".into(),
                sender: "s".into(),
                ts_unix_ms: 0,
                ts_mono_ns: 0,
                kind: MessageKind::Text,
                text: "x".into(),
                media_token: None,
                reply_to: None,
                mentions: Vec::new(),
                from_me: false,
                is_group: false,
                view_once: false,
                ephemeral_expires_at_seconds: None,
            }),
            caller_uid: "test".into(),
            now_ms: 0,
            daemon: h.clone(),
            metrics: None,
        }
    }

    #[tokio::test]
    async fn rejects_empty_template_execute() {
        assert!(execute("", &ctx()).await.is_err());
    }

    #[tokio::test]
    async fn accepts_non_empty_execute() {
        assert!(execute("tpl", &ctx()).await.is_ok());
    }

    #[tokio::test]
    async fn rejects_empty_template_dispatch() {
        assert!(dispatch("", &ctx()).await.is_err());
    }

    #[tokio::test]
    async fn dispatch_pushes_notification_into_events_buffer() {
        let h = handle();
        let c = ctx_for(&h);
        let pre = h.events_buffer().len();
        dispatch("tpl-test", &c).await.unwrap();
        let post = h.events_buffer().len();
        assert_eq!(post, pre + 1);
    }
}
