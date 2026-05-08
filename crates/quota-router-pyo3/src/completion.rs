// Completion functions for PyO3 bindings

#![allow(clippy::too_many_arguments)]
#![allow(deprecated)]

use crate::model::ParsedModel;
use crate::providers::anthropic::AnthropicProvider;
use crate::providers::azure::AZUREProvider;
use crate::providers::base::LLMProvider;
use crate::providers::cohere::COHEREProvider;
use crate::providers::deepseek::DEEPSEEKProvider;
use crate::providers::gemini::GeminiProvider;
use crate::providers::groq::GROQProvider;
use crate::providers::mistral::MistralProvider;
use crate::providers::openai::OpenAIProvider;
use crate::providers::perplexity::PERPLEXITYProvider;
use crate::providers::together::TOGETHERProvider;
use crate::streaming::{chunks_to_pylist, create_chunk_list};
use crate::types::{ChatCompletion, Choice, Message};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// completion - Sync completion call
#[pyfunction]
#[pyo3(name = "completion", text_signature = "(model, messages, **kwargs)")]
pub fn completion(
    model: String,
    messages: Vec<Message>,
    // Optional parameters (match LiteLLM)
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _n: Option<i32>,
    stream: Option<bool>,
    _stop: Option<String>,
    _presence_penalty: Option<f64>,
    _frequency_penalty: Option<f64>,
    _user: Option<String>,
    // quota-router specific
    api_key: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Log the request parameters (for debugging)
    println!(
        "completion called: model={}, messages={}, stream={:?}",
        model,
        messages.len(),
        stream
    );

    // Parse model string to determine provider
    let parsed = match ParsedModel::parse(&model) {
        Ok(p) => p,
        Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e)),
    };

    // If streaming requested, use mock for now (streaming requires async)
    if stream == Some(true) {
        let content = messages
            .first()
            .map(|m| format!("Echo: {}", m.content))
            .unwrap_or_default();
        let chunks = create_chunk_list(model, content);
        return Python::with_gil(|py| chunks_to_pylist(chunks, py));
    }

    // For OpenAI provider, use real SDK
    if parsed.provider == "openai" {
        let provider = OpenAIProvider::new();

        // Initialize with api_key if provided
        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init OpenAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                // Convert to Python dict
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("OpenAI API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Anthropic provider, use real SDK
    if parsed.provider == "anthropic" {
        let provider = AnthropicProvider::new();

        // Initialize with api_key if provided
        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Anthropic client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                // Convert to Python dict
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Anthropic API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Mistral provider, use real SDK
    if parsed.provider == "mistral" {
        let provider = MistralProvider::new();

        // Initialize with api_key if provided
        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Mistral client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                // Convert to Python dict
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Mistral API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Gemini provider, use real SDK
    if parsed.provider == "gemini" {
        let provider = GeminiProvider::new();

        // Initialize with api_key if provided
        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Gemini client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                // Convert to Python dict
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Gemini API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Groq provider, use real SDK
    if parsed.provider == "groq" {
        let provider = GROQProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Groq client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Groq API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Cohere provider, use real SDK
    if parsed.provider == "cohere" {
        let provider = COHEREProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Cohere client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Cohere API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Perplexity provider, use real SDK
    if parsed.provider == "perplexity" {
        let provider = PERPLEXITYProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Perplexity client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Perplexity API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For DeepSeek provider, use real SDK
    if parsed.provider == "deepseek" {
        let provider = DEEPSEEKProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init DeepSeek client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("DeepSeek API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Azure provider, use real SDK
    if parsed.provider == "azure" {
        let provider = AZUREProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Azure client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Azure API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Together provider, use real SDK
    if parsed.provider == "together" {
        let provider = TOGETHERProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Together client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Together API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For other providers, use mock response
    let content = messages
        .first()
        .map(|m| format!("{} Echo: {}", parsed.provider, m.content))
        .unwrap_or_default();

    let choices: Vec<Choice> = vec![Choice::new(
        0,
        Message::new("assistant", content),
        "stop",
    )];

    let response =
        ChatCompletion::new(format!("chatcmpl-{}", uuid::Uuid::new_v4()), model, choices);

    // Convert to Python dict
    let result = Python::with_gil(|py| response.to_dict(py))?;

    Ok(result)
}

/// acompletion - Async completion call
#[pyfunction]
#[pyo3(name = "acompletion")]
pub async fn acompletion(
    model: String,
    messages: Vec<Message>,
    // Optional parameters (match LiteLLM)
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _n: Option<i32>,
    _stream: Option<bool>,
    _stop: Option<String>,
    _presence_penalty: Option<f64>,
    _frequency_penalty: Option<f64>,
    _user: Option<String>,
    // quota-router specific
    _api_key: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Log the request parameters
    println!(
        "acompletion called: model={}, messages={}",
        model,
        messages.len()
    );

    // Convert messages to response choices
    let choices: Vec<Choice> = messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            Choice::new(
                i as u32,
                Message::new("assistant", format!("Async Echo: {}", msg.content)),
                "stop",
            )
        })
        .collect();

    let response =
        ChatCompletion::new(format!("chatcmpl-{}", uuid::Uuid::new_v4()), model, choices);

    // Convert to Python dict
    Python::with_gil(|py| response.to_dict(py))
}

/// embedding - Sync embedding call
#[pyfunction]
#[pyo3(name = "embedding", text_signature = "(input, model, **kwargs)")]
pub fn embedding(input: Vec<String>, model: String) -> PyResult<Py<PyAny>> {
    println!("embedding called: model={}, input={}", model, input.len());

    // Mock embedding response
    let embeddings: Vec<crate::types::Embedding> = input
        .iter()
        .enumerate()
        .map(|(i, _)| {
            // Generate a simple mock embedding (in production, call the model)
            let embedding: Vec<f32> = (0..384).map(|_| 0.1).collect();
            crate::types::Embedding::new(i as u32, embedding)
        })
        .collect();

    let response = crate::types::EmbeddingsResponse::new(model, embeddings);

    // Convert to dict
    let result = Python::with_gil(|py| {
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
        for (i, emb) in response.data.iter().enumerate() {
            let emb_dict = PyDict::new(py);
            emb_dict.set_item("object", "embedding")?;
            emb_dict.set_item("embedding", &emb.embedding)?;
            emb_dict.set_item("index", emb.index)?;
            data_list.set_item(i, emb_dict)?;
        }
        dict.set_item("data", data_list)?;
        dict.set_item("model", &response.model)?;

        let usage_dict = PyDict::new(py);
        usage_dict.set_item("prompt_tokens", 0)?;
        usage_dict.set_item("total_tokens", 0)?;
        dict.set_item("usage", usage_dict)?;

        Ok::<_, PyErr>(dict.into())
    })?;

    Ok(result)
}

/// aembedding - Async embedding call
#[pyfunction]
#[pyo3(name = "aembedding")]
pub async fn aembedding(input: Vec<String>, model: String) -> PyResult<Py<PyAny>> {
    println!("aembedding called: model={}, input={}", model, input.len());

    // Mock embedding response
    let embeddings: Vec<crate::types::Embedding> = input
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let embedding: Vec<f32> = (0..384).map(|_| 0.1).collect();
            crate::types::Embedding::new(i as u32, embedding)
        })
        .collect();

    let response = crate::types::EmbeddingsResponse::new(model, embeddings);

    // Convert to dict
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

// =============================================================================
// Messages API (text completion with messages format)
// =============================================================================

/// messages - Sync messages API call
#[pyfunction]
#[pyo3(name = "messages", text_signature = "(model, messages, **kwargs)")]
pub fn messages(
    model: String,
    messages: Vec<Message>,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _stop: Option<String>,
    _user: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!(
        "messages called: model={}, messages={}",
        model,
        messages.len()
    );

    // Mock response
    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", format!("msg-{}", uuid::Uuid::new_v4()))?;
        dict.set_item("object", "chat.completion.message")?;
        dict.set_item(
            "created",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )?;

        let role_dict = PyDict::new(py);
        role_dict.set_item("role", "assistant")?;
        role_dict.set_item("content", "Mock response from messages API")?;
        dict.set_item("role", role_dict)?;

        let usage_dict = PyDict::new(py);
        usage_dict.set_item("prompt_tokens", 10)?;
        usage_dict.set_item("completion_tokens", 20)?;
        usage_dict.set_item("total_tokens", 30)?;
        dict.set_item("usage", usage_dict)?;

        Ok::<_, PyErr>(dict.into())
    })
}

/// amessages - Async messages API call
#[pyfunction]
#[pyo3(name = "amessages")]
pub async fn amessages(
    model: String,
    messages: Vec<Message>,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _stop: Option<String>,
    _user: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!(
        "amessages called: model={}, messages={}",
        model,
        messages.len()
    );

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", format!("msg-{}", uuid::Uuid::new_v4()))?;
        dict.set_item("object", "chat.completion.message")?;
        dict.set_item(
            "created",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )?;

        let role_dict = PyDict::new(py);
        role_dict.set_item("role", "assistant")?;
        role_dict.set_item("content", "Mock async response from messages API")?;
        dict.set_item("role", role_dict)?;

        let usage_dict = PyDict::new(py);
        usage_dict.set_item("prompt_tokens", 10)?;
        usage_dict.set_item("completion_tokens", 20)?;
        usage_dict.set_item("total_tokens", 30)?;
        dict.set_item("usage", usage_dict)?;

        Ok::<_, PyErr>(dict.into())
    })
}

// =============================================================================
// Responses API (OpenAI Responses API)
// =============================================================================

/// responses - Sync responses API call
#[pyfunction]
#[pyo3(name = "responses", text_signature = "(model, input, **kwargs)")]
pub fn responses(
    model: String,
    input: String,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _user: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!("responses called: model={}, input={}", model, input.len());

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", format!("resp-{}", uuid::Uuid::new_v4()))?;
        dict.set_item("object", "response")?;
        dict.set_item(
            "created",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )?;
        dict.set_item("model", &model)?;

        let output_dict = PyDict::new(py);
        output_dict.set_item("type", "message")?;
        let message_dict = PyDict::new(py);
        message_dict.set_item("role", "assistant")?;
        message_dict.set_item("content", vec![PyDict::new(py)])?;
        output_dict.set_item("message", message_dict)?;
        dict.set_item("output", vec![output_dict])?;

        let usage_dict = PyDict::new(py);
        usage_dict.set_item("input_tokens", 10)?;
        usage_dict.set_item("output_tokens", 20)?;
        usage_dict.set_item("total_tokens", 30)?;
        dict.set_item("usage", usage_dict)?;

        Ok::<_, PyErr>(dict.into())
    })
}

/// aresponses - Async responses API call
#[pyfunction]
#[pyo3(name = "aresponses")]
pub async fn aresponses(
    model: String,
    input: String,
    _temperature: Option<f64>,
    _max_tokens: Option<i32>,
    _top_p: Option<f64>,
    _user: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!("aresponses called: model={}, input={}", model, input.len());

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", format!("resp-{}", uuid::Uuid::new_v4()))?;
        dict.set_item("object", "response")?;
        dict.set_item(
            "created",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )?;
        dict.set_item("model", &model)?;

        let output_dict = PyDict::new(py);
        output_dict.set_item("type", "message")?;
        let message_dict = PyDict::new(py);
        message_dict.set_item("role", "assistant")?;
        message_dict.set_item("content", vec![PyDict::new(py)])?;
        output_dict.set_item("message", message_dict)?;
        dict.set_item("output", vec![output_dict])?;

        let usage_dict = PyDict::new(py);
        usage_dict.set_item("input_tokens", 10)?;
        usage_dict.set_item("output_tokens", 20)?;
        usage_dict.set_item("total_tokens", 30)?;
        dict.set_item("usage", usage_dict)?;

        Ok::<_, PyErr>(dict.into())
    })
}

