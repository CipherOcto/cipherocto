//! Production SsoKeyStorageExt implementation wrapping StoolapKeyStorage.
//!
//! Provides SSO key storage using the existing api_keys table with
//! JSON metadata for sso_subject lookups.

use super::{SsoError, SsoKeyMetadata, SsoKeyStorageExt, SsoUser};
use crate::keys::{ApiKey, KeyType};
use crate::storage::{KeyStorage, StoolapKeyStorage};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;

/// Production SsoKeyStorageExt implementation wrapping StoolapKeyStorage.
pub struct StoolapSsoKeyStorage {
    inner: Arc<StoolapKeyStorage>,
}

impl StoolapSsoKeyStorage {
    /// Create a new StoolapSsoKeyStorage wrapping an existing StoolapKeyStorage.
    pub fn new(inner: Arc<StoolapKeyStorage>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl SsoKeyStorageExt for StoolapSsoKeyStorage {
    async fn get_key_by_sso_subject(&self, subject: &str) -> Result<Option<ApiKey>, SsoError> {
        // List all keys and filter by sso_subject in metadata
        let all_keys = self
            .inner
            .list_keys(None)
            .map_err(|e| SsoError::ProviderError(format!("key storage error: {}", e)))?;

        for key in all_keys {
            if let Some(metadata_str) = &key.metadata {
                if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_str) {
                    if metadata
                        .get("sso_subject")
                        .and_then(|v| v.as_str())
                        .map(|s| s == subject)
                        .unwrap_or(false)
                    {
                        return Ok(Some(key));
                    }
                }
            }
        }
        Ok(None)
    }

    async fn create_key_for_sso_user(
        &self,
        user: &SsoUser,
        metadata: SsoKeyMetadata,
    ) -> Result<ApiKey, SsoError> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_prefix = format!("sso-{}", &user.sub[..8.min(user.sub.len())]);
        let metadata_json =
            serde_json::to_string(&metadata).map_err(|e| SsoError::ProviderError(e.to_string()))?;

        let api_key = ApiKey {
            key_id: key_id.clone(),
            key_hash: vec![], // SSO keys don't use traditional hash
            key_prefix,
            team_id: None,
            budget_limit: 1, // SSO keys inherit team budget from IdP (min 1 to pass validation)
            rpm_limit: None,
            tpm_limit: None,
            created_at: Utc::now().timestamp(),
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Sso,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: Some(format!(
                "SSO user: {}",
                user.email.as_deref().unwrap_or(&user.sub)
            )),
            metadata: Some(metadata_json),
        };

        self.inner
            .create_key(&api_key)
            .map_err(|e| SsoError::ProviderError(format!("key creation failed: {}", e)))?;

        Ok(api_key)
    }

    async fn update_key_sso_metadata(
        &self,
        key_id: &str,
        metadata: SsoKeyMetadata,
    ) -> Result<(), SsoError> {
        let metadata_json =
            serde_json::to_string(&metadata).map_err(|e| SsoError::ProviderError(e.to_string()))?;

        let updates = crate::keys::KeyUpdates {
            metadata: Some(metadata_json),
            ..Default::default()
        };

        self.inner
            .update_key(key_id, &updates)
            .map_err(|e| SsoError::ProviderError(format!("key update failed: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::sso::*;
    use crate::schema::init_database;
    use crate::storage::KeyStorage;

    fn create_test_storage() -> StoolapKeyStorage {
        let db = stoolap::Database::open_in_memory().unwrap();
        init_database(&db).unwrap();
        StoolapKeyStorage::new(db)
    }

    #[tokio::test]
    async fn test_get_key_by_sso_subject_not_found() {
        let inner = Arc::new(create_test_storage());
        let storage = StoolapSsoKeyStorage::new(inner);

        let result = storage.get_key_by_sso_subject("nonexistent-sub").await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_create_and_find_sso_key() {
        let inner = Arc::new(create_test_storage());
        let storage = StoolapSsoKeyStorage::new(inner);

        let user = SsoUser {
            sub: "user123@example.com".to_string(),
            email: Some("user@example.com".to_string()),
            name: Some("Test User".to_string()),
            groups: vec!["engineers".to_string()],
            roles: vec![],
            provider_id: "okta-1".to_string(),
        };

        let metadata = SsoKeyMetadata {
            sso_subject: Some(user.sub.clone()),
            sso_provider: Some(user.provider_id.clone()),
        };

        // Create key
        let created = storage
            .create_key_for_sso_user(&user, metadata.clone())
            .await
            .unwrap();

        assert!(created.key_prefix.starts_with("sso-user12"));

        // Find by sso_subject
        let found = storage.get_key_by_sso_subject(&user.sub).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.key_id, created.key_id);
        assert_eq!(found.key_type, KeyType::Sso);
    }

    #[tokio::test]
    async fn test_update_sso_metadata() {
        let inner = Arc::new(create_test_storage());
        let storage = StoolapSsoKeyStorage::new(inner);

        let user = SsoUser {
            sub: "update-test".to_string(),
            email: Some("update@test.com".to_string()),
            name: Some("Update Test".to_string()),
            groups: vec![],
            roles: vec![],
            provider_id: "okta-1".to_string(),
        };

        let metadata = SsoKeyMetadata {
            sso_subject: Some(user.sub.clone()),
            sso_provider: Some(user.provider_id.clone()),
        };

        let created = storage
            .create_key_for_sso_user(&user, metadata.clone())
            .await
            .unwrap();

        // Update metadata
        let mut new_metadata = metadata.clone();
        new_metadata.sso_provider = Some("new-provider".to_string());

        storage
            .update_key_sso_metadata(&created.key_id, new_metadata.clone())
            .await
            .unwrap();

        // Verify update
        let found = storage.get_key_by_sso_subject(&user.sub).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        let updated_meta: SsoKeyMetadata =
            serde_json::from_str(found.metadata.as_ref().unwrap()).unwrap();
        assert_eq!(updated_meta.sso_provider, Some("new-provider".to_string()));
    }
}
