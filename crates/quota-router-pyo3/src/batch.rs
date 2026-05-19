// Batch completion functions for PyO3 bindings
// In-memory parallel batch processing per RFC-0920 lines 2207-2261

use crate::completion::completion;
use crate::types::Message;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::sync::mpsc;
use std::thread;

/// batch_completion - In-memory parallel batch completion
///
/// Executes completion requests in parallel using ThreadPoolExecutor.
/// All requests use the same model and messages.
///
/// # Arguments
/// * `model` - Model name (e.g., "openai:gpt-4")
/// * `messages` - Chat messages
/// * `n` - Number of parallel requests (default 2)
///
/// # Returns
/// List of completion results
#[pyfunction]
#[pyo3(name = "batch_completion")]
pub fn batch_completion(
    model: String,
    messages: Vec<Message>,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _n: Option<i32>,
) -> PyResult<Py<PyAny>> {
    let n = _n.unwrap_or(2).clamp(1, 10) as usize;

    let messages_clone = messages.clone();
    let model_clone = model.clone();

    // Spawn parallel threads
    let handles: Vec<_> = (0..n)
        .map(|i| {
            let model = model_clone.clone();
            let messages = messages_clone.clone();
            thread::spawn(move || {
                let result = completion(
                    model,
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
                );
                (i, result)
            })
        })
        .collect();

    // Collect results
    let results: Vec<(usize, Py<PyAny>)> = handles
        .into_iter()
        .filter_map(|h| h.join().ok())
        .filter_map(|(i, result)| result.ok().map(|r| (i, r)))
        .collect();

    Python::with_gil(|py| {
        let list = PyList::new(py, results.into_iter().map(|(_, r)| r));
        Ok(list.into())
    })
}

/// batch_completion_models - All models race, first response wins
///
/// Executes completion across multiple models in parallel.
/// Returns the first response received (wins the race).
///
/// # Arguments
/// * `models` - List of model names (e.g., ["openai:gpt-4", "anthropic:claude-3"])
/// * `messages` - Chat messages
///
/// # Returns
/// First response received (single result, not a list)
#[pyfunction]
#[pyo3(name = "batch_completion_models")]
pub fn batch_completion_models(
    models: Vec<String>,
    messages: Vec<Message>,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
) -> PyResult<Py<PyAny>> {
    if models.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "models list cannot be empty",
        ));
    }

    let messages_clone = messages.clone();

    // Use channel to receive first result (proper race semantics)
    let (tx, rx) = mpsc::channel();

    // Spawn parallel threads for each model
    for model in models {
        let tx = tx.clone();
        let messages = messages_clone.clone();
        thread::spawn(move || {
            let result = completion(
                model,
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
            );
            let _ = tx.send(result);
        });
    }

    // Wait for first result (true race - whoever completes first wins)
    match rx.recv() {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(e)) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "Model failed: {}",
            e
        ))),
        Err(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "All models failed",
        )),
    }
}

/// batch_completion_models_all_responses - All responses from all models
///
/// Executes completion across multiple models in parallel.
/// Returns all responses from all models.
///
/// # Arguments
/// * `models` - List of model names (e.g., ["openai:gpt-4", "anthropic:claude-3"])
/// * `messages` - Chat messages
///
/// # Returns
/// Dict with "responses" (list of all responses), "models" (list of model names),
/// "total_requested" (number of models requested), "total_successful" (number that succeeded),
/// and "total_failed" (number that failed)
#[pyfunction]
#[pyo3(name = "batch_completion_models_all_responses")]
pub fn batch_completion_models_all_responses(
    models: Vec<String>,
    messages: Vec<Message>,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
) -> PyResult<Py<PyAny>> {
    let total_requested = models.len();

    if models.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "models list cannot be empty",
        ));
    }

    let messages_clone = messages.clone();

    // Spawn parallel threads for each model
    let handles: Vec<_> = models
        .iter()
        .map(|model| {
            let model_name = model.clone();
            let model = model.clone();
            let messages = messages_clone.clone();
            thread::spawn(move || {
                let result = completion(
                    model,
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
                );
                (model_name, result)
            })
        })
        .collect();

    // Collect all results
    let mut responses: Vec<Py<PyAny>> = Vec::new();
    let mut model_names: Vec<String> = Vec::new();
    let mut failed_count = 0;

    for handle in handles {
        if let Ok((model_name, Ok(response))) = handle.join() {
            model_names.push(model_name);
            responses.push(response);
        } else {
            failed_count += 1;
        }
    }

    let total_successful = responses.len();

    Python::with_gil(|py| {
        let dict = PyDict::new(py);

        let responses_list = PyList::new(py, responses);
        dict.set_item("responses", responses_list)?;

        let models_list = PyList::new(py, model_names);
        dict.set_item("models", models_list)?;

        dict.set_item("total_requested", total_requested)?;
        dict.set_item("total_successful", total_successful)?;
        dict.set_item("total_failed", failed_count)?;

        Ok(dict.into())
    })
}
