pub mod storage;
pub mod template;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

use storage::PromptStorage;
#[cfg(test)]
use template::TemplateEngine;
use template::TemplateError;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum PromptError {
    #[error("Prompt not found: {0}")]
    PromptNotFound(String),
    #[error("Prompt version not found: {0}@{1}")]
    PromptVersionNotFound(String, String),
    #[error("Template render error: {0}")]
    TemplateRenderError(String),
    #[error("Template variable missing: {0}")]
    VariableMissing(String),
    #[error("A/B test not found: {0}")]
    AbTestNotFound(String),
    #[error("A/B test ended for {0}, using version_a fallback")]
    AbTestEnded(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Cache timeout: {0}")]
    CacheTimeout(String),
}

impl From<TemplateError> for PromptError {
    fn from(e: TemplateError) -> Self {
        match e {
            TemplateError::VariableMissing(v) => PromptError::VariableMissing(v),
            TemplateError::RenderError(msg) => PromptError::TemplateRenderError(msg),
        }
    }
}

impl From<storage::StorageError> for PromptError {
    fn from(e: storage::StorageError) -> Self {
        match e {
            storage::StorageError::PromptNotFound(id) => PromptError::PromptNotFound(id),
            storage::StorageError::PromptVersionNotFound(id, ver) => {
                PromptError::PromptVersionNotFound(id, ver)
            }
            storage::StorageError::AbTestNotFound(id) => PromptError::AbTestNotFound(id),
            storage::StorageError::Backend(msg) => PromptError::StorageError(msg),
        }
    }
}

// ============================================================================
// Prompt Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub version: String,
    pub team_id: Option<String>,
    pub template: String,
    #[serde(default)]
    pub defaults: HashMap<String, String>,
    pub model: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub prompt_id: String,
    pub version: String,
    pub template: String,
    pub changelog: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptFilter {
    pub team_id: Option<String>,
    pub name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub model: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFields {
    pub prompt_id: Option<String>,
    pub prompt_variables: Option<HashMap<String, String>>,
}

// ============================================================================
// A/B Testing
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTest {
    pub prompt_id: String,
    pub version_a: String,
    pub version_b: String,
    pub weight_b: f64,
    pub start_at: DateTime<Utc>,
    pub end_at: Option<DateTime<Utc>>,
    pub metrics: AbTestMetrics,
}

impl AbTest {
    /// Select version based on deterministic hashing of request_id.
    /// request_id source priority: API key ID > X-Request-Id > generated UUID.
    pub fn select_version(&self, request_id: &str) -> &str {
        // Check if test has ended
        if let Some(end_at) = self.end_at {
            if Utc::now() > end_at {
                return &self.version_a;
            }
        }

        // Deterministic hash
        let hash = simple_hash(request_id);
        if (hash % 1000) as f64 / 1000.0 < self.weight_b {
            &self.version_b
        } else {
            &self.version_a
        }
    }
}

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestMetrics {
    pub requests_a: u64,
    pub requests_b: u64,
    pub avg_latency_a: f64,
    pub avg_latency_b: f64,
    pub error_rate_a: f64,
    pub error_rate_b: f64,
    pub avg_tokens_a: u64,
    pub avg_tokens_b: u64,
}

impl Default for AbTestMetrics {
    fn default() -> Self {
        Self {
            requests_a: 0,
            requests_b: 0,
            avg_latency_a: 0.0,
            avg_latency_b: 0.0,
            error_rate_a: 0.0,
            error_rate_b: 0.0,
            avg_tokens_a: 0,
            avg_tokens_b: 0,
        }
    }
}

// ============================================================================
// Prompt Registry
// ============================================================================

/// Thread-safe prompt registry shared across proxy workers via Arc.
pub struct PromptRegistry {
    storage: PromptStorage,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self {
            storage: PromptStorage::new(),
        }
    }

    pub fn get(&self, prompt_id: &str) -> Result<PromptTemplate, PromptError> {
        Ok(self.storage.get_prompt(prompt_id)?)
    }

    pub fn get_version(
        &self,
        prompt_id: &str,
        version: &str,
    ) -> Result<PromptTemplate, PromptError> {
        let ver = self.storage.get_version(prompt_id, version)?;
        let mut prompt = self.storage.get_prompt(prompt_id)?;
        prompt.version = ver.version;
        prompt.template = ver.template;
        Ok(prompt)
    }

    pub fn create(&mut self, prompt: PromptTemplate) -> Result<String, PromptError> {
        let id = prompt.id.clone();
        let version = PromptVersion {
            prompt_id: id.clone(),
            version: prompt.version.clone(),
            template: prompt.template.clone(),
            changelog: "Initial version".to_string(),
            active: true,
            created_at: Utc::now(),
            created_by: prompt.created_by.clone(),
        };
        self.storage.store_prompt(prompt)?;
        self.storage.store_version(version)?;
        Ok(id)
    }

    pub fn update(
        &mut self,
        prompt_id: &str,
        template: &str,
        changelog: &str,
        created_by: &str,
    ) -> Result<String, PromptError> {
        let mut prompt = self.storage.get_prompt(prompt_id)?;

        // Bump minor version
        let new_version = bump_version(&prompt.version);

        let version = PromptVersion {
            prompt_id: prompt_id.to_string(),
            version: new_version.clone(),
            template: template.to_string(),
            changelog: changelog.to_string(),
            active: true,
            created_at: Utc::now(),
            created_by: created_by.to_string(),
        };

        // Deactivate previous versions
        self.storage.activate_version(prompt_id, &new_version)?;
        self.storage.store_version(version)?;

        // Update prompt
        prompt.template = template.to_string();
        prompt.version = new_version.clone();
        prompt.updated_at = Utc::now();
        self.storage.store_prompt(prompt)?;

        Ok(new_version)
    }

    pub fn rollback(&mut self, prompt_id: &str, version: &str) -> Result<(), PromptError> {
        self.storage.activate_version(prompt_id, version)?;
        Ok(())
    }

    pub fn delete(&mut self, prompt_id: &str) -> Result<(), PromptError> {
        self.storage.delete_prompt(prompt_id)?;
        Ok(())
    }

    pub fn list(&self, filter: &PromptFilter) -> Vec<PromptTemplate> {
        self.storage.list_prompts(filter)
    }

    /// Resolve prompt with A/B testing support.
    /// If A/B test exists and is active, selects version deterministically.
    /// If A/B test ended, falls back to version_a.
    pub fn resolve(
        &mut self,
        prompt_id: &str,
        request_id: &str,
    ) -> Result<PromptTemplate, PromptError> {
        // Check for A/B test
        if let Ok(test) = self.storage.get_ab_test(prompt_id) {
            let version = test.select_version(request_id).to_string();
            let ended = test.end_at.map(|end| Utc::now() > end).unwrap_or(false);

            if ended {
                return Err(PromptError::AbTestEnded(prompt_id.to_string()));
            }

            return self.get_version(prompt_id, &version);
        }

        // No A/B test — return active version
        let ver = self.storage.get_active_version(prompt_id)?;
        let mut prompt = self.storage.get_prompt(prompt_id)?;
        prompt.version = ver.version;
        prompt.template = ver.template;
        Ok(prompt)
    }

    pub fn set_ab_test(&mut self, test: AbTest) {
        self.storage.set_ab_test(test);
    }

    pub fn remove_ab_test(&mut self, prompt_id: &str) -> Option<AbTest> {
        self.storage.remove_ab_test(prompt_id)
    }

    pub fn get_ab_test(&self, prompt_id: &str) -> Result<&AbTest, PromptError> {
        Ok(self.storage.get_ab_test(prompt_id)?)
    }
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared prompt registry type (Arc<RwLock<PromptRegistry>>)
pub type SharedPromptRegistry = Arc<RwLock<PromptRegistry>>;

fn bump_version(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 3 {
        if let Ok(minor) = parts[1].parse::<u32>() {
            return format!("{}.{}.{}", parts[0], minor + 1, parts[2]);
        }
    }
    format!("{}.0", version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_prompt(id: &str) -> PromptTemplate {
        PromptTemplate {
            id: id.to_string(),
            name: format!("Test {}", id),
            version: "1.0.0".to_string(),
            team_id: None,
            template: "Hello {{name}}".to_string(),
            defaults: HashMap::from([("name".to_string(), "World".to_string())]),
            model: None,
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: "test".to_string(),
        }
    }

    #[test]
    fn test_registry_create_and_get() {
        let mut registry = PromptRegistry::new();
        let prompt = make_test_prompt("p1");
        registry.create(prompt).unwrap();
        let result = registry.get("p1").unwrap();
        assert_eq!(result.name, "Test p1");
    }

    #[test]
    fn test_registry_resolve_with_defaults() {
        let mut registry = PromptRegistry::new();
        let prompt = make_test_prompt("p1");
        registry.create(prompt).unwrap();
        let result = registry.resolve("p1", "req-1").unwrap();
        let rendered =
            TemplateEngine::render(&result.template, &HashMap::new(), &result.defaults).unwrap();
        assert_eq!(rendered, "Hello World");
    }

    #[test]
    fn test_ab_test_deterministic() {
        let test = AbTest {
            prompt_id: "test".to_string(),
            version_a: "1.0.0".to_string(),
            version_b: "2.0.0".to_string(),
            weight_b: 0.5,
            start_at: Utc::now(),
            end_at: None,
            metrics: AbTestMetrics::default(),
        };
        // Same request_id always gets same version
        let v1 = test.select_version("req-123");
        let v2 = test.select_version("req-123");
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_ab_test_weight_boundaries() {
        let mut test = AbTest {
            prompt_id: "test".to_string(),
            version_a: "1.0.0".to_string(),
            version_b: "2.0.0".to_string(),
            weight_b: 0.0,
            start_at: Utc::now(),
            end_at: None,
            metrics: AbTestMetrics::default(),
        };
        assert_eq!(test.select_version("any"), "1.0.0");

        test.weight_b = 1.0;
        assert_eq!(test.select_version("any"), "2.0.0");
    }

    #[test]
    fn test_ab_test_ended_fallback() {
        let test = AbTest {
            prompt_id: "test".to_string(),
            version_a: "1.0.0".to_string(),
            version_b: "2.0.0".to_string(),
            weight_b: 1.0,
            start_at: Utc::now() - chrono::Duration::hours(2),
            end_at: Some(Utc::now() - chrono::Duration::hours(1)),
            metrics: AbTestMetrics::default(),
        };
        // Ended test should fallback to version_a
        assert_eq!(test.select_version("any"), "1.0.0");
    }

    #[test]
    fn test_bump_version() {
        assert_eq!(bump_version("1.0.0"), "1.1.0");
        assert_eq!(bump_version("2.3.5"), "2.4.5");
    }
}
