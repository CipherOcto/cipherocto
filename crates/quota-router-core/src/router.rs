// Router module - Routing strategies for multi-provider load balancing
// Based on LiteLLM's simple_shuffle algorithm

use crate::providers::Provider;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Routing strategy types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingStrategy {
    /// Default - Weighted random selection based on rpm/tpm/weight
    #[default]
    SimpleShuffle,
    /// Round-robin through available providers
    RoundRobin,
    /// Route to provider with fewest active requests
    LeastBusy,
    /// Route to fastest responding provider
    LatencyBased,
    /// Route to cheapest provider
    CostBased,
    /// Route based on current usage (RPM/TPM)
    UsageBased,
}

impl std::fmt::Display for RoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingStrategy::SimpleShuffle => write!(f, "simple-shuffle"),
            RoutingStrategy::RoundRobin => write!(f, "round-robin"),
            RoutingStrategy::LeastBusy => write!(f, "least-busy"),
            RoutingStrategy::LatencyBased => write!(f, "latency-based"),
            RoutingStrategy::CostBased => write!(f, "cost-based"),
            RoutingStrategy::UsageBased => write!(f, "usage-based"),
        }
    }
}

impl std::str::FromStr for RoutingStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "simple-shuffle" | "simple_shuffle" | "simple" => Ok(RoutingStrategy::SimpleShuffle),
            "round-robin" | "round_robin" | "roundrobin" => Ok(RoutingStrategy::RoundRobin),
            "least-busy" | "least_busy" | "leastbusy" => Ok(RoutingStrategy::LeastBusy),
            "latency-based" | "latency_based" | "latency" => Ok(RoutingStrategy::LatencyBased),
            "cost-based" | "cost_based" | "cost" => Ok(RoutingStrategy::CostBased),
            "usage-based" | "usage_based" | "usage" => Ok(RoutingStrategy::UsageBased),
            _ => Err(format!("Unknown routing strategy: {}", s)),
        }
    }
}

/// Router configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// Default routing strategy
    #[serde(default)]
    pub routing_strategy: RoutingStrategy,
    /// Track latency window size for latency-based routing
    #[serde(default = "default_latency_window")]
    pub latency_window: usize,
    /// Enable verbose logging
    #[serde(default)]
    pub verbose: bool,
}

fn default_latency_window() -> usize {
    10
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            routing_strategy: RoutingStrategy::SimpleShuffle,
            latency_window: 10,
            verbose: false,
        }
    }
}

/// Provider with runtime state for routing
#[derive(Debug, Clone)]
pub struct ProviderWithState {
    pub provider: Provider,
    /// Current active requests (for LeastBusy)
    pub active_requests: u32,
    /// Rolling latency samples (for LatencyBased)
    pub latencies: Vec<f64>,
    /// Current RPM usage (for UsageBased)
    pub current_rpm: u32,
    /// Current TPM usage (for UsageBased)
    pub current_tpm: u32,
}

impl ProviderWithState {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            active_requests: 0,
            latencies: Vec::new(),
            current_rpm: 0,
            current_tpm: 0,
        }
    }

    /// Record a request start
    pub fn request_started(&mut self) {
        self.active_requests = self.active_requests.saturating_add(1);
    }

    /// Record a request end with latency
    pub fn request_ended(&mut self, latency_ms: f64, tokens: u32, latency_window: usize) {
        self.active_requests = self.active_requests.saturating_sub(1);
        self.latencies.push(latency_ms);
        // Trim latencies to window size
        if self.latencies.len() > latency_window {
            self.latencies.drain(0..self.latencies.len() - latency_window);
        }
        self.current_rpm = self.current_rpm.saturating_add(1);
        self.current_tpm = self.current_tpm.saturating_add(tokens);
    }

    /// Reset RPM/TPM counters (call periodically for sliding window)
    pub fn reset_usage(&mut self) {
        self.current_rpm = 0;
        self.current_tpm = 0;
    }

    /// Get average latency
    pub fn avg_latency(&self) -> f64 {
        if self.latencies.is_empty() {
            f64::MAX // Very high latency for unproven providers
        } else {
            self.latencies.iter().sum::<f64>() / self.latencies.len() as f64
        }
    }

    /// Get the routing weight
    pub fn get_routing_weight(&self) -> u32 {
        self.provider.get_routing_weight()
    }
}

/// Router - handles routing decisions across multiple providers
#[derive(Debug)]
pub struct Router {
    config: RouterConfig,
    /// Providers organized by model group: model_name -> (index, ProviderWithState)
    providers: HashMap<String, Vec<ProviderWithState>>,
    /// Round-robin index per model group
    round_robin_index: HashMap<String, usize>,
}

