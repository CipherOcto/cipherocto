use thiserror::Error;

use super::models::SpendEventError;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    #[error("Key not found")]
    NotFound,

    #[error("Key expired at {0}")]
    Expired(i64),

    #[error("Key revoked: {0}")]
    Revoked(String),

    #[error("Budget exceeded: current={current}, limit={limit}")]
    BudgetExceeded { current: u64, limit: u64 },

    #[error("Team budget exceeded: current={current}, limit={limit}")]
    TeamBudgetExceeded { current: u64, limit: u64 },

    #[error("Team key limit exceeded: current={current}, limit={limit}")]
    TeamKeyLimitExceeded { current: u32, limit: u32 },

    #[error("Rate limited, retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Invalid key format")]
    InvalidFormat,

    #[error("Key already exists")]
    AlreadyExists,

    #[error("Missing API key")]
    MissingKey,

    #[error("Route not allowed: {0}")]
    RouteNotAllowed(String),

    /// SpendEvent boundary validation failure (mission 0862-c7).
    /// Carries the `SpendEventError` for diagnostic context.
    #[error("Spend event validation failed: {0}")]
    SpendEvent(SpendEventError),
}

impl From<SpendEventError> for KeyError {
    fn from(e: SpendEventError) -> Self {
        KeyError::SpendEvent(e)
    }
}

/// Budget enforcement errors for cost computation and balance operations.
/// Delegates to RFC-0910 CostError for cost computation overflow.
#[derive(Error, Debug, Clone)]
pub enum BudgetError {
    #[error("API key not found")]
    KeyNotFound,

    #[error("Team not found")]
    TeamNotFound,

    #[error("Key budget exceeded: current={current}, limit={limit}, requested={requested}")]
    KeyBudgetExceeded {
        key_id: uuid::Uuid,
        current: u64,
        limit: u64,
        requested: u64,
    },

    #[error("Team budget exceeded: current={current}, limit={limit}, requested={requested}")]
    TeamBudgetExceeded {
        team_id: uuid::Uuid,
        current: u64,
        limit: u64,
        requested: u64,
    },

    #[error("Model not found in pricing table: {0}")]
    ModelNotFound(String),

    #[error("Cost computation overflow")]
    CostOverflow,

    #[error("Insufficient OCTO-W balance for key {key_id}: available={available}, estimated={estimated}")]
    InsufficientBalance {
        key_id: uuid::Uuid,
        available: u64,
        estimated: u64,
    },

    #[error("Storage error: {0}")]
    Storage(String),
}

/// Storage and database operation errors (RFC-0903/0904).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    #[error("Key not found in storage")]
    KeyNotFound,

    #[error("OCTO-W not enabled for this key")]
    OctoWNotEnabled,

    #[error("Insufficient OCTO-W balance: available={available}, requested={requested}")]
    InsufficientBalance { available: u64, requested: u64 },

    #[error("Database error: {0}")]
    Database(String),
}

/// Unified error type for RFC-0917 public API.
///
/// Wraps error types from constituent RFCs:
/// - RFC-0903: KeyError (API key validation, team operations)
/// - RFC-0904: BudgetError (budget enforcement, spend tracking)
/// - RFC-0910: RegistryError (pricing table registration)
/// - RFC-0917: RouterError (routing, provider dispatch)
/// - RFC-0903/0904: StorageError (database operations)
///
/// This enum is retrofitted across all public API return types in
/// RFC-0903, RFC-0904, RFC-0909, RFC-0910, and RFC-0917.
#[derive(Error, Debug, Clone)]
pub enum QuotaRouterError {
    #[error("Key error: {0}")]
    Key(KeyError),

    #[error("Budget error: {0}")]
    Budget(BudgetError),

    #[error("Router error: {0:?}")]
    Router(crate::fallback::RouterError),

    #[error("Registry error: {0:?}")]
    Registry(crate::pricing::RegistryError),

    #[error("Storage error: {0}")]
    Storage(StorageError),

    #[error("Provider {provider} error: {message}")]
    ProviderError { provider: String, message: String },
}
