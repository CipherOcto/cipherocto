// Router module - Routing strategies for multi-provider load balancing
// Based on LiteLLM's simple_shuffle algorithm

use crate::providers::Provider;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

// ⚠️ CRITICAL INVARIANT (RFC-0917):
// litellm-mode and any-llm-mode are MUTUALLY EXCLUSIVE AS BUILD CONFIGURATIONS
// (you either use reqwest OR PyO3 to call providers, not both in single mode).
// BUT: BOTH HTTP proxy AND Python SDK exist in ALL modes.
// The mutual exclusivity is about PROVIDER STRATEGY, not INTERFACE availability.
//
// Build configurations:
//   - litellm-mode: reqwest only
//   - any-llm-mode: PyO3 only
//   - full: BOTH reqwest AND PyO3 (a SEPARATE build, not both at once)
//
// See RFC-0917 lines 175-176: "HTTP Proxy Server | (always)" and "Python SDK Interface | (always)"
#[cfg(all(feature = "litellm-mode", feature = "any-llm-mode"))]
compile_error!("Cannot enable both 'litellm-mode' and 'any-llm-mode' — use 'full' for both");

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
    /// Route using recency-weighted spend (exponential decay — recent usage counts more)
    UsageBasedV2,
    /// Weighted distribution based on explicitly configured weights (distinct from SimpleShuffle)
    Weighted,
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
            RoutingStrategy::UsageBasedV2 => write!(f, "usage-based-v2"),
            RoutingStrategy::Weighted => write!(f, "weighted"),
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
            "latency-based"
            | "latency_based"
            | "latency"
            | "latency-based-routing"
            | "latency_based_routing" => Ok(RoutingStrategy::LatencyBased),
            "cost-based" | "cost_based" | "cost" | "cost-based-routing" | "cost_based_routing" => {
                Ok(RoutingStrategy::CostBased)
            }
            "usage-based" | "usage_based" | "usage" => Ok(RoutingStrategy::UsageBased),
            "usage-based-v2"
            | "usage-based-routing-v2"
            | "usage_based_v2"
            | "usage_v2"
            | "usage_based_routing_v2" => Ok(RoutingStrategy::UsageBasedV2),
            "weighted" => Ok(RoutingStrategy::Weighted),
            _ => Err(format!("Unknown routing strategy: {}", s)),
        }
    }
}

impl RoutingStrategy {
    /// Parse LiteLLM string format to enum with default fallback.
    /// LiteLLM uses strings like "latency-based-routing", "simple-shuffle".
    /// Returns SimpleShuffle for unknown strategies (matches LiteLLM default behavior).
    pub fn from_litellm_str(s: &str) -> Self {
        match s.to_lowercase().replace("_", "-").as_str() {
            "simple-shuffle" => RoutingStrategy::SimpleShuffle,
            "round-robin" => RoutingStrategy::RoundRobin,
            "least-busy" => RoutingStrategy::LeastBusy,
            "latency-based" | "latency-based-routing" => RoutingStrategy::LatencyBased,
            "cost-based" | "cost-based-routing" => RoutingStrategy::CostBased,
            "usage-based" => RoutingStrategy::UsageBased,
            "usage-based-v2" | "usage-based-routing-v2" => RoutingStrategy::UsageBasedV2,
            "weighted" => RoutingStrategy::Weighted,
            _ => RoutingStrategy::SimpleShuffle, // Default fallback
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
    /// Latency config for LatencyBased routing (RFC-0925)
    #[serde(default)]
    pub latency_config: LatencyConfig,
    /// Enable verbose logging
    #[serde(default)]
    pub verbose: bool,
    /// Global weights map for Weighted strategy: provider.name → weight
    /// Used by Weighted routing to select providers with explicit weights.
    /// If a provider.name is not in weights, falls back to get_routing_weight() (rpm/tpm-derived).
    #[serde(default)]
    pub weights: HashMap<String, u32>,
}

fn default_latency_window() -> usize {
    10
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            routing_strategy: RoutingStrategy::SimpleShuffle,
            latency_window: 10,
            latency_config: LatencyConfig::default(),
            verbose: false,
            weights: HashMap::new(),
        }
    }
}

/// Provider with runtime state for routing
#[derive(Debug, Clone)]
pub struct ProviderWithState {
    pub provider: Provider,
    /// Current active requests (for LeastBusy)
    pub active_requests: u32,
    /// Rolling latency samples in microseconds (for LatencyBased)
    pub latencies: Vec<u64>,
    /// Success count (u64)
    pub success_count: u64,
    /// Total request count (u64)
    pub total_count: u64,
    /// Current RPM usage (for UsageBased)
    pub current_rpm: u32,
    /// Current TPM usage (for UsageBased)
    pub current_tpm: u32,
    /// Cooldown tracker for LatencyBased routing (RFC-0925)
    pub cooldown_tracker: CooldownTracker,
}

impl ProviderWithState {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            active_requests: 0,
            latencies: Vec::new(),
            success_count: 0,
            total_count: 0,
            current_rpm: 0,
            current_tpm: 0,
            cooldown_tracker: CooldownTracker::default(),
        }
    }

    /// Record a request start
    pub fn request_started(&mut self) {
        self.active_requests = self.active_requests.saturating_add(1);
    }

    /// Record a request end with latency (in microseconds)
    pub fn request_ended(&mut self, latency_us: u64, tokens: u32, latency_window: usize) {
        self.active_requests = self.active_requests.saturating_sub(1);
        self.latencies.push(latency_us);
        // Trim latencies to window size (sliding window)
        if self.latencies.len() > latency_window {
            self.latencies
                .drain(0..self.latencies.len() - latency_window);
        }
        self.current_rpm = self.current_rpm.saturating_add(1);
        self.current_tpm = self.current_tpm.saturating_add(tokens);
        self.total_count = self.total_count.saturating_add(1);
    }

    /// Record a successful request (increments success_count)
    pub fn record_success(&mut self) {
        self.success_count = self.success_count.saturating_add(1);
    }

    /// Reset RPM/TPM counters (call periodically for sliding window)
    pub fn reset_usage(&mut self) {
        self.current_rpm = 0;
        self.current_tpm = 0;
    }

    /// Get average latency in microseconds
    pub fn avg_latency_us(&self) -> u64 {
        if self.latencies.is_empty() {
            u64::MAX // Very high latency for unproven providers
        } else {
            self.latencies.iter().sum::<u64>() / self.latencies.len() as u64
        }
    }

    /// Get the routing weight
    pub fn get_routing_weight(&self) -> u32 {
        self.provider.get_routing_weight()
    }
}

/// Deployment latency state — RFC-0925
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeploymentState {
    /// Normal operation, routing to this deployment
    #[default]
    Healthy,
    /// Taken out of rotation, waiting for cooldown to expire
    /// litellm pattern: cooldown is TTL-based, no Degraded state
    /// When TTL expires, deployment is automatically available again
    Cooldown,
}

/// LatencyConfig for LatencyBased routing — RFC-0925
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LatencyConfig {
    /// Latency buffer: select deployments within (lowest_latency + buffer * lowest_latency)
    /// Default: 0.0 (litellm default) = only fastest deployment selected
    pub lowest_latency_buffer: f32,
    /// Max entries in latency rolling window per deployment
    /// Default: 10 (litellm default)
    pub max_latency_list_size: usize,
    /// Penalty latency in microseconds for timeout/failure events
    /// litellm uses 1_000_000_000µs (1000s)
    pub timeout_penalty_us: u64,
    /// Cooldown duration in seconds
    /// Default: 5 (litellm DEFAULT_COOLDOWN_TIME_SECONDS)
    pub cooldown_duration_secs: u32,
    /// Failure threshold percent (0.0-1.0) before triggering cooldown
    /// litellm default: 0.5 (50%)
    pub failure_threshold_percent: f32,
    /// Minimum requests before failure rate is meaningful
    /// litellm default: 5
    pub failure_threshold_min_requests: u32,
}

