// quota-router-core - Core library for quota-router
// Contains business logic shared between CLI and PyO3 bindings

pub mod admin;
pub mod balance;
pub mod cache;
pub mod config;
pub mod fallback;
pub mod key_rate_limiter;
pub mod keys;
pub mod middleware;
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
    generate_key_id, generate_key_string, normalize_path, validate_key, KeyError,
};
pub use middleware::KeyMiddleware;
pub use schema::init_database;
pub use storage::KeyStorage;
pub use storage::StoolapKeyStorage;
