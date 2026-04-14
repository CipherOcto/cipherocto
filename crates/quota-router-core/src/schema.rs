use crate::keys::KeyError;

/// Initialize database with api_keys and teams tables
pub fn init_database(db: &stoolap::Database) -> Result<(), KeyError> {
    // Create api_keys table
    // Note: Using rowid as implicit primary key, key_id is a unique text identifier
    // key_hash is BYTEA(32) for HMAC-SHA256 binary storage.
    db.execute(
        "CREATE TABLE IF NOT EXISTS api_keys (
            key_id TEXT NOT NULL UNIQUE,
            key_hash BYTEA(32) NOT NULL UNIQUE,
            key_prefix TEXT NOT NULL,
            team_id TEXT,
            budget_limit INTEGER NOT NULL,
            rpm_limit INTEGER,
            tpm_limit INTEGER,
            created_at INTEGER NOT NULL,
            expires_at INTEGER,
            revoked INTEGER DEFAULT 0,
            revoked_at INTEGER,
            revoked_by TEXT,
            revocation_reason TEXT,
            key_type TEXT DEFAULT 'default',
            allowed_routes TEXT,
            auto_rotate INTEGER DEFAULT 0,
            rotation_interval_days INTEGER,
            description TEXT,
            metadata TEXT
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create teams table
    db.execute(
        "CREATE TABLE IF NOT EXISTS teams (
            team_id TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            budget_limit INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create key_spend table for budget tracking
    db.execute(
        "CREATE TABLE IF NOT EXISTS key_spend (
            key_id TEXT NOT NULL UNIQUE,
            total_spend INTEGER NOT NULL DEFAULT 0,
            window_start INTEGER NOT NULL,
            last_updated INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create indexes
    // Note: idx_api_keys_hash is on key_hash BYTEA(32) column (binary).
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_api_keys_team_id ON api_keys(team_id)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_key_spend_key_id ON key_spend(key_id)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create spend_ledger table for ledger-based budget enforcement (RFC-0903)
    // pricing_hash is stored as BLOB (32 bytes) — stoolap supports native Blob type
    db.execute(
        "CREATE TABLE IF NOT EXISTS spend_ledger (
            event_id TEXT NOT NULL,
            request_id TEXT NOT NULL,
            key_id TEXT NOT NULL,
            UNIQUE(key_id, request_id),
            team_id TEXT,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cost_amount INTEGER NOT NULL,
            pricing_hash BLOB NOT NULL,
            token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
            tokenizer_version TEXT,
            provider_usage_json TEXT,
            timestamp INTEGER NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create indexes for spend_ledger
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_key_id ON spend_ledger(key_id)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_team_id ON spend_ledger(team_id)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_timestamp ON spend_ledger(timestamp)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_team_time ON spend_ledger(team_id, timestamp)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_database() {
        let db = stoolap::Database::open_in_memory().unwrap();
        init_database(&db).unwrap();
    }
}