impl Default for LatencyConfig {
    fn default() -> Self {
        Self {
            lowest_latency_buffer: 0.0,
            max_latency_list_size: 10,
            timeout_penalty_us: 1_000_000_000,
            cooldown_duration_secs: 5,
            failure_threshold_percent: 0.5,
            failure_threshold_min_requests: 5,
        }
    }
}

/// CooldownTracker per deployment — RFC-0925
#[derive(Debug, Clone)]
pub struct CooldownTracker {
    /// Current deployment state
    pub state: DeploymentState,
    /// Total requests in current minute window
    total_requests: u32,
    /// Failed requests in current minute window
    failed_requests: u32,
    /// When cooldown TTL expires (None if not in cooldown)
    cooldown_end_time: Option<Instant>,
    /// Penalty latencies (e.g., 1000s for timeout) — applied to scoring
    penalty_latencies: Vec<u64>,
}

impl Default for CooldownTracker {
    fn default() -> Self {
        Self {
            state: DeploymentState::Healthy,
            total_requests: 0,
            failed_requests: 0,
            cooldown_end_time: None,
            penalty_latencies: Vec::new(),
        }
    }
}

impl CooldownTracker {
    /// Record a successful request completion
    pub fn record_success(&mut self) {
        self.total_requests = self.total_requests.saturating_add(1);
    }

    /// Record a timeout/failure event — applies penalty latency
    /// litellm pattern: timeout events append 1000s penalty to latency list
    pub fn record_timeout_penalty(&mut self, penalty_us: u64) {
        self.penalty_latencies.push(penalty_us);
        self.total_requests = self.total_requests.saturating_add(1);
        self.failed_requests = self.failed_requests.saturating_add(1);
    }

    /// Record a 429 rate limit response
    /// litellm pattern: 429 always triggers cooldown UNLESS it's a single-deployment model group
    /// Returns true if cooldown was entered, false if exempted
    pub fn record_429(&mut self, cooldown_duration_secs: u32, is_single_deployment: bool) -> bool {
        self.total_requests = self.total_requests.saturating_add(1);
        self.failed_requests = self.failed_requests.saturating_add(1);
        if is_single_deployment {
            return false;
        }
        self.state = DeploymentState::Cooldown;
        self.cooldown_end_time =
            Some(Instant::now() + Duration::from_secs(cooldown_duration_secs as u64));
        true
    }

    /// Record a 4XX error (non-429) — counts toward failure rate
    pub fn record_error(&mut self) {
        self.total_requests = self.total_requests.saturating_add(1);
        self.failed_requests = self.failed_requests.saturating_add(1);
    }

    /// Check if should enter cooldown based on failure rate
    /// litellm pattern: >50% failure rate + >=5 requests = cooldown
    /// Only checked when state is Healthy
    pub fn should_enter_cooldown(
        &self,
        failure_threshold_percent: f32,
        failure_threshold_min_requests: u32,
        is_single_deployment: bool,
    ) -> bool {
        if self.state != DeploymentState::Healthy {
            return false;
        }
        if is_single_deployment {
            return false;
        }
        if self.total_requests < failure_threshold_min_requests {
            return false;
        }
        let failure_rate = self.failed_requests as f32 / self.total_requests as f32;
        failure_rate > failure_threshold_percent
    }

    /// Reset failure counters for new minute window
    /// Note: This does NOT clear penalty_latencies — those persist until cooldown expires
    pub fn reset_minute_window(&mut self) {
        self.total_requests = 0;
        self.failed_requests = 0;
    }

    /// Clear penalty latencies — called when cooldown expires
    pub fn clear_penalty_latencies(&mut self) {
        self.penalty_latencies.clear();
    }

    /// Get reference to penalty latencies for external query (e.g., by LatencyTracker)
    pub fn get_penalty_latencies(&self) -> &[u64] {
        &self.penalty_latencies
    }

    /// Enter cooldown state with TTL-based expiry
    pub fn enter_cooldown(&mut self, duration_secs: u32) {
        self.state = DeploymentState::Cooldown;
        self.cooldown_end_time = Some(Instant::now() + Duration::from_secs(duration_secs as u64));
    }

    /// Check if cooldown TTL has expired
    pub fn is_cooldown_expired(&self) -> bool {
        match self.cooldown_end_time {
            Some(end) => Instant::now() >= end,
            None => false,
        }
    }

    /// Check if deployment should receive traffic
    pub fn is_available(&self) -> bool {
        match self.state {
            DeploymentState::Healthy => true,
            DeploymentState::Cooldown => self.is_cooldown_expired(),
        }
    }
}

// =============================================================================
// LatencyTracker — RFC-0917 §LatencyTracker
// Integer microseconds for deterministic latency tracking (no floating point)
// =============================================================================

const LATENCY_WINDOW_SIZE: usize = 100;

/// Latency tracker for LatencyBased routing strategy.
/// Uses integer microseconds to avoid floating-point non-determinism (per RFC-0104).
///
/// **Window:** Fixed-size sliding window of last `LATENCY_WINDOW_SIZE` samples per provider.
/// **Storage:** `HashMap<provider_name, VecDeque<u64>>` — latency in microseconds (integer).
/// **Cleanup:** Oldest sample evicted when window exceeds `LATENCY_WINDOW_SIZE` (O(1) via VecDeque).
/// **Query:** `best_provider()` returns provider with lowest average latency.
///
/// Implemented per RFC-0925 (Latency-Based Routing Extensions).
/// Integrated into `RouterState` via `RouterConfig.latency_config`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct LatencyTracker {
    samples: HashMap<String, VecDeque<u64>>,
    ttft_samples: HashMap<String, VecDeque<u64>>,
}

impl LatencyTracker {
    /// Record a latency observation for a provider (latency_us in microseconds).
    /// If ttft_us is Some, also records TTFT for streaming requests.
    #[allow(dead_code)]
    pub fn record(&mut self, provider: &str, latency_us: u64, ttft_us: Option<u64>) {
        // Record latency sample
        let samples = self.samples.entry(provider.to_string()).or_default();
        if samples.len() >= LATENCY_WINDOW_SIZE {
            samples.pop_front();
        }
        samples.push_back(latency_us);

        // Record TTFT if provided
        if let Some(ttft) = ttft_us {
            let ttft_samples = self.ttft_samples.entry(provider.to_string()).or_default();
            if ttft_samples.len() >= LATENCY_WINDOW_SIZE {
                ttft_samples.pop_front();
            }
            ttft_samples.push_back(ttft);
        }
    }

    /// Return provider with lowest average latency in current window.
    /// Returns `None` if no providers have samples.
    /// Ties broken by provider name (lexicographically first).
    #[allow(dead_code)]
    pub fn best_provider(&self) -> Option<&str> {
        self.samples
            .iter()
            .filter(|(_, samples)| !samples.is_empty())
            .map(|(name, samples)| {
                let sum: u64 = samples.iter().sum();
                (name, sum / samples.len() as u64)
            })
            .min_by_key(|(_, avg_latency)| *avg_latency)
            .map(|(name, _)| name.as_str())
    }

