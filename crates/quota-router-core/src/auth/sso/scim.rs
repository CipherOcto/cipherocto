//! SCIM 2.0 types and operations (RFC-0949).
//!
//! Implements the System for Cross-domain Identity Management protocol
//! for user provisioning and deprovisioning.

use serde::{Deserialize, Serialize};

// ============================================================================
// SCIM Schemas
// ============================================================================

const SCIM_USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const SCIM_GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
const SCIM_LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
const _SCIM_PATCH_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
const SCIM_ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
const SCIM_SP_CONFIG_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig";
const SCIM_RESOURCE_TYPE_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ResourceType";

// ============================================================================
// SCIM Error (RFC 7644 Section 3.12)
// ============================================================================

/// SCIM-specific error response format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimError {
    pub schemas: Vec<String>,
    #[serde(rename = "scimType", skip_serializing_if = "Option::is_none")]
    pub scim_type: Option<String>,
    pub detail: String,
    pub status: String,
}

impl ScimError {
    pub fn new(status: &str, detail: &str, scim_type: Option<&str>) -> Self {
        Self {
            schemas: vec![SCIM_ERROR_SCHEMA.to_string()],
            scim_type: scim_type.map(|s| s.to_string()),
            detail: detail.to_string(),
            status: status.to_string(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"schemas":["urn:ietf:params:scim:api:messages:2.0:Error"],"detail":"internal error","status":"500"}"#.to_string())
    }
}

// ============================================================================
// SCIM User
// ============================================================================

/// SCIM 2.0 User resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimUser {
    pub schemas: Vec<String>,
    pub id: Option<String>,
    #[serde(rename = "externalId", skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(rename = "userName")]
    pub user_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<ScimName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emails: Option<Vec<ScimEmail>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<ScimGroupRef>>,
    #[serde(rename = "meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScimMeta>,
}

/// SCIM name components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimName {
    #[serde(rename = "givenName", skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,
    #[serde(rename = "familyName", skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,
}

/// SCIM email entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimEmail {
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub email_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
}

/// SCIM group reference within a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimGroupRef {
    pub value: String,
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// SCIM resource metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimMeta {
    #[serde(rename = "resourceType", skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

impl ScimUser {
    /// Create a minimal SCIM user.
    pub fn new(user_name: String) -> Self {
        Self {
            schemas: vec![SCIM_USER_SCHEMA.to_string()],
            id: None,
            external_id: None,
            user_name,
            name: None,
            emails: None,
            active: Some(true),
            groups: None,
            meta: Some(ScimMeta {
                resource_type: Some("User".to_string()),
                created: None,
                last_modified: None,
                location: None,
            }),
        }
    }

    /// Check if the user is active.
    pub fn is_active(&self) -> bool {
        self.active.unwrap_or(true)
    }

    /// Deactivate the user (preferred over deletion per SCIM spec).
    pub fn deactivate(&mut self) {
        self.active = Some(false);
    }
}

// ============================================================================
// SCIM Group
// ============================================================================

/// SCIM 2.0 Group resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimGroup {
    pub schemas: Vec<String>,
    pub id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<ScimMemberRef>>,
    #[serde(rename = "meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScimMeta>,
}

/// SCIM member reference within a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimMemberRef {
    pub value: String,
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

impl ScimGroup {
    pub fn new(display_name: String) -> Self {
        Self {
            schemas: vec![SCIM_GROUP_SCHEMA.to_string()],
            id: None,
            display_name,
            members: None,
            meta: Some(ScimMeta {
                resource_type: Some("Group".to_string()),
                created: None,
                last_modified: None,
                location: None,
            }),
        }
    }
}

// ============================================================================
// SCIM Patch Operation
// ============================================================================

/// SCIM 2.0 PatchOp request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimPatchOp {
    pub schemas: Vec<String>,
    #[serde(rename = "Operations")]
    pub operations: Vec<ScimOperation>,
}

/// A single SCIM patch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimOperation {
    pub op: String,
    pub path: Option<String>,
    pub value: Option<serde_json::Value>,
}

// ============================================================================
// SCIM List Response
// ============================================================================

/// SCIM 2.0 list response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimListResponse<T: Serialize> {
    pub schemas: Vec<String>,
    #[serde(rename = "totalResults")]
    pub total_results: usize,
    #[serde(rename = "startIndex", skip_serializing_if = "Option::is_none")]
    pub start_index: Option<usize>,
    #[serde(rename = "itemsPerPage", skip_serializing_if = "Option::is_none")]
    pub items_per_page: Option<usize>,
    #[serde(rename = "Resources")]
    pub resources: Vec<T>,
}

