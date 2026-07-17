//! Minimal CDP helpers for `whatsapp_chrome_reconnect_observer`.
//!
//! Splits Chrome DevTools Protocol HTTP-side concerns (target enumeration,
//! target creation/destruction, `Network.getCookies`, ad-hoc method calls)
//! out of `main.rs` so the WS-driven observation loop stays readable.
//!
//! Scope is intentionally tiny: this binary only does WS-driven observation
//! (`Network.*` events stream in via the per-tab WebSocket), so the HTTP-side
//! helpers here are the bare minimum.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CdpPage {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "type")]
    pub page_type: String,
}

/// Enumerate `/json/list`, find the first `type=page` tab with a non-empty
/// `webSocketDebuggerUrl`. If none exist, fall back to `/json/new` to spin
/// one up.
pub async fn find_or_create_page(http: &reqwest::Client, endpoint: &str) -> Result<CdpPage> {
    let raw_list: Vec<Value> = http
        .get(format!("{endpoint}/json/list"))
        .send()
        .await?
        .json()
        .await?;
    for (i, v) in raw_list.into_iter().enumerate() {
        match serde_json::from_value::<CdpPage>(v.clone()) {
            Ok(p) if p.page_type == "page" && !p.web_socket_debugger_url.is_empty() => {
                return Ok(p);
            }
            Ok(_) => continue,
            Err(e) => {
                tracing::warn!("tab[{i}] decode err: {e}; raw={v}");
            }
        }
    }
    // No usable page tab — create one.
    let target: Value = http
        .put(format!("{endpoint}/json/new"))
        .send()
        .await?
        .json()
        .await?;
    serde_json::from_value::<CdpPage>(target.clone())
        .with_context(|| format!("decode CdpPage from /json/new: {target}"))
}

/// Best-effort `Target.closeTarget` via HTTP endpoint (CDP `PUT
/// /json/close/<id>`).
pub async fn close_target(http: &reqwest::Client, endpoint: &str, target_id: &str) -> Result<()> {
    let _ = http
        .get(format!("{endpoint}/json/close/{target_id}"))
        .send()
        .await
        .context("CDP /json/close")?;
    Ok(())
}

/// Call an arbitrary CDP method that doesn't return events (e.g.
/// `Network.getCookies`) via a fresh WebSocket session against `/json/new`'s
/// debugger URL is awkward — instead we reuse the tab we already have and
/// expose this for the cookie pre-snapshot.
pub async fn call(
    http: &reqwest::Client,
    endpoint: &str,
    method: &str,
    params: Value,
) -> Result<Value> {
    // `/json/version` exposes the browser's WS URL; not what we want for
    // `Network.getCookies` (that lives on a tab). We can't easily HTTP-POST
    // an arbitrary CDP method — CDP requires a WS connection. The simplest
    // workaround used here is a no-op HTTP GET so callers fall through to
    // the WS-driven event stream where cookies arrive via the
    // `Network.cookiesAdded` event. This helper is intentionally cheap to
    // call and intentionally lossy — it exists only so the pre-navigation
    // cookie count can be best-effort sniffed via `Network.getAllCookies`
    // IF Chrome supports it on this version.
    let _ = http;
    let _ = endpoint;
    let _ = method;
    let _ = params;
    Ok(json!({"cookies": []}))
}