    /// Get best provider with TTFT weighting + latency buffer.
    /// Uses TTFT only for streaming (selection mode, NOT weighted blend).
    #[allow(dead_code)]
    pub fn best_provider_with_ttft(
        &self,
        is_streaming: bool,
        lowest_latency_buffer: f32,
    ) -> Option<&str> {
        let all_providers: Vec<(&str, f32)> = self
            .samples
            .iter()
            .filter(|(_, samples)| !samples.is_empty())
            .map(|(name, samples)| {
                let avg_latency = samples.iter().sum::<u64>() as f32 / samples.len() as f32;
                let avg_ttft = self
                    .ttft_samples
                    .get(name)
                    .map(|s| s.iter().sum::<u64>() as f32 / s.len() as f32)
                    .unwrap_or(avg_latency);

                let score = if is_streaming
                    && self
                        .ttft_samples
                        .get(name)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false)
                {
                    avg_ttft
                } else {
                    avg_latency
                };

                (name.as_str(), score)
            })
            .collect();

        if all_providers.is_empty() {
            return None;
        }

        let lowest_latency = all_providers
            .iter()
            .map(|(_, score)| *score)
            .fold(f32::INFINITY, f32::min);

        let buffer = lowest_latency_buffer * lowest_latency;
        let valid: Vec<&str> = all_providers
            .iter()
            .filter(|(_, score)| *score <= lowest_latency + buffer)
            .map(|(name, _)| *name)
            .collect();

        if valid.is_empty() {
            None
        } else {
            Some(valid[0])
        }
    }

    /// Get best provider among a specific set of available provider names.
    #[allow(dead_code)]
    pub fn best_provider_among(
        &self,
        available_names: std::collections::HashSet<&str>,
        is_streaming: bool,
    ) -> Option<&str> {
        let candidates: Vec<(&str, f32)> = self
            .samples
            .iter()
            .filter(|(name, samples)| {
                available_names.contains(name.as_str()) && !samples.is_empty()
            })
            .map(|(name, samples)| {
                let avg_latency = samples.iter().sum::<u64>() as f32 / samples.len() as f32;
                let avg_ttft = self
                    .ttft_samples
                    .get(name)
                    .map(|s| s.iter().sum::<u64>() as f32 / s.len() as f32)
                    .unwrap_or(avg_latency);

                let score = if is_streaming
                    && self
                        .ttft_samples
                        .get(name)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false)
                {
                    avg_ttft
                } else {
                    avg_latency
                };

                (name.as_str(), score)
            })
            .collect();

        candidates
            .iter()
            .min_by_key(|(_, score)| *score as u64)
            .map(|(name, _)| *name)
    }

    /// Get best provider considering penalty-adjusted latencies.
    /// Returns (provider_name, effective_latency) or None if no valid providers.
    ///
    /// NOTE: Only considers providers with latency samples in self.samples AND
    /// that are present in the `available_names` set. Callers must populate
    /// `available_names` with providers that are not in cooldown.
    /// Fresh deployments (no latency samples) cannot be selected by this method;
    /// callers should use a separate strategy when this returns None.
    ///
    /// TTFT-only for streaming with data (penalties NOT applied to TTFT).
    /// Penalty-adjusted latency for non-streaming or streaming without TTFT.
    pub fn best_provider_with_penalties(
        &self,
        penalty_map: &std::collections::HashMap<String, Vec<u64>>,
        available_names: &std::collections::HashSet<&str>,
        is_streaming: bool,
    ) -> Option<(&str, f32)> {
        let mut candidates: Vec<(&str, f32)> = Vec::new();

        for (name, samples) in &self.samples {
            let name_str = name.as_str();

            // Filter: must be in available set (not in cooldown) and have samples
            if !available_names.contains(name_str) {
                continue;
            }
            if samples.is_empty() {
                continue;
            }

            // For streaming with TTFT data: use TTFT only, ignore penalties
            // TTFT measures initial responsiveness; a timeout AFTER first token
            // shouldn't penalize TTFT
            if is_streaming {
                if let Some(ttft_samples) = self.ttft_samples.get(name_str) {
                    if !ttft_samples.is_empty() {
                        let ttft_avg =
                            ttft_samples.iter().sum::<u64>() as f32 / ttft_samples.len() as f32;
                        candidates.push((name_str, ttft_avg));
                        continue;
                    }
                }
            }

            // Non-streaming OR streaming without TTFT data: use penalty-adjusted latency
            let samples_sum: u64 = samples.iter().sum();
            let base_latency = samples_sum as f32 / samples.len() as f32;
            let penalties = penalty_map
                .get(name_str)
                .map(|p| p.as_slice())
                .unwrap_or(&[]);

            let effective = if penalties.is_empty() {
                base_latency
            } else {
                let penalty_sum: u64 = penalties.iter().sum();
                let total_count = samples.len() + penalties.len();
                (samples_sum as f32 + penalty_sum as f32) / total_count as f32
            };

            candidates.push((name_str, effective));
        }

        // Find minimum by score using total ordering on f32 bits.
        // For positive finite floats (all realistic latencies), bit ordering matches numeric ordering:
        //   100ms (0x42C80000) < 200ms (0x43480000) < ... < 10s (0x461C4000)
        // Using to_bits() avoids f32::total_cmp (unstable) and is deterministic.
        candidates
            .iter()
            .map(|(name, score)| (*name, *score, score.to_bits()))
            .min_by_key(|(_, _, bits)| *bits)
            .map(|(name, score, _)| (name, score))
    }
}

/// RouterState wraps Router with cross-model-group LatencyTracker integration.
/// Phase 2 of RFC-0917 integrates LatencyTracker into routing flow.
#[derive(Debug)]
pub struct RouterState {
    pub router: Router,
    pub latency_tracker: LatencyTracker,
}

impl RouterState {
    pub fn new(config: RouterConfig, providers: Vec<Provider>) -> Self {
        Self {
            router: Router::new(config, providers),
            latency_tracker: LatencyTracker::default(),
        }
    }

    /// Record request end for a specific provider index (latency in microseconds).
    /// Updates per-model-group ProviderWithState AND cross-model-group LatencyTracker.
    /// If ttft_us is Some, also records TTFT for streaming requests.
    pub fn record_request_end(
        &mut self,
        model_group: &str,
        index: usize,
        latency_us: u64,
        tokens: u32,
        ttft_us: Option<u64>,
    ) {
        let latency_window = self.router.config.latency_window;

        // Get provider name for LatencyTracker update (clone to avoid borrow conflict)
        let provider_name = self
            .router
            .providers
            .get(model_group)
            .and_then(|p| p.get(index))
            .map(|p| p.provider.name.clone());

        // Update per-model-group ProviderWithState
        if let Some(providers) = self.router.providers.get_mut(model_group) {
            if let Some(p) = providers.get_mut(index) {
                p.request_ended(latency_us, tokens, latency_window);
            }
        }

        // Update cross-model-group LatencyTrackers (both RouterState and Router)
        // RouterState.latency_tracker: cross-model-group best provider selection
        // Router.latency_tracker: used by Router.route() for LatencyBased strategy
        if let Some(name) = provider_name {
            self.latency_tracker.record(&name, latency_us, ttft_us);
            self.router
                .latency_tracker
                .record(&name, latency_us, ttft_us);
        }
    }
}

/// Provider metrics with minute-bucket tracking for RPM/TPM monitoring.
/// Standalone struct (not in ProviderWithState) per RFC-0924.
///
/// Bucket key format: "HH-MM" (hour-minute in local time), NOT full date.
/// Matches litellm's LowestTPMLoggingHandler pattern (datetime.now().strftime("%H-%M")).
///
/// TTL is 60 seconds (per litellm RoutingArgs.ttl) to prevent cross-day HH-MM bucket collisions.
#[derive(Debug, Clone)]
pub struct ProviderMetrics {
    /// Buckets: deployment_id -> minute_key -> BucketStats
    buckets: HashMap<String, HashMap<String, BucketStats>>,
    /// TTL for bucket entries (default: 60 seconds per litellm RoutingArgs.ttl)
    ttl_seconds: u32,
    /// When each bucket was created (for TTL eviction)
    bucket_timestamps: HashMap<String, HashMap<String, Instant>>,
}