impl<T: Serialize> ScimListResponse<T> {
    pub fn new(resources: Vec<T>) -> Self {
        let total = resources.len();
        Self {
            schemas: vec![SCIM_LIST_SCHEMA.to_string()],
            total_results: total,
            start_index: Some(1),
            items_per_page: Some(total),
            resources,
        }
    }
}

// ============================================================================
// SCIM Service Provider Config
// ============================================================================

/// SCIM 2.0 ServiceProviderConfig response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimServiceProviderConfig {
    pub schemas: Vec<String>,
    pub patch: ScimFeatureConfig,
    pub bulk: ScimFeatureConfig,
    pub filter: ScimFilterConfig,
    #[serde(rename = "changePassword")]
    pub change_password: ScimFeatureConfig,
    pub sort: ScimFeatureConfig,
    #[serde(rename = "etag")]
    pub etag: ScimFeatureConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimFeatureConfig {
    pub supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimFilterConfig {
    pub supported: bool,
    #[serde(rename = "maxResults")]
    pub max_results: u32,
}

impl Default for ScimServiceProviderConfig {
    fn default() -> Self {
        Self {
            schemas: vec![SCIM_SP_CONFIG_SCHEMA.to_string()],
            patch: ScimFeatureConfig { supported: true },
            bulk: ScimFeatureConfig { supported: false },
            filter: ScimFilterConfig {
                supported: true,
                max_results: 200,
            },
            change_password: ScimFeatureConfig { supported: false },
            sort: ScimFeatureConfig { supported: false },
            etag: ScimFeatureConfig { supported: false },
        }
    }
}

// ============================================================================
// SCIM Resource Type
// ============================================================================

/// SCIM 2.0 ResourceType descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimResourceType {
    pub schemas: Vec<String>,
    pub id: String,
    pub name: String,
    #[serde(rename = "endpoint")]
    pub endpoint: String,
    #[serde(rename = "schema")]
    pub schema: String,
}

impl ScimResourceType {
    pub fn user() -> Self {
        Self {
            schemas: vec![SCIM_RESOURCE_TYPE_SCHEMA.to_string()],
            id: "User".to_string(),
            name: "User".to_string(),
            endpoint: "/scim/v2/Users".to_string(),
            schema: SCIM_USER_SCHEMA.to_string(),
        }
    }

    pub fn group() -> Self {
        Self {
            schemas: vec![SCIM_RESOURCE_TYPE_SCHEMA.to_string()],
            id: "Group".to_string(),
            name: "Group".to_string(),
            endpoint: "/scim/v2/Groups".to_string(),
            schema: SCIM_GROUP_SCHEMA.to_string(),
        }
    }
}

// ============================================================================
// SCIM Provisioner (client-side for pulling users from IdP)
// ============================================================================

/// Result of a user sync operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub errors: Vec<SyncError>,
}

/// Per-user error during sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncError {
    pub user_id: String,
    pub error: String,
}

/// Client-side SCIM provisioner for pulling users from an IdP.
pub struct ScimProvisioner {
    scim_url: String,
    scim_token: String,
}

impl ScimProvisioner {
    pub fn new(scim_url: String, scim_token: String) -> Self {
        Self {
            scim_url,
            scim_token,
        }
    }

    /// Sync users from the IdP with per-user error isolation.
    pub async fn sync_users(&self) -> Result<SyncResult, String> {
        let url = format!("{}/Users", self.scim_url);
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .bearer_auth(&self.scim_token)
            .send()
            .await
            .map_err(|e| format!("SCIM request failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("SCIM returned status {}", resp.status()));
        }

        let list: ScimListResponse<ScimUser> = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse SCIM response: {}", e))?;

        let mut result = SyncResult {
            total: list.total_results,
            succeeded: 0,
            failed: 0,
            errors: vec![],
        };

        for user in list.resources {
            // Per-user error isolation: each user is processed independently
            match self.process_user(&user).await {
                Ok(_) => result.succeeded += 1,
                Err(e) => {
                    result.failed += 1;
                    result.errors.push(SyncError {
                        user_id: user.id.clone().unwrap_or_default(),
                        error: e,
                    });
                }
            }
        }

        Ok(result)
    }

    async fn process_user(&self, _user: &ScimUser) -> Result<(), String> {
        // Placeholder: integrate with user storage
        Ok(())
    }
}

// ============================================================================
// SCIM Filter Parser
// ============================================================================

/// Supported SCIM filter operators.
#[derive(Debug, Clone, PartialEq)]
pub enum ScimFilterOp {
    Eq,
    Ne,
    Co,
    Sw,
    Ew,
}

/// A parsed SCIM filter expression.
#[derive(Debug, Clone)]
pub struct ScimFilter {
    pub attribute: String,
    pub op: ScimFilterOp,
    pub value: String,
}

impl ScimFilter {
    /// Parse a SCIM filter string (e.g., `userName eq "john"`).
    pub fn parse(filter_str: &str) -> Result<Self, String> {
        let parts: Vec<&str> = filter_str.splitn(3, ' ').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid SCIM filter: {}", filter_str));
        }

