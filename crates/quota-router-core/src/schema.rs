use crate::keys::KeyError;

/// Initialize database with api_keys and teams tables
pub fn init_database(db: &stoolap::Database) -> Result<(), KeyError> {
    // Create api_keys table
    // Note: Using rowid as implicit primary key, key_id is a unique text identifier
    db.execute(
        "CREATE TABLE IF NOT EXISTS api_keys (
            key_id TEXT NOT NULL UNIQUE,
            key_hash TEXT NOT NULL UNIQUE,
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

    // Create indexes
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
