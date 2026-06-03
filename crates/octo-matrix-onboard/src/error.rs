//! Error type for `octo-matrix-onboard`.
//!
//! Wraps `anyhow::Error` with an exit code so the binary can return
//! the right process status without each call site deciding on its
//! own. The exit-code table is in `docs/plans/2026-06-02-matrix-auth-
//! onboarding-design.md` §5:
//!
//! | Code | Meaning                                       |
//! |------|-----------------------------------------------|
//! |  0   | Success                                       |
//! |  1   | Generic (catch-all)                           |
//! |  2   | Auth rejected (wrong password, OAuth denied)  |
//! |  3   | Homeserver unreachable / DNS / TLS            |
//! |  4   | User cancelled (Ctrl-C, QR timeout, etc.)     |
//! |  5   | Bad config (output path unwritable, etc.)     |
//!
//! R1-M6: the `Display` impl for each kind variant renders ONLY the
//! short kind label. The inner `String` is reachable via the
//! `OnboardError::inner()` accessor for log enrichment and via
//! `Debug`, but is NOT shown to the operator by default — the
//! previous `format!("{:#}", e)` path at `main.rs:58` rendered the
//! SDK's full error string, which for `login_username` includes the
//! user-supplied username and homeserver URL.

use std::process::ExitCode;

#[derive(Debug, thiserror::Error)]
pub enum OnboardError {
    #[error("{0}")]
    Generic(#[from] anyhow::Error),

    #[error("auth rejected")]
    AuthRejected(String),

    #[error("homeserver unreachable")]
    Unreachable(String),

    #[error("cancelled")]
    Cancelled(String),

    #[error("bad config")]
    BadConfig(String),

    /// The homeserver explicitly throttled the request (HTTP 429).
    /// Distinct from `Unreachable` (network/DNS) so the operator can
    /// back off vs. retry the URL.
    #[error("rate limited")]
    RateLimited(String),
}

impl OnboardError {
    /// Inner message, if any. The Display impl does NOT include this;
    /// the operator sees only the kind label. The full message is
    /// preserved for log enrichment (caller routes through the
    /// redacting `tracing_subscriber` layer at DEBUG).
    #[allow(dead_code)] // Future-use API for log enrichment (R1-M6).
    pub fn inner(&self) -> Option<&str> {
        match self {
            OnboardError::Generic(_) => None,
            OnboardError::AuthRejected(s)
            | OnboardError::Unreachable(s)
            | OnboardError::Cancelled(s)
            | OnboardError::BadConfig(s)
            | OnboardError::RateLimited(s) => Some(s.as_str()),
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            OnboardError::Generic(_) => 1,
            OnboardError::AuthRejected(_) => 2,
            OnboardError::Unreachable(_) => 3,
            OnboardError::Cancelled(_) => 4,
            OnboardError::BadConfig(_) => 5,
            OnboardError::RateLimited(_) => 6,
        }
    }

