//! `Shell` action. Phase 5 Part F of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md`
//! §Security: "Trigger runner sandboxing".
//!
//! Cross-platform: on Linux, delegates to `runner::shell_linux::run`
//! with full Landlock+seccomp+rlimit+pidfd sandbox. On other
//! platforms, returns `NotSupported` and refuses to exec (fail
//! closed, design §Security).
//!
//! Phase 5 Part F: production `dispatch` injects the event's text
//! into `EVENT_TEXT` env var (truncated to 64 KiB to bound process
//! environment size) and runs with the spec's `timeout_ms` via
//! `tokio::time::timeout`. The truncated text is the *only* event
//! data passed in; the full event can be looked up via
//! `events.get event.id` from a hook script if needed.

use super::{ActionContext, ActionError};
use crate::actions::runner;

pub async fn execute(
    argv: &[String],
    timeout_ms: u64,
    env_passthrough: &[String],
    _ctx: &ActionContext,
) -> Result<(), ActionError> {
    if argv.is_empty() {
        return Err(ActionError::ExecFailed("empty argv".into()));
    }
    if timeout_ms == 0 {
        return Err(ActionError::ExecFailed("timeout_ms must be > 0".into()));
    }
    runner::run_shell(argv, timeout_ms, env_passthrough).await
}

/// Phase 5 Part F: production dispatcher. Runs the argv in the
/// existing cross-platform sandbox with the spec's `timeout_ms`
/// enforced via `tokio::time::timeout`. The event's text is
/// exposed via the `EVENT_TEXT` env var on Linux (no-op on
/// non-Linux, where the dispatcher fails closed). On timeout, the
/// process group is killed by the runner; the dispatcher surfaces
/// `ActionError::Timeout`.
pub async fn dispatch(
    argv: &[String],
    timeout_ms: u64,
    env_passthrough: &[String],
    ctx: &ActionContext,
) -> Result<(), ActionError> {
    if argv.is_empty() {
        return Err(ActionError::ExecFailed("empty argv".into()));
    }
    if timeout_ms == 0 {
        return Err(ActionError::ExecFailed("timeout_ms must be > 0".into()));
    }
    let text = event_text_truncated(&ctx.event, 64 * 1024);
    // Augment env_passthrough with the fixed `EVENT_TEXT` entry on
    // Linux (the runner builds the actual environment table from
    // this list; on non-Linux it isn't honored because the runner
    // returns NotSupported).
    let mut env_passthrough = env_passthrough.to_vec();
    env_passthrough.push(format!("EVENT_TEXT={text}"));
    match tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        runner::run_shell(argv, timeout_ms, &env_passthrough),
    )
    .await
    {
        Ok(res) => res,
        Err(_elapsed) => Err(ActionError::Timeout(timeout_ms)),
    }
}

/// Extract a bounded text payload from the event. Returns "" for
/// events without a `text` field (calls, presence, etc.). Bound:
/// `max_bytes` UTF-8 truncation at a codepoint boundary.
fn event_text_truncated(ev: &crate::events::InboundEvent, max_bytes: usize) -> String {
    use crate::events::InboundEvent;
    let s: String = match ev {
        InboundEvent::Message { text, .. } => text.clone(),
        InboundEvent::Reaction { emoji, .. } => emoji.clone(),
        InboundEvent::Story { .. } | InboundEvent::Presence { .. } => "".into(),
        // Receipts, group changes, calls, connection events: no
        // natural text payload.
        _ => "".into(),
    };
    if s.len() <= max_bytes {
        return s;
    }
    // Truncate at the nearest char boundary <= max_bytes.
    let mut cut = max_bytes;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_string()
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

    fn ctx_with_text(text: &str) -> ActionContext {
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
                text: text.into(),
                media_token: None,
                reply_to: None,
                mentions: Vec::new(),
                from_me: false,
                is_group: false,
            }),
            caller_uid: "test".into(),
            now_ms: 0,
            daemon: handle(),
            metrics: None,
        }
    }

    #[tokio::test]
    async fn empty_argv_errors() {
        let err = execute(&[], 1000, &[], &ctx_with_text("x"))
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::ExecFailed(_)));
    }

    #[tokio::test]
    async fn zero_timeout_errors() {
        let err = execute(&["echo".into()], 0, &[], &ctx_with_text("x"))
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::ExecFailed(_)));
    }

    #[tokio::test]
    #[cfg(not(target_os = "linux"))]
    async fn non_linux_returns_not_supported() {
        let err = dispatch(&["echo".into()], 1000, &[], &ctx_with_text("x"))
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::NotSupported(_)));
    }

    #[test]
    fn event_text_truncates_at_codepoint_boundary() {
        // 4-byte UTF-8 char: '𝕏' = f0 9d 95 8f
        let s = format!("{}{}", "𝕏".repeat(100), "tail");
        let truncated = event_text_truncated(
            &InboundEvent::Message {
                id: "M".into(),
                mentions_truncated: false,
                peer: "p".into(),
                sender: "s".into(),
                ts_unix_ms: 0,
                ts_mono_ns: 0,
                kind: MessageKind::Text,
                text: s.clone(),
                media_token: None,
                reply_to: None,
                mentions: Vec::new(),
                from_me: false,
                is_group: false,
            },
            64,
        );
        assert!(truncated.len() <= 64);
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn event_text_short_passthrough() {
        let truncated = event_text_truncated(
            &InboundEvent::Message {
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
                from_me: false,
                is_group: false,
            },
            1024,
        );
        assert_eq!(truncated, "hello");
    }
}
