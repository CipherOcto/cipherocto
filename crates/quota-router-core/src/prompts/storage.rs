use std::collections::HashMap;
use thiserror::Error;

use super::{AbTest, PromptFilter, PromptTemplate, PromptVersion};

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Prompt not found: {0}")]
    PromptNotFound(String),
    #[error("Prompt version not found: {0}@{1}")]
    PromptVersionNotFound(String, String),
    #[error("A/B test not found: {0}")]
    AbTestNotFound(String),
    #[error("Storage backend error: {0}")]
    Backend(String),
}

/// In-memory prompt storage (stoolap-backed in production).
/// For Phase 1, this is a HashMap-based in-memory store.
pub struct PromptStorage {
    prompts: HashMap<String, PromptTemplate>,
    versions: HashMap<String, Vec<PromptVersion>>,
    ab_tests: HashMap<String, AbTest>,
}

impl PromptStorage {
    pub fn new() -> Self {
        Self {
            prompts: HashMap::new(),
            versions: HashMap::new(),
            ab_tests: HashMap::new(),
        }
    }

    pub fn get_prompt(&self, prompt_id: &str) -> Result<PromptTemplate, StorageError> {
        self.prompts
            .get(prompt_id)
            .cloned()
            .ok_or_else(|| StorageError::PromptNotFound(prompt_id.to_string()))
    }

    pub fn get_version(
        &self,
        prompt_id: &str,
        version: &str,
    ) -> Result<PromptVersion, StorageError> {
        let versions = self
            .versions
            .get(prompt_id)
            .ok_or_else(|| StorageError::PromptNotFound(prompt_id.to_string()))?;

        versions
            .iter()
            .find(|v| v.version == version)
            .cloned()
            .ok_or_else(|| {
                StorageError::PromptVersionNotFound(prompt_id.to_string(), version.to_string())
            })
    }

    pub fn get_active_version(&self, prompt_id: &str) -> Result<PromptVersion, StorageError> {
        let versions = self
            .versions
            .get(prompt_id)
            .ok_or_else(|| StorageError::PromptNotFound(prompt_id.to_string()))?;

        versions.iter().find(|v| v.active).cloned().ok_or_else(|| {
            StorageError::PromptNotFound(format!("{} (no active version)", prompt_id))
        })
    }

    pub fn store_prompt(&mut self, prompt: PromptTemplate) -> Result<String, StorageError> {
        let id = prompt.id.clone();
        self.prompts.insert(id.clone(), prompt);
        Ok(id)
    }

    pub fn store_version(&mut self, version: PromptVersion) -> Result<(), StorageError> {
        let versions = self.versions.entry(version.prompt_id.clone()).or_default();
        versions.push(version);
        Ok(())
    }

    pub fn list_prompts(&self, filter: &PromptFilter) -> Vec<PromptTemplate> {
        let mut results: Vec<PromptTemplate> = self
            .prompts
            .values()
            .filter(|p| {
                if let Some(ref team_id) = filter.team_id {
                    if p.team_id.as_ref() != Some(team_id) {
                        return false;
                    }
                }
                if let Some(ref name) = filter.name {
                    if !p.name.contains(name) {
                        return false;
                    }
                }
                if let Some(ref tags) = filter.tags {
                    if !tags.iter().all(|t| p.tags.contains(t)) {
                        return false;
                    }
                }
                if let Some(ref model) = filter.model {
                    if p.model.as_ref() != Some(model) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by created_at descending
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply pagination
        let offset = filter.offset.unwrap_or(0) as usize;
        let limit = filter.limit.unwrap_or(u32::MAX) as usize;
        results.into_iter().skip(offset).take(limit).collect()
    }

    pub fn delete_prompt(&mut self, prompt_id: &str) -> Result<(), StorageError> {
        if self.prompts.remove(prompt_id).is_some() {
            self.versions.remove(prompt_id);
            self.ab_tests.remove(prompt_id);
            Ok(())
        } else {
            Err(StorageError::PromptNotFound(prompt_id.to_string()))
        }
    }

    pub fn activate_version(&mut self, prompt_id: &str, version: &str) -> Result<(), StorageError> {
        let versions = self
            .versions
            .get_mut(prompt_id)
            .ok_or_else(|| StorageError::PromptNotFound(prompt_id.to_string()))?;

        let found = versions.iter().any(|v| v.version == version);
        if !found {
            return Err(StorageError::PromptVersionNotFound(
                prompt_id.to_string(),
                version.to_string(),
            ));
        }

        for v in versions.iter_mut() {
            v.active = v.version == version;
        }
        Ok(())
    }

    pub fn get_ab_test(&self, prompt_id: &str) -> Result<&AbTest, StorageError> {
        self.ab_tests
            .get(prompt_id)
            .ok_or_else(|| StorageError::AbTestNotFound(prompt_id.to_string()))
    }

    pub fn set_ab_test(&mut self, test: AbTest) {
        self.ab_tests.insert(test.prompt_id.clone(), test);
    }

    pub fn remove_ab_test(&mut self, prompt_id: &str) -> Option<AbTest> {
        self.ab_tests.remove(prompt_id)
    }
}

impl Default for PromptStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_prompt(id: &str, name: &str) -> PromptTemplate {
        PromptTemplate {
            id: id.to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            team_id: None,
            template: "Hello {{name}}".to_string(),
            defaults: HashMap::new(),
            model: None,
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: "test".to_string(),
        }
    }

    #[test]
    fn test_store_and_get_prompt() {
        let mut storage = PromptStorage::new();
        let prompt = make_prompt("p1", "Test");
        storage.store_prompt(prompt).unwrap();
        let result = storage.get_prompt("p1").unwrap();
        assert_eq!(result.name, "Test");
    }

    #[test]
    fn test_get_prompt_not_found() {
        let storage = PromptStorage::new();
        assert!(storage.get_prompt("nonexistent").is_err());
    }

    #[test]
    fn test_list_prompts_with_filter() {
        let mut storage = PromptStorage::new();
        storage.store_prompt(make_prompt("p1", "Alpha")).unwrap();
        storage.store_prompt(make_prompt("p2", "Beta")).unwrap();

        let filter = PromptFilter {
            team_id: None,
            name: Some("Alpha".to_string()),
            tags: None,
            model: None,
            limit: None,
            offset: None,
        };
        let results = storage.list_prompts(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alpha");
    }

    #[test]
    fn test_delete_prompt() {
        let mut storage = PromptStorage::new();
        storage.store_prompt(make_prompt("p1", "Test")).unwrap();
        storage.delete_prompt("p1").unwrap();
        assert!(storage.get_prompt("p1").is_err());
    }
}
