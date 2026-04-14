use crate::keys::{ApiKey, KeyError, KeySpend, KeyType, KeyUpdates, SpendEvent, Team};

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
}

pub struct StoolapKeyStorage {
    db: stoolap::Database,
}

impl StoolapKeyStorage {
    pub fn new(db: stoolap::Database) -> Self {
        Self { db }
    }

    fn row_to_api_key(&self, row: &stoolap::ResultRow) -> Result<ApiKey, KeyError> {
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

        Ok(ApiKey {
            key_id: row
                .get_by_name("key_id")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            key_hash,
            key_prefix: row
                .get_by_name("key_prefix")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
            team_id: row
                .get_by_name("team_id")
                .map_err(|e| KeyError::Storage(e.to_string()))?,
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

        let params: Vec<stoolap::Value> = vec![
            key.key_id.clone().into(),
            key_hash_value,
            key.key_prefix.clone().into(),
            key.team_id.clone().into(),
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

        set_clauses.push(format!("key_id = ${}", params.len() + 1));
        params.push(key_id.into());

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
            let params: Vec<stoolap::Value> = vec![tid.into()];
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
        let mut rows = self
            .db
            .query(
                "SELECT COUNT(*) FROM api_keys WHERE team_id = $1 AND revoked = 0",
                vec![team_id.into()],
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
        self.db
            .execute(
                "INSERT INTO teams (team_id, name, budget_limit, created_at) VALUES ($1, $2, $3, $4)",
                vec![
                    team.team_id.clone().into(),
                    team.name.clone().into(),
                    team.budget_limit.into(),
                    team.created_at.into(),
                ],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        Ok(())
    }

    fn get_team(&self, team_id: &str) -> Result<Option<Team>, KeyError> {
        let rows = self
            .db
            .query(
                "SELECT * FROM teams WHERE team_id = $1",
                vec![team_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        if let Some(Ok(row)) = rows.into_iter().next() {
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
            Ok(Some(team))
        } else {
            Ok(None)
        }
    }

    fn update_team(&self, team_id: &str, name: &str, budget_limit: i64) -> Result<(), KeyError> {
        self.db
            .execute(
                "UPDATE teams SET name = $1, budget_limit = $2 WHERE team_id = $3",
                vec![name.into(), budget_limit.into(), team_id.into()],
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

        self.db
            .execute("DELETE FROM teams WHERE team_id = $1", vec![team_id.into()])
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

        let key_id_str = event.key_id.to_string();

        // Begin transaction for atomic budget enforcement with FOR UPDATE locking
        let mut tx = self
            .db
            .begin()
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 1. Lock key row FOR UPDATE to prevent concurrent modifications
        let budget: i64 = tx
            .query(
                "SELECT budget_limit FROM api_keys WHERE key_id = $1",
                vec![key_id_str.clone().into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .next()
            .ok_or(KeyError::NotFound)?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 2. Compute current spend from ledger
        let mut rows = tx
            .query(
                "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
                vec![key_id_str.clone().into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let current: i64 = rows
            .next()
            .ok_or(KeyError::Storage("Expected row".to_string()))?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 3. Verify budget against cost_amount
        let cost_i64 = event.cost_amount as i64;
        if current + cost_i64 > budget {
            return Err(KeyError::BudgetExceeded {
                current: current as u64,
                limit: budget as u64,
            });
        }

        // 4. Build params for INSERT
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let params: Vec<stoolap::Value> = vec![
            event.event_id.clone().into(),
            event.request_id.clone().into(),
            key_id_str.into(),
            event.team_id.clone().into(),
            event.provider.clone().into(),
            event.model.clone().into(),
            event.input_tokens.into(),
            event.output_tokens.into(),
            cost_i64.into(),
            stoolap::core::Value::blob(event.pricing_hash.clone()),
            token_source_str.into(),
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
                token_source, tokenizer_version, provider_usage_json, timestamp,
                created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
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

        // Begin transaction for atomic budget enforcement with FOR UPDATE locking
        let mut tx = self
            .db
            .begin()
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 1. Lock team row FIRST (deadlock prevention per RFC-0903 §Lock Ordering Invariant)
        let team_budget: i64 = tx
            .query(
                "SELECT budget_limit FROM teams WHERE team_id = $1",
                vec![team_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .next()
            .ok_or(KeyError::NotFound)?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 2. Lock key row SECOND
        let key_budget: i64 = tx
            .query(
                "SELECT budget_limit FROM api_keys WHERE key_id = $1",
                vec![key_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .next()
            .ok_or(KeyError::NotFound)?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 3. Compute key spend from ledger
        let mut rows = tx
            .query(
                "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
                vec![key_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let key_current: i64 = rows
            .next()
            .ok_or(KeyError::Storage("Expected row".to_string()))?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 4. Compute team spend from ledger
        let mut rows = tx
            .query(
                "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE team_id = $1",
                vec![team_id.into()],
            )
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        let team_current: i64 = rows
            .next()
            .ok_or(KeyError::Storage("Expected row".to_string()))?
            .map_err(|e| KeyError::Storage(e.to_string()))?
            .get(0)
            .map_err(|e| KeyError::Storage(e.to_string()))?;

        // 5. Verify both budgets
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

        // 6. Build params for INSERT
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let params: Vec<stoolap::Value> = vec![
            event.event_id.clone().into(),
            event.request_id.clone().into(),
            key_id.into(),
            Some(team_id.to_string()).into(),
            event.provider.clone().into(),
            event.model.clone().into(),
            event.input_tokens.into(),
            event.output_tokens.into(),
            cost_i64.into(),
            stoolap::core::Value::blob(event.pricing_hash.clone()),
            token_source_str.into(),
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
                token_source, tokenizer_version, provider_usage_json, timestamp,
                created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyType;

    fn create_test_storage() -> StoolapKeyStorage {
        let db = stoolap::Database::open_in_memory().unwrap();
        crate::schema::init_database(&db).unwrap();
        StoolapKeyStorage::new(db)
    }

    #[test]
    fn test_create_and_lookup_key() {
        let storage = create_test_storage();

        let key = ApiKey {
            key_id: "test-key-1".to_string(),
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
        assert_eq!(lookup.unwrap().key_id, "test-key-1");
    }

    #[test]
    fn test_update_key() {
        let storage = create_test_storage();

        let key = ApiKey {
            key_id: "test-key-update".to_string(),
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
                "test-key-update",
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

        // Create keys
        for i in 0..3 {
            let key = ApiKey {
                key_id: format!("test-key-{}", i),
                key_hash: vec![i as u8],
                key_prefix: "sk-qr-tes".to_string(),
                team_id: Some("team1".to_string()),
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
        let team_keys = storage.list_keys(Some("team1")).unwrap();
        assert_eq!(team_keys.len(), 3);

        // List by non-existent team
        let other_keys = storage.list_keys(Some("nonexistent")).unwrap();
        assert_eq!(other_keys.len(), 0);
    }

    #[test]
    fn test_create_and_get_team() {
        let storage = create_test_storage();

        let team = Team {
            team_id: "team-1".to_string(),
            name: "Test Team".to_string(),
            budget_limit: 10000,
            created_at: 100,
        };

        storage.create_team(&team).unwrap();

        let retrieved = storage.get_team("team-1").unwrap();
        assert!(retrieved.is_some());
        let t = retrieved.unwrap();
        assert_eq!(t.team_id, "team-1");
        assert_eq!(t.name, "Test Team");
        assert_eq!(t.budget_limit, 10000);
    }

    #[test]
    fn test_get_nonexistent_team() {
        let storage = create_test_storage();

        let retrieved = storage.get_team("nonexistent").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_list_teams() {
        let storage = create_test_storage();

        // Create multiple teams
        for i in 0..3 {
            let team = Team {
                team_id: format!("team-{}", i),
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

        // Create a team
        let team = Team {
            team_id: "team-with-keys".to_string(),
            name: "Team With Keys".to_string(),
            budget_limit: 10000,
            created_at: 100,
        };
        storage.create_team(&team).unwrap();

        // Create a key belonging to this team
        let key = ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![1, 2, 3],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: Some("team-with-keys".to_string()),
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
        let result = storage.delete_team("team-with-keys");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_team_success() {
        let storage = create_test_storage();

        // Create a team with no keys
        let team = Team {
            team_id: "orphan-team".to_string(),
            name: "Orphan Team".to_string(),
            budget_limit: 5000,
            created_at: 100,
        };
        storage.create_team(&team).unwrap();

        // Delete should succeed
        storage.delete_team("orphan-team").unwrap();

        // Verify deleted
        let retrieved = storage.get_team("orphan-team").unwrap();
        assert!(retrieved.is_none());
    }
}