// =============================================================================
// Model Listing API
// =============================================================================

/// list_models - Sync list models API
#[pyfunction]
#[pyo3(name = "list_models")]
pub fn list_models(_provider: Option<String>) -> PyResult<Py<PyAny>> {
    println!("list_models called: provider={:?}", _provider);

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("object", "list")?;

        // Add mock models
        let models = [
            ("gpt-4o", "openai"),
            ("gpt-4o-mini", "openai"),
            ("claude-3-5-sonnet-20241022", "anthropic"),
            ("claude-3-5-haiku-20241022", "anthropic"),
            ("mistral-large-latest", "mistral"),
            ("llama-3.1-70b-instruct", "meta-llama"),
        ];

        let data_list = PyList::new(
            py,
            models.iter().enumerate().map(|(i, (id, provider))| {
                let model_dict = PyDict::new(py);
                model_dict.set_item("id", *id).unwrap();
                model_dict.set_item("object", "model").unwrap();
                model_dict.set_item("provider", *provider).unwrap();
                model_dict
                    .set_item("created", 1700000000u64 + i as u64)
                    .unwrap();
                model_dict.set_item("context_window", 128000).unwrap();
                model_dict.to_object(py)
            }),
        );

        dict.set_item("data", data_list)?;
        Ok::<_, PyErr>(dict.into())
    })
}