    pub fn as_exit_code(&self) -> ExitCode {
        ExitCode::from(self.exit_code())
    }
}

/// Classify a matrix-sdk error variant (or any `Display`-able error)
/// into an `OnboardError` using the HTTP status code embedded in the
/// SDK's formatted message when available, falling back to substring
/// heuristics for the transport layer where the SDK does not surface
/// an API error.
///
/// R1-M10 / R1-M12: the previous substring-based classification
/// (`msg.contains("Unauthorized")`, `msg.contains("dns")`, etc.) is
/// fragile — a 401 with a body of `"dns_error: temporary"` would
/// misclassify as Unreachable, and a generic transport error would
/// never reach `AuthRejected` because `"Unauthorized"` is not in
/// the message.
///
/// The new path inspects the leading `[<status> / <errcode>]`
/// prefix that ruma's `ruma::api::error::Error::Display` emits
/// (see `ruma-common-0.18.0/src/api/error.rs:60-72`). That prefix
/// is the SDK's authoritative transport for the status code, so
/// dispatching on it is materially more reliable than guessing
/// from the free-form body text.
///
/// `matrix_sdk::Error` is `#[non_exhaustive]` in 0.17.0, which
/// prevents a non-SDK caller from pattern-matching its `Http`
/// variant. The status-code prefix on the formatted message is the
/// available typed-equivalent path. We also accept `HttpError` and
/// `ClientBuildError` and `OAuthError` here — all of them format
/// their error chains with a leading `[NNN ...]` ruma prefix when
/// the SDK has a status code to report.
///
/// 401 / 403 → `AuthRejected`, 429 → `RateLimited`, 5xx →
/// `Unreachable`, anything else → `Generic`. Transport errors
/// (DNS, connect) that don't carry a status code fall through to
/// the substring heuristic.
pub fn classify_sdk_err(where_: &str, e: impl std::fmt::Display) -> OnboardError {
    let msg = e.to_string();
    if let Some(status) = leading_status_code(&msg) {
        if status == 401 || status == 403 {
            return OnboardError::AuthRejected(format!("{where_}: HTTP {status}"));
        }
        if status == 429 {
            return OnboardError::RateLimited(format!("{where_}: HTTP 429"));
        }
        if (500..600).contains(&status) {
            return OnboardError::Unreachable(format!("{where_}: HTTP {status}"));
        }
        return OnboardError::Generic(anyhow::anyhow!("{where_}: HTTP {status}: {msg}"));
    }
    if msg.contains("dns") || msg.contains("DNS") || msg.contains("connect") {
        OnboardError::Unreachable(format!("{where_}: {msg}"))
    } else {
        OnboardError::Generic(anyhow::anyhow!("{where_}: {msg}"))
    }
}

/// If `msg` starts with a `[NNN ...]` ruma-style status code
/// prefix, return `Some(NNN)`. Otherwise return `None`. The match
/// is exact on the bracket + digits, so an unrelated leading `[` in
/// free-form text won't match.
fn leading_status_code(msg: &str) -> Option<u16> {
    let rest = msg.strip_prefix('[')?;
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

pub type Result<T> = std::result::Result<T, OnboardError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_status_code_extracts_401() {
        assert_eq!(
            leading_status_code("[401 / M_FORBIDDEN] bad token"),
            Some(401)
        );
        assert_eq!(leading_status_code("[429] slow down"), Some(429));
        assert_eq!(leading_status_code("[500] server error"), Some(500));
    }

    #[test]
    fn leading_status_code_rejects_non_ruma_format() {
        assert_eq!(leading_status_code("dns error: temporary"), None);
        assert_eq!(leading_status_code("401 Unauthorized"), None);
        assert_eq!(leading_status_code("[] empty"), None);
    }

    #[test]
    fn classify_sdk_err_routes_401_to_auth_rejected() {
        let e = classify_sdk_err("login", "[401 / M_FORBIDDEN] bad token");
        assert!(matches!(e, OnboardError::AuthRejected(_)), "got {:?}", e);
    }

    #[test]
    fn classify_sdk_err_routes_429_to_rate_limited() {
        let e = classify_sdk_err("login", "[429] slow down");
        assert!(matches!(e, OnboardError::RateLimited(_)), "got {:?}", e);
    }

    #[test]
    fn classify_sdk_err_routes_5xx_to_unreachable() {
        let e = classify_sdk_err("login", "[502] bad gateway");
        assert!(matches!(e, OnboardError::Unreachable(_)), "got {:?}", e);
    }

    #[test]
    fn classify_sdk_err_routes_dns_to_unreachable() {
        let e = classify_sdk_err("login", "dns error: failed to resolve");
        assert!(matches!(e, OnboardError::Unreachable(_)), "got {:?}", e);
    }

    #[test]
    fn classify_sdk_err_routes_unknown_to_generic() {
        let e = classify_sdk_err("login", "some other failure");
        assert!(matches!(e, OnboardError::Generic(_)), "got {:?}", e);
    }
}
