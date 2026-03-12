// Router module for quota-router-core
// Provides routing logic for multi-provider load balancing

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::providers::Provider;

/// Routing strategy for selecting providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RoutingStrategy {
    /// Simple round-robin or first-available
    #[default]
    Simple,
    /// Select provider with least active requests
    LeastBusy,
    /// Select provider with lowest latency
    LatencyBased,
    /// Select provider with lowest cost
    CostBased,
}

/// Model definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Model name (e.g., "gpt-4", "claude-3-opus")
    pub name: String,
    /// Provider name this model belongs to
    pub provider: String,
    /// Price per 1K input tokens
    pub input_cost_per_1k: Option<f64>,
    /// Price per 1K output tokens
    pub output_cost_per_1k: Option<f64>,
    /// Whether this model supports streaming
    pub supports_streaming: bool,
}

impl Model {
    pub fn new(name: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provider: provider.into(),
            input_cost_per_1k: None,
            output_cost_per_1k: None,
            supports_streaming: true,
        }
    }

    pub fn with_costs(mut self, input: f64, output: f64) -> Self {
        self.input_cost_per_1k = Some(input);
        self.output_cost_per_1k = Some(output);
        self
    }
}

/// Router configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// List of available models
    #[serde(default)]
    pub model_list: Vec<Model>,
    /// Routing strategy
    #[serde(default)]
    pub routing_strategy: RoutingStrategy,
    /// Number of fallbacks to try
    #[serde(default = "default_fallbacks")]
    pub num_fallbacks: usize,
    /// Enable response caching
    #[serde(default)]
    pub cache: bool,
}

fn default_fallbacks() -> usize {
    2
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            model_list: vec![],
            routing_strategy: RoutingStrategy::Simple,
            num_fallbacks: 2,
            cache: false,
        }
    }
}

/// Router for managing multi-provider requests
#[derive(Debug, Clone)]
pub struct Router {
    config: RouterConfig,
    /// Map of provider name to provider
    providers: HashMap<String, Provider>,
    /// Current index for simple routing
    round_robin_index: usize,
}

impl Router {
    /// Create a new Router with the given config and providers
    pub fn new(config: RouterConfig, providers: Vec<Provider>) -> Self {
        let providers_map: HashMap<String, Provider> =
            providers.into_iter().map(|p| (p.name.clone(), p)).collect();

        Self {
            config,
            providers: providers_map,
            round_robin_index: 0,
        }
    }

    /// Create a router with default configuration
    pub fn default_router() -> Self {
        Self {
            config: RouterConfig::default(),
            providers: HashMap::new(),
            round_robin_index: 0,
        }
    }

    /// Get a provider based on the routing strategy
    pub fn get_provider(&mut self, model_name: &str) -> Option<&Provider> {
        // Find the model to get its provider
        let provider_name = self
            .config
            .model_list
            .iter()
            .find(|m| m.name == model_name)
            .map(|m| m.provider.clone())?;

        // Get the provider
        self.providers.get(&provider_name)
    }

    /// Select the best provider based on routing strategy
    pub fn select_provider(&mut self, model_name: &str) -> Option<(&Provider, Vec<&Provider>)> {
        let model = self
            .config
            .model_list
            .iter()
            .find(|m| m.name == model_name)?;

        let primary = self.providers.get(&model.provider)?;
        let fallbacks = self.get_fallbacks(&model.provider);

        Some((primary, fallbacks))
    }

    /// Get fallback providers (excluding the primary)
    fn get_fallbacks(&self, primary_provider: &str) -> Vec<&Provider> {
        self.providers
            .values()
            .filter(|p| p.name != primary_provider)
            .take(self.config.num_fallbacks)
            .collect()
    }

    /// Add a model to the model list
    pub fn add_model(&mut self, model: Model) {
        self.config.model_list.push(model);
    }

    /// Get the router configuration
    pub fn config(&self) -> &RouterConfig {
        &self.config
    }

    /// Get all models
    pub fn models(&self) -> &[Model] {
        &self.config.model_list
    }

    /// Get all providers
    pub fn provider_list(&self) -> Vec<&Provider> {
        self.providers.values().collect()
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::default_router()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_creation() {
        let config = RouterConfig::default();
        let providers = vec![Provider::new("openai", "https://api.openai.com/v1")];
        let router = Router::new(config, providers);

        assert_eq!(router.models().len(), 0);
    }

    #[test]
    fn test_add_model() {
        let config = RouterConfig::default();
        let providers = vec![Provider::new("openai", "https://api.openai.com/v1")];
        let mut router = Router::new(config, providers);

        let model = Model::new("gpt-4", "openai").with_costs(0.03, 0.06);
        router.add_model(model);

        assert_eq!(router.models().len(), 1);
        assert_eq!(router.models()[0].name, "gpt-4");
    }

    #[test]
    fn test_select_provider() {
        let mut config = RouterConfig::default();
        config.model_list.push(Model::new("gpt-4", "openai"));

        let providers = vec![Provider::new("openai", "https://api.openai.com/v1")];
        let mut router = Router::new(config, providers);

        let result = router.select_provider("gpt-4");
        assert!(result.is_some());

        let (primary, fallbacks) = result.unwrap();
        assert_eq!(primary.name, "openai");
        assert!(fallbacks.is_empty());
    }

    #[test]
    fn test_select_provider_not_found() {
        let config = RouterConfig::default();
        let providers = vec![Provider::new("openai", "https://api.openai.com/v1")];
        let mut router = Router::new(config, providers);

        let result = router.select_provider("nonexistent-model");
        assert!(result.is_none());
    }
}