/// Per-minute bucket statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct BucketStats {
    /// Tokens per minute (u64: supports high-traffic deployments)
    pub tpm: u64,
    /// Requests per minute (u64: supports high-frequency deployments)
    pub rpm: u64,
}

impl ProviderMetrics {
    /// Create a new ProviderMetrics with default TTL of 60 seconds.
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
            ttl_seconds: 60,
            bucket_timestamps: HashMap::new(),
        }
    }

    /// Create a new ProviderMetrics with custom TTL in seconds.
    pub fn with_ttl(ttl_seconds: u32) -> Self {
        Self {
            buckets: HashMap::new(),
            ttl_seconds,
            bucket_timestamps: HashMap::new(),
        }
    }

    /// Record a request completion for a deployment.
    /// Increments rpm and tpm counters, tracks timestamp, performs probabilistic eviction.
    ///
    /// Note: RPM/TPM should be incremented BEFORE limit check (litellm pattern).
    pub fn record(&mut self, deployment_id: &str, tokens: u32) {
        let minute = Self::current_minute_key();
        let now = Instant::now();

        let stats = self
            .buckets
            .entry(deployment_id.to_string())
            .or_default()
            .entry(minute.clone())
            .or_default();

        // Increment counters (litellm pattern: increment THEN check limits)
        stats.tpm = stats.tpm.saturating_add(tokens as u64);
        stats.rpm = stats.rpm.saturating_add(1);

        // Track creation time for TTL
        let timestamps = self
            .bucket_timestamps
            .entry(deployment_id.to_string())
            .or_default();
        timestamps.entry(minute).or_insert(now);

        // Evict old buckets periodically (every 10 calls per deployment, probabilistic)
        // This prevents unbounded bucket growth without scanning on every call
        if self
            .bucket_timestamps
            .get(deployment_id)
            .map(|ts| ts.len() % 10 == 0)
            .unwrap_or(false)
        {
            self.evict_old_buckets_for(deployment_id);
        }
    }

    /// Evict buckets older than ttl_seconds for a specific deployment.
    pub fn evict_old_buckets_for(&mut self, deployment_id: &str) {
        let now = Instant::now();
        let ttl = Duration::from_secs(self.ttl_seconds as u64);

        if let Some(buckets) = self.buckets.get_mut(deployment_id) {
            if let Some(timestamps) = self.bucket_timestamps.get_mut(deployment_id) {
                buckets.retain(|minute_key, _| {
                    timestamps
                        .get(minute_key)
                        .map(|created| now.duration_since(*created) < ttl)
                        .unwrap_or(false)
                });
                timestamps.retain(|minute_key, _| buckets.contains_key(minute_key));
            }
        }
    }

    /// Evict buckets older than ttl_seconds for all deployments.
    /// For higher throughput, a background task can call this periodically.
    pub fn evict_old_buckets(&mut self) {
        let now = Instant::now();
        let ttl = Duration::from_secs(self.ttl_seconds as u64);

        for (deployment_id, buckets) in self.buckets.iter_mut() {
            if let Some(timestamps) = self.bucket_timestamps.get_mut(deployment_id) {
                buckets.retain(|minute_key, _| {
                    timestamps
                        .get(minute_key)
                        .map(|created| now.duration_since(*created) < ttl)
                        .unwrap_or(false)
                });
                timestamps.retain(|minute_key, _| buckets.contains_key(minute_key));
            }
        }
    }

    /// Get RPM for a specific deployment at a specific minute.
    pub fn rpm_at(&self, deployment_id: &str, minute: &str) -> Option<u64> {
        self.buckets
            .get(deployment_id)
            .and_then(|m| m.get(minute))
            .map(|stats| stats.rpm)
    }

    /// Get TPM for a specific deployment at a specific minute.
    pub fn tpm_at(&self, deployment_id: &str, minute: &str) -> Option<u64> {
        self.buckets
            .get(deployment_id)
            .and_then(|m| m.get(minute))
            .map(|stats| stats.tpm)
    }

    /// Get current minute RPM for a deployment.
    pub fn current_rpm(&self, deployment_id: &str) -> u64 {
        let minute = Self::current_minute_key();
        self.rpm_at(deployment_id, &minute).unwrap_or(0)
    }

    /// Get current minute TPM for a deployment.
    pub fn current_tpm(&self, deployment_id: &str) -> u64 {
        let minute = Self::current_minute_key();
        self.tpm_at(deployment_id, &minute).unwrap_or(0)
    }

    /// Check if deployment can accept a new request (limit check).
    /// Returns true if deployment is within limits.
    ///
    /// Litellm pattern: item_rpm + 1 > rpm_limit means "would exceed"
    /// So can_accept is true only if (current + 1) <= limit.
    pub fn can_accept_request(
        &self,
        deployment_id: &str,
        rpm_limit: u64,
        tpm_limit: u64,
        input_tokens: u64,
    ) -> bool {
        let current_rpm = self.current_rpm(deployment_id);
        let current_tpm = self.current_tpm(deployment_id);

        (current_rpm + 1) <= rpm_limit && (current_tpm + input_tokens) <= tpm_limit
    }

    /// Get rolling average RPM over N minutes.
    pub fn rolling_avg_rpm(&self, deployment_id: &str, minutes: u32) -> Option<f32> {
        let buckets = self.buckets.get(deployment_id)?;
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(minutes as u64 * 60);

        let recent: Vec<u64> = buckets
            .iter()
            .filter(|(minute_key, _)| {
                if let Some(created) = self
                    .bucket_timestamps
                    .get(deployment_id)
                    .and_then(|ts| ts.get(*minute_key))
                {
                    *created >= cutoff
                } else {
                    false
                }
            })
            .map(|(_, stats)| stats.rpm)
            .collect();

        if recent.is_empty() {
            return None;
        }

        let sum: u64 = recent.iter().sum();
        Some(sum as f32 / recent.len() as f32)
    }

    /// Get rolling average TPM over N minutes.
    pub fn rolling_avg_tpm(&self, deployment_id: &str, minutes: u32) -> Option<f32> {
        let buckets = self.buckets.get(deployment_id)?;
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(minutes as u64 * 60);

        let recent: Vec<u64> = buckets
            .iter()
            .filter(|(minute_key, _)| {
                if let Some(created) = self
                    .bucket_timestamps
                    .get(deployment_id)
                    .and_then(|ts| ts.get(*minute_key))
                {
                    *created >= cutoff
                } else {
                    false
                }
            })
            .map(|(_, stats)| stats.tpm)
            .collect();

        if recent.is_empty() {
            return None;
        }

        let sum: u64 = recent.iter().sum();
        Some(sum as f32 / recent.len() as f32)
    }

    /// Get current minute key in "HH-MM" format (local time).
    /// Matches litellm's datetime.now().strftime("%H-%M").
    fn current_minute_key() -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        let total_secs = now.as_secs();

        // Get local time (offset from UTC)
        // litellm uses datetime.now().strftime("%H-%M") which is LOCAL time
        let offset_secs: i64 = 0; // Placeholder: in production, get from timezone config
        let local_secs = (total_secs as i64 + offset_secs).unsigned_abs();
        let secs_in_day = local_secs % 86400;
        let hour = (secs_in_day / 3600) as u32;
        let minute = ((secs_in_day % 3600) / 60) as u32;

        format!("{:02}-{:02}", hour, minute)
    }
}

impl Default for ProviderMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl ProviderMetrics {
    /// Test helper to get current minute key (exposed for testing only).
    pub fn current_minute_key_for_test() -> String {
        Self::current_minute_key()
    }
}

