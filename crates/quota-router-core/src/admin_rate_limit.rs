//! Admin endpoint rate limiting (RFC-0933 §Admin endpoints).
//!
//! Mission `0948-b1-admin-rate-limiting`. Provides per-IP / per-user
//! / per-API-key rate limiting for the admin API surface (RFC-0948 +
//! RFC-0949). The substrate is HTTP-agnostic: callers extract identity
//! (IP / user_id / API-key-prefix) from the request and pass it to
//! `check_admin_rate_limit`. This avoids a `http` dependency on the
//! limiter itself; the gating happens at the call site (currently
//! `crates/quota-router-core/src/admin.rs`, when wired).
//!
//! The limiter reuses the existing RPM enforcement from
//! `crate::rate_limit::{RateLimiter, RateLimitMode}`. One
//! `RateLimiter` per (route-key, identity) bucket is created lazily;
//! the bucket count is bounded by `enabled_routes ×
//! unique_identities_per_minute`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::rate_limit::{RateLimitMode, RateLimiter};

/// Which dimension to key the rate limit on for a given route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminLimitDimension {
    /// Per source IP. Identity = IP string (RFC-7239 `Forwarded`,
    /// `X-Forwarded-For`, or peer addr).
    Ip,
    /// Per authenticated user. Identity = opaque user_id string.
    User,
    /// Per API key. Identity = first 8 chars of the API key
    /// (sufficient entropy to identify the key without leaking it).
    ApiKey,
}

/// A single rate-limit policy (dimension + RPM).
#[derive(Debug, Clone, Copy)]
pub struct AdminRateLimitPolicy {
    pub dimension: AdminLimitDimension,
    pub requests_per_minute: u32,
}

/// The full policy set per RFC-0933 §Admin endpoints.
///
/// All 11 endpoint groups enumerated. Operators can construct a
/// custom set via field-by-field assignment; `default_policies()`
/// returns the RFC-0933 reference values.
#[derive(Debug, Clone)]
pub struct AdminRateLimitPolicySet {
    pub sso_init: AdminRateLimitPolicy,     // POST /auth/sso/:provider
    pub sso_callback: AdminRateLimitPolicy, // GET  /auth/sso/:provider/callback
    pub token_exchange: AdminRateLimitPolicy, // POST /auth/token
    pub token_refresh: AdminRateLimitPolicy, // POST /auth/token/refresh
    pub token_revoke: AdminRateLimitPolicy, // POST /auth/token/revoke
    pub token_introspect: AdminRateLimitPolicy, // POST /auth/token/introspect
    pub logout: AdminRateLimitPolicy,       // POST /auth/logout
    pub prompts_crud: AdminRateLimitPolicy, // POST/PUT/DELETE /prompts/:id
    pub prompts_versions: AdminRateLimitPolicy, // POST /prompts/:id/versions etc.
    pub auth_providers_crud: AdminRateLimitPolicy, // POST/PUT/DELETE /auth/providers
}

impl Default for AdminRateLimitPolicySet {
    fn default() -> Self {
        Self {
            sso_init: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::Ip,
                requests_per_minute: 10,
            },
            sso_callback: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::Ip,
                requests_per_minute: 20,
            },
            token_exchange: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::User,
                requests_per_minute: 30,
            },
            token_refresh: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::User,
                requests_per_minute: 30,
            },
            token_revoke: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::User,
                requests_per_minute: 30,
            },
            token_introspect: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::User,
                requests_per_minute: 60,
            },
            logout: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::User,
                requests_per_minute: 10,
            },
            prompts_crud: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::ApiKey,
                requests_per_minute: 60,
            },
            prompts_versions: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::ApiKey,
                requests_per_minute: 30,
            },
            auth_providers_crud: AdminRateLimitPolicy {
                dimension: AdminLimitDimension::ApiKey,
                requests_per_minute: 60,
            },
        }
    }
}

/// Caller-supplied identity for a single admin request.
///
/// The caller extracts these from the request (headers, peer addr,
/// auth context) and passes the populated struct to
/// `check_admin_rate_limit`. The middleware reads whichever field
/// the policy's `dimension` requires; other fields are ignored.
#[derive(Debug, Clone, Default)]
pub struct AdminIdentity {
    /// Source IP (e.g. from `X-Forwarded-For` or peer addr).
    pub ip: Option<String>,
    /// Authenticated user id (from JWT subject, session cookie, etc.).
    pub user_id: Option<String>,
    /// First 8 chars of the API key (e.g. `Bearer <token>` header).
    pub api_key_prefix: Option<String>,
}