/// alist_models - Async list models API
#[pyfunction]
#[pyo3(name = "alist_models")]
pub async fn alist_models(provider: Option<String>) -> PyResult<Py<PyAny>> {
    println!("alist_models called: provider={:?}", provider);
    list_models(provider)
}

// =============================================================================
// Batch API
// =============================================================================

/// create_batch - Sync create batch API
#[pyfunction]
#[pyo3(
    name = "create_batch",
    text_signature = "(model, input_file_id, **kwargs)"
)]
pub fn create_batch(
    model: String,
    input_file_id: String,
    _endpoint: Option<String>,
    _completion_window: Option<String>,
    _metadata: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!(
        "create_batch called: model={}, input_file_id={}",
        model, input_file_id
    );

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", format!("batch-{}", uuid::Uuid::new_v4()))?;
        dict.set_item("object", "batch")?;
        dict.set_item(
            "created_at",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )?;
        dict.set_item("model", &model)?;
        dict.set_item("input_file_id", &input_file_id)?;
        dict.set_item("status", "validating")?;
        dict.set_item("completion_window", "24h")?;
        Ok::<_, PyErr>(dict.into())
    })
}

/// acreate_batch - Async create batch API
#[pyfunction]
#[pyo3(name = "acreate_batch")]
pub async fn acreate_batch(
    model: String,
    input_file_id: String,
    endpoint: Option<String>,
    completion_window: Option<String>,
    metadata: Option<String>,
) -> PyResult<Py<PyAny>> {
    create_batch(model, input_file_id, endpoint, completion_window, metadata)
}

