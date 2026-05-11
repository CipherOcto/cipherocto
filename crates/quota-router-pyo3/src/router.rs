// Router class for PyO3 bindings
// Thin wrapper around completion() - routing strategies are Phase 4

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Routing strategy enumeration
#[derive(Debug, Clone, Default)]
pub enum RoutingStrategy {
    #[default]
    SimpleShuffle,
    RoundRobin,
    LeastBusy,
    LatencyBased,
    CostBased,
    UsageBased,
    UsageBasedV2,
    Weighted,
}

impl RoutingStrategy {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "round-robin" => RoutingStrategy::RoundRobin,
            "least-busy" => RoutingStrategy::LeastBusy,
            "latency-based" | "latency-based-routing" => RoutingStrategy::LatencyBased,
            "cost-based" | "cost-based-routing" => RoutingStrategy::CostBased,
            "usage-based" | "usage-based-routing" => RoutingStrategy::UsageBased,
            "usage-based-v2" | "usage-based-routing-v2" => RoutingStrategy::UsageBasedV2,
            "weighted" => RoutingStrategy::Weighted,
            _ => RoutingStrategy::SimpleShuffle,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            RoutingStrategy::SimpleShuffle => "simple-shuffle",
            RoutingStrategy::RoundRobin => "round-robin",
            RoutingStrategy::LeastBusy => "least-busy",
            RoutingStrategy::LatencyBased => "latency-based",
            RoutingStrategy::CostBased => "cost-based",
            RoutingStrategy::UsageBased => "usage-based",
            RoutingStrategy::UsageBasedV2 => "usage-based-v2",
            RoutingStrategy::Weighted => "weighted",
        }
    }
}

/// Router class - Thread-safe router for multi-model requests
///
/// The Router class provides a high-level interface for routing requests
/// across multiple models. It is a thin wrapper that delegates to the
/// underlying completion() function.
///
/// # Attributes
/// * `models` - List of available models
/// * `strategy` - Routing strategy (default: "simple-shuffle")
///
/// # Example
/// ```python
/// router = Router(
///     models=["openai:gpt-4", "anthropic:claude-3"],
///     strategy="round-robin"
/// )
/// response = router.completion(messages=[{"role": "user", "content": "Hello"}])
/// ```
#[pyclass]
#[pyo3(name = "Router")]
pub struct Router {
    models: Vec<String>,
    strategy: RoutingStrategy,
    /// Current index for round-robin (read-only from Python)
    #[pyo3(get)]
    current_index: usize,
}

#[pymethods]
impl Router {
    /// Create a new Router
    ///
    /// # Arguments
    /// * `models` - List of model names
    /// * `strategy` - Routing strategy (default: "simple-shuffle")
    /// * `weights` - Optional weights for weighted routing (Phase 4)
    #[new]
    pub fn new(models: Vec<String>, strategy: Option<String>, _weights: Option<Vec<f64>>) -> Self {
        let strat = strategy
            .as_deref()
            .map(RoutingStrategy::from_str)
            .unwrap_or_default();

        Router {
            models,
            strategy: strat,
            current_index: 0,
        }
    }

    /// Get current routing strategy
    #[getter]
    fn get_strategy(&self) -> &'static str {
        self.strategy.as_str()
    }

    /// Set routing strategy
    #[setter]
    fn set_strategy(&mut self, strategy: String) {
        self.strategy = RoutingStrategy::from_str(&strategy);
    }

    /// Get list of models
    #[getter]
    fn get_models(&self) -> Vec<String> {
        self.models.clone()
    }

    /// completion - Execute completion via router
    ///
    /// Routes the request based on the current strategy.
    /// For Phase 3, this is a stub that delegates to completion().
    pub fn completion(
        &mut self,
        messages: Vec<crate::types::Message>,
        _temperature: Option<f64>,
        _max_tokens: Option<i32>,
        _timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        if self.models.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Router has no models",
            ));
        }

        // Select model based on strategy
        let model = match self.strategy {
            RoutingStrategy::RoundRobin => {
                let idx = self.current_index;
                self.current_index = (self.current_index + 1) % self.models.len().max(1);
                self.models[idx % self.models.len()].clone()
            }
            _ => {
                // Simple shuffle - just use first model for now
                // Real weighted selection is Phase 4
                self.models[0].clone()
            }
        };

        // Delegate to completion
        crate::completion::completion(
            model,
            messages,
            _temperature,
            _max_tokens,
            None,     // top_p
            None,     // n
            None,     // stream
            None,     // stop
            None,     // presence_penalty
            None,     // frequency_penalty
            None,     // user
            None,     // seed
            _timeout, // timeout
            None,     // extra_headers
            None,     // base_url
            None,     // api_version
            None,     // api_key
            None,     // service_tier
            None,     // background
            None,     // prompt_cache_key
            None,     // prompt_cache_retention
            None,     // conversation
        )
    }

    /// acompletion - Async completion via router
    ///
    /// For Phase 3, this is a stub that delegates to sync completion.
    /// Round-robin state is only updated on sync completion calls.
    pub async fn acompletion(
        &self,
        messages: Vec<crate::types::Message>,
        _temperature: Option<f64>,
        _max_tokens: Option<i32>,
    ) -> PyResult<Py<PyAny>> {
        // For now, delegate to sync completion using first model
        // Real async router with state updates is Phase 4
        if self.models.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Router has no models",
            ));
        }
        crate::completion::completion(
            self.models[0].clone(),
            messages,
            _temperature,
            _max_tokens,
            None, // top_p
            None, // n
            None, // stream
            None, // stop
            None, // presence_penalty
            None, // frequency_penalty
            None, // user
            None, // seed
            None, // timeout
            None, // extra_headers
            None, // base_url
            None, // api_version
            None, // api_key
            None, // service_tier
            None, // background
            None, // prompt_cache_key
            None, // prompt_cache_retention
            None, // conversation
        )
    }

    /// list_models - List available models
    pub fn list_models(&self) -> Vec<String> {
        self.models.clone()
    }

    /// __len__ - Number of models in router
    fn __len__(&self) -> usize {
        self.models.len()
    }

    /// Get metrics from the router
    pub fn get_metrics(&self) -> PyResult<Py<PyAny>> {
        crate::sdk::get_metrics()
    }

    /// Get router info as dict
    fn __repr__(&self) -> String {
        format!(
            "Router(models={:?}, strategy={})",
            self.models,
            self.strategy.as_str()
        )
    }

    /// Get routing statistics (stub for Phase 3)
    fn get_stats(&self) -> PyResult<Py<PyAny>> {
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("strategy", self.strategy.as_str())?;
            dict.set_item("model_count", self.models.len())?;
            dict.set_item("current_index", self.current_index)?;
            Ok(dict.into())
        })
    }
}
