use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use super::{
    AbTest, AbTestMetrics, AbTestMetricsAtomic, PromptFilter, PromptTemplate, PromptVersion,
};

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
    /// Persisted A/B metric snapshots. The `AbTest::metrics` field is
    /// the live in-memory atomic state; this map holds the durable
    /// snapshot last written via `persist_ab_test_metrics`.
    ab_test_metrics: HashMap<String, AbTestMetrics>,
}

impl PromptStorage {
    pub fn new() -> Self {
        Self {
            prompts: HashMap::new(),
            versions: HashMap::new(),
            ab_tests: HashMap::new(),
            ab_test_metrics: HashMap::new(),
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
        results.sort_by_key(|b| std::cmp::Reverse(b.created_at));

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

    pub fn list_versions(&self, prompt_id: &str) -> Result<Vec<PromptVersion>, StorageError> {
        self.versions
            .get(prompt_id)
            .cloned()
            .ok_or_else(|| StorageError::PromptNotFound(prompt_id.to_string()))
    }

    /// Persist the current A/B metrics snapshot for `prompt_id`. In the
    /// in-memory substrate this writes into the `ab_test_metrics` map; in
    /// the stoolap-backed backend (future work) it writes a row into the
    /// `ab_test_metrics` table.
    pub fn persist_ab_test_metrics(
        &mut self,
        prompt_id: &str,
        metrics: &AbTestMetricsAtomic,
    ) -> Result<(), StorageError> {
        self.ab_test_metrics
            .insert(prompt_id.to_string(), metrics.snapshot());
        Ok(())
    }

    /// Persist from a pre-built snapshot. Used when the caller has
    /// already taken the snapshot on another thread (avoiding the cost
    /// of a second atomic read).
    pub fn persist_ab_test_metrics_snapshot(
        &mut self,
        prompt_id: &str,
        snapshot: AbTestMetrics,
    ) -> Result<(), StorageError> {
        self.ab_test_metrics.insert(prompt_id.to_string(), snapshot);
        Ok(())
    }

    /// Read the last persisted A/B metrics snapshot. `None` if no
    /// snapshot has been written for this prompt.
    pub fn get_ab_test_metrics(&self, prompt_id: &str) -> Option<AbTestMetrics> {
        self.ab_test_metrics.get(prompt_id).cloned()
    }

    /// Return the live atomic metrics holder for an A/B test. `None` if
    /// no test exists for the prompt.
    pub fn live_ab_test_metrics(&self, prompt_id: &str) -> Option<Arc<AbTestMetricsAtomic>> {
        self.ab_tests.get(prompt_id).map(|t| t.metrics.clone())
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

    #[test]
    fn test_persist_ab_test_metrics() {
        use super::super::ab_test::AbTestMetrics;
        use chrono::Utc;

        let mut storage = PromptStorage::new();
        let prompt = make_prompt("p1", "Test");
        storage.store_prompt(prompt).unwrap();
        let test = super::super::AbTest::new(
            "p1".into(),
            "1.0.0".into(),
            "2.0.0".into(),
            0.5,
            Utc::now(),
            None,
            &AbTestMetrics::default(),
        );
        storage.set_ab_test(test);

        // Pre-populate atomic metrics then persist
        let atomic = storage.live_ab_test_metrics("p1").unwrap();
        atomic.inc_requests_a();
        atomic.inc_requests_a();
        atomic.inc_requests_b();
        storage.persist_ab_test_metrics("p1", &atomic).unwrap();

        let snap = storage.get_ab_test_metrics("p1").unwrap();
        assert_eq!(snap.requests_a, 2);
        assert_eq!(snap.requests_b, 1);

        // Snapshot-only path
        let snap = AbTestMetrics {
            requests_a: 10,
            requests_b: 20,
            ..AbTestMetrics::default()
        };
        storage
            .persist_ab_test_metrics_snapshot("p1", snap.clone())
            .unwrap();
        let snap2 = storage.get_ab_test_metrics("p1").unwrap();
        assert_eq!(snap2.requests_a, 10);
        assert_eq!(snap2.requests_b, 20);

        // Unknown prompt returns None
        assert!(storage.get_ab_test_metrics("missing").is_none());
    }

    #[test]
    fn test_live_ab_test_metrics_missing_returns_none() {
        let storage = PromptStorage::new();
        assert!(storage.live_ab_test_metrics("missing").is_none());
    }
}