/// retrieve_batch - Sync retrieve batch API
#[pyfunction]
#[pyo3(name = "retrieve_batch", text_signature = "(batch_id)")]
pub fn retrieve_batch(batch_id: String) -> PyResult<Py<PyAny>> {
    println!("retrieve_batch called: batch_id={}", batch_id);

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", &batch_id)?;
        dict.set_item("object", "batch")?;
        dict.set_item("created_at", 1700000000u64)?;
        dict.set_item("model", "gpt-4o")?;
        dict.set_item("input_file_id", "file-abc123")?;
        dict.set_item("status", "in_progress")?;
        dict.set_item("completion_window", "24h")?;
        dict.set_item("output_file_id", py.None())?;
        dict.set_item("error_file_id", py.None())?;
        dict.set_item("metadata", PyDict::new(py))?;
        Ok::<_, PyErr>(dict.into())
    })
}

/// aretrieve_batch - Async retrieve batch API
#[pyfunction]
#[pyo3(name = "aretrieve_batch")]
pub async fn aretrieve_batch(batch_id: String) -> PyResult<Py<PyAny>> {
    retrieve_batch(batch_id)
}

/// cancel_batch - Sync cancel batch API
#[pyfunction]
#[pyo3(name = "cancel_batch", text_signature = "(batch_id)")]
pub fn cancel_batch(batch_id: String) -> PyResult<Py<PyAny>> {
    println!("cancel_batch called: batch_id={}", batch_id);

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", &batch_id)?;
        dict.set_item("object", "batch")?;
        dict.set_item("created_at", 1700000000u64)?;
        dict.set_item("model", "gpt-4o")?;
        dict.set_item("input_file_id", "file-abc123")?;
        dict.set_item("status", "cancelled")?;
        dict.set_item("completion_window", "24h")?;
        dict.set_item(
            "cancelled_at",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )?;
        dict.set_item("metadata", PyDict::new(py))?;
        Ok::<_, PyErr>(dict.into())
    })
}

