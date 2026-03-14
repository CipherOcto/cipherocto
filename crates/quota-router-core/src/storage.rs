use crate::keys::{ApiKey, KeyError, KeyType, KeyUpdates};

pub trait KeyStorage: Send + Sync {
    fn create_key(&self, key: &ApiKey) -> Result<(), KeyError>;
    fn lookup_by_hash(&self, key_hash: &[u8]) -> Result<Option<ApiKey>, KeyError>;
    fn update_key(&self, key_id: &str, updates: &KeyUpdates) -> Result<(), KeyError>;
    fn list_keys(&self, team_id: Option<&str>) -> Result<Vec<ApiKey>, KeyError>;
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

        // key_hash is stored as hex string in DB
        let key_hash_hex: String = row
            .get_by_name("key_hash")
            .map_err(|e| KeyError::Storage(e.to_string()))?;
        let key_hash = hex::decode(&key_hash_hex).map_err(|e| KeyError::Storage(e.to_string()))?;

        Ok(ApiKey {
            key_id: row.get_by_name("key_id").map_err(|e| KeyError::Storage(e.to_string()))?,
            key_hash,
            key_prefix: row.get_by_name("key_prefix").map_err(|e| KeyError::Storage(e.to_string()))?,
            team_id: row.get_by_name("team_id").map_err(|e| KeyError::Storage(e.to_string()))?,
            budget_limit: row.get_by_name("budget_limit").map_err(|e| KeyError::Storage(e.to_string()))?,
            rpm_limit: row.get_by_name("rpm_limit").map_err(|e| KeyError::Storage(e.to_string()))?,
            tpm_limit: row.get_by_name("tpm_limit").map_err(|e| KeyError::Storage(e.to_string()))?,
            created_at: row.get_by_name("created_at").map_err(|e| KeyError::Storage(e.to_string()))?,
            expires_at: row.get_by_name("expires_at").map_err(|e| KeyError::Storage(e.to_string()))?,
            revoked: row.get_by_name::<i32>("revoked").map_err(|e| KeyError::Storage(e.to_string()))? != 0,
            revoked_at: row.get_by_name("revoked_at").map_err(|e| KeyError::Storage(e.to_string()))?,
            revoked_by: row.get_by_name("revoked_by").map_err(|e| KeyError::Storage(e.to_string()))?,
            revocation_reason: row.get_by_name("revocation_reason").map_err(|e| KeyError::Storage(e.to_string()))?,
            key_type,
            allowed_routes: row.get_by_name("allowed_routes").map_err(|e| KeyError::Storage(e.to_string()))?,
            auto_rotate: row.get_by_name::<i32>("auto_rotate").map_err(|e| KeyError::Storage(e.to_string()))? != 0,
            rotation_interval_days: row.get_by_name("rotation_interval_days").map_err(|e| KeyError::Storage(e.to_string()))?,
            description: row.get_by_name("description").map_err(|e| KeyError::Storage(e.to_string()))?,
            metadata: row.get_by_name("metadata").map_err(|e| KeyError::Storage(e.to_string()))?,
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
        // Store key_hash as hex string
        let key_hash_hex = hex::encode(&key.key_hash);

        // Helper to convert Option<i64> to stoolap::Value (None = Null)
        let opt_i64_to_value = |opt: Option<i64>| -> stoolap::Value {
            opt.map(|v| v.into()).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null))
        };
        // Helper to convert Option<i32> to stoolap::Value (None = Null)
        let opt_i32_to_value = |opt: Option<i32>| -> stoolap::Value {
            opt.map(|v| v.into()).unwrap_or(stoolap::Value::Null(stoolap::DataType::Null))
        };

        let params: Vec<stoolap::Value> = vec![
            key.key_id.clone().into(),
            key_hash_hex.into(),
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
        let key_hash_hex = hex::encode(key_hash);
        let params: Vec<stoolap::Value> = vec![key_hash_hex.into()];

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

        let sql = format!("UPDATE api_keys SET {} WHERE key_id = ${}", set_clauses.join(", "), params.len());

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
}
