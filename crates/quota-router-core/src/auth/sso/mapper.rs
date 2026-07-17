//! SSO-to-API-Key Mapper (RFC-0949)
//!
//! Maps SSO users to virtual API keys via user.sub (IdP subject identifier).

use super::{IdentityProvider, SsoError, SsoKeyMetadata, SsoUser};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// SsoKeyStorageExt — Extension trait for KeyStorage
// ============================================================================

/// Extension trait for KeyStorage to support SSO lookups
#[async_trait::async_trait]
pub trait SsoKeyStorageExt: Send + Sync {
    /// Find virtual key by SSO subject identifier
    async fn get_key_by_sso_subject(
        &self,
        subject: &str,
    ) -> Result<Option<crate::keys::ApiKey>, SsoError>;

    /// Create virtual key for SSO user
    async fn create_key_for_sso_user(
        &self,
        user: &SsoUser,
        metadata: SsoKeyMetadata,
    ) -> Result<crate::keys::ApiKey, SsoError>;

    /// Update SSO metadata on existing key
    async fn update_key_sso_metadata(
        &self,
        key_id: &str,
        metadata: SsoKeyMetadata,
    ) -> Result<(), SsoError>;
}

// ============================================================================
// SsoKeyMapper
// ============================================================================

pub struct SsoKeyMapper {
    /// Key storage backend with SSO extension
    key_storage: Arc<dyn SsoKeyStorageExt>,
    /// Role mapping config (IdP group → quota-router role)
    role_mapping: HashMap<String, String>,
    /// Team mapping config (IdP group → quota-router team)
    team_mapping: HashMap<String, String>,
}

impl SsoKeyMapper {
    pub fn new(
        key_storage: Arc<dyn SsoKeyStorageExt>,
        role_mapping: HashMap<String, String>,
        team_mapping: HashMap<String, String>,
    ) -> Self {
        Self {
            key_storage,
            role_mapping,
            team_mapping,
        }
    }

    /// Get or create virtual key for SSO user
    pub async fn get_or_create_key(
        &self,
        user: &SsoUser,
        provider: &IdentityProvider,
    ) -> Result<crate::keys::ApiKey, SsoError> {
        // 1. Look up existing key by user.sub (IdP subject)
        if let Some(key) = self.key_storage.get_key_by_sso_subject(&user.sub).await? {
            return Ok(key);
        }

        // 2. Check if user is deactivated
        // (In a real implementation, check IdP status or local user record)

        // 3. Auto-provision if enabled
        if provider.auto_provision {
            let metadata = SsoKeyMetadata {
                sso_subject: Some(user.sub.clone()),
                sso_provider: Some(provider.id.clone()),
            };
            let key = self
                .key_storage
                .create_key_for_sso_user(user, metadata)
                .await?;
            return Ok(key);
        }

        // 4. No key mapping found and auto-provision disabled
        Err(SsoError::NoKeyMapping(user.sub.clone()))
    }

    /// Map IdP groups to quota-router roles
    pub fn map_role(&self, groups: &[String]) -> Option<String> {
        for group in groups {
            if let Some(role) = self.role_mapping.get(group) {
                return Some(role.clone());
            }
        }
        None
    }

    /// Map IdP groups to quota-router team
    pub fn map_team(&self, groups: &[String]) -> Option<String> {
        for group in groups {
            if let Some(team) = self.team_mapping.get(group) {
                return Some(team.clone());
            }
        }
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::{ProviderConfig, ProviderType};
    use super::*;

    #[test]
    fn test_map_role() {
        let mut role_mapping = HashMap::new();
        role_mapping.insert("admins".to_string(), "admin".to_string());
        role_mapping.insert("developers".to_string(), "developer".to_string());

        let mapper = SsoKeyMapper::new(Arc::new(MockKeyStorage), role_mapping, HashMap::new());

        assert_eq!(
            mapper.map_role(&["developers".to_string()]),
            Some("developer".to_string())
        );
        assert_eq!(
            mapper.map_role(&["admins".to_string(), "other".to_string()]),
            Some("admin".to_string())
        );
        assert_eq!(mapper.map_role(&["unknown".to_string()]), None);
    }

    #[test]
    fn test_map_team() {
        let mut team_mapping = HashMap::new();
        team_mapping.insert("engineering".to_string(), "eng-team".to_string());

        let mapper = SsoKeyMapper::new(Arc::new(MockKeyStorage), HashMap::new(), team_mapping);

        assert_eq!(
            mapper.map_team(&["engineering".to_string()]),
            Some("eng-team".to_string())
        );
        assert_eq!(mapper.map_team(&["other".to_string()]), None);
    }

    #[tokio::test]
    async fn test_get_or_create_key_no_mapping() {
        let mapper = SsoKeyMapper::new(Arc::new(MockKeyStorage), HashMap::new(), HashMap::new());
        let user = SsoUser {
            sub: "user-123".into(),
            email: Some("user@example.com".into()),
            name: Some("Test User".into()),
            groups: vec![],
            roles: vec![],
            provider_id: "okta".into(),
        };
        let provider = IdentityProvider {
            id: "okta".into(),
            name: "Okta".into(),
            provider_type: ProviderType::Okta,
            config: ProviderConfig {
                client_id: None,
                client_secret: None,
                issuer: None,
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };
        let result = mapper.get_or_create_key(&user, &provider).await;
        assert!(matches!(result, Err(SsoError::NoKeyMapping(_))));
    }

    #[tokio::test]
    async fn test_get_or_create_key_auto_provision() {
        let mapper = SsoKeyMapper::new(Arc::new(MockKeyStorage), HashMap::new(), HashMap::new());
        let user = SsoUser {
            sub: "user-456".into(),
            email: Some("user2@example.com".into()),
            name: Some("Test User 2".into()),
            groups: vec![],
            roles: vec![],
            provider_id: "okta".into(),
        };
        let provider = IdentityProvider {
            id: "okta".into(),
            name: "Okta".into(),
            provider_type: ProviderType::Okta,
            config: ProviderConfig {
                client_id: None,
                client_secret: None,
                issuer: None,
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: true,
            default_team: None,
        };
        let result = mapper.get_or_create_key(&user, &provider).await;
        // MockKeyStorage returns error for create, so this should fail
        assert!(result.is_err());
    }

    struct MockKeyStorage;

    #[async_trait::async_trait]
    impl SsoKeyStorageExt for MockKeyStorage {
        async fn get_key_by_sso_subject(
            &self,
            _subject: &str,
        ) -> Result<Option<crate::keys::ApiKey>, SsoError> {
            Ok(None)
        }

        async fn create_key_for_sso_user(
            &self,
            _user: &SsoUser,
            _metadata: SsoKeyMetadata,
        ) -> Result<crate::keys::ApiKey, SsoError> {
            Err(SsoError::ProviderError("mock".into()))
        }

        async fn update_key_sso_metadata(
            &self,
            _key_id: &str,
            _metadata: SsoKeyMetadata,
        ) -> Result<(), SsoError> {
            Ok(())
        }
    }
}