impl Router {
    pub fn new(config: RouterConfig, providers: Vec<Provider>) -> Self {
        // Group providers by model_name
        let mut providers_map: HashMap<String, Vec<ProviderWithState>> = HashMap::new();

        for provider in providers {
            let model_name = provider
                .model_name
                .clone()
                .unwrap_or_else(|| provider.name.clone());
            providers_map
                .entry(model_name)
                .or_default()
                .push(ProviderWithState::new(provider));
        }

        // Initialize round-robin indices
        let round_robin_index = providers_map
            .keys()
            .map(|k| (k.clone(), 0))
            .collect();

        Self {
            config,
            providers: providers_map,
            round_robin_index,
        }
    }

    /// Get all model groups
    pub fn model_groups(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Get provider count for a model group
    pub fn provider_count(&self, model_group: &str) -> usize {
        self.providers
            .get(model_group)
            .map(|p| p.len())
            .unwrap_or(0)
    }

    /// Get a provider by index
    pub fn get_provider(&mut self, model_group: &str, index: usize) -> Option<&mut ProviderWithState> {
        self.providers.get_mut(model_group)?.get_mut(index)
    }

    /// Route to a provider using the configured strategy - returns index
    pub fn route(&mut self, model_group: &str) -> Option<usize> {
        let strategy = self.config.routing_strategy;
        let latency_window = self.config.latency_window;

        // Get mutable reference to providers
        let providers = self.providers.get_mut(model_group)?;

        if providers.is_empty() {
            return None;
        }

        // Route based on strategy - all methods take only the data they need
        let selected_idx = match strategy {
            RoutingStrategy::SimpleShuffle => Self::simple_shuffle_impl(providers),
            RoutingStrategy::RoundRobin => {
                let idx = self.round_robin_index.entry(model_group.to_string()).or_insert(0);
                let selected = *idx % providers.len();
                *idx = selected.wrapping_add(1);
                selected
            }
            RoutingStrategy::LeastBusy => Self::least_busy_impl(providers),
            RoutingStrategy::LatencyBased => Self::latency_based_impl(providers, latency_window),
            RoutingStrategy::CostBased => Self::simple_shuffle_impl(providers), // Fallback
            RoutingStrategy::UsageBased => Self::usage_based_impl(providers),
        };

        Some(selected_idx)
    }

    /// SimpleShuffle: Weighted random selection based on rpm/tpm/weight
    fn simple_shuffle_impl(providers: &[ProviderWithState]) -> usize {
        let mut rng = rand::thread_rng();

        // Check for explicit weights
        let weights: Vec<u32> = providers.iter().map(|p| p.get_routing_weight()).collect();

        let total_weight: u32 = weights.iter().sum();

        if total_weight == 0 {
            // No weights - uniform random
            rng.gen_range(0..providers.len())
        } else {
            // Weighted random selection
            let mut cumulative = 0u32;
            let weighted: Vec<u32> = weights
                .iter()
                .map(|&w| {
                    cumulative += w;
                    cumulative
                })
                .collect();

            let roll = rng.gen_range(1..=total_weight);
            weighted.iter().position(|&w| w >= roll).unwrap_or(0)
        }
    }

    /// LeastBusy: Select provider with fewest active requests
    fn least_busy_impl(providers: &[ProviderWithState]) -> usize {
        providers
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.active_requests)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// LatencyBased: Select provider with lowest average latency
    fn latency_based_impl(providers: &[ProviderWithState], _latency_window: usize) -> usize {
        providers
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.avg_latency()
                    .partial_cmp(&b.avg_latency())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// UsageBased: Select provider with lowest current usage
    fn usage_based_impl(providers: &[ProviderWithState]) -> usize {
        providers
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.current_rpm)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Record request start for a specific provider index
    pub fn record_request_start(&mut self, model_group: &str, index: usize) {
        if let Some(providers) = self.providers.get_mut(model_group) {
            if let Some(p) = providers.get_mut(index) {
                p.request_started();
            }
        }
    }

    /// Record request end for a specific provider index
    pub fn record_request_end(&mut self, model_group: &str, index: usize, latency_ms: f64, tokens: u32) {
        let latency_window = self.config.latency_window;
        if let Some(providers) = self.providers.get_mut(model_group) {
            if let Some(p) = providers.get_mut(index) {
                p.request_ended(latency_ms, tokens, latency_window);
            }
        }
    }

    /// Reset usage counters for all providers (call periodically for sliding window)
    pub fn reset_all_usage(&mut self) {
        for providers in self.providers.values_mut() {
            for p in providers.iter_mut() {
                p.reset_usage();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_providers() -> Vec<Provider> {
        vec![
            Provider {
                name: "openai".to_string(),
                endpoint: "https://api.openai.com/v1".to_string(),
                rpm: Some(900),
                tpm: None,
                weight: None,
                model_name: Some("gpt-3.5-turbo".to_string()),
            },
            Provider {
                name: "azure".to_string(),
                endpoint: "https://azure.openai.com".to_string(),
                rpm: Some(100),
                tpm: None,
                weight: None,
                model_name: Some("gpt-3.5-turbo".to_string()),
            },
        ]
    }

    #[test]
    fn test_simple_shuffle_weights() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);

        // Should favor openai (900 RPM) over azure (100 RPM)
        let mut openai_count = 0;
        let mut azure_count = 0;

        for _ in 0..1000 {
            if let Some(idx) = router.route("gpt-3.5-turbo") {
                if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                    if p.provider.name == "openai" {
                        openai_count += 1;
                    } else {
                        azure_count += 1;
                    }
                }
            }
        }

        // OpenAI should be selected significantly more often
        assert!(openai_count > azure_count * 5);
    }

    #[test]
    fn test_round_robin() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::RoundRobin,
            ..Default::default()
        };
        let mut router = Router::new(config, providers);

        let mut results = Vec::new();
        for _ in 0..4 {
            if let Some(idx) = router.route("gpt-3.5-turbo") {
                if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                    results.push(p.provider.name.clone());
                }
            }
        }

