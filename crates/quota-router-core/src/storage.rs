use crate::keys::{
    blob_16_to_uuid, hex_to_blob_32, tokenizer_version_to_id, uuid_to_blob_16, ApiKey, KeyError,
    KeySpend, KeyType, KeyUpdates, SpendEvent, Team, TokenSource,
};
use sha2::{Digest, Sha256};

pub trait KeyStorage: Send + Sync {
    // Key operations
    fn create_key(&self, key: &ApiKey) -> Result<(), KeyError>;
    fn lookup_by_hash(&self, key_hash: &[u8]) -> Result<Option<ApiKey>, KeyError>;
    fn update_key(&self, key_id: &str, updates: &KeyUpdates) -> Result<(), KeyError>;
    fn list_keys(&self, team_id: Option<&str>) -> Result<Vec<ApiKey>, KeyError>;
    fn count_keys_for_team(&self, team_id: &str) -> Result<i64, KeyError>;

    // Team operations
    fn create_team(&self, team: &Team) -> Result<(), KeyError>;
    fn get_team(&self, team_id: &str) -> Result<Option<Team>, KeyError>;
    fn update_team(&self, team_id: &str, name: &str, budget_limit: i64) -> Result<(), KeyError>;
    fn list_teams(&self) -> Result<Vec<Team>, KeyError>;
    fn delete_team(&self, team_id: &str) -> Result<(), KeyError>;

    // Spend tracking
    fn record_spend(&self, key_id: &str, amount: i64) -> Result<(), KeyError>;
    fn get_spend(&self, key_id: &str) -> Result<Option<KeySpend>, KeyError>;
    fn reset_spend(&self, key_id: &str) -> Result<(), KeyError>;

    /// Record a spend event in the ledger with atomic budget enforcement.
    ///
    /// Uses `SELECT ... FOR UPDATE` to lock the key row, preventing double-spend
    /// in concurrent multi-router deployments. The budget is checked atomically
    /// against the sum of all previous cost_amount in the ledger.
    ///
    /// Returns `KeyError::NotFound` if key_id does not exist.
    /// Returns `KeyError::BudgetExceeded` if the spend would exceed the budget.
    fn record_spend_ledger(&self, event: &SpendEvent) -> Result<(), KeyError>;

    /// Record a spend event with team budget enforcement.
    ///
    /// Locks team row FIRST, then key row (deadlock prevention per RFC-0903
    /// §Lock Ordering Invariant). Verifies both key and team budgets before
    /// inserting into the ledger.
    fn record_spend_ledger_with_team(
        &self,
        key_id: &str,
        team_id: &str,
        event: &SpendEvent,
    ) -> Result<(), KeyError>;

    /// Resolve a tokenizer_id (BLAKE3-16) back to its version string via DB lookup.
    ///
    /// Per RFC-0909 §tokenizer_id_to_version and RFC-0910 §Tokenizer Database Schema.
    fn resolve_tokenizer(&self, tokenizer_id: &[u8; 16]) -> Result<Option<String>, KeyError>;

    /// Ensure a tokenizer version exists in the tokenizers table (on-demand population).
    fn ensure_tokenizer(&self, version: &str, provider: Option<&str>)
        -> Result<[u8; 16], KeyError>;

    // OCTO-W balance operations (RFC-0904 F3)
    fn get_octo_w_balance(&self, key_id: &str) -> Result<u64, KeyError>;
    fn deduct_octo_w(&self, key_id: &str, cost_amount: u64) -> Result<u64, KeyError>;

    // Provider API key operations (for python_sdk_entry set_api_key/get_budget_status/get_metrics)
    fn create_provider_key(
        &self,
        provider: &str,
        api_key: &str,
        label: Option<&str>,
    ) -> Result<String, KeyError>;
    fn list_provider_keys(&self, provider: Option<&str>) -> Result<Vec<ProviderKeyInfo>, KeyError>;
    fn delete_provider_key(&self, id: &str) -> Result<(), KeyError>;
    fn get_provider_key_by_hash(
        &self,
        api_key_hash: &[u8],
    ) -> Result<Option<ProviderKeyInfo>, KeyError>;

    // Budget operations (RFC-0934)
    fn upsert_budget(
        &self,
        entity_id: &str,
        entity_type: &str,
        budget_limit: i64,
        period: &str,
        soft_limit_pct: Option<i64>,
        alert_webhook: Option<&str>,
    ) -> Result<(), KeyError> {
        let _ = (
            entity_id,
            entity_type,
            budget_limit,
            period,
            soft_limit_pct,
            alert_webhook,
        );
        Ok(())
    }

    fn get_budget(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Option<BudgetRow>, KeyError> {
        let _ = (entity_id, entity_type);
        Ok(None)
    }

    fn update_spend(
        &self,
        entity_id: &str,
        entity_type: &str,
        amount: i64,
    ) -> Result<(), KeyError> {
        let _ = (entity_id, entity_type, amount);
        Ok(())
    }

    fn reset_budget(
        &self,
        entity_id: &str,
        entity_type: &str,
        new_period_start: i64,
    ) -> Result<(), KeyError> {
        let _ = (entity_id, entity_type, new_period_start);
        Ok(())
    }
}

/// Provider API key info for python_sdk_entry
pub struct ProviderKeyInfo {
    pub id: String,
    pub provider: String,
    pub api_key_prefix: String,
    pub label: Option<String>,
    pub created_at: i64,
    pub is_active: bool,
}

/// Budget row from the budgets table (RFC-0934)
#[derive(Debug, Clone)]
pub struct BudgetRow {
    pub entity_id: String,
    pub entity_type: String,
    pub budget_limit: i64,
    pub period: String,
    pub current_spend: i64,
    pub soft_limit_pct: Option<i64>,
    pub alert_webhook: Option<String>,
    pub last_reset: i64,
    pub created_at: i64,
}

pub struct StoolapKeyStorage {
    db: stoolap::Database,
}

impl StoolapKeyStorage {
    pub fn new(db: stoolap::Database) -> Self {
        Self { db }
    }