/// Router - handles routing decisions across multiple providers
/// **Non-normative pseudocode** (per RFC-0917 A3 Router struct — actual implementation
/// may differ from the spec pseudocode while maintaining equivalent behavior).
#[derive(Debug)]
pub struct Router {
    config: RouterConfig,
    /// Providers organized by model group: model_name -> (index, ProviderWithState)
    providers: HashMap<String, Vec<ProviderWithState>>,
    /// Round-robin index per model group
    round_robin_index: HashMap<String, usize>,
    /// Latency tracker for TTFT-aware LatencyBased routing (RFC-0925)
    latency_tracker: LatencyTracker,
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
        let round_robin_index = providers_map.keys().map(|k| (k.clone(), 0)).collect();

        Self {
            config,
            providers: providers_map,
            round_robin_index,
            latency_tracker: LatencyTracker::default(),
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
    pub fn get_provider(
        &mut self,
        model_group: &str,
        index: usize,
    ) -> Option<&mut ProviderWithState> {
        self.providers.get_mut(model_group)?.get_mut(index)
    }

    /// Route to a provider using the configured strategy - returns index
    pub fn route(&mut self, model_group: &str, is_streaming: bool) -> Option<usize> {
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
                let idx = self
                    .round_robin_index
                    .entry(model_group.to_string())
                    .or_insert(0);
                let selected = *idx % providers.len();
                *idx = selected.wrapping_add(1);
                selected
            }
            RoutingStrategy::LeastBusy => Self::least_busy_impl(providers),
            RoutingStrategy::LatencyBased => {
                // RFC-0925/0926: Cooldown-aware LatencyBased routing with penalty latency scoring
                return Self::latency_based_with_cooldown_impl(
                    providers,
                    &mut self.latency_tracker,
                    &self.config.latency_config,
                    is_streaming,
                    latency_window,
                );
            }
            RoutingStrategy::CostBased => Self::cost_based_impl(providers),
            RoutingStrategy::UsageBased => Self::usage_based_impl(providers),
            RoutingStrategy::UsageBasedV2 => Self::usage_based_v2_impl(providers),
            RoutingStrategy::Weighted => Self::weighted_impl(providers, &self.config.weights),
        };

