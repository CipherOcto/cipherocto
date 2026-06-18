use crate::keys::KeyError;

/// Initialize database with api_keys and teams tables
pub fn init_database(db: &stoolap::Database) -> Result<(), KeyError> {
    // Create api_keys table
    // key_id and team_id are BLOB(16) per RFC-0903-C1 (raw UUID bytes).
    // key_hash is BYTEA(32) for HMAC-SHA256 binary storage.
    db.execute(
        "CREATE TABLE IF NOT EXISTS api_keys (
            key_id BLOB(16) NOT NULL,
            key_hash BYTEA(32) NOT NULL UNIQUE,
            key_prefix TEXT NOT NULL,
            team_id BLOB(16),
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
            metadata TEXT,
            rotated_from BLOB(16),
            rotation_grace_until INTEGER,
            UNIQUE(key_id)
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create teams table
    db.execute(
        "CREATE TABLE IF NOT EXISTS teams (
            team_id BLOB(16) NOT NULL,
            name TEXT NOT NULL,
            budget_limit INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            UNIQUE(team_id)
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
    // BLOB storage per RFC-0903-B1/C1: event_id (raw SHA256 32B), request_id (raw SHA256 32B),
    // key_id (raw UUID 16B), team_id (raw UUID 16B), pricing_hash (raw SHA256 32B).
    db.execute(
        "CREATE TABLE IF NOT EXISTS spend_ledger (
            event_id BLOB(32) NOT NULL,
            request_id BLOB(32) NOT NULL,
            key_id BLOB(16) NOT NULL,
            UNIQUE(key_id, request_id),
            team_id BLOB(16),
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cost_amount INTEGER NOT NULL,
            pricing_hash BLOB NOT NULL,
            token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
            tokenizer_id BLOB(16),
            tokenizer_version TEXT,
            provider_usage_json TEXT,
            timestamp INTEGER NOT NULL,
            created_at INTEGER NOT NULL DEFAULT 0
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create indexes for spend_ledger per RFC-0909
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

    // RFC-0909 additional indexes
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_event_id ON spend_ledger(event_id)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_key_created ON spend_ledger(key_id, created_at)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_spend_ledger_tokenizer ON spend_ledger(tokenizer_id)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Create tokenizers table per RFC-0910 §Tokenizer Database Schema
    // tokenizer_id is BLAKE3(version) truncated to 16 bytes — FK target from spend_ledger.tokenizer_id
    db.execute(
        "CREATE TABLE IF NOT EXISTS tokenizers (
            tokenizer_id BLOB(16) NOT NULL,
            version TEXT NOT NULL,
            vocab_size INTEGER,
            encoding_type TEXT,
            provider TEXT,
            PRIMARY KEY (tokenizer_id),
            UNIQUE(version, provider)
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // RFC-0904/F3: OCTO-W balance table for fee-based budget enforcement.
    db.execute(
        "CREATE TABLE IF NOT EXISTS octo_w_balances (
        key_id TEXT NOT NULL UNIQUE,
        balance INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL
    )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Provider API keys table — one provider can have multiple API keys
    // Used by python_sdk_entry set_api_key/get_budget_status/get_metrics
    db.execute(
        "CREATE TABLE IF NOT EXISTS provider_api_keys (
            id TEXT NOT NULL,
            provider TEXT NOT NULL,
            api_key_hash BYTEA(32) NOT NULL,
            api_key_prefix TEXT NOT NULL,
            label TEXT,
            created_at INTEGER NOT NULL,
            is_active INTEGER DEFAULT 1,
            UNIQUE(id)
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_provider_keys_provider ON provider_api_keys(provider)",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // rate_limit_state table (RFC-0914, Mission 0914-a)
    db.execute(
        "CREATE TABLE IF NOT EXISTS rate_limit_state (
            entity_id TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            counter_type TEXT NOT NULL,
            current_count INTEGER NOT NULL DEFAULT 0,
            window_start INTEGER NOT NULL,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (entity_id, entity_type, counter_type)
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // budgets table (RFC-0914, Mission 0914-a)
    db.execute(
        "CREATE TABLE IF NOT EXISTS budgets (
            entity_id TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            budget_limit INTEGER NOT NULL,
            period TEXT NOT NULL,
            current_spend INTEGER NOT NULL DEFAULT 0,
            soft_limit_pct INTEGER,
            alert_webhook TEXT,
            last_reset INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (entity_id, entity_type)
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    // Token blacklist for SSO token revocation (RFC-0949 Phase 5)
    db.execute(
        "CREATE TABLE IF NOT EXISTS token_blacklist (
            token_id TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            UNIQUE(token_id)
        )",
        [],
    )
    .map_err(|e| KeyError::Storage(e.to_string()))?;

    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires ON token_blacklist(expires_at)",
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