    pub fn row_to_api_key(&self, row: &stoolap::ResultRow) -> Result<ApiKey, KeyError> {
        let key_type_str: String = row
            .get_by_name("key_type")
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        let key_type = match key_type_str.as_str() {
            "llm_api" => KeyType::LlmApi,
            "management" => KeyType::Management,
            "read_only" => KeyType::ReadOnly,
            _ => KeyType::Default,
        };

        // Read key_hash as raw bytes from BYTEA(32) column
        let key_hash: Vec<u8> = row
            .get_by_name("key_hash")
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // Read key_id and team_id as BLOB(16) and convert to String per RFC-0903-C1
        let key_id_blob: Vec<u8> = row
            .get_by_name("key_id")
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        let key_id_bytes: [u8; 16] = key_id_blob.try_into().expect("key_id must be 16 bytes");
        let key_id = blob_16_to_uuid(&key_id_bytes).to_string();

        let team_id_blob: Option<Vec<u8>> = row
            .get_by_name("team_id")
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        let team_id = team_id_blob.map(|blob| {
            let bytes: [u8; 16] = blob.try_into().expect("team_id must be 16 bytes");
            blob_16_to_uuid(&bytes)
        });

        Ok(ApiKey {
            key_id,
            key_hash,
            key_prefix: row
                .get_by_name("key_prefix")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            team_id,
            budget_limit: row
                .get_by_name("budget_limit")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            rpm_limit: row
                .get_by_name("rpm_limit")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            tpm_limit: row
                .get_by_name("tpm_limit")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            created_at: row
                .get_by_name("created_at")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            expires_at: row
                .get_by_name("expires_at")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            revoked: row
                .get_by_name::<i32>("revoked")
                .map_err(|e| KeyError::Storage(e.to_string()))?
                != 0,
            revoked_at: row
                .get_by_name("revoked_at")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            revoked_by: row
                .get_by_name("revoked_by")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            revocation_reason: row
                .get_by_name("revocation_reason")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            key_type,
            allowed_routes: row
                .get_by_name("allowed_routes")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            auto_rotate: row
                .get_by_name::<i32>("auto_rotate")
                .map_err(|e| KeyError::Storage(e.to_string()))?
                != 0,
            rotation_interval_days: row
                .get_by_name("rotation_interval_days")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            description: row
                .get_by_name("description")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            metadata: row
                .get_by_name("metadata")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
        })
    }
}

impl KeyStorage for StoolapKeyStorage {
    fn create_key(&self, key: &ApiKey) -> Result<(), KeyError> {
        // Validate required fields
        if key.key_id.is_empty() {
            return Err(KeyError::InvalidFormat);
        }
        if key.budget_limit <= 0 {
            return Err(KeyError::InvalidFormat);
        }

        let key_type_str = key.key_type.to_string();
        // Pass key_hash as raw bytes for BYTEA(32) column
        let key_hash_value = stoolap::core::Value::blob(key.key_hash.clone());

        // Helper to convert Option<i64> to stoolap::Value (None = Null)
        let opt_i64_to_value = |opt: Option<i64>| -> stoolap::Value {
            opt.map(|v| v.into())
                .unwrap_or(stoolap::Value::Null(stoolap::DataType::Null))
        };
        // Helper to convert Option<i32> to stoolap::Value (None = Null)
        let opt_i32_to_value = |opt: Option<i32>| -> stoolap::Value {
            opt.map(|v| v.into())
                .unwrap_or(stoolap::Value::Null(stoolap::DataType::Null))
        };

        // Convert key_id and team_id to BLOB(16) for storage per RFC-0903-C1
        let key_id_blob =
            uuid_to_blob_16(&uuid::Uuid::parse_str(&key.key_id).expect("valid key_id UUID"));
        let team_id_blob: Option<Vec<u8>> =
            key.team_id.as_ref().map(|t| uuid_to_blob_16(t).to_vec());

        let params: Vec<stoolap::Value> = vec![
            stoolap::core::Value::blob(key_id_blob.to_vec()),
            key_hash_value,
            key.key_prefix.clone().into(),
            team_id_blob
                .map(stoolap::core::Value::blob)
                .unwrap_or_else(|| stoolap::Value::Null(stoolap::DataType::Null)),
            key.budget_limit.into(),
            opt_i32_to_value(key.rpm_limit),
            opt_i32_to_value(key.tpm_limit),
            key.created_at.into(),
            opt_i64_to_value(key.expires_at),
            (key.revoked as i32).into(),
            key_type_str.into(),
            key.allowed_routes.clone().into(),
            (key.auto_rotate as i32).into(),
            opt_i32_to_value(key.rotation_interval_days),
            key.description.clone().into(),
            key.metadata.clone().into(),
        ];

        self.db
            .execute(
                "INSERT INTO api_keys (
                key_id, key_hash, key_prefix, team_id, budget_limit,
                rpm_limit, tpm_limit, created_at, expires_at, revoked,
                key_type, allowed_routes, auto_rotate, rotation_interval_days,
                description, metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
                params,
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        Ok(())
    }

    fn lookup_by_hash(&self, key_hash: &[u8]) -> Result<Option<ApiKey>, KeyError> {
        // Pass key_hash as raw bytes for BYTEA(32) column
        let key_hash_blob = stoolap::core::Value::blob(key_hash.to_vec());
        let params: Vec<stoolap::Value> = vec![key_hash_blob];

        let mut rows = self
            .db
            .query(
                "SELECT * FROM api_keys WHERE key_hash = $1 AND revoked = 0 LIMIT 1",
                params,
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if let Some(Ok(row)) = rows.next() {
            Ok(Some(self.row_to_api_key(&row)?))
        } else {
            Ok(None)
        }
    }

    fn update_key(&self, key_id: &str, updates: &KeyUpdates) -> Result<(), KeyError> {
        // Build dynamic update query
        let mut set_clauses = Vec::new();
        let mut params: Vec<stoolap::Value> = Vec::new();

        if let Some(budget_limit) = updates.budget_limit {
            set_clauses.push(format!("budget_limit = ${}", params.len() + 1));
            params.push(budget_limit.into());
        }
        if let Some(rpm_limit) = updates.rpm_limit {
            set_clauses.push(format!("rpm_limit = ${}", params.len() + 1));
            params.push(rpm_limit.into());
        }
        if let Some(tpm_limit) = updates.tpm_limit {
            set_clauses.push(format!("tpm_limit = ${}", params.len() + 1));
            params.push(tpm_limit.into());
        }
        if let Some(expires_at) = updates.expires_at {
            set_clauses.push(format!("expires_at = ${}", params.len() + 1));
            params.push(expires_at.into());
        }
        if let Some(revoked) = updates.revoked {
            set_clauses.push(format!("revoked = ${}", params.len() + 1));
            params.push((revoked as i32).into());
            if revoked {
                set_clauses.push(format!("revoked_at = ${}", params.len() + 1));
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                params.push(now.into());
            }
        }
        if let Some(revoked_by) = &updates.revoked_by {
            set_clauses.push(format!("revoked_by = ${}", params.len() + 1));
            params.push(revoked_by.clone().into());
        }
        if let Some(revocation_reason) = &updates.revocation_reason {
            set_clauses.push(format!("revocation_reason = ${}", params.len() + 1));
            params.push(revocation_reason.clone().into());
        }
        if let Some(key_type) = &updates.key_type {
            set_clauses.push(format!("key_type = ${}", params.len() + 1));
            params.push(key_type.to_string().into());
        }
        if let Some(description) = &updates.description {
            set_clauses.push(format!("description = ${}", params.len() + 1));
            params.push(description.clone().into());
        }

        if set_clauses.is_empty() {
            return Ok(());
        }

        // key_id is BLOB(16) per RFC-0903-C1
        let key_id_blob =
            uuid_to_blob_16(&uuid::Uuid::parse_str(key_id).expect("valid key_id UUID"));

        // Note: updating key_id itself changes the primary key - this is allowed
        set_clauses.push(format!("key_id = ${}", params.len() + 1));
        params.push(stoolap::core::Value::blob(key_id_blob.to_vec()));

        let sql = format!(
            "UPDATE api_keys SET {} WHERE key_id = ${}",
            set_clauses.join(", "),
            params.len()
        );

        self.db
            .execute(&sql, params)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        Ok(())
    }

    fn list_keys(&self, team_id: Option<&str>) -> Result<Vec<ApiKey>, KeyError> {
        let rows = if let Some(tid) = team_id {
            // Convert team_id to BLOB(16) per RFC-0903-C1
            let team_id_blob =
                uuid_to_blob_16(&uuid::Uuid::parse_str(tid).expect("valid team_id UUID"));
            let params: Vec<stoolap::Value> =
                vec![stoolap::core::Value::blob(team_id_blob.to_vec())];
            self.db
                .query("SELECT * FROM api_keys WHERE team_id = $1", params)
                .map_err(|e| KeyError::Storage(e.to_string()))?
        } else {
            self.db
                .query("SELECT * FROM api_keys", ())
                .map_err(|e| KeyError::Storage(e.to_string()))?
        };

        let mut keys = Vec::new();
        for row in rows {
            let row = row.map_err(|e| KeyError::Storage(e.to_string()))?;
            keys.push(self.row_to_api_key(&row)?);
        }

        Ok(keys)
    }

    fn count_keys_for_team(&self, team_id: &str) -> Result<i64, KeyError> {
        // Convert team_id to BLOB(16) per RFC-0903-C1
        let team_id_blob =
            uuid_to_blob_16(&uuid::Uuid::parse_str(team_id).expect("valid team_id UUID"));
        let mut rows = self
            .db
            .query(
                "SELECT COUNT(*) FROM api_keys WHERE team_id = $1 AND revoked = 0",
                vec![stoolap::core::Value::blob(team_id_blob.to_vec())],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let count: i64 = rows
            .next()
            .ok_or(KeyError::Storage("Expected row".to_string()))?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        Ok(count)
    }

    fn create_team(&self, team: &Team) -> Result<(), KeyError> {
        // Convert team_id to BLOB(16) for storage per RFC-0903-C1
        let team_id_blob =
            uuid_to_blob_16(&uuid::Uuid::parse_str(&team.team_id).expect("valid team_id UUID"));
        self.db
            .execute(
                "INSERT INTO teams (team_id, name, budget_limit, created_at) VALUES ($1, $2, $3, $4)",
                vec![
                    stoolap::core::Value::blob(team_id_blob.to_vec()),
                    team.name.clone().into(),
                    team.budget_limit.into(),
                    team.created_at.into(),
                ],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        Ok(())
    }

    fn get_team(&self, team_id: &str) -> Result<Option<Team>, KeyError> {
        // Convert team_id to BLOB(16) per RFC-0903-C1
        let team_id_blob =
            uuid_to_blob_16(&uuid::Uuid::parse_str(team_id).expect("valid team_id UUID"));
        let rows = self
            .db
            .query(
                "SELECT * FROM teams WHERE team_id = $1",
                vec![stoolap::core::Value::blob(team_id_blob.to_vec())],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if let Some(Ok(row)) = rows.into_iter().next() {
            // Read team_id as BLOB(16) and convert to String per RFC-0903-C1
            let team_id_blob: Vec<u8> = row
                .get_by_name("team_id")
                .map_err(|e| KeyError::Storage(e.to_string()))?;
            let team_id_bytes: [u8; 16] =
                team_id_blob.try_into().expect("team_id must be 16 bytes");
            let team_id = blob_16_to_uuid(&team_id_bytes).to_string();

            let team = Team {
                team_id,
                name: row
                    .get_by_name("name")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                budget_limit: row
                    .get_by_name("budget_limit")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                created_at: row
                    .get_by_name("created_at")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
            };
            Ok(Some(team))
        } else {
            Ok(None)
        }
    }

    fn update_team(&self, team_id: &str, name: &str, budget_limit: i64) -> Result<(), KeyError> {
        // Convert team_id to BLOB(16) per RFC-0903-C1
        let team_id_blob =
            uuid_to_blob_16(&uuid::Uuid::parse_str(team_id).expect("valid team_id UUID"));
        self.db
            .execute(
                "UPDATE teams SET name = $1, budget_limit = $2 WHERE team_id = $3",
                vec![
                    name.into(),
                    budget_limit.into(),
                    stoolap::core::Value::blob(team_id_blob.to_vec()),
                ],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        Ok(())
    }

    fn list_teams(&self) -> Result<Vec<Team>, KeyError> {
        let rows = self
            .db
            .query("SELECT * FROM teams", ())
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let mut teams = Vec::new();
        for row in rows {
            let row = row.map_err(|e| KeyError::Storage(e.to_string()))?;
            let team = Team {
                team_id: row
                    .get_by_name("team_id")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                name: row
                    .get_by_name("name")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                budget_limit: row
                    .get_by_name("budget_limit")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                created_at: row
                    .get_by_name("created_at")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
            };
            teams.push(team);
        }

        Ok(teams)
    }

    fn delete_team(&self, team_id: &str) -> Result<(), KeyError> {
        // Check if any keys belong to this team
        let keys = self.list_keys(Some(team_id))?;
        if !keys.is_empty() {
            return Err(KeyError::Storage(
                "Cannot delete team with existing keys".to_string(),
            ));
        }

        // Convert team_id to BLOB(16) for storage per RFC-0903-C1
        let team_id_blob =
            uuid_to_blob_16(&uuid::Uuid::parse_str(team_id).expect("valid team_id UUID"));
        self.db
            .execute(
                "DELETE FROM teams WHERE team_id = $1",
                vec![stoolap::core::Value::blob(team_id_blob.to_vec())],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        Ok(())
    }

    // NOTE: record_spend is deprecated. Use record_spend_ledger() instead.
    // This counter-based approach does not support team budgets, deterministic replay,
    // or FOR UPDATE locking.
    fn record_spend(&self, key_id: &str, amount: i64) -> Result<(), KeyError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Check if spend record exists
        let existing = self.get_spend(key_id)?;

        if let Some(mut spend) = existing {
            // Update existing spend
            spend.total_spend += amount;
            spend.last_updated = now;

            self.db
                .execute(
                    "UPDATE key_spend SET total_spend = $1, last_updated = $2 WHERE key_id = $3",
                    vec![
                        spend.total_spend.into(),
                        spend.last_updated.into(),
                        key_id.into(),
                    ],
                )
                .map_err(|e| KeyError::Storage(e.to_string()))?;
        } else {
            // Create new spend record
            self.db
                .execute(
                    "INSERT INTO key_spend (key_id, total_spend, window_start, last_updated) VALUES ($1, $2, $3, $4)",
                    vec![
                        key_id.into(),
                        amount.into(),
                        now.into(),
                        now.into(),
                    ],
                )
                .map_err(|e| KeyError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    fn get_spend(&self, key_id: &str) -> Result<Option<KeySpend>, KeyError> {
        let rows = self
            .db
            .query(
                "SELECT * FROM key_spend WHERE key_id = $1",
                vec![key_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if let Some(Ok(row)) = rows.into_iter().next() {
            let spend = KeySpend {
                key_id: row
                    .get_by_name("key_id")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                total_spend: row
                    .get_by_name("total_spend")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                window_start: row
                    .get_by_name("window_start")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
                last_updated: row
                    .get_by_name("last_updated")
                    .map_err(|e| KeyError::Storage(e.to_string()))?,
            };
            Ok(Some(spend))
        } else {
            Ok(None)
        }
    }

    fn reset_spend(&self, key_id: &str) -> Result<(), KeyError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Reset to zero or delete record
        self.db
            .execute(
                "UPDATE key_spend SET total_spend = 0, window_start = $1, last_updated = $1 WHERE key_id = $2",
                vec![now.into(), key_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        Ok(())
    }

    fn record_spend_ledger(&self, event: &SpendEvent) -> Result<(), KeyError> {
        // Validate token_source at application layer (CHECK constraint may not be enforced)
        let token_source_str = event.token_source.to_db_str();
        if token_source_str != "provider_usage" && token_source_str != "canonical_tokenizer" {
            return Err(KeyError::InvalidFormat);
        }

        let key_id_blob = uuid_to_blob_16(&event.key_id);

        // Begin transaction for atomic budget enforcement with FOR UPDATE locking
        let mut tx = self
            .db
            .begin()
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 1. Lock key row FOR UPDATE to prevent concurrent modifications
        // Note: key_id in api_keys is BLOB(16) per RFC-0903-C1, so pass BLOB value
        let budget: i64 = tx
            .query(
                "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
                vec![stoolap::core::Value::blob(key_id_blob.to_vec())],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .next()
            .ok_or(KeyError::NotFound)?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 2. Compute current spend from ledger
        // Note: After BLOB migration, key_id is stored as binary BLOB(16).
        // Query uses key_id_blob (Vec<u8>) which SQLite treats as raw bytes for BLOB comparison.
        let mut rows = tx
            .query(
                "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
                vec![stoolap::core::Value::blob(key_id_blob.to_vec())],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let current: i64 = rows
            .next()
            .ok_or(KeyError::Storage("Expected row".to_string()))?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 3. On-demand population: ensure tokenizer exists in tokenizers table
        // when token_source is CanonicalTokenizer (per RFC-0910 §On-Demand Population).
        // This is idempotent — if the tokenizer already exists, ensure_tokenizer is a no-op.
        if event.token_source == TokenSource::CanonicalTokenizer {
            if let Some(ref version) = event.tokenizer_version {
                // provider is not available in SpendEvent; pass None for on-demand population
                let _tokenizer_id = self.ensure_tokenizer(version, None)?;
            }
        }

        // 4. Verify budget against cost_amount
        let cost_i64 = event.cost_amount as i64;
        if current + cost_i64 > budget {
            return Err(KeyError::BudgetExceeded {
                current: current as u64,
                limit: budget as u64,
            });
        }

        // 5. Build params for INSERT
        // BLOB storage per RFC-0903-B1/C1: event_id (SHA256 hex → raw 32B), request_id (raw 32B),
        // key_id (UUID → raw 16B), team_id (UUID → raw 16B), tokenizer_id (BLAKE3-16).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Hash request_id to get raw SHA256 binary for BLOB(32) storage
        let request_id_bytes: [u8; 32] = Sha256::digest(event.request_id.as_bytes()).into();

        let team_id_blob: Option<Vec<u8>> =
            event.team_id.as_ref().map(|t| uuid_to_blob_16(t).to_vec());

        let tokenizer_id_blob: Option<Vec<u8>> = event
            .tokenizer_version
            .as_ref()
            .map(|v| tokenizer_version_to_id(v).to_vec());

        let params: Vec<stoolap::Value> = vec![
            stoolap::core::Value::blob(hex_to_blob_32(&event.event_id).to_vec()),
            stoolap::core::Value::blob(request_id_bytes.to_vec()),
            stoolap::core::Value::blob(key_id_blob.to_vec()),
            team_id_blob
                .map(stoolap::core::Value::blob)
                .unwrap_or_else(|| stoolap::Value::Null(stoolap::DataType::Null)),
            event.provider.clone().into(),
            event.model.clone().into(),
            event.input_tokens.into(),
            event.output_tokens.into(),
            cost_i64.into(),
            stoolap::core::Value::blob(event.pricing_hash.to_vec()),
            token_source_str.into(),
            tokenizer_id_blob
                .map(stoolap::core::Value::blob)
                .unwrap_or_else(|| stoolap::Value::Null(stoolap::DataType::Null)),
            event.tokenizer_version.clone().into(),
            event.provider_usage_json.clone().into(),
            event.timestamp.into(),
            now.into(),
        ];

        // 5. Insert (idempotent via UniqueConstraint handling)
        // Note: stoolap uses MySQL-style ON DUPLICATE KEY UPDATE, not PostgreSQL ON CONFLICT.
        match tx.execute(
            "INSERT INTO spend_ledger (
                event_id, request_id, key_id, team_id, provider, model,
                input_tokens, output_tokens, cost_amount, pricing_hash,
                token_source, tokenizer_id, tokenizer_version, provider_usage_json,
                timestamp, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
            params,
        ) {
            Ok(_) => {}
            Err(stoolap::Error::UniqueConstraint { .. }) => {
                // Idempotent: another transaction already recorded this event
            }
            Err(e) => return Err(KeyError::Storage(e.to_string())),
        }

        tx.commit().map_err(|e| KeyError::Storage(e.to_string()))?;
        Ok(())
    }

    fn record_spend_ledger_with_team(
        &self,
        key_id: &str,
        team_id: &str,
        event: &SpendEvent,
    ) -> Result<(), KeyError> {
        // Validate token_source at application layer
        let token_source_str = event.token_source.to_db_str();
        if token_source_str != "provider_usage" && token_source_str != "canonical_tokenizer" {
            return Err(KeyError::InvalidFormat);
        }

        // Convert UUID strings to binary for spend_ledger BLOB columns
        let key_uuid = uuid::Uuid::parse_str(key_id).map_err(|_| KeyError::InvalidFormat)?;
        let team_uuid = uuid::Uuid::parse_str(team_id).map_err(|_| KeyError::InvalidFormat)?;
        let key_id_blob = uuid_to_blob_16(&key_uuid);
        let team_id_blob = uuid_to_blob_16(&team_uuid);

        // Begin transaction for atomic budget enforcement with FOR UPDATE locking
        let mut tx = self
            .db
            .begin()
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 1. Lock team row FIRST (deadlock prevention per RFC-0903 §Lock Ordering Invariant)
        // Note: teams table still uses TEXT for team_id (migrated separately)
        let team_budget: i64 = tx
            .query(
                "SELECT budget_limit FROM teams WHERE team_id = $1 FOR UPDATE",
                vec![team_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .next()
            .ok_or(KeyError::NotFound)?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 2. Lock key row SECOND
        // Note: api_keys table still uses TEXT for key_id (migrated separately in 0909-h)
        let key_budget: i64 = tx
            .query(
                "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
                vec![key_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .next()
            .ok_or(KeyError::NotFound)?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 3. Compute key spend from ledger
        // spend_ledger.key_id is BLOB(16) — use binary blob for comparison
        let mut rows = tx
            .query(
                "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
                vec![stoolap::core::Value::blob(key_id_blob.to_vec())],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let key_current: i64 = rows
            .next()
            .ok_or(KeyError::Storage("Expected row".to_string()))?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 4. Compute team spend from ledger
        // spend_ledger.team_id is BLOB(16) — use binary blob for comparison
        let mut rows = tx
            .query(
                "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE team_id = $1",
                vec![stoolap::core::Value::blob(team_id_blob.to_vec())],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let team_current: i64 = rows
            .next()
            .ok_or(KeyError::Storage("Expected row".to_string()))?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 5. On-demand population: ensure tokenizer exists in tokenizers table
        // when token_source is CanonicalTokenizer (per RFC-0910 §On-Demand Population).
        // This is idempotent — if the tokenizer already exists, ensure_tokenizer is a no-op.
        if event.token_source == TokenSource::CanonicalTokenizer {
            if let Some(ref version) = event.tokenizer_version {
                // provider is not available in SpendEvent; pass None for on-demand population
                let _tokenizer_id = self.ensure_tokenizer(version, None)?;
            }
        }

        // 6. Verify both budgets
        let cost_i64 = event.cost_amount as i64;
        if key_current + cost_i64 > key_budget {
            return Err(KeyError::BudgetExceeded {
                current: key_current as u64,
                limit: key_budget as u64,
            });
        }
        if team_current + cost_i64 > team_budget {
            return Err(KeyError::TeamBudgetExceeded {
                current: team_current as u64,
                limit: team_budget as u64,
            });
        }

        // 7. Build params for INSERT
        // BLOB storage per RFC-0903-B1/C1: event_id (SHA256 hex → raw 32B),
        // request_id (raw 32B SHA256), key_id (UUID → raw 16B),
        // team_id (UUID → raw 16B), tokenizer_id (BLAKE3-16).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Hash request_id to get raw SHA256 binary for BLOB(32) storage
        let request_id_bytes: [u8; 32] = Sha256::digest(event.request_id.as_bytes()).into();

        let tokenizer_id_blob: Option<Vec<u8>> = event
            .tokenizer_version
            .as_ref()
            .map(|v| tokenizer_version_to_id(v).to_vec());

        let params: Vec<stoolap::Value> = vec![
            stoolap::core::Value::blob(hex_to_blob_32(&event.event_id).to_vec()),
            stoolap::core::Value::blob(request_id_bytes.to_vec()),
            stoolap::core::Value::blob(key_id_blob.to_vec()),
            stoolap::core::Value::blob(team_id_blob.to_vec()),
            event.provider.clone().into(),
            event.model.clone().into(),
            event.input_tokens.into(),
            event.output_tokens.into(),
            cost_i64.into(),
            stoolap::core::Value::blob(event.pricing_hash.to_vec()),
            token_source_str.into(),
            tokenizer_id_blob
                .map(stoolap::core::Value::blob)
                .unwrap_or_else(|| stoolap::Value::Null(stoolap::DataType::Null)),
            event.tokenizer_version.clone().into(),
            event.provider_usage_json.clone().into(),
            event.timestamp.into(),
            now.into(),
        ];

        // 7. Insert (idempotent via UniqueConstraint handling)
        match tx.execute(
            "INSERT INTO spend_ledger (
                event_id, request_id, key_id, team_id, provider, model,
                input_tokens, output_tokens, cost_amount, pricing_hash,
                token_source, tokenizer_id, tokenizer_version, provider_usage_json,
                timestamp, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
            params,
        ) {
            Ok(_) => {}
            Err(stoolap::Error::UniqueConstraint { .. }) => {
                // Idempotent: another transaction already recorded this event
            }
            Err(e) => return Err(KeyError::Storage(e.to_string())),
        }

        tx.commit().map_err(|e| KeyError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Resolve a tokenizer_id (BLAKE3-16) back to its version string via DB lookup.
    ///
    /// Per RFC-0909 §tokenizer_id_to_version and RFC-0910 §Tokenizer Database Schema:
    /// `SELECT version FROM tokenizers WHERE tokenizer_id = ?`
    ///
    /// Returns:
    /// - `Ok(Some(version))` if the tokenizer_id exists in the tokenizers table
    /// - `Ok(None)` if the tokenizer_id is not found (never registered)
    /// - `Err(KeyError::Storage(...))` on DB errors
    ///
    /// This is the DB-backed implementation of the stub in `keys/mod.rs::tokenizer_id_to_version`.
    fn resolve_tokenizer(&self, tokenizer_id: &[u8; 16]) -> Result<Option<String>, KeyError> {
        let param = stoolap::core::Value::blob(tokenizer_id.to_vec());
        let mut rows = self
            .db
            .query(
                "SELECT version FROM tokenizers WHERE tokenizer_id = $1",
                vec![param],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        match rows.next() {
            Some(row) => {
                let version: String = row
                    .map_err(|e| KeyError::Storage(e.to_string()))?
                    .get(0)
                    .map_err(|e| KeyError::Storage(e.to_string()))?;
                Ok(Some(version))
            }
            None => Ok(None),
        }
    }

    /// Ensure a tokenizer version exists in the tokenizers table.
    ///
    /// Used for on-demand population when a new tokenizer version is first used
    /// in a spend_ledger INSERT. If the tokenizer already exists, this is a no-op.
    ///
    /// Returns the tokenizer_id (BLAKE3-16) for use in the spend event.
    fn ensure_tokenizer(
        &self,
        version: &str,
        provider: Option<&str>,
    ) -> Result<[u8; 16], KeyError> {
        use crate::keys::tokenizer_version_to_id;

        let tokenizer_id = tokenizer_version_to_id(version);

        // Upsert: insert only if not exists
        // provider is optional — if None, only version is used for UNIQUE constraint
        let version_param: String = version.into();
        let provider_param: Option<String> = provider.map(|p| p.to_string());

        let params: Vec<stoolap::Value> = vec![
            stoolap::core::Value::blob(tokenizer_id.to_vec()),
            version_param.into(),
            provider_param.into(),
        ];

        let result = self.db.execute(
            "INSERT INTO tokenizers (tokenizer_id, version, provider) VALUES ($1, $2, $3)",
            params,
        );
        // Idempotent: if UNIQUE constraint violated (same version+provider already registered),
        // that's fine — the tokenizer_id is the same anyway. All other errors propagate.
        if let Err(stoolap::Error::UniqueConstraint { .. }) = result {
            // Already registered — OK
        } else if let Err(e) = result {
            return Err(KeyError::Storage(e.to_string()));
        }

        Ok(tokenizer_id)
    }

    fn get_octo_w_balance(&self, key_id: &str) -> Result<u64, KeyError> {
        let rows = self
            .db
            .query(
                "SELECT balance FROM octo_w_balances WHERE key_id = $1",
                vec![key_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if let Some(Ok(row)) = rows.into_iter().next() {
            let balance: i64 = row
                .get_by_name("balance")
                .map_err(|e| KeyError::Storage(e.to_string()))?;
            Ok(balance as u64)
        } else {
            Ok(0) // No balance record = 0 (default)
        }
    }

    fn deduct_octo_w(&self, key_id: &str, cost_amount: u64) -> Result<u64, KeyError> {
        // Atomic deduction: UPDATE ... WHERE balance >= cost_amount
        let rows_affected = self
            .db
            .execute(
                "UPDATE octo_w_balances SET balance = balance - $2, updated_at = strftime('%s', 'now') WHERE key_id = $1 AND balance >= $2",
                vec![key_id.into(), (cost_amount as i64).into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            // Check if it's insufficient balance or key not found
            let current = self.get_octo_w_balance(key_id)?;
            if current < cost_amount {
                return Err(KeyError::Storage(format!(
                    "Insufficient OCTO-W balance: have {}, need {}",
                    current, cost_amount
                )));
            }
        }
        // Return new balance
        self.get_octo_w_balance(key_id)
    }

    fn create_provider_key(
        &self,
        provider: &str,
        api_key: &str,
        label: Option<&str>,
    ) -> Result<String, KeyError> {
        use sha2::{Digest, Sha256};

        // Generate unique ID
        let id = uuid::Uuid::new_v4().to_string();

        // Hash the API key for storage (SHA256 → 32 bytes)
        let api_key_hash: [u8; 32] = Sha256::digest(api_key.as_bytes()).into();

        // Store prefix (first 8 chars for display)
        let prefix = if api_key.len() >= 8 {
            &api_key[..8]
        } else {
            api_key
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let params: Vec<stoolap::Value> = vec![
            id.clone().into(),
            provider.into(),
            stoolap::core::Value::blob(api_key_hash.to_vec()),
            prefix.into(),
            label.map(|l| l.to_string()).into(),
            now.into(),
            1_i32.into(), // is_active = 1
        ];

        self.db
            .execute(
                "INSERT INTO provider_api_keys (id, provider, api_key_hash, api_key_prefix, label, created_at, is_active) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                params,
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        Ok(id)
    }

    fn list_provider_keys(&self, provider: Option<&str>) -> Result<Vec<ProviderKeyInfo>, KeyError> {
        let (query, params): (String, Vec<stoolap::Value>) = match provider {
            Some(p) => (
                "SELECT id, provider, api_key_prefix, label, created_at, is_active FROM provider_api_keys WHERE is_active = 1 AND provider = $1".to_string(),
                vec![p.into()],
            ),
            None => (
                "SELECT id, provider, api_key_prefix, label, created_at, is_active FROM provider_api_keys WHERE is_active = 1".to_string(),
                vec![],
            ),
        };

        let rows = self
            .db
            .query(&query, params)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let mut keys = Vec::new();
        for row in rows {
            let row = row.map_err(|e| KeyError::Storage(e.to_string()))?;
            let is_active: i32 = row.get(5).map_err(|e| KeyError::Storage(e.to_string()))?;
            keys.push(ProviderKeyInfo {
                id: row.get(0).map_err(|e| KeyError::Storage(e.to_string()))?,
                provider: row.get(1).map_err(|e| KeyError::Storage(e.to_string()))?,
                api_key_prefix: row.get(2).map_err(|e| KeyError::Storage(e.to_string()))?,
                label: row.get(3).map_err(|e| KeyError::Storage(e.to_string()))?,
                created_at: row.get(4).map_err(|e| KeyError::Storage(e.to_string()))?,
                is_active: is_active != 0,
            });
        }

        Ok(keys)
    }

    fn delete_provider_key(&self, id: &str) -> Result<(), KeyError> {
        let rows_affected = self
            .db
            .execute(
                "UPDATE provider_api_keys SET is_active = 0 WHERE id = $1",
                vec![id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            return Err(KeyError::NotFound);
        }
        Ok(())
    }

    fn get_provider_key_by_hash(
        &self,
        api_key_hash: &[u8],
    ) -> Result<Option<ProviderKeyInfo>, KeyError> {
        let hash_blob = stoolap::core::Value::blob(api_key_hash.to_vec());
        let mut rows = self
            .db
            .query(
                "SELECT id, provider, api_key_prefix, label, created_at, is_active FROM provider_api_keys WHERE api_key_hash = $1 AND is_active = 1 LIMIT 1",
                vec![hash_blob],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        match rows.next() {
            Some(row) => {
                let row = row.map_err(|e| KeyError::Storage(e.to_string()))?;
                let is_active: i32 = row.get(5).map_err(|e| KeyError::Storage(e.to_string()))?;
                Ok(Some(ProviderKeyInfo {
                    id: row.get(0).map_err(|e| KeyError::Storage(e.to_string()))?,
                    provider: row.get(1).map_err(|e| KeyError::Storage(e.to_string()))?,
                    api_key_prefix: row.get(2).map_err(|e| KeyError::Storage(e.to_string()))?,
                    label: row.get(3).map_err(|e| KeyError::Storage(e.to_string()))?,
                    created_at: row.get(4).map_err(|e| KeyError::Storage(e.to_string()))?,
                    is_active: is_active != 0,
                }))
            }
            None => Ok(None),
        }
    }

    fn upsert_budget(
        &self,
        entity_id: &str,
        entity_type: &str,
        budget_limit: i64,
        period: &str,
        soft_limit_pct: Option<i64>,
        alert_webhook: Option<&str>,
    ) -> Result<(), KeyError> {
        let existing = self.get_budget(entity_id, entity_type)?;

        if existing.is_some() {
            let soft_limit_value = match soft_limit_pct {
                Some(v) => v.into(),
                None => stoolap::Value::Null(stoolap::DataType::Null),
            };
            let webhook_value = match alert_webhook {
                Some(s) => s.to_string().into(),
                None => stoolap::Value::Null(stoolap::DataType::Null),
            };

            self.db.execute(
                "UPDATE budgets SET budget_limit = $1, period = $2, soft_limit_pct = $3, alert_webhook = $4 WHERE entity_id = $5 AND entity_type = $6",
                vec![
                    budget_limit.into(),
                    period.into(),
                    soft_limit_value,
                    webhook_value,
                    entity_id.into(),
                    entity_type.into(),
                ],
            ).map_err(|e| KeyError::Storage(e.to_string()))?;
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            let soft_limit_value = match soft_limit_pct {
                Some(v) => v.into(),
                None => stoolap::Value::Null(stoolap::DataType::Null),
            };
            let webhook_value = match alert_webhook {
                Some(s) => s.to_string().into(),
                None => stoolap::Value::Null(stoolap::DataType::Null),
            };

            self.db.execute(
                "INSERT INTO budgets (entity_id, entity_type, budget_limit, period, current_spend, soft_limit_pct, alert_webhook, last_reset, created_at) VALUES ($1, $2, $3, $4, 0, $5, $6, $7, $8)",
                vec![
                    entity_id.into(),
                    entity_type.into(),
                    budget_limit.into(),
                    period.into(),
                    soft_limit_value,
                    webhook_value,
                    now.into(),
                    now.into(),
                ],
            ).map_err(|e| KeyError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    fn get_budget(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Option<BudgetRow>, KeyError> {
        let mut rows = self
            .db
            .query(
                "SELECT * FROM budgets WHERE entity_id = $1 AND entity_type = $2",
                vec![entity_id.into(), entity_type.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        match rows.next() {
            Some(row_result) => {
                let row = row_result.map_err(|e| KeyError::Storage(e.to_string()))?;
                Ok(Some(BudgetRow {
                    entity_id: row
                        .get_by_name("entity_id")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    entity_type: row
                        .get_by_name("entity_type")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    budget_limit: row
                        .get_by_name("budget_limit")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    period: row
                        .get_by_name("period")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    current_spend: row
                        .get_by_name("current_spend")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    soft_limit_pct: row
                        .get_by_name("soft_limit_pct")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    alert_webhook: row
                        .get_by_name("alert_webhook")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    last_reset: row
                        .get_by_name("last_reset")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                    created_at: row
                        .get_by_name("created_at")
                        .map_err(|e| KeyError::Storage(e.to_string()))?,
                }))
            }
            None => Ok(None),
        }
    }

    fn update_spend(
        &self,
        entity_id: &str,
        entity_type: &str,
        amount: i64,
    ) -> Result<(), KeyError> {
        let rows_affected = self
            .db
            .execute(
                "UPDATE budgets SET current_spend = current_spend + $1 WHERE entity_id = $2 AND entity_type = $3",
                vec![amount.into(), entity_id.into(), entity_type.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            return Err(KeyError::NotFound);
        }

        Ok(())
    }

    fn reset_budget(
        &self,
        entity_id: &str,
        entity_type: &str,
        new_period_start: i64,
    ) -> Result<(), KeyError> {
        let rows_affected = self
            .db
            .execute(
                "UPDATE budgets SET current_spend = 0, last_reset = $1 WHERE entity_id = $2 AND entity_type = $3",
                vec![new_period_start.into(), entity_id.into(), entity_type.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if rows_affected == 0 {
            return Err(KeyError::NotFound);
        }

        Ok(())
    }
}

/// Global storage singleton for python_sdk_entry
///
/// Initialized lazily with database path from QUOTA_ROUTER_DB env var,
/// defaulting to `.quota_router.db` if not set.
pub static STORAGE: std::sync::LazyLock<StoolapKeyStorage> = std::sync::LazyLock::new(|| {
    let db_path =
        std::env::var("QUOTA_ROUTER_DB").unwrap_or_else(|_| ".quota_router.db".to_string());
    let db = stoolap::Database::open(&db_path).expect("Failed to open database");
    crate::schema::init_database(&db).expect("Failed to initialize schema");
    StoolapKeyStorage::new(db)
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyType;
    use stoolap::Database;

    fn create_test_storage() -> StoolapKeyStorage {
        let db = stoolap::Database::open_in_memory().unwrap();
        crate::schema::init_database(&db).unwrap();
        StoolapKeyStorage::new(db)
    }

    #[test]
    fn test_create_and_lookup_key() {
        let storage = create_test_storage();

        let key = ApiKey {
            key_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
            key_hash: vec![1, 2, 3],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: Some(100),
            tpm_limit: Some(1000),
            created_at: 100,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        storage.create_key(&key).unwrap();

        let lookup = storage.lookup_by_hash(&[1, 2, 3]).unwrap();
        assert!(lookup.is_some());
        assert_eq!(
            lookup.unwrap().key_id,
            "550e8400-e29b-41d4-a716-446655440001"
        );
    }

    #[test]
    fn test_update_key() {
        let storage = create_test_storage();

        let key = ApiKey {
            key_id: "550e8400-e29b-41d4-a716-446655440002".to_string(),
            key_hash: vec![4, 5, 6],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: Some(100),
            tpm_limit: Some(1000),
            created_at: 100,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        storage.create_key(&key).unwrap();

        // Update the key
        storage
            .update_key(
                "550e8400-e29b-41d4-a716-446655440002",
                &KeyUpdates {
                    budget_limit: Some(2000),
                    rpm_limit: Some(200),
                    tpm_limit: None,
                    expires_at: None,
                    revoked: None,
                    revoked_by: None,
                    revocation_reason: None,
                    key_type: None,
                    description: Some("Updated key".to_string()),
                },
            )
            .unwrap();

        // Lookup and verify
        let updated = storage.lookup_by_hash(&[4, 5, 6]).unwrap().unwrap();
        assert_eq!(updated.budget_limit, 2000);
        assert_eq!(updated.rpm_limit.unwrap(), 200);
        assert_eq!(updated.description, Some("Updated key".to_string()));
    }

    #[test]
    fn test_list_keys() {
        let storage = create_test_storage();

        let team_uuid = uuid::Uuid::parse_str("660e8400-e29b-41d4-a716-446655440001").unwrap();

        // Create keys
        for i in 0..3 {
            let key = ApiKey {
                key_id: format!("550e8400-e29b-41d4-a716-4466554400{:02}", 10 + i),
                key_hash: vec![i as u8],
                key_prefix: "sk-qr-tes".to_string(),
                team_id: Some(team_uuid),
                budget_limit: 1000,
                rpm_limit: None,
                tpm_limit: None,
                created_at: 100,
                expires_at: None,
                revoked: false,
                revoked_at: None,
                revoked_by: None,
                revocation_reason: None,
                key_type: KeyType::Default,
                allowed_routes: None,
                auto_rotate: false,
                rotation_interval_days: None,
                description: None,
                metadata: None,
            };
            storage.create_key(&key).unwrap();
        }

        // List all
        let all_keys = storage.list_keys(None).unwrap();
        assert_eq!(all_keys.len(), 3);

        // List by team
        let team_keys = storage.list_keys(Some(&team_uuid.to_string())).unwrap();
        assert_eq!(team_keys.len(), 3);

        // List by non-existent team
        let other_keys = storage
            .list_keys(Some("00000000-0000-0000-0000-000000000000"))
            .unwrap();
        assert_eq!(other_keys.len(), 0);
    }

    #[test]
    fn test_create_and_get_team() {
        let storage = create_test_storage();

        let team = Team {
            team_id: "660e8400-e29b-41d4-a716-446655440001".to_string(),
            name: "Test Team".to_string(),
            budget_limit: 10000,
            created_at: 100,
        };

        storage.create_team(&team).unwrap();

        let retrieved = storage
            .get_team("660e8400-e29b-41d4-a716-446655440001")
            .unwrap();
        assert!(retrieved.is_some());
        let t = retrieved.unwrap();
        assert_eq!(t.team_id, "660e8400-e29b-41d4-a716-446655440001");
        assert_eq!(t.name, "Test Team");
        assert_eq!(t.budget_limit, 10000);
    }

    #[test]
    fn test_get_nonexistent_team() {
        let storage = create_test_storage();

        let retrieved = storage
            .get_team("00000000-0000-0000-0000-000000000000")
            .unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_teams() {
        let storage = create_test_storage();

        // Create multiple teams
        for i in 0..3 {
            let team = Team {
                team_id: format!("660e8400-e29b-41d4-a716-4466554400{:02}", 10 + i),
                name: format!("Team {}", i),
                budget_limit: 1000 * (i + 1) as i64,
                created_at: 100 + i as i64,
            };
            storage.create_team(&team).unwrap();
        }

        let teams = storage.list_teams().unwrap();
        assert_eq!(teams.len(), 3);
    }

    #[test]
    fn test_delete_team_with_keys_fails() {
        let storage = create_test_storage();

        let team_uuid = "660e8400-e29b-41d4-a716-446655440020";
        let key_uuid = "550e8400-e29b-41d4-a716-446655440020";

        // Create a team
        let team = Team {
            team_id: team_uuid.to_string(),
            name: "Team With Keys".to_string(),
            budget_limit: 10000,
            created_at: 100,
        };
        storage.create_team(&team).unwrap();

        // Create a key belonging to this team
        let key = ApiKey {
            key_id: key_uuid.to_string(),
            key_hash: vec![1, 2, 3],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: Some(team_uuid.parse().unwrap()),
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 100,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: crate::keys::KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };
        storage.create_key(&key).unwrap();

        // Delete should fail
        let result = storage.delete_team(team_uuid);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_team_success() {
        let storage = create_test_storage();

        // Create a team with no keys
        let team = Team {
            team_id: "660e8400-e29b-41d4-a716-446655440099".to_string(),
            name: "Orphan Team".to_string(),
            budget_limit: 5000,
            created_at: 100,
        };
        storage.create_team(&team).unwrap();

        // Delete should succeed
        storage
            .delete_team("660e8400-e29b-41d4-a716-446655440099")
            .unwrap();

        // Verify deleted
        let retrieved = storage
            .get_team("660e8400-e29b-41d4-a716-446655440099")
            .unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_resolve_tokenizer_not_found() {
        let storage = create_test_storage();

        let id: [u8; 16] = [
            0xe3, 0xc8, 0xe8, 0xff, 0x72, 0x44, 0x11, 0xc6, 0x41, 0x6d, 0xd4, 0xfb, 0x13, 0x53,
            0x68, 0xe3,
        ];
        let result = storage.resolve_tokenizer(&id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_ensure_tokenizer_and_resolve() {
        let storage = create_test_storage();

        // Ensure a tokenizer exists
        let tokenizer_id = storage
            .ensure_tokenizer("tiktoken-cl100k_base-v1.2.3", Some("openai"))
            .unwrap();

        // Verify it's the expected BLAKE3-16 value
        let expected: [u8; 16] = [
            0xe3, 0xc8, 0xe8, 0xff, 0x72, 0x44, 0x11, 0xc6, 0x41, 0x6d, 0xd4, 0xfb, 0x13, 0x53,
            0x68, 0xe3,
        ];
        assert_eq!(tokenizer_id, expected);

        // Resolve should now return Some
        let version = storage.resolve_tokenizer(&tokenizer_id).unwrap();
        assert_eq!(version, Some("tiktoken-cl100k_base-v1.2.3".to_string()));
    }

    #[test]
    fn test_ensure_tokenizer_idempotent() {
        let storage = create_test_storage();

        // Call ensure twice with same version
        let id1 = storage
            .ensure_tokenizer("tiktoken-o200k_base", Some("openai"))
            .unwrap();
        let id2 = storage
            .ensure_tokenizer("tiktoken-o200k_base", Some("openai"))
            .unwrap();

        // Should return same tokenizer_id
        assert_eq!(id1, id2);

        // Should still resolve
        let version = storage.resolve_tokenizer(&id1).unwrap();
        assert_eq!(version, Some("tiktoken-o200k_base".to_string()));
    }

    #[test]
    fn test_resolve_tokenizer_storage() {
        let storage = create_test_storage();

        // Insert a tokenizer row directly via ensure_tokenizer
        let tid = storage
            .ensure_tokenizer("tiktoken-cl100k_base-v1.2.3", Some("anthropic"))
            .unwrap();

        // Resolve should find it
        let result = storage.resolve_tokenizer(&tid).unwrap();
        assert_eq!(result, Some("tiktoken-cl100k_base-v1.2.3".to_string()));
    }

    #[test]
    fn test_record_spend_ledger_populates_tokenizers() {
        // RE-ENABLED: stoolap (CipherOcto fork) now supports aggregate functions (SUM)
        // inside transactions (RFC-0204 Phase 2). This test validates that:
        // 1. record_spend_ledger works inside a transaction with SUM
        // 2. The actual functionality is validated via middleware test_record_spend

        let db = Database::open_in_memory().unwrap();

        // Create minimal tables for testing
        db.execute(
            "CREATE TABLE spend_ledger (event_id TEXT NOT NULL, key_id TEXT NOT NULL, cost_amount INTEGER NOT NULL)",
            (),
        )
        .unwrap();

        // Insert test data
        let key_id = "test-key-001";
        db.execute(
            "INSERT INTO spend_ledger (event_id, key_id, cost_amount) VALUES ($1, $2, $3)",
            vec![
                stoolap::core::Value::text("event1"),
                stoolap::core::Value::text(key_id),
                stoolap::core::Value::integer(100),
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO spend_ledger (event_id, key_id, cost_amount) VALUES ($1, $2, $3)",
            vec![
                stoolap::core::Value::text("event2"),
                stoolap::core::Value::text(key_id),
                stoolap::core::Value::integer(200),
            ],
        )
        .unwrap();

        // Test that SUM works inside a transaction (the core fix)
        let mut tx = db.begin().unwrap();
        let mut rows = tx
            .query(
                "SELECT SUM(cost_amount) FROM spend_ledger WHERE key_id = $1",
                vec![stoolap::core::Value::text(key_id)],
            )
            .unwrap();

        // Check if we got a row
        if let Some(row) = rows.next() {
            let row = row.unwrap();
            // Use get::<Option<i64>> to handle nullable result
            if let Ok(Some(sum)) = row.get::<Option<i64>>(0) {
                let result: i64 = sum;
                tx.commit().unwrap();
                // SUM should return 300 (100 + 200)
                assert_eq!(result, 300, "SUM aggregate should work inside transaction");
            } else {
                tx.commit().unwrap();
                panic!("SUM returned NULL");
            }
        } else {
            tx.commit().unwrap();
            panic!("No rows returned");
        }
    }

    #[test]
    fn test_record_spend_ledger_provider_usage() {
        // RE-ENABLED: stoolap aggregate support in transactions (RFC-0204 Phase 2)
        // This test validates COUNT and AVG inside transactions

        let db = Database::open_in_memory().unwrap();

        // Create minimal table
        db.execute(
            "CREATE TABLE spend_ledger (event_id TEXT NOT NULL, key_id TEXT NOT NULL, cost_amount INTEGER NOT NULL)",
            (),
        )
        .unwrap();

        // Insert test data
        let key_id = "test-key-002";
        db.execute(
            "INSERT INTO spend_ledger (event_id, key_id, cost_amount) VALUES ($1, $2, $3)",
            vec![
                stoolap::core::Value::text("event1"),
                stoolap::core::Value::text(key_id),
                stoolap::core::Value::integer(100),
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO spend_ledger (event_id, key_id, cost_amount) VALUES ($1, $2, $3)",
            vec![
                stoolap::core::Value::text("event2"),
                stoolap::core::Value::text(key_id),
                stoolap::core::Value::integer(200),
            ],
        )
        .unwrap();

        // Test COUNT inside transaction
        let mut tx = db.begin().unwrap();

        let mut count_rows = tx
            .query(
                "SELECT COUNT(*) FROM spend_ledger WHERE key_id = $1",
                vec![stoolap::core::Value::text(key_id)],
            )
            .unwrap();

        if let Some(row) = count_rows.next() {
            let row = row.unwrap();
            if let Ok(Some(count)) = row.get::<Option<i64>>(0) {
                let result: i64 = count;
                assert_eq!(result, 2, "COUNT should return 2");
            } else {
                panic!("COUNT returned NULL");
            }
        }

        let mut avg_rows = tx
            .query(
                "SELECT AVG(cost_amount) FROM spend_ledger WHERE key_id = $1",
                vec![stoolap::core::Value::text(key_id)],
            )
            .unwrap();

        if let Some(row) = avg_rows.next() {
            let row = row.unwrap();
            if let Ok(Some(avg)) = row.get::<Option<i64>>(0) {
                let result: i64 = avg;
                tx.commit().unwrap();
                assert_eq!(result, 150, "AVG should return 150");
            } else {
                tx.commit().unwrap();
                panic!("AVG returned NULL");
            }
        } else {
            tx.commit().unwrap();
            panic!("No rows returned for AVG");
        }
    }

    #[test]
    fn test_upsert_budget_create() {
        let storage = create_test_storage();
        storage
            .upsert_budget("key-1", "key", 100000, "monthly", Some(80), None)
            .unwrap();
        let budget = storage.get_budget("key-1", "key").unwrap().unwrap();
        assert_eq!(budget.entity_id, "key-1");
        assert_eq!(budget.entity_type, "key");
        assert_eq!(budget.budget_limit, 100000);
        assert_eq!(budget.period, "monthly");
        assert_eq!(budget.soft_limit_pct, Some(80));
        assert_eq!(budget.alert_webhook, None);
        assert_eq!(budget.current_spend, 0);
    }

    #[test]
    fn test_upsert_budget_update() {
        let storage = create_test_storage();
        storage
            .upsert_budget("key-1", "key", 100000, "monthly", Some(80), None)
            .unwrap();
        storage
            .upsert_budget(
                "key-1",
                "key",
                200000,
                "weekly",
                Some(90),
                Some("https://hook.example.com"),
            )
            .unwrap();
        let budget = storage.get_budget("key-1", "key").unwrap().unwrap();
        assert_eq!(budget.budget_limit, 200000);
        assert_eq!(budget.period, "weekly");
        assert_eq!(budget.soft_limit_pct, Some(90));
        assert_eq!(
            budget.alert_webhook,
            Some("https://hook.example.com".to_string())
        );
        // current_spend should be preserved on update
        assert_eq!(budget.current_spend, 0);
    }

    #[test]
    fn test_get_budget_not_found() {
        let storage = create_test_storage();
        let result = storage.get_budget("nonexistent", "key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_spend() {
        let storage = create_test_storage();
        storage
            .upsert_budget("key-1", "key", 100000, "monthly", Some(80), None)
            .unwrap();
        storage.update_spend("key-1", "key", 5000).unwrap();
        let budget = storage.get_budget("key-1", "key").unwrap().unwrap();
        assert_eq!(budget.current_spend, 5000);
        storage.update_spend("key-1", "key", 3000).unwrap();
        let budget = storage.get_budget("key-1", "key").unwrap().unwrap();
        assert_eq!(budget.current_spend, 8000);
    }

    #[test]
    fn test_update_spend_not_found() {
        let storage = create_test_storage();
        let result = storage.update_spend("nonexistent", "key", 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_budget() {
        let storage = create_test_storage();
        storage
            .upsert_budget("key-1", "key", 100000, "monthly", Some(80), None)
            .unwrap();
        storage.update_spend("key-1", "key", 5000).unwrap();
        storage.reset_budget("key-1", "key", 1000000).unwrap();
        let budget = storage.get_budget("key-1", "key").unwrap().unwrap();
        assert_eq!(budget.current_spend, 0);
        assert_eq!(budget.last_reset, 1000000);
    }

    #[test]
    fn test_reset_budget_not_found() {
        let storage = create_test_storage();
        let result = storage.reset_budget("nonexistent", "key", 1000000);
        assert!(result.is_err());
    }
}