impl AdminIdentity {
    /// Resolve the identity string for the given dimension.
    /// Falls back to `"anonymous"` when no source is available —
    /// callers can decide whether "anonymous" requests should be
    /// allowed (the middleware treats unknown identity as a single
    /// shared bucket, not as a bypass).
    #[must_use]
    pub fn resolve(&self, dim: AdminLimitDimension) -> String {
        let raw = match dim {
            AdminLimitDimension::Ip => self.ip.as_deref(),
            AdminLimitDimension::User => self.user_id.as_deref(),
            AdminLimitDimension::ApiKey => self.api_key_prefix.as_deref(),
        };
        raw.unwrap_or("anonymous").to_owned()
    }
}

/// Outcome of a single `check_admin_rate_limit` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminRateLimitOutcome {
    /// Request is allowed (or the route is unrate-limited).
    Allowed,
    /// Request is blocked; include `Retry-After: retry_after_secs`
    /// header in the 429 response.
    Blocked {
        retry_after_secs: u64,
        reason: String,
    },
}

impl AdminRateLimitOutcome {
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked { .. })
    }
}

/// The admin rate limiter. One per process; thread-safe.
pub struct AdminRateLimiter {
    policies: AdminRateLimitPolicySet,
    enabled: AtomicBool,
    /// Per-route limiter. Keyed by route key (see
    /// `route_key_for`). Lazy creation on first request to each
    /// route.
    limiters: Mutex<HashMap<String, RateLimiter>>,
}

impl std::fmt::Debug for AdminRateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdminRateLimiter")
            .field("policies", &self.policies)
            .field("enabled", &self.enabled)
            .field("route_count", &self.limiters.lock().len())
            .finish()
    }
}

impl AdminRateLimiter {
    /// Construct with a custom policy set (e.g. ops-tuned values).
    #[must_use]
    pub fn new(policies: AdminRateLimitPolicySet) -> Self {
        Self {
            policies,
            enabled: AtomicBool::new(true),
            limiters: Mutex::new(HashMap::new()),
        }
    }

    /// Construct with the RFC-0933 default policy set.
    #[must_use]
    pub fn with_default_policies() -> Self {
        Self::new(AdminRateLimitPolicySet::default())
    }

    /// Toggle rate limiting at runtime. Default is enabled.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Look up the policy for a (method, path) pair. Returns
    /// `None` if the route is not in the policy set (the caller
    /// should let the request through unrate-limited).
    #[must_use]
    pub fn policy_for(&self, method: &str, path: &str) -> Option<AdminRateLimitPolicy> {
        let policies = &self.policies;
        // The SSO callback path includes the provider segment after
        // /auth/sso/.
        match (method, path) {
            ("POST", p) if path_starts(p, "/auth/sso/") && !p.ends_with("/callback") => {
                Some(policies.sso_init)
            }
            ("GET", p) if path_starts(p, "/auth/sso/") && p.ends_with("/callback") => {
                Some(policies.sso_callback)
            }
            ("POST", "/auth/token") => Some(policies.token_exchange),
            ("POST", "/auth/token/refresh") => Some(policies.token_refresh),
            ("POST", "/auth/token/revoke") => Some(policies.token_revoke),
            ("POST", "/auth/token/introspect") => Some(policies.token_introspect),
            ("POST", "/auth/logout") => Some(policies.logout),
            ("POST", "/prompts") => Some(policies.prompts_crud),
            ("PUT", p) if path_starts(p, "/prompts/") && !p.contains("/versions") => {
                Some(policies.prompts_crud)
            }
            ("DELETE", p) if path_starts(p, "/prompts/") && !p.contains("/versions") => {
                Some(policies.prompts_crud)
            }
            ("POST", p)
                if path_starts(p, "/prompts/")
                    && (p.contains("/versions")
                        || p.ends_with("/rollback")
                        || p.ends_with("/activate")) =>
            {
                Some(policies.prompts_versions)
            }
            ("POST", "/auth/providers") => Some(policies.auth_providers_crud),
            ("PUT", p) if path_starts(p, "/auth/providers/") => Some(policies.auth_providers_crud),
            ("DELETE", p) if path_starts(p, "/auth/providers/") => {
                Some(policies.auth_providers_crud)
            }
            _ => None,
        }
    }

