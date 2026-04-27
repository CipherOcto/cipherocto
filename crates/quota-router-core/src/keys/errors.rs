use thiserror::Error;

#[derive(Error, Debug)]
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
}

/// Budget enforcement errors for cost computation and balance operations.
/// Delegates to RFC-0910 CostError for cost computation overflow.
#[derive(Error, Debug)]
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
