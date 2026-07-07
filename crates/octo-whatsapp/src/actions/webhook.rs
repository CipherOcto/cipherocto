//! Webhook action dispatcher. Phase 5 Part F of
//! `docs/plans/2026-07-04-whatsapp-runtime-cli-mcp-design.md` §Security.

use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{ActionContext, ActionError};

/// Backwards-compatible structural validator. Phase 4 surface —
/// returns `Ok(())` once secret+allowlist pass structural checks.
pub async fn execute(
    url: &str,
    signing_secret_env: Option<&str>,
    allowed_domains: &[String],
    _ctx: &ActionContext,
) -> Result<(), ActionError> {
    if signing_secret_env.is_none() {
        return Err(ActionError::WebhookNotConfigured(format!(
            "url={url} has no signing_secret_env"
        )));
    }
    if allowed_domains.is_empty() {
        return Err(ActionError::WebhookNotConfigured(format!(
            "url={url} has empty allowed_domains"
        )));
    }
    Ok(())
}

/// Phase 5 Part F: production dispatcher. Resolves the signing
/// secret from the env var named `signing_secret_env` AT DISPATCH
/// TIME (not cached at rule creation — secrets can rotate), signs
/// the JSON body with HMAC-SHA256, sends the POST via the daemon's
/// shared `reqwest::Client`, and surfaces any HTTP failure as
/// `ActionError::Http`.
///
/// `allowed_domains` is enforced: an empty allowlist refuses
/// (defense-in-depth — callers should not be constructing webhook
/// actions with empty allowlists; the rule validation should have
/// rejected them at create time).
pub async fn dispatch(
    url: &str,
    signing_secret_env: Option<&str>,
    allowed_domains: &[String],
    ctx: &ActionContext,
) -> Result<(), ActionError> {
    if signing_secret_env.is_none() {
        return Err(ActionError::WebhookNotConfigured(format!(
            "url={url} has no signing_secret_env"
        )));
    }
    if allowed_domains.is_empty() {
        return Err(ActionError::WebhookNotConfigured(format!(
            "url={url} has empty allowed_domains"
        )));
    }
    // TLS-only (defense in depth; reqwest is already configured for
    // rustls-tls so http:// would fail at the URL parse stage when
    // we get to `.send()`). Explicitly reject http(s):// URLs that
    // are not parseable here.
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| ActionError::Http(format!("url parse: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(ActionError::Http(format!(
            "webhook scheme must be https, got {}",
            parsed.scheme()
        )));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| ActionError::Http("webhook url missing host".into()))?;
    if !domain_allowed(host, allowed_domains) {
        return Err(ActionError::Http(format!(
            "webhook host {host:?} not in allowlist {allowed_domains:?}"
        )));
    }

    // Resolve the secret from env at dispatch time. Missing env
    // ⇒ `WebhookNotConfigured` (fail closed).
    let env_var = signing_secret_env.unwrap();
    let secret = std::env::var(env_var).map_err(|_| {
        ActionError::WebhookNotConfigured(format!(
            "signing_secret_env {env_var:?} is not set or unreadable"
        ))
    })?;
    if secret.len() < 16 {
        return Err(ActionError::WebhookNotConfigured(format!(
            "signing_secret_env {env_var:?} is too short (< 16 chars); need >=128-bit entropy"
        )));
    }

    // Build the payload: a small envelope documenting the rule +
    // event summary. The full event is large; we ship the
    // canonical summary to keep the webhook payload bounded.
    let envelope = json!({
        "rule_id": ctx.rule_id,
        "rule_version": ctx.rule_version,
        "now_ms": ctx.now_ms,
        "caller_uid": ctx.caller_uid,
        "event": event_summary(&ctx.event),
    });
    let body = serde_json::to_vec(&envelope)
        .map_err(|e| ActionError::Http(format!("payload serialize: {e}")))?;

    // HMAC-SHA256 over the body. Header format:
    //   X-Octo-Signature: sha256=<hex>
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|e| ActionError::Http(format!("hmac key: {e}")))?;
    mac.update(&body);
    let sig = hex::encode(mac.finalize().into_bytes());

    let resp = ctx
        .daemon
        .http_client()
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Octo-Rule-Id", &ctx.rule_id)
        .header("X-Octo-Signature", format!("sha256={sig}"))
        .body(body)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            return Err(ActionError::Http(format!("send: {e}")));
        }
    };
    let status = resp.status();
    if !status.is_success() {
        return Err(ActionError::Http(format!(
            "webhook returned status {status}"
        )));
    }
    Ok(())
}

/// Returns true if `host` (which may be a bare domain, a wildcard
/// like `*.example.com`, or a full domain) matches any entry in
/// `allowed_domains`. Empty allowlist is handled by the caller.
fn domain_allowed(host: &str, allowed_domains: &[String]) -> bool {
    allowed_domains.iter().any(|pat| {
        if pat == host {
            return true;
        }
        // Wildcard: `*.example.com` matches `api.example.com` but
        // NOT `example.com` and NOT `evil.com`.
        if let Some(suffix) = pat.strip_prefix("*.") {
            return host.ends_with(&format!(".{suffix}"));
        }
        false
    })
}