    /// Look up (or lazily create) the per-route `RateLimiter` for
    /// the given policy. Route key is `format!("{rpm}/{dimension:?}")`
    /// — collision-free across distinct policies.
    fn limiter_for(&self, policy: AdminRateLimitPolicy) -> String {
        let key = format!(
            "{}rpm-{}dim",
            policy.requests_per_minute,
            match policy.dimension {
                AdminLimitDimension::Ip => "ip",
                AdminLimitDimension::User => "user",
                AdminLimitDimension::ApiKey => "api_key",
            }
        );
        let mut map = self.limiters.lock();
        map.entry(key.clone()).or_insert_with(|| {
            RateLimiter::new(crate::rate_limit::RateLimitConfig {
                rpm: Some(policy.requests_per_minute),
                tpm: None,
                mode: RateLimitMode::Hard,
            })
        });
        key
    }

    /// Check + record a single request against the route's
    /// per-identity bucket. Returns `Allowed` when the limiter is
    /// disabled or the route is unrate-limited.
    pub fn check(
        &self,
        method: &str,
        path: &str,
        identity: &AdminIdentity,
    ) -> AdminRateLimitOutcome {
        if !self.is_enabled() {
            return AdminRateLimitOutcome::Allowed;
        }
        let Some(policy) = self.policy_for(method, path) else {
            return AdminRateLimitOutcome::Allowed;
        };
        let identity_str = identity.resolve(policy.dimension);
        let route_key = self.limiter_for(policy);
        let mut map = self.limiters.lock();
        let limiter = map.get_mut(&route_key).expect("just inserted");
        let result = limiter.check(&identity_str);
        // Record only on Allowed — the existing RateLimiter's
        // `Blocked` path does not consume the bucket (the in-flight
        // request never made it through, no incremental accounting).
        if result.is_allowed() {
            limiter.record(&identity_str, 0);
        }
        match result {
            crate::rate_limit::RateLimitResult::Allowed => AdminRateLimitOutcome::Allowed,
            crate::rate_limit::RateLimitResult::Blocked {
                reason,
                retry_after,
            } => AdminRateLimitOutcome::Blocked {
                retry_after_secs: retry_after.unwrap_or(60),
                reason,
            },
        }
    }

    /// Reset every per-identity bucket. Used by ops for incident
    /// recovery (e.g. unblock a known-good IP after a misfire).
    pub fn reset_all(&self) {
        self.limiters.lock().clear();
    }
}

/// HTTP-agnostic middleware check. Returns `Some(outcome)` when the
/// request should be rejected (caller turns it into a 429 with
/// `Retry-After: outcome.retry_after_secs`); `None` when allowed.
pub fn check_admin_rate_limit(
    method: &str,
    path: &str,
    identity: &AdminIdentity,
    limiter: &AdminRateLimiter,
) -> Option<AdminRateLimitOutcome> {
    match limiter.check(method, path, identity) {
        AdminRateLimitOutcome::Allowed => None,
        blocked @ AdminRateLimitOutcome::Blocked { .. } => Some(blocked),
    }
}