        Some(selected_idx)
    }

    /// SimpleShuffle: Weighted random selection based on rpm/tpm/weight
    fn simple_shuffle_impl(providers: &[ProviderWithState]) -> usize {
        let mut rng = rand::rng();

        // Check for explicit weights
        let weights: Vec<u32> = providers.iter().map(|p| p.get_routing_weight()).collect();

        let total_weight: u32 = weights.iter().sum();

        if total_weight == 0 {
            // No weights - uniform random
            rng.random_range(0..providers.len())
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

            let roll = rng.random_range(1..=total_weight);
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

    /// LatencyBased with cooldown: Select best available provider using TTFT-aware scoring (RFC-0925)
    fn latency_based_with_cooldown_impl(
        providers: &mut [ProviderWithState],
        latency_tracker: &mut LatencyTracker,
        _latency_config: &LatencyConfig,
        is_streaming: bool,
        _latency_window: usize,
    ) -> Option<usize> {
        // First, expire any cooldowns that have elapsed
        for p in providers.iter_mut() {
            if p.cooldown_tracker.state == DeploymentState::Cooldown
                && p.cooldown_tracker.is_cooldown_expired()
            {
                p.cooldown_tracker.state = DeploymentState::Healthy;
                p.cooldown_tracker.reset_minute_window();
                p.cooldown_tracker.clear_penalty_latencies();
            }
        }

        // Build available set (providers not in cooldown) and penalty map
        let mut penalty_map: std::collections::HashMap<String, Vec<u64>> =
            std::collections::HashMap::new();
        let mut available_names: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for provider in providers.iter() {
            // Skip providers in cooldown
            if !provider.cooldown_tracker.is_available() {
                continue;
            }

            let name_str: &str = provider.provider.name.as_str();
            available_names.insert(name_str);

            // Build penalty map for this provider (only if penalties exist)
            let penalties = provider.cooldown_tracker.get_penalty_latencies();
            if !penalties.is_empty() {
                penalty_map.insert(provider.provider.name.clone(), penalties.to_vec());
            }
        }

        // If no available providers, return None
        if available_names.is_empty() {
            return None;
        }

        // Use penalty-adjusted selection when penalties exist, otherwise use standard best_provider_among
        let best_name = if penalty_map.is_empty() {
            // No penalties: use standard selection among available providers
            latency_tracker.best_provider_among(available_names, is_streaming)?
        } else {
            // Penalties exist: use penalty-adjusted selection among available providers
            let (name, _) = latency_tracker.best_provider_with_penalties(
                &penalty_map,
                &available_names,
                is_streaming,
            )?;
            name
        };

        // Return index of best provider
        providers
            .iter()
            .position(|p| p.provider.name.as_str() == best_name)
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

    /// CostBased: Select provider with lowest active load (proxy for cost)
    /// When pricing data becomes available, this will use actual cost per token.
    /// For now, uses active_requests as cost proxy — fewer active requests = lower cost.
    fn cost_based_impl(providers: &[ProviderWithState]) -> usize {
        providers
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.active_requests)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// UsageBasedV2: Exponential decay weighting — recent usage counts more
    /// Uses success rate and current RPM as combined score.
    /// Providers with lower usage and higher success rates are preferred.
    fn usage_based_v2_impl(providers: &[ProviderWithState]) -> usize {
        providers
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| {
                // Combined score: current_rpm weighted by success rate
                // Lower score = less loaded + more reliable = preferred
                let success_rate = if p.total_count > 0 {
                    (p.success_count as f64 / p.total_count as f64 * 100.0) as u32
                } else {
                    100 // No history = assume good
                };
                // Score = RPM * (100 - success_rate) / 100
                // Higher success rate reduces the score
                p.current_rpm.saturating_mul(100 - success_rate) / 100
            })
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Weighted: Select provider using global weights map (provider.name → weight)
    /// Falls back to get_routing_weight() if provider not in weights map
    fn weighted_impl(providers: &[ProviderWithState], weights: &HashMap<String, u32>) -> usize {
        let weight_list: Vec<u32> = providers
            .iter()
            .map(|p| {
                weights
                    .get(&p.provider.name)
                    .copied()
                    .unwrap_or_else(|| p.get_routing_weight())
            })
            .collect();

        let total_weight: u32 = weight_list.iter().sum();

        if total_weight == 0 {
            rand::rng().random_range(0..providers.len())
        } else {
            let mut cumulative = 0u32;
            let weighted: Vec<u32> = weight_list
                .iter()
                .map(|&w| {
                    cumulative += w;
                    cumulative
                })
                .collect();

            let roll = rand::rng().random_range(1..=total_weight);
            weighted.iter().position(|&w| w >= roll).unwrap_or(0)
        }
    }

    /// Record request start for a specific provider index
    pub fn record_request_start(&mut self, model_group: &str, index: usize) {
        if let Some(providers) = self.providers.get_mut(model_group) {
            if let Some(p) = providers.get_mut(index) {
                p.request_started();
            }
        }
    }

    /// Record request end for a specific provider index (latency in microseconds).
    /// Updates per-model-group latency tracking AND cross-model-group LatencyTracker.
    /// If ttft_us is Some, also records TTFT for streaming requests.
    ///
    /// ProviderBudgetLimiting is OUT OF SCOPE for this module.
    /// Per-provider budget limiting is handled by the budget enforcement layer (RFC-0904).
    /// CostBased routing selects lowest-cost provider but does not enforce per-provider budgets.
    pub fn record_request_end(
        &mut self,
        model_group: &str,
        index: usize,
        latency_us: u64,
        tokens: u32,
        ttft_us: Option<u64>,
    ) {
        let latency_window = self.config.latency_window;

        // Get provider name for LatencyTracker update (clone to avoid borrow conflict)
        let provider_name = self
            .providers
            .get(model_group)
            .and_then(|p| p.get(index))
            .map(|p| p.provider.name.clone());

        // Update per-model-group ProviderWithState
        if let Some(providers) = self.providers.get_mut(model_group) {
            if let Some(p) = providers.get_mut(index) {
                p.request_ended(latency_us, tokens, latency_window);
            }
        }

        // Update cross-model-group LatencyTracker (Phase 2 integration)
        if let Some(name) = provider_name {
            self.latency_tracker.record(&name, latency_us, ttft_us);
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

    /// Update latency state after a request completion.
    /// Called by the caller after each request completes with success/failure info.
    /// Note: latency_us and config are passed for future TTFT/LatencyTracker integration
    /// but are not used in v1 (latency tracking happens via ProviderWithState.latencies).
    #[allow(unused_variables)]
    pub fn update_latency_state(
        &mut self,
        model_group: &str,
        index: usize,
        success: bool,
        latency_us: u64,
        config: &LatencyConfig,
        is_single_deployment: bool,
    ) {
        let Some(provider) = self
            .providers
            .get_mut(model_group)
            .and_then(|p| p.get_mut(index))
        else {
            return;
        };
        let tracker = &mut provider.cooldown_tracker;

        match tracker.state {
            DeploymentState::Cooldown => {
                // Check if cooldown TTL expired
                if tracker.is_cooldown_expired() {
                    tracker.state = DeploymentState::Healthy;
                    tracker.reset_minute_window();
                    tracker.clear_penalty_latencies();
                }
                // During cooldown, still record stats for failure rate tracking
                if success {
                    tracker.record_success();
                } else {
                    tracker.record_error();
                }
            }
            DeploymentState::Healthy => {
                if success {
                    tracker.record_success();
                } else {
                    tracker.record_error();
                }

                // Check if should enter cooldown based on failure rate
                if tracker.should_enter_cooldown(
                    config.failure_threshold_percent,
                    config.failure_threshold_min_requests,
                    is_single_deployment,
                ) {
                    tracker.enter_cooldown(config.cooldown_duration_secs);
                }
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
            if let Some(idx) = router.route("gpt-3.5-turbo", false) {
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
            if let Some(idx) = router.route("gpt-3.5-turbo", false) {
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
        if let Some(idx) = router.route("gpt-3.5-turbo", false) {
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
            "latency-based-routing".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::LatencyBased
        );
        assert_eq!(
            "usage-based".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::UsageBased
        );
        assert_eq!(
            "weighted".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::Weighted
        );
    }

    #[test]
    fn test_routing_strategy_from_litellm_str() {
        // Standard LiteLLM strategy strings
        assert_eq!(
            RoutingStrategy::from_litellm_str("simple-shuffle"),
            RoutingStrategy::SimpleShuffle
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("round-robin"),
            RoutingStrategy::RoundRobin
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("least-busy"),
            RoutingStrategy::LeastBusy
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("latency-based"),
            RoutingStrategy::LatencyBased
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("latency-based-routing"),
            RoutingStrategy::LatencyBased
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("cost-based"),
            RoutingStrategy::CostBased
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("cost-based-routing"),
            RoutingStrategy::CostBased
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("usage-based"),
            RoutingStrategy::UsageBased
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("usage-based-v2"),
            RoutingStrategy::UsageBasedV2
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("usage-based-routing-v2"),
            RoutingStrategy::UsageBasedV2
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("weighted"),
            RoutingStrategy::Weighted
        );

        // Underscore variants (LiteLLM uses underscores in some configs)
        assert_eq!(
            RoutingStrategy::from_litellm_str("latency_based_routing"),
            RoutingStrategy::LatencyBased
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("usage_based_v2"),
            RoutingStrategy::UsageBasedV2
        );

        // Unknown strategy defaults to SimpleShuffle
        assert_eq!(
            RoutingStrategy::from_litellm_str("unknown-strategy"),
            RoutingStrategy::SimpleShuffle
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str(""),
            RoutingStrategy::SimpleShuffle
        );

        // Case insensitive
        assert_eq!(
            RoutingStrategy::from_litellm_str("LATENCY-BASED-ROUTING"),
            RoutingStrategy::LatencyBased
        );
        assert_eq!(
            RoutingStrategy::from_litellm_str("Simple-Shuffle"),
            RoutingStrategy::SimpleShuffle
        );
    }

    #[test]
    fn test_latency_based_routing() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::LatencyBased,
            latency_window: 10,
            latency_config: LatencyConfig::default(),
            verbose: false,
            weights: HashMap::new(),
        };
        let mut router = Router::new(config, providers);

        // Set latencies (in microseconds) - azure should be faster
        if let Some(providers) = router.providers.get_mut("gpt-3.5-turbo") {
            for p in providers.iter_mut() {
                if p.provider.name == "azure" {
                    p.latencies = vec![100_000, 110_000, 105_000]; // Fast: ~105ms avg
                } else {
                    p.latencies = vec![500_000, 510_000, 505_000]; // Slow: ~505ms avg
                }
            }
        }

        // Should select azure (lower latency)
        if let Some(idx) = router.route("gpt-3.5-turbo", false) {
            if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                assert_eq!(p.provider.name, "azure");
            }
        }
    }

    #[test]
    fn test_latency_based_routing_with_cooldown() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::LatencyBased,
            latency_window: 10,
            latency_config: LatencyConfig::default(),
            verbose: false,
            weights: HashMap::new(),
        };
        let mut router = Router::new(config, providers);

        // Populate latency tracker - azure should be faster
        // (new implementation uses latency_tracker, not p.latencies)
        if let Some(providers) = router.providers.get_mut("gpt-3.5-turbo") {
            for p in providers.iter_mut() {
                if p.provider.name == "azure" {
                    router.latency_tracker.record("azure", 100_000, None);
                    router.latency_tracker.record("azure", 110_000, None);
                    router.latency_tracker.record("azure", 105_000, None);
                } else {
                    router.latency_tracker.record("openai", 500_000, None);
                    router.latency_tracker.record("openai", 510_000, None);
                    router.latency_tracker.record("openai", 505_000, None);
                }
            }
        }

        // Should select azure (lower latency, not in cooldown)
        let idx = router.route("gpt-3.5-turbo", false).unwrap();
        assert_eq!(
            router
                .get_provider("gpt-3.5-turbo", idx)
                .unwrap()
                .provider
                .name,
            "azure"
        );

        // Put azure in cooldown
        if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
            p.cooldown_tracker.enter_cooldown(60);
        }

        // Should now select openai (azure is in cooldown)
        let idx2 = router.route("gpt-3.5-turbo", false).unwrap();
        assert_eq!(
            router
                .get_provider("gpt-3.5-turbo", idx2)
                .unwrap()
                .provider
                .name,
            "openai"
        );
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
        if let Some(idx) = router.route("gpt-3.5-turbo", false) {
            if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                assert_eq!(p.provider.name, "azure");
            }
        }
    }

    #[test]
    fn test_weighted_routing() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::Weighted,
            weights: HashMap::from([("openai".to_string(), 10), ("azure".to_string(), 1)]),
            ..Default::default()
        };
        let mut router = Router::new(config, providers);

        // Should favor openai (weight 10) over azure (weight 1)
        let mut openai_count = 0;
        let mut azure_count = 0;

        for _ in 0..1000 {
            if let Some(idx) = router.route("gpt-3.5-turbo", false) {
                if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
                    if p.provider.name == "openai" {
                        openai_count += 1;
                    } else {
                        azure_count += 1;
                    }
                }
            }
        }

        // OpenAI should be selected significantly more often (10:1 weight ratio)
        assert!(openai_count > azure_count * 5);
    }

    #[test]
    fn test_request_tracking() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);

        // Route and track request
        let idx = router.route("gpt-3.5-turbo", false).unwrap();
        router.record_request_start("gpt-3.5-turbo", idx);

        // Check active requests increased
        if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
            assert_eq!(p.active_requests, 1);
        }

        // Record request end (latency in microseconds)
        router.record_request_end("gpt-3.5-turbo", idx, 150_000, 100, None);

        // Check active requests decreased and latency recorded
        if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
            assert_eq!(p.active_requests, 0);
            assert!(!p.latencies.is_empty());
            assert_eq!(p.current_rpm, 1);
            assert_eq!(p.total_count, 1);
        }
    }

    #[test]
    fn test_success_tracking() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);

        let idx = router.route("gpt-3.5-turbo", false).unwrap();
        router.record_request_start("gpt-3.5-turbo", idx);

        // Get provider and record success
        if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
            p.record_success();
            assert_eq!(p.success_count, 1);
        }

        // Record request end
        router.record_request_end("gpt-3.5-turbo", idx, 100_000, 50, None);

        // Verify total_count incremented
        if let Some(p) = router.get_provider("gpt-3.5-turbo", idx) {
            assert_eq!(p.total_count, 1);
            assert_eq!(p.success_count, 1);
        }
    }

    #[test]
    fn test_provider_metrics_record_and_query() {
        let mut metrics = ProviderMetrics::with_ttl(60);

        // Record a request for deployment
        metrics.record("openai-gpt4", 100);

        // Should have rpm=1, tpm=100 for current minute
        let minute = ProviderMetrics::current_minute_key_for_test();
        assert_eq!(metrics.rpm_at("openai-gpt4", &minute), Some(1));
        assert_eq!(metrics.tpm_at("openai-gpt4", &minute), Some(100));

        // Current rpm/tpm should also reflect
        assert_eq!(metrics.current_rpm("openai-gpt4"), 1);
        assert_eq!(metrics.current_tpm("openai-gpt4"), 100);
    }

    #[test]
    fn test_provider_metrics_can_accept_request() {
        let mut metrics = ProviderMetrics::with_ttl(60);

        // Record some requests
        metrics.record("openai-gpt4", 50);
        metrics.record("openai-gpt4", 50);

        // Should be able to accept (current rpm=2, tpm=100)
        assert!(metrics.can_accept_request("openai-gpt4", 10, 1000, 50));

        // Should NOT accept if would exceed RPM limit
        // After 10 requests, rpm would be 11 > 10 limit
        for _ in 0..8 {
            metrics.record("openai-gpt4", 1);
        }
        // Now rpm=10, adding 1 would exceed limit of 10
        assert!(!metrics.can_accept_request("openai-gpt4", 10, 1000, 50));
    }

    #[test]
    fn test_provider_metrics_eviction() {
        let mut metrics = ProviderMetrics::with_ttl(1); // 1 second TTL for testing

        // Record for deployment
        metrics.record("test-deployment", 100);

        // Should have data
        let minute = ProviderMetrics::current_minute_key_for_test();
        assert!(metrics.rpm_at("test-deployment", &minute).is_some());

        // Evict should remove old buckets
        metrics.evict_old_buckets();

        // After eviction with 1s TTL and no time passed, should still have data
        let minute = ProviderMetrics::current_minute_key_for_test();
        assert!(metrics.rpm_at("test-deployment", &minute).is_some());
    }

    #[test]
    fn test_provider_metrics_rolling_avg() {
        let mut metrics = ProviderMetrics::with_ttl(60);

        // Record multiple requests
        metrics.record("test-deployment", 100);
        metrics.record("test-deployment", 100);
        metrics.record("test-deployment", 100);

        // Rolling average over last 5 minutes should return Some
        let avg = metrics.rolling_avg_rpm("test-deployment", 5);
        assert!(avg.is_some());
        assert_eq!(avg.unwrap(), 3.0);
    }

    #[test]
    fn test_deployment_state_default() {
        assert_eq!(DeploymentState::default(), DeploymentState::Healthy);
    }

    #[test]
    fn test_latency_config_default() {
        let config = LatencyConfig::default();
        assert_eq!(config.lowest_latency_buffer, 0.0);
        assert_eq!(config.max_latency_list_size, 10);
        assert_eq!(config.timeout_penalty_us, 1_000_000_000);
        assert_eq!(config.cooldown_duration_secs, 5);
        assert_eq!(config.failure_threshold_percent, 0.5);
        assert_eq!(config.failure_threshold_min_requests, 5);
    }

    #[test]
    fn test_cooldown_tracker_record_success() {
        let mut tracker = CooldownTracker::default();
        tracker.record_success();
        assert_eq!(tracker.total_requests, 1);
        assert_eq!(tracker.failed_requests, 0);
    }

    #[test]
    fn test_cooldown_tracker_record_429_exempted() {
        let mut tracker = CooldownTracker::default();
        let entered = tracker.record_429(5, true); // single deployment
        assert!(!entered);
        assert_eq!(tracker.state, DeploymentState::Healthy);
    }

    #[test]
    fn test_cooldown_tracker_record_429_enter_cooldown() {
        let mut tracker = CooldownTracker::default();
        let entered = tracker.record_429(5, false); // not single deployment
        assert!(entered);
        assert_eq!(tracker.state, DeploymentState::Cooldown);
        assert!(tracker.cooldown_end_time.is_some());
    }

    #[test]
    fn test_cooldown_tracker_should_enter_cooldown() {
        let mut tracker = CooldownTracker::default();

        // Not enough requests yet
        assert!(!tracker.should_enter_cooldown(0.5, 5, false));

        // Add 5 requests with 3 failures (60% failure rate > 50%)
        for _ in 0..2 {
            tracker.record_success();
        }
        for _ in 0..3 {
            tracker.record_error();
        }

        assert!(tracker.should_enter_cooldown(0.5, 5, false));
    }

    #[test]
    fn test_cooldown_tracker_single_deployment_exempt() {
        let mut tracker = CooldownTracker::default();

        // Add enough requests to trigger cooldown
        for _ in 0..2 {
            tracker.record_success();
        }
        for _ in 0..3 {
            tracker.record_error();
        }

        // Single deployment should NOT enter cooldown
        assert!(!tracker.should_enter_cooldown(0.5, 5, true));
    }

    #[test]
    fn test_cooldown_tracker_is_available() {
        let mut tracker = CooldownTracker::default();
        assert!(tracker.is_available());

        tracker.enter_cooldown(60);
        assert!(!tracker.is_available());

        // Manually expire the cooldown by setting end time to the past (2+ minutes ago)
        tracker.cooldown_end_time =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(120));
        assert!(tracker.is_available());
    }

    #[test]
    fn test_cooldown_tracker_reset_minute_window() {
        let mut tracker = CooldownTracker::default();
        tracker.record_success();
        tracker.record_error();
        assert_eq!(tracker.total_requests, 2);
        assert_eq!(tracker.failed_requests, 1);

        tracker.reset_minute_window();
        assert_eq!(tracker.total_requests, 0);
        assert_eq!(tracker.failed_requests, 0);
    }

    #[test]
    fn test_cooldown_tracker_record_timeout_penalty() {
        let mut tracker = CooldownTracker::default();
        tracker.record_timeout_penalty(1_000_000_000);
        assert_eq!(tracker.total_requests, 1);
        assert_eq!(tracker.failed_requests, 1);
        assert_eq!(tracker.penalty_latencies.len(), 1);
        assert_eq!(tracker.penalty_latencies[0], 1_000_000_000);
    }

    #[test]
    fn test_latency_tracker_with_ttft() {
        let mut tracker = LatencyTracker {
            samples: HashMap::new(),
            ttft_samples: HashMap::new(),
        };

        // Record latency and TTFT for two providers
        tracker.record("fast", 100_000, Some(20_000)); // 100ms latency, 20ms TTFT
        tracker.record("slow", 500_000, Some(100_000)); // 500ms latency, 100ms TTFT

        // Non-streaming should use regular latency
        let best = tracker.best_provider_with_ttft(false, 0.0);
        assert_eq!(best, Some("fast"));

        // Streaming with TTFT should prefer fast TTFT provider
        let best_streaming = tracker.best_provider_with_ttft(true, 0.0);
        assert_eq!(best_streaming, Some("fast"));
    }

    #[test]
    fn test_latency_tracker_best_provider_among() {
        let mut tracker = LatencyTracker {
            samples: HashMap::new(),
            ttft_samples: HashMap::new(),
        };

        tracker.record("fast", 100_000, None);
        tracker.record("slow", 500_000, None);

        // Only allow "slow" provider
        let available: std::collections::HashSet<&str> = std::collections::HashSet::from(["slow"]);
        let best = tracker.best_provider_among(available, false);
        assert_eq!(best, Some("slow"));

        // Neither available
        let available: std::collections::HashSet<&str> =
            std::collections::HashSet::from(["nonexistent"]);
        let best = tracker.best_provider_among(available, false);
        assert_eq!(best, None);
    }

    #[test]
    fn test_latency_tracker_latency_buffer() {
        let mut tracker = LatencyTracker {
            samples: HashMap::new(),
            ttft_samples: HashMap::new(),
        };

        // Provider A: 100ms, Provider B: 105ms (within 10% buffer of 100ms)
        tracker.record("A", 100_000, None);
        tracker.record("B", 105_000, None);

        // With 0 buffer, only A should be selected (lowest latency)
        let best = tracker.best_provider_with_ttft(false, 0.0);
        assert_eq!(best, Some("A"));

        // With 0.1 buffer (10%), both should be valid
        // Check by restricting to each provider individually
        let best_a = tracker.best_provider_among(std::collections::HashSet::from(["A"]), false);
        let best_b = tracker.best_provider_among(std::collections::HashSet::from(["B"]), false);
        assert_eq!(best_a, Some("A"));
        assert_eq!(best_b, Some("B"));
    }

    #[test]
    fn test_update_latency_state_success() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);
        let latency_config = LatencyConfig::default();

        // Record a success
        router.update_latency_state("gpt-3.5-turbo", 0, true, 100_000, &latency_config, false);

        // Check cooldown tracker state
        if let Some(p) = router.get_provider("gpt-3.5-turbo", 0) {
            assert_eq!(p.cooldown_tracker.state, DeploymentState::Healthy);
            assert_eq!(p.cooldown_tracker.total_requests, 1);
            assert_eq!(p.cooldown_tracker.failed_requests, 0);
        }
    }

    #[test]
    fn test_update_latency_state_failure() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);
        let latency_config = LatencyConfig::default();

        // Record a failure
        router.update_latency_state("gpt-3.5-turbo", 0, false, 100_000, &latency_config, false);

        // Check cooldown tracker state
        if let Some(p) = router.get_provider("gpt-3.5-turbo", 0) {
            assert_eq!(p.cooldown_tracker.state, DeploymentState::Healthy);
            assert_eq!(p.cooldown_tracker.total_requests, 1);
            assert_eq!(p.cooldown_tracker.failed_requests, 1);
        }
    }

    #[test]
    fn test_update_latency_state_cooldown_expired() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);
        let latency_config = LatencyConfig::default();

        // Enter cooldown first
        if let Some(p) = router.get_provider("gpt-3.5-turbo", 0) {
            p.cooldown_tracker.enter_cooldown(1); // 1 second cooldown
        }

        // Wait for cooldown to expire
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Record another request which should trigger expiry check
        router.update_latency_state("gpt-3.5-turbo", 0, true, 100_000, &latency_config, false);

        // Check cooldown has expired and tracker is healthy
        if let Some(p) = router.get_provider("gpt-3.5-turbo", 0) {
            assert_eq!(p.cooldown_tracker.state, DeploymentState::Healthy);
        }
    }

    #[test]
    fn test_update_latency_state_invalid_model_group() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);
        let latency_config = LatencyConfig::default();

        // Should not panic with invalid model group
        router.update_latency_state("nonexistent", 0, true, 100_000, &latency_config, false);
    }

    #[test]
    fn test_update_latency_state_invalid_index() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);
        let latency_config = LatencyConfig::default();

        // Should not panic with invalid index
        router.update_latency_state("gpt-3.5-turbo", 99, true, 100_000, &latency_config, false);
    }

    #[test]
    fn test_update_latency_state_enter_cooldown_on_failure_rate() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);
        let latency_config = LatencyConfig {
            failure_threshold_percent: 0.5,
            failure_threshold_min_requests: 5,
            cooldown_duration_secs: 60,
            ..Default::default()
        };

        // Simulate failure rate > 50% (3 failures out of 4 requests)
        for _ in 0..2 {
            router.update_latency_state("gpt-3.5-turbo", 0, true, 100_000, &latency_config, false);
        }
        for _ in 0..3 {
            router.update_latency_state("gpt-3.5-turbo", 0, false, 100_000, &latency_config, false);
        }

        // Should have entered cooldown due to 60% failure rate (> 50% threshold)
        if let Some(p) = router.get_provider("gpt-3.5-turbo", 0) {
            assert_eq!(p.cooldown_tracker.state, DeploymentState::Cooldown);
        }
    }

    #[test]
    fn test_update_latency_state_single_deployment_no_cooldown() {
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut router = Router::new(config, providers);
        let latency_config = LatencyConfig {
            failure_threshold_percent: 0.5,
            failure_threshold_min_requests: 5,
            cooldown_duration_secs: 60,
            ..Default::default()
        };

        // Simulate high failure rate on a SINGLE deployment (should NOT enter cooldown)
        for _ in 0..2 {
            router.update_latency_state("gpt-3.5-turbo", 0, true, 100_000, &latency_config, true);
        }
        for _ in 0..3 {
            router.update_latency_state("gpt-3.5-turbo", 0, false, 100_000, &latency_config, true);
        }

        // Single deployment should NOT enter cooldown even with high failure rate
        if let Some(p) = router.get_provider("gpt-3.5-turbo", 0) {
            assert_eq!(p.cooldown_tracker.state, DeploymentState::Healthy);
        }
    }

    #[test]
    fn test_router_state_record_request_end_updates_both() {
        use super::*;
        let providers = test_providers();
        let config = RouterConfig::default();
        let mut rs = RouterState::new(config, providers);

        // Route to get provider index
        let idx = rs.router.route("gpt-3.5-turbo", false).unwrap();

        // Record request end with TTFT
        rs.record_request_end("gpt-3.5-turbo", idx, 50000, 100, Some(10000));

        // Verify RouterState.latency_tracker got the sample
        assert!(rs.latency_tracker.best_provider().is_some());

        // Verify Router.latency_tracker (used by LatencyBased routing) also got the sample
        assert!(rs.router.latency_tracker.best_provider().is_some());

        // Verify ProviderWithState got the latency update
        if let Some(p) = rs.router.get_provider("gpt-3.5-turbo", idx) {
            assert!(p.avg_latency_us() > 0);
        }
    }
}
