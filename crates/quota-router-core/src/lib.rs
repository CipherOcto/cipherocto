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

pub mod admin;
pub mod balance;
pub mod cache;
pub mod config;
pub mod fallback;
pub mod key_rate_limiter;
pub mod keys;
pub mod middleware;
pub mod pricing;
pub mod providers;
pub mod proxy;
pub mod rate_limit;
pub mod router;
pub mod schema;
pub mod storage;

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
pub use schema::init_database;
pub use storage::KeyStorage;
pub use storage::StoolapKeyStorage;