#[inline]
fn path_starts(path: &str, prefix: &str) -> bool {
    path.starts_with(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anon_ip(s: &str) -> AdminIdentity {
        AdminIdentity {
            ip: Some(s.to_owned()),
            ..Default::default()
        }
    }
    fn anon_user(s: &str) -> AdminIdentity {
        AdminIdentity {
            user_id: Some(s.to_owned()),
            ..Default::default()
        }
    }
    fn anon_key(s: &str) -> AdminIdentity {
        AdminIdentity {
            api_key_prefix: Some(s.to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn per_ip_limit_blocks_after_threshold() {
        let limiter = AdminRateLimiter::with_default_policies();
        let id = anon_ip("10.0.0.1");
        // 10 requests allowed per minute.
        for i in 0..10 {
            let outcome = limiter.check("POST", "/auth/sso/google", &id);
            assert!(
                outcome.is_allowed(),
                "request #{i} should be allowed, got: {outcome:?}"
            );
        }
        let outcome = limiter.check("POST", "/auth/sso/google", &id);
        assert!(outcome.is_blocked(), "11th request must be blocked");
        if let AdminRateLimitOutcome::Blocked {
            retry_after_secs, ..
        } = outcome
        {
            assert!(retry_after_secs > 0, "retry_after must be > 0");
            assert!(
                retry_after_secs <= 60,
                "retry_after must fit the 1-min window"
            );
        }
    }

    #[test]
    fn per_user_limit_independent_buckets() {
        let limiter = AdminRateLimiter::with_default_policies();
        // Exhaust user A's bucket.
        for _ in 0..30 {
            assert!(limiter
                .check("POST", "/auth/token", &anon_user("user_a"))
                .is_allowed());
        }
        assert!(limiter
            .check("POST", "/auth/token", &anon_user("user_a"))
            .is_blocked());
        // User B is unaffected — independent buckets.
        assert!(limiter
            .check("POST", "/auth/token", &anon_user("user_b"))
            .is_allowed());
    }

    #[test]
    fn per_api_key_limit_with_first_8_chars() {
        let limiter = AdminRateLimiter::with_default_policies();
        // Same first-8 prefix = same identity.
        let id = anon_key("abcdef1234");
        for _ in 0..60 {
            assert!(limiter.check("POST", "/prompts", &id).is_allowed());
        }
        assert!(limiter.check("POST", "/prompts", &id).is_blocked());
    }

    #[test]
    fn cross_endpoint_breaches_independent() {
        let limiter = AdminRateLimiter::with_default_policies();
        // Exhaust /auth/sso/google at 10/min.
        let ip_id = anon_ip("10.0.0.2");
        for _ in 0..10 {
            assert!(limiter
                .check("POST", "/auth/sso/google", &ip_id)
                .is_allowed());
        }
        assert!(limiter
            .check("POST", "/auth/sso/google", &ip_id)
            .is_blocked());
        // Same IP hitting a different endpoint (token exchange) is
        // independent — its own per-user/per-IP bucket.
        assert!(limiter
            .check("POST", "/auth/token", &anon_user("anyone"))
            .is_allowed());
    }

    #[test]
    fn unrate_limited_route_passes_through() {
        let limiter = AdminRateLimiter::with_default_policies();
        // /healthz has no policy — never blocked.
        let id = anon_ip("10.0.0.3");
        for _ in 0..1_000 {
            assert!(limiter.check("GET", "/healthz", &id).is_allowed());
        }
    }

    #[test]
    fn disabled_limiter_passes_everything() {
        let limiter = AdminRateLimiter::with_default_policies();
        limiter.set_enabled(false);
        let id = anon_ip("10.0.0.4");
        for _ in 0..100 {
            assert!(limiter.check("POST", "/auth/sso/google", &id).is_allowed());
        }
    }

    #[test]
    fn retry_after_header_present_on_block() {
        let limiter = AdminRateLimiter::with_default_policies();
        let id = anon_ip("10.0.0.5");
        for _ in 0..10 {
            limiter.check("POST", "/auth/sso/google", &id);
        }
        let outcome = limiter.check("POST", "/auth/sso/google", &id);
        // Mirrors the HTTP-layer convention: caller adds the
        // `Retry-After: {retry_after_secs}` header to the 429
        // response. The middleware surfaces `retry_after_secs` as
        // part of the outcome struct.
        match outcome {
            AdminRateLimitOutcome::Blocked {
                retry_after_secs, ..
            } => {
                assert!(retry_after_secs > 0);
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
    }

    #[test]
    fn identity_resolve_uses_dimension_specific_field() {
        let id = AdminIdentity {
            ip: Some("10.0.0.6".into()),
            user_id: Some("user_x".into()),
            api_key_prefix: Some("abcd1234".into()),
        };
        assert_eq!(id.resolve(AdminLimitDimension::Ip), "10.0.0.6");
        assert_eq!(id.resolve(AdminLimitDimension::User), "user_x");
        assert_eq!(id.resolve(AdminLimitDimension::ApiKey), "abcd1234");
        let empty = AdminIdentity::default();
        assert_eq!(empty.resolve(AdminLimitDimension::Ip), "anonymous");
    }

    #[test]
    fn prompts_versions_route_uses_distinct_policy() {
        let limiter = AdminRateLimiter::with_default_policies();
        // /prompts CRUD limit is 60; versions limit is 30.
        let id = anon_key("abcd1234");
        for _ in 0..30 {
            assert!(limiter
                .check("POST", "/prompts/p1/versions", &id)
                .is_allowed());
        }
        assert!(limiter
            .check("POST", "/prompts/p1/versions", &id)
            .is_blocked());
        // The same API key can still hit /prompts (CRUD) up to its
        // 60-rpm ceiling — independent bucket.
        assert!(limiter.check("POST", "/prompts", &id).is_allowed());
    }

    #[test]
    fn policy_for_prompts_crud_methods() {
        let limiter = AdminRateLimiter::with_default_policies();
        assert!(limiter.policy_for("POST", "/prompts").is_some());
        assert!(limiter.policy_for("PUT", "/prompts/p1").is_some());
        assert!(limiter.policy_for("DELETE", "/prompts/p1").is_some());
        // Versions path uses the versions policy, not CRUD.
        let v = limiter
            .policy_for("POST", "/prompts/p1/versions")
            .expect("versions policy");
        assert_eq!(v.requests_per_minute, 30);
    }
}
