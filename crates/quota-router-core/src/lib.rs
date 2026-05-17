// quota-router-core - Core library for quota-router
// Contains business logic shared between CLI and PyO3 bindings
//
// ⚠️ CRITICAL INVARIANT (RFC-0917):
// Mode gate (litellm-mode/any-llm-mode/full) controls PROVIDER STRATEGY, NOT interface availability.
// BOTH HTTP proxy AND Python SDK exist in ALL modes:
//   - litellm-mode:  reqwest → provider REST APIs.    HTTP proxy ✅  Python SDK ✅
//   - any-llm-mode:  PyO3   → official Python SDKs.  HTTP proxy ✅  Python SDK ✅
//   - full:          Both reqwest AND PyO3.          HTTP proxy ✅  Python SDK ✅
//
// NEVER think "litellm-mode = proxy only" or "any-llm-mode = SDK only".
// See RFC-0917 lines 175-176: "HTTP Proxy Server | (always)" and "Python SDK Interface | (always)"

#![allow(deprecated)]

// HTTP server modules (admin, proxy, middleware) — ALWAYS available per RFC-0917 line 182:
// "HTTP Proxy Server | (always)" — NO feature gate, these are unconditionally compiled
pub mod admin;
pub mod balance;
pub mod cache;
pub mod config;
pub mod fallback;
pub mod key_rate_limiter;
pub mod keys;
pub mod metrics;
pub mod middleware;
pub mod pre_call_checks;
pub mod pricing;
pub mod providers;
pub mod proxy;
pub mod rate_limit;
pub mod router;
pub mod schema;
pub mod secret_manager;
pub mod storage;

// native_http — reqwest → provider REST APIs (INTERNAL boundary #1 per RFC-0917)
// Only compiled when litellm-mode or full feature is enabled
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub mod native_http;

/// Initialize native_http providers (litellm-mode/full only).
/// Must be called at binary startup before handling requests.
/// Safe to call multiple times — subsequent calls are no-ops.
#[cfg(any(feature = "litellm-mode", feature = "full"))]
pub fn init_native_http_providers() {
    crate::native_http::init_providers();
}

// py_bridge — PyO3 → official Python SDKs (INTERNAL boundary #1 per RFC-0917)
// Only compiled when any-llm-mode or full feature is enabled
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod py_bridge;

// python_sdk_entry — PyO3 entry point (EXTERNAL boundary #2 per RFC-0917)
// Only compiled when any-llm-mode or full feature is enabled
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod python_sdk_entry;

// shared_types — core types without PyO3 deps (used by native_http)
pub mod shared_types;

// py_bridge types (with PyO3 conversions) — for python_sdk_entry only
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod model;

// Shared types for py_bridge/python_sdk_entry
#[cfg(any(feature = "any-llm-mode", feature = "full"))]
pub mod types;

pub use cache::{
    check_budget_soft_limit, rotation_worker, validate_key_with_cache, CacheInvalidation, KeyCache,
    CACHE_SIZE, CACHE_TTL_SECS,
};
pub use key_rate_limiter::RateLimiterStore;
pub use keys::models::{
    ApiKey, CreateTeamRequest, GenerateKeyRequest, GenerateKeyResponse, KeySpend, KeyType,
    KeyUpdates, RevokeKeyRequest, SpendEvent, Team, TokenSource, UpdateTeamRequest,
};
pub use keys::{
    check_route_permission, check_team_key_limit, compute_event_id, compute_key_hash,
    encode_request_id, generate_key_id, generate_key_string, normalize_path, validate_key,
    validate_request_id, BudgetError, KeyError,
};
pub use middleware::KeyMiddleware;
pub use pricing::{
    compute_cost, get_canonical_tokenizer, tokenizer_version_to_id, CostError, PricingRegistry,
    PricingTable, RegistryError,
};
pub use router::RouterState;
pub use schema::init_database;
pub use storage::KeyStorage;
pub use storage::StoolapKeyStorage;
