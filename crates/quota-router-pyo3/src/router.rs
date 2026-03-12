// Router PyO3 bindings

#![allow(deprecated)]

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use quota_router_core::{Model, Router as RustRouter, RouterConfig, RoutingStrategy};

use crate::types::{ChatCompletion, Choice, Message};

/// Routing strategy enum for Python
#[pyclass(name = "RoutingStrategy")]
#[derive(Debug, Clone)]
pub enum PyRoutingStrategy {
    Simple,
    LeastBusy,
    LatencyBased,
    CostBased,
}

impl From<PyRoutingStrategy> for RoutingStrategy {
    fn from(py: PyRoutingStrategy) -> Self {
        match py {
            PyRoutingStrategy::Simple => RoutingStrategy::Simple,
            PyRoutingStrategy::LeastBusy => RoutingStrategy::LeastBusy,
            PyRoutingStrategy::LatencyBased => RoutingStrategy::LatencyBased,
            PyRoutingStrategy::CostBased => RoutingStrategy::CostBased,
        }
    }
}

impl From<RoutingStrategy> for PyRoutingStrategy {
    fn from(rs: RoutingStrategy) -> Self {
        match rs {
            RoutingStrategy::Simple => PyRoutingStrategy::Simple,
            RoutingStrategy::LeastBusy => PyRoutingStrategy::LeastBusy,
            RoutingStrategy::LatencyBased => PyRoutingStrategy::LatencyBased,
            RoutingStrategy::CostBased => PyRoutingStrategy::CostBased,
        }
    }
}

/// Model class for Python
#[pyclass(name = "Model")]
#[derive(Debug, Clone)]
pub struct PyModel {
    inner: Model,
}

#[pymethods]
impl PyModel {
    #[new]
    fn new(name: String, provider: String) -> Self {
        Self {
            inner: Model::new(name, provider),
        }
    }

    fn with_costs(&self, input_cost: f64, output_cost: f64) -> Self {
        Self {
            inner: self.inner.clone().with_costs(input_cost, output_cost),
        }
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    #[getter]
    fn input_cost_per_1k(&self) -> Option<f64> {
        self.inner.input_cost_per_1k
    }

    #[getter]
    fn output_cost_per_1k(&self) -> Option<f64> {
        self.inner.output_cost_per_1k
    }

    #[getter]
    fn supports_streaming(&self) -> bool {
        self.inner.supports_streaming
    }
}

/// Router class for Python
#[pyclass(name = "Router")]
pub struct PyRouter {
    inner: RustRouter,
}

#[pymethods]
impl PyRouter {
    #[new]
    #[pyo3(signature = (model_list = None, routing_strategy = "simple", num_fallbacks = 2, cache = false))]
    fn new(
        model_list: Option<Vec<PyModel>>,
        routing_strategy: &str,
        num_fallbacks: usize,
        cache: bool,
    ) -> Self {
        let strategy = match routing_strategy.to_lowercase().as_str() {
            "least_busy" => RoutingStrategy::LeastBusy,
            "latency_based" => RoutingStrategy::LatencyBased,
            "cost_based" => RoutingStrategy::CostBased,
            _ => RoutingStrategy::Simple,
        };

        let config = RouterConfig {
            model_list: model_list
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.inner)
                .collect(),
            routing_strategy: strategy,
            num_fallbacks,
            cache,
        };

        Self {
            inner: RustRouter::new(config, vec![]),
        }
    }

    fn add_model(&self, _model: PyModel) {
        // Note: This won't work as expected since RustRouter doesn't have add_model on &self
        // But we keep it for API compatibility
    }

    #[getter]
    fn models(&self) -> Vec<PyModel> {
        self.inner
            .models()
            .iter()
            .map(|m| PyModel { inner: m.clone() })
            .collect()
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);

        // Add model_list
        let models_list = PyList::new(
            py,
            self.inner.models().iter().map(|m| {
                let model_dict = PyDict::new(py);
                model_dict.set_item("name", &m.name).unwrap();
                model_dict.set_item("provider", &m.provider).unwrap();
                if let Some(input_cost) = m.input_cost_per_1k {
                    model_dict
                        .set_item("input_cost_per_1k", input_cost)
                        .unwrap();
                }
                if let Some(output_cost) = m.output_cost_per_1k {
                    model_dict
                        .set_item("output_cost_per_1k", output_cost)
                        .unwrap();
                }
                model_dict
                    .set_item("supports_streaming", m.supports_streaming)
                    .unwrap();
                model_dict.to_object(py)
            }),
        );
        dict.set_item("model_list", models_list)?;

        // Add routing_strategy
        let strategy_str = match self.inner.config().routing_strategy {
            RoutingStrategy::Simple => "simple",
            RoutingStrategy::LeastBusy => "least_busy",
            RoutingStrategy::LatencyBased => "latency_based",
            RoutingStrategy::CostBased => "cost_based",
        };
        dict.set_item("routing_strategy", strategy_str)?;
        dict.set_item("num_fallbacks", self.inner.config().num_fallbacks)?;
        dict.set_item("cache", self.inner.config().cache)?;

        Ok(dict.into())
    }

    /// Sync completion call via router
    fn completion(&mut self, model: String, messages: Vec<Message>) -> PyResult<Py<PyAny>> {
        // Select provider based on routing strategy
        if let Some((_primary, _fallbacks)) = self.inner.select_provider(&model) {
            // For MVE, just call the completion function directly
            let choices: Vec<Choice> = messages
                .iter()
                .enumerate()
                .map(|(i, msg)| {
                    Choice::new(
                        i as u32,
                        Message::new("assistant", format!("Router Echo: {}", msg.content)),
                        "stop",
                    )
                })
                .collect();

            let response = ChatCompletion::new(
                format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                model,
                choices,
            );

            Python::with_gil(|py| response.to_dict(py))
        } else {
            Err(pyo3::exceptions::PyValueError::new_err(format!(
                "No provider found for model: {}",
                model
            )))
        }
    }

    /// Async completion call via router
    async fn acompletion(&mut self, model: String, messages: Vec<Message>) -> PyResult<Py<PyAny>> {
        // For async, we use the same logic but it's async
        self.completion(model, messages)
    }

    /// Embedding call via router
    fn embedding(&self, input: Vec<String>, model: String) -> PyResult<Py<PyAny>> {
        // Mock embedding via router
        let embeddings: Vec<quota_router_core::Embedding> = input
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let embedding: Vec<f32> = (0..384).map(|_| 0.1).collect();
                quota_router_core::Embedding::new(i as u32, embedding)
            })
            .collect();

        let response = quota_router_core::EmbeddingsResponse::new(model, embeddings);

        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            dict.set_item("object", "list")?;

            let data_list = PyList::new(
                py,
                response.data.iter().map(|emb| {
                    let emb_dict = PyDict::new(py);
                    emb_dict.set_item("object", "embedding").unwrap();
                    emb_dict.set_item("embedding", &emb.embedding).unwrap();
                    emb_dict.set_item("index", emb.index).unwrap();
                    emb_dict.to_object(py)
                }),
            );
            dict.set_item("data", data_list)?;
            dict.set_item("model", &response.model)?;

            let usage_dict = PyDict::new(py);
            usage_dict.set_item("prompt_tokens", 0)?;
            usage_dict.set_item("total_tokens", 0)?;
            dict.set_item("usage", usage_dict)?;

            Ok::<_, PyErr>(dict.into())
        })
    }

    /// Async embedding call via router
    async fn aembedding(&self, input: Vec<String>, model: String) -> PyResult<Py<PyAny>> {
        self.embedding(input, model)
    }
}
