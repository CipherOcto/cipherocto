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

    #[error("Rate limited")]
    RateLimited,

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Invalid key format")]
    InvalidFormat,

    #[error("Key already exists")]
    AlreadyExists,

    #[error("Missing API key")]
    MissingKey,
}
