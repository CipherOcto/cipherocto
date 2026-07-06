//! `McpNotify` action. Pushes event to subscribed MCP clients.

use super::{ActionContext, ActionError};

/// Phase 4 stub: structural validation. Real fanout requires
/// access to the daemon's MCP client registry, which the handlers
/// drive directly. This stub keeps the dispatcher type-checking
/// for non-handler call paths.
pub async fn execute(template: &str, _ctx: &ActionContext) -> Result<(), ActionError> {
    if template.is_empty() {
        return Err(ActionError::ExecFailed("empty template".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionContext;
    use crate::events::{InboundEvent, MessageKind};
    use std::sync::Arc;

    fn ctx() -> ActionContext {
        ActionContext {
            rule_id: "r".into(),
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
                is_group: false,
            }),
            caller_uid: "test".into(),
            now_ms: 0,
        }
    }

    #[tokio::test]
    async fn rejects_empty_template() {
        assert!(execute("", &ctx()).await.is_err());
    }

    #[tokio::test]
    async fn accepts_non_empty() {
        assert!(execute("tpl", &ctx()).await.is_ok());
    }
}