        // Should alternate: openai, azure, openai, azure
        assert_eq!(results, vec!["openai", "azure", "openai", "azure"]);
    }

    #[test]
    fn test_least_busy() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::LeastBusy,
            ..Default::default()
        };
        let mut router = Router::new(config, providers);

        // Manually set active requests
        if let Some(providers) = router.providers.get_mut("gpt-3.5-turbo") {
            for (i, p) in providers.iter_mut().enumerate() {
                p.active_requests = i as u32; // openai=0, azure=1
            }
        }

        // Should select openai (fewer active requests)
        if let Some(idx) = router.route("gpt-3.5-turbo") {
            if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                assert_eq!(p.provider.name, "openai");
            }
        }
    }

    #[test]
    fn test_routing_strategy_from_str() {
        assert_eq!(
            "simple-shuffle".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::SimpleShuffle
        );
        assert_eq!(
            "round-robin".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::RoundRobin
        );
        assert_eq!(
            "least-busy".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::LeastBusy
        );
        assert_eq!(
            "latency-based".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::LatencyBased
        );
        assert_eq!(
            "usage-based".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::UsageBased
        );
    }

    #[test]
    fn test_latency_based_routing() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::LatencyBased,
            latency_window: 10,
            verbose: false,
        };
        let mut router = Router::new(config, providers);

        // Set latencies - azure should be faster
        if let Some(providers) = router.providers.get_mut("gpt-3.5-turbo") {
            for p in providers.iter_mut() {
                if p.provider.name == "azure" {
                    p.latencies = vec![100.0, 110.0, 105.0]; // Fast: ~105ms avg
                } else {
                    p.latencies = vec![500.0, 510.0, 505.0]; // Slow: ~505ms avg
                }
            }
        }

        // Should select azure (lower latency)
        if let Some(idx) = router.route("gpt-3.5-turbo") {
            if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                assert_eq!(p.provider.name, "azure");
            }
        }
    }

    #[test]
    fn test_usage_based_routing() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::UsageBased,
            ..Default::default()
        };
        let mut router = Router::new(config, providers);

        // Set current usage - azure has lower usage
        if let Some(providers) = router.providers.get_mut("gpt-3.5-turbo") {
            for p in providers.iter_mut() {
                if p.provider.name == "azure" {
                    p.current_rpm = 10; // Low usage
                } else {
                    p.current_rpm = 500; // High usage
                }
            }
        }

        // Should select azure (lower current usage)
        if let Some(idx) = router.route("gpt-3.5-turbo") {
            if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                assert_eq!(p.provider.name, "azure");
            }
        }
    }

    #[test]
    fn test_request_tracking() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);

        // Route and track request
        let idx = router.route("gpt-3.5-turbo").unwrap();
        router.record_request_start("gpt-3.5-turbo", idx);

        // Check active requests increased
        if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
            assert_eq!(p.active_requests, 1);
        }

        // Record request end
        router.record_request_end("gpt-3.5-turbo", idx, 150.0, 100);

        // Check active requests decreased and latency recorded
        if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
            assert_eq!(p.active_requests, 0);
            assert!(!p.latencies.is_empty());
            assert_eq!(p.current_rpm, 1);
        }
    }
}
