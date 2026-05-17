//! SCIM 2.0 server-side endpoints (RFC-0949).
//!
//! Provides REST endpoints for SCIM user and group management.
//! Designed to be mounted alongside the admin API.

use super::scim::{
    ScimError, ScimGroup, ScimListResponse, ScimOperation, ScimPatchOp, ScimResourceType,
    ScimServiceProviderConfig, ScimUser,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// In-memory SCIM resource store.
pub struct ScimStore {
    users: Arc<RwLock<HashMap<String, ScimUser>>>,
    groups: Arc<RwLock<HashMap<String, ScimGroup>>>,
}

impl ScimStore {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            groups: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for ScimStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SCIM User Operations
// ============================================================================

/// List SCIM users with optional pagination.
/// start_index: 1-based offset (default 1)
/// count: max results per page (default 100)
pub fn list_users(
    store: &ScimStore,
    start_index: Option<usize>,
    count: Option<usize>,
) -> ScimListResponse<ScimUser> {
    let users = match store.users.read() {
        Ok(u) => u,
        Err(_) => return ScimListResponse::new(vec![]),
    };
    let start = start_index.unwrap_or(1).saturating_sub(1); // Convert to 0-based
    let limit = count.unwrap_or(100);
    let list: Vec<ScimUser> = users.values().skip(start).cloned().take(limit).collect();
    ScimListResponse::new(list)
}

/// Get a SCIM user by ID.
pub fn get_user(store: &ScimStore, id: &str) -> Result<ScimUser, ScimError> {
    let users = store
        .users
        .read()
        .map_err(|e| ScimError::new("500", &format!("Lock error: {}", e), None))?;
    users
        .get(id)
        .cloned()
        .ok_or_else(|| ScimError::new("404", &format!("User {} not found", id), Some("noTarget")))
}

/// Create a new SCIM user.
pub fn create_user(store: &ScimStore, mut user: ScimUser) -> Result<ScimUser, ScimError> {
    let id = Uuid::new_v4().to_string();
    user.id = Some(id.clone());
    let mut users = store
        .users
        .write()
        .map_err(|e| ScimError::new("500", &format!("Lock error: {}", e), None))?;

    // Check for duplicate userName
    for existing in users.values() {
        if existing.user_name == user.user_name {
            return Err(ScimError::new(
                "409",
                "User with this userName already exists",
                Some("uniqueness"),
            ));
        }
    }

    users.insert(id, user.clone());
    Ok(user)
}

/// Replace a SCIM user (PUT).
pub fn replace_user(
    store: &ScimStore,
    id: &str,
    mut user: ScimUser,
) -> Result<ScimUser, ScimError> {
    let mut users = store
        .users
        .write()
        .map_err(|e| ScimError::new("500", &format!("Lock error: {}", e), None))?;

    if !users.contains_key(id) {
        return Err(ScimError::new(
            "404",
            &format!("User {} not found", id),
            Some("noTarget"),
        ));
    }

    user.id = Some(id.to_string());
    users.insert(id.to_string(), user.clone());
    Ok(user)
}

/// Patch a SCIM user (PATCH).
pub fn patch_user(store: &ScimStore, id: &str, patch: ScimPatchOp) -> Result<ScimUser, ScimError> {
    let mut users = store
        .users
        .write()
        .map_err(|e| ScimError::new("500", &format!("Lock error: {}", e), None))?;

    let user = users.get_mut(id).ok_or_else(|| {
        ScimError::new("404", &format!("User {} not found", id), Some("noTarget"))
    })?;

    for op in &patch.operations {
        match op.op.to_lowercase().as_str() {
            "replace" => apply_replace(user, op)?,
            "add" => apply_add(user, op)?,
            "remove" => apply_remove(user, op)?,
            other => {
                return Err(ScimError::new(
                    "400",
                    &format!("Unsupported patch operation: {}", other),
                    Some("invalidFilter"),
                ))
            }
        }
    }

    Ok(user.clone())
}

/// Delete a SCIM user (deactivation preferred).
pub fn delete_user(store: &ScimStore, id: &str) -> Result<(), ScimError> {
    let mut users = store
        .users
        .write()
        .map_err(|e| ScimError::new("500", &format!("Lock error: {}", e), None))?;

    // Per SCIM spec: deactivate rather than delete
    if let Some(user) = users.get_mut(id) {
        user.deactivate();
        Ok(())
    } else {
        Err(ScimError::new(
            "404",
            &format!("User {} not found", id),
            Some("noTarget"),
        ))
    }
}

// ============================================================================
// SCIM Group Operations
// ============================================================================

/// List SCIM groups with optional pagination.
/// start_index: 1-based offset (default 1)
/// count: max results per page (default 100)
pub fn list_groups(
    store: &ScimStore,
    start_index: Option<usize>,
    count: Option<usize>,
) -> ScimListResponse<ScimGroup> {
    let groups = match store.groups.read() {
        Ok(g) => g,
        Err(_) => return ScimListResponse::new(vec![]),
    };
    let start = start_index.unwrap_or(1).saturating_sub(1); // Convert to 0-based
    let limit = count.unwrap_or(100);
    let list: Vec<ScimGroup> = groups.values().skip(start).cloned().take(limit).collect();
    ScimListResponse::new(list)
}

/// Create a new SCIM group.
pub fn create_group(store: &ScimStore, mut group: ScimGroup) -> Result<ScimGroup, ScimError> {
    let id = Uuid::new_v4().to_string();
    group.id = Some(id.clone());
    let mut groups = store
        .groups
        .write()
        .map_err(|e| ScimError::new("500", &format!("Lock error: {}", e), None))?;

    groups.insert(id, group.clone());
    Ok(group)
}

// ============================================================================
// SCIM Service Provider Config / Resource Types
// ============================================================================

/// Get SCIM service provider configuration.
pub fn get_service_provider_config() -> ScimServiceProviderConfig {
    ScimServiceProviderConfig::default()
}

/// Get SCIM resource types.
pub fn get_resource_types() -> Vec<ScimResourceType> {
    vec![ScimResourceType::user(), ScimResourceType::group()]
}

// ============================================================================
// Patch Operation Helpers
// ============================================================================

fn apply_replace(user: &mut ScimUser, op: &ScimOperation) -> Result<(), ScimError> {
    match op.path.as_deref() {
        Some("active") => {
            if let Some(val) = &op.value {
                user.active = val.as_bool();
            }
        }
        Some("userName") => {
            if let Some(val) = &op.value {
                if let Some(s) = val.as_str() {
                    user.user_name = s.to_string();
                }
            }
        }
        Some("name.givenName") => {
            if let Some(val) = &op.value {
                if let Some(s) = val.as_str() {
                    let name = user.name.get_or_insert(super::scim::ScimName {
                        given_name: None,
                        family_name: None,
                        formatted: None,
                    });
                    name.given_name = Some(s.to_string());
                }
            }
        }
        Some("name.familyName") => {
            if let Some(val) = &op.value {
                if let Some(s) = val.as_str() {
                    let name = user.name.get_or_insert(super::scim::ScimName {
                        given_name: None,
                        family_name: None,
                        formatted: None,
                    });
                    name.family_name = Some(s.to_string());
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn apply_add(_user: &mut ScimUser, _op: &ScimOperation) -> Result<(), ScimError> {
    // Placeholder for add operations
    Ok(())
}

fn apply_remove(_user: &mut ScimUser, _op: &ScimOperation) -> Result<(), ScimError> {
    // Placeholder for remove operations
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_user() {
        let store = ScimStore::new();
        let user = ScimUser::new("alice@example.com".to_string());
        let created = create_user(&store, user).unwrap();
        let id = created.id.as_ref().unwrap();
        let fetched = get_user(&store, id).unwrap();
        assert_eq!(fetched.user_name, "alice@example.com");
    }

    #[test]
    fn test_list_users_empty() {
        let store = ScimStore::new();
        let resp = list_users(&store, None, None);
        assert_eq!(resp.total_results, 0);
    }

    #[test]
    fn test_list_users_after_create() {
        let store = ScimStore::new();
        create_user(&store, ScimUser::new("a@test.com".to_string())).unwrap();
        create_user(&store, ScimUser::new("b@test.com".to_string())).unwrap();
        let resp = list_users(&store, None, None);
        assert_eq!(resp.total_results, 2);
    }

    #[test]
    fn test_replace_user() {
        let store = ScimStore::new();
        let created = create_user(&store, ScimUser::new("old@test.com".to_string())).unwrap();
        let id = created.id.as_ref().unwrap();
        let mut updated = ScimUser::new("new@test.com".to_string());
        updated.name = Some(super::super::scim::ScimName {
            given_name: Some("New".to_string()),
            family_name: None,
            formatted: None,
        });
        let result = replace_user(&store, id, updated).unwrap();
        assert_eq!(result.user_name, "new@test.com");
    }

    #[test]
    fn test_patch_user_deactivate() {
        let store = ScimStore::new();
        let created = create_user(&store, ScimUser::new("deact@test.com".to_string())).unwrap();
        let id = created.id.as_ref().unwrap();
        let patch = ScimPatchOp {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".to_string()],
            operations: vec![ScimOperation {
                op: "Replace".to_string(),
                path: Some("active".to_string()),
                value: Some(serde_json::Value::Bool(false)),
            }],
        };
        let result = patch_user(&store, id, patch).unwrap();
        assert_eq!(result.active, Some(false));
    }

    #[test]
    fn test_delete_user_deactivates() {
        let store = ScimStore::new();
        let created = create_user(&store, ScimUser::new("del@test.com".to_string())).unwrap();
        let id = created.id.as_ref().unwrap();
        delete_user(&store, id).unwrap();
        let user = get_user(&store, id).unwrap();
        assert!(!user.is_active()); // deactivated, not deleted
    }

    #[test]
    fn test_get_user_not_found() {
        let store = ScimStore::new();
        let result = get_user(&store, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_duplicate_username() {
        let store = ScimStore::new();
        create_user(&store, ScimUser::new("dup@test.com".to_string())).unwrap();
        let result = create_user(&store, ScimUser::new("dup@test.com".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_and_list_groups() {
        let store = ScimStore::new();
        create_group(&store, ScimGroup::new("Admins".to_string())).unwrap();
        create_group(&store, ScimGroup::new("Users".to_string())).unwrap();
        let resp = list_groups(&store, None, None);
        assert_eq!(resp.total_results, 2);
    }

    #[test]
    fn test_service_provider_config() {
        let config = get_service_provider_config();
        assert!(config.patch.supported);
        assert!(config.filter.supported);
    }

    #[test]
    fn test_resource_types() {
        let types = get_resource_types();
        assert_eq!(types.len(), 2);
        assert_eq!(types[0].name, "User");
        assert_eq!(types[1].name, "Group");
    }
}