/// Compact, deterministic summary of an `InboundEvent` for the
/// webhook payload. Trades fidelity for bounded size; callers
/// needing the full event can fetch via `events.get` keyed by
/// `event.id`.
fn event_summary(ev: &crate::events::InboundEvent) -> serde_json::Value {
    use crate::events::InboundEvent;
    match ev {
        InboundEvent::Message {
            id,
            peer,
            sender,
            ts_unix_ms,
            kind,
            text,
            is_group,
            ..
        } => json!({
            "kind": "message",
            "id": id,
            "peer": peer,
            "sender": sender,
            "ts_unix_ms": ts_unix_ms,
            "msg_kind": kind,
            "text": text,
            "is_group": is_group,
        }),
        InboundEvent::Reaction { id, peer, from, ts_unix_ms, target_msg_id, emoji, .. } => {
            json!({
                "kind": "reaction",
                "id": id,
                "peer": peer,
                "sender": from,
                "ts_unix_ms": ts_unix_ms,
                "target_msg_id": target_msg_id,
                "reaction": emoji,
            })
        }
        InboundEvent::Receipt { msg_id, peer, ts_unix_ms, kind, .. } => {
            json!({
                "kind": "receipt",
                "id": msg_id,
                "peer": peer,
                "ts_unix_ms": ts_unix_ms,
                "receipt_kind": kind,
            })
        }
        InboundEvent::GroupChange { group_jid, actor, ts_unix_ms, kind, .. } => {
            json!({
                "kind": "group_change",
                "group_jid": group_jid,
                "actor": actor,
                "ts_unix_ms": ts_unix_ms,
                "change_kind": kind,
            })
        }
        InboundEvent::Presence { jid, kind, .. } => {
            json!({
                "kind": "presence",
                "jid": jid,
                "presence_kind": kind,
            })
        }
        InboundEvent::Connection { kind, ts_unix_ms, .. } => {
            json!({
                "kind": "connection",
                "connection_kind": kind,
                "ts_unix_ms": ts_unix_ms,
            })
        }
        InboundEvent::Call { id, peer, kind, ts_unix_ms, .. } => {
            json!({
                "kind": "call",
                "id": id,
                "peer": peer,
                "call_kind": kind,
                "ts_unix_ms": ts_unix_ms,
            })
        }
        InboundEvent::Story { id, peer, kind, ts_unix_ms, .. } => {
            json!({
                "kind": "story",
                "id": id,
                "peer": peer,
                "story_kind": kind,
                "ts_unix_ms": ts_unix_ms,
            })
        }
        InboundEvent::Unknown { raw, ts_unix_ms, .. } => {
            json!({
                "kind": "unknown",
                "ts_unix_ms": ts_unix_ms,
                "raw_sha256": hex::encode(Sha256::digest(raw.as_bytes())),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionContext;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;
    use crate::events::{InboundEvent, MessageKind};
    use std::sync::Arc;

    fn handle() -> crate::daemon::DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "wh""#).unwrap();
        Daemon::new(cfg).handle()
    }

    fn ctx() -> ActionContext {
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
                is_group: false,
            }),
            caller_uid: "test".into(),
            now_ms: 0,
            daemon: handle(),
            metrics: None,
        }
    }

    #[test]
    fn domain_match_exact() {
        assert!(domain_allowed("example.com", &["example.com".into()]));
        assert!(!domain_allowed("evil.com", &["example.com".into()]));
    }

    #[test]
    fn domain_match_wildcard() {
        assert!(domain_allowed("api.example.com", &["*.example.com".into()]));
        assert!(!domain_allowed("example.com", &["*.example.com".into()]));
        assert!(!domain_allowed("evil-example.com", &["*.example.com".into()]));
    }

    #[tokio::test]
    async fn refuses_without_secret() {
        let err = dispatch(
            "https://example.com/h",
            None,
            &["example.com".into()],
            &ctx(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ActionError::WebhookNotConfigured(_)));
    }

    #[tokio::test]
    async fn refuses_empty_allowlist() {
        let err = dispatch(
            "https://example.com/h",
            Some("WH_SECRET"),
            &[],
            &ctx(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ActionError::WebhookNotConfigured(_)));
    }

    #[tokio::test]
    async fn refuses_http_scheme() {
        std::env::set_var("WH_TLS_TEST", "abcdef0123456789");
        let err = dispatch(
            "http://example.com/h",
            Some("WH_TLS_TEST"),
            &["example.com".into()],
            &ctx(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ActionError::Http(_)));
        std::env::remove_var("WH_TLS_TEST");
    }

    #[tokio::test]
    async fn refuses_host_not_in_allowlist() {
        std::env::set_var("WH_HOST_TEST", "abcdef0123456789");
        let err = dispatch(
            "https://evil.com/h",
            Some("WH_HOST_TEST"),
            &["example.com".into()],
            &ctx(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ActionError::Http(_)));
        std::env::remove_var("WH_HOST_TEST");
    }

    #[tokio::test]
    async fn refuses_short_secret() {
        std::env::set_var("WH_SHORT_TEST", "short");
        let err = dispatch(
            "https://example.com/h",
            Some("WH_SHORT_TEST"),
            &["example.com".into()],
            &ctx(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ActionError::WebhookNotConfigured(_)));
        std::env::remove_var("WH_SHORT_TEST");
    }

    #[tokio::test]
    async fn refuses_unset_secret_var() {
        std::env::remove_var("WH_UNSET_TEST");
        let err = dispatch(
            "https://example.com/h",
            Some("WH_UNSET_TEST"),
            &["example.com".into()],
            &ctx(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ActionError::WebhookNotConfigured(_)));
    }
}