        let attribute = parts[0].to_string();
        let op = match parts[1].to_lowercase().as_str() {
            "eq" => ScimFilterOp::Eq,
            "ne" => ScimFilterOp::Ne,
            "co" => ScimFilterOp::Co,
            "sw" => ScimFilterOp::Sw,
            "ew" => ScimFilterOp::Ew,
            other => return Err(format!("Unsupported SCIM filter op: {}", other)),
        };
        let value = parts[2].trim_matches('"').to_string();

        Ok(Self {
            attribute,
            op,
            value,
        })
    }

    /// Check if a user matches this filter.
    pub fn matches_user(&self, user: &ScimUser) -> bool {
        let attr_value = match self.attribute.as_str() {
            "userName" => Some(user.user_name.clone()),
            "id" => user.id.clone(),
            "externalId" => user.external_id.clone(),
            _ => None,
        };

        let attr_value = match attr_value {
            Some(v) => v.to_lowercase(),
            None => return false,
        };

        let filter_val = self.value.to_lowercase();

        match self.op {
            ScimFilterOp::Eq => attr_value == filter_val,
            ScimFilterOp::Ne => attr_value != filter_val,
            ScimFilterOp::Co => attr_value.contains(&filter_val),
            ScimFilterOp::Sw => attr_value.starts_with(&filter_val),
            ScimFilterOp::Ew => attr_value.ends_with(&filter_val),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scim_user_new() {
        let user = ScimUser::new("john@example.com".to_string());
        assert_eq!(user.user_name, "john@example.com");
        assert!(user.is_active());
    }

    #[test]
    fn test_scim_user_deactivate() {
        let mut user = ScimUser::new("john@example.com".to_string());
        assert!(user.is_active());
        user.deactivate();
        assert!(!user.is_active());
    }

    #[test]
    fn test_scim_group_new() {
        let group = ScimGroup::new("Admins".to_string());
        assert_eq!(group.display_name, "Admins");
    }

    #[test]
    fn test_scim_error_format() {
        let err = ScimError::new("404", "User not found", Some("noTarget"));
        assert_eq!(err.status, "404");
        assert_eq!(err.scim_type, Some("noTarget".to_string()));
    }

    #[test]
    fn test_scim_filter_eq() {
        let filter = ScimFilter::parse(r#"userName eq "john""#).unwrap();
        assert_eq!(filter.attribute, "userName");
        assert_eq!(filter.op, ScimFilterOp::Eq);
        assert_eq!(filter.value, "john");

        let mut user = ScimUser::new("john@example.com".to_string());
        user.id = Some("123".to_string());
        assert!(!filter.matches_user(&user)); // "john@example.com" != "john"
    }

    #[test]
    fn test_scim_filter_co() {
        let filter = ScimFilter::parse(r#"userName co "john""#).unwrap();
        let user = ScimUser::new("john@example.com".to_string());
        assert!(filter.matches_user(&user));
    }

    #[test]
    fn test_scim_filter_sw() {
        let filter = ScimFilter::parse(r#"userName sw "john""#).unwrap();
        let user = ScimUser::new("john@example.com".to_string());
        assert!(filter.matches_user(&user));
    }

    #[test]
    fn test_scim_list_response() {
        let users = vec![
            ScimUser::new("a@test.com".to_string()),
            ScimUser::new("b@test.com".to_string()),
        ];
        let resp = ScimListResponse::new(users);
        assert_eq!(resp.total_results, 2);
        assert_eq!(resp.resources.len(), 2);
    }

    #[test]
    fn test_scim_service_provider_config_default() {
        let config = ScimServiceProviderConfig::default();
        assert!(config.patch.supported);
        assert!(!config.bulk.supported);
        assert!(config.filter.supported);
        assert_eq!(config.filter.max_results, 200);
    }

    #[test]
    fn test_scim_resource_types() {
        let user_type = ScimResourceType::user();
        assert_eq!(user_type.name, "User");
        assert_eq!(user_type.endpoint, "/scim/v2/Users");

        let group_type = ScimResourceType::group();
        assert_eq!(group_type.name, "Group");
        assert_eq!(group_type.endpoint, "/scim/v2/Groups");
    }
}