/// acancel_batch - Async cancel batch API
#[pyfunction]
#[pyo3(name = "acancel_batch")]
pub async fn acancel_batch(batch_id: String) -> PyResult<Py<PyAny>> {
    cancel_batch(batch_id)
}

/// list_batches - Sync list batches API
#[pyfunction]
#[pyo3(name = "list_batches")]
pub fn list_batches(
    _limit: Option<i32>,
    _after: Option<String>,
    _before: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!("list_batches called");

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("object", "list")?;

        // Add mock batches
        let batches: Vec<(i32, &str, &str)> = vec![
            (0, "completed", "file-0"),
            (1, "in_progress", "file-1"),
            (2, "in_progress", "file-2"),
        ];

        let data_list = PyList::new(
            py,
            batches.iter().map(|(i, status, file_id)| {
                let batch_dict = PyDict::new(py);
                batch_dict.set_item("id", format!("batch-{}", i)).unwrap();
                batch_dict.set_item("object", "batch").unwrap();
                batch_dict
                    .set_item("created_at", 1700000000u64 + *i as u64 * 3600)
                    .unwrap();
                batch_dict.set_item("model", "gpt-4o").unwrap();
                batch_dict.set_item("input_file_id", *file_id).unwrap();
                batch_dict.set_item("status", *status).unwrap();
                batch_dict.set_item("completion_window", "24h").unwrap();
                batch_dict.to_object(py)
            }),
        );

        dict.set_item("data", data_list)?;
        dict.set_item("has_more", false)?;
        Ok::<_, PyErr>(dict.into())
    })
}

/// alist_batches - Async list batches API
#[pyfunction]
#[pyo3(name = "alist_batches")]
pub async fn alist_batches(
    limit: Option<i32>,
    after: Option<String>,
    before: Option<String>,
) -> PyResult<Py<PyAny>> {
    list_batches(limit, after, before)
}

/// retrieve_batch_results - Sync retrieve batch results API
#[pyfunction]
#[pyo3(name = "retrieve_batch_results", text_signature = "(batch_id)")]
pub fn retrieve_batch_results(batch_id: String) -> PyResult<Py<PyAny>> {
    println!("retrieve_batch_results called: batch_id={}", batch_id);

    Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("id", &batch_id)?;
        dict.set_item("object", "batch")?;
        dict.set_item("status", "completed")?;
        dict.set_item("output_file_id", "file-output-abc123")?;
        dict.set_item("error_file_id", py.None())?;
        dict.set_item("created_at", 1700000000u64)?;
        dict.set_item("completed_at", 1700010000u64)?;
        dict.set_item("expires_at", 1700090000u64)?;
        dict.set_item("metadata", PyDict::new(py))?;
        Ok::<_, PyErr>(dict.into())
    })
}

/// aretrieve_batch_results - Async retrieve batch results API
#[pyfunction]
#[pyo3(name = "aretrieve_batch_results")]
pub async fn aretrieve_batch_results(batch_id: String) -> PyResult<Py<PyAny>> {
    retrieve_batch_results(batch_id)
}
