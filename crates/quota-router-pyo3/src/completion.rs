// Completion functions for PyO3 bindings
//
// Supports both modes:
// - litellm-mode: delegates to core's native_http (reqwest → provider REST APIs)
// - any-llm-mode: delegates to core's py_bridge (PyO3 → official Python SDKs)
//
// Mode is selected via the module-level `mode` attribute or per-call `_mode` parameter.

#![allow(clippy::too_many_arguments)]
#![allow(deprecated)]

use crate::model::ParsedModel;
use crate::types::Message;
use pyo3::prelude::*;
use std::sync::Once;

/// Initialize providers once at startup (both modes)
static INIT_PROVIDERS: Once = Once::new();

fn ensure_providers_initialized() {
    INIT_PROVIDERS.call_once(|| {
        #[cfg(feature = "full")]
        quota_router_core::init_native_http_providers();

        #[cfg(feature = "full")]
        quota_router_core::init_py_bridge_providers();
    });
}

/// Get the current mode from module-level setting, or default
fn get_mode() -> quota_router_core::mode::ProviderMode {
    // Default based on compiled features
    quota_router_core::mode::default_mode()
}

/// Convert PyO3 Message to core shared_types Message (for litellm-mode)
fn to_shared_messages(messages: &[Message]) -> Vec<quota_router_core::shared_types::Message> {
    messages
        .iter()
        .map(|m| quota_router_core::shared_types::Message {
            role: m.role.clone(),
            content: Some(m.content.clone()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            function_call: None,
        })
        .collect()
}

/// Convert PyO3 Message to core types Message (for any-llm-mode/py_bridge)
fn to_core_messages(messages: &[Message]) -> Vec<quota_router_core::types::Message> {
    messages
        .iter()
        .map(|m| quota_router_core::types::Message::new(&m.role, &m.content))
        .collect()
}

/// Resolve the provider mode from an optional string.
fn resolve_mode(mode_str: Option<&str>) -> quota_router_core::mode::ProviderMode {
    match mode_str {
        Some(s) => quota_router_core::mode::ProviderMode::from_str(s)
            .unwrap_or_else(|| {
                eprintln!("Unknown mode '{}', using default", s);
                get_mode()
            }),
        None => get_mode(),
    }
}

/// completion - Sync completion call
///
/// Supports both modes:
/// - litellm (default): reqwest → provider REST APIs
/// - any-llm: PyO3 → official Python SDKs
///
/// Pass `_mode="litellm"` or `_mode="any-llm"` to override the default.
#[pyfunction]
#[pyo3(name = "completion", text_signature = "(model, messages, **kwargs)")]
pub fn completion(
    model: String,
    messages: Vec<Message>,
    // Optional parameters (match LiteLLM)
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    top_p: Option<f64>,
    n: Option<i32>,
    stream: Option<bool>,
    stop: Option<String>,
    presence_penalty: Option<f64>,
    frequency_penalty: Option<f64>,
    user: Option<String>,
    seed: Option<i32>,
    timeout: Option<f64>,
    _extra_headers: Option<String>,
    base_url: Option<String>,
    _api_version: Option<String>,
    // quota-router specific
    api_key: Option<String>,
    // Phase 4 parameters
    _service_tier: Option<String>,
    _background: Option<bool>,
    _prompt_cache_key: Option<String>,
    _prompt_cache_retention: Option<String>,
    _conversation: Option<String>,
    // Mode selection: "litellm" or "any-llm"
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Parse model string to determine provider
    let parsed = match ParsedModel::parse(&model) {
        Ok(p) => p,
        Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e)),
    };

    // Streaming requires async mode
    if stream == Some(true) {
        return Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "Streaming is not supported in synchronous completion(). \
             Use acompletion(stream=True) for streaming responses.",
        ));
    }

    // Initialize providers (once)
    ensure_providers_initialized();

    // Select mode (per-call override or default)
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            completion_litellm(&parsed, &messages, temperature, max_tokens, top_p, n, stop,
                presence_penalty, frequency_penalty, user, seed, api_key, base_url)
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            completion_any_llm(&parsed, &messages, api_key, base_url)
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err("No mode compiled")),
    }
}

/// Completion via litellm-mode (reqwest → provider REST APIs)
#[cfg(feature = "full")]
fn completion_litellm(
    parsed: &ParsedModel,
    messages: &[Message],
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    top_p: Option<f64>,
    n: Option<i32>,
    stop: Option<String>,
    presence_penalty: Option<f64>,
    frequency_penalty: Option<f64>,
    user: Option<String>,
    seed: Option<i32>,
    api_key: Option<String>,
    base_url: Option<String>,
) -> PyResult<Py<PyAny>> {
    use quota_router_core::native_http::HttpProviderFactory;

    let mut provider = HttpProviderFactory::create(&parsed.provider)
        .ok_or_else(|| pyo3::exceptions::PyNotImplementedError::new_err(
            format!("Provider '{}' not supported in litellm-mode", parsed.provider)
        ))?;

    // Build request
    let request = quota_router_core::native_http::HttpCompletionRequest {
        model: parsed.model.clone(),
        messages: to_shared_messages(messages),
        stream: Some(false),
        temperature: temperature.map(|v| v as f32),
        max_tokens: max_tokens.map(|v| v as u32),
        top_p: top_p.map(|v| v as f32),
        stop: stop.map(|s| vec![s]),
        n: n.map(|v| v as u32),
        presence_penalty: presence_penalty.map(|v| v as f32),
        frequency_penalty: frequency_penalty.map(|v| v as f32),
        user,
        seed: seed.map(|v| v as i64),
        api_base: base_url.clone(),
        tools: None,
        tool_choice: None,
        response_format: None,
        logprobs: None,
        top_logprobs: None,
        parallel_tool_calls: None,
        prompt_id: None,
        prompt_variables: None,
        provider_params: None,
    };

    // Use tokio runtime for async call
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e)))?;

    let result = rt.block_on(async {
        provider.completion(&request, api_key.as_deref()).await
    }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", parsed.provider, e)))?;

    // Convert to Python dict
    Python::with_gil(|py| {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("id", &result.id)?;
        dict.set_item("object", &result.object)?;
        dict.set_item("created", result.created)?;
        dict.set_item("model", &result.model)?;

        let choices: Vec<_> = result.choices.iter().map(|c| {
            let choice_dict = pyo3::types::PyDict::new(py);
            choice_dict.set_item("index", c.index).ok();
            let msg_dict = pyo3::types::PyDict::new(py);
            msg_dict.set_item("role", &c.message.role).ok();
            msg_dict.set_item("content", &c.message.content).ok();
            choice_dict.set_item("message", msg_dict).ok();
            choice_dict.set_item("finish_reason", &c.finish_reason).ok();
            choice_dict
        }).collect();
        dict.set_item("choices", choices)?;

        let usage_dict = pyo3::types::PyDict::new(py);
        usage_dict.set_item("prompt_tokens", result.usage.prompt_tokens)?;
        usage_dict.set_item("completion_tokens", result.usage.completion_tokens)?;
        usage_dict.set_item("total_tokens", result.usage.total_tokens)?;
        dict.set_item("usage", usage_dict)?;

        Ok(dict.into())
    })
}

/// Completion via any-llm-mode (PyO3 → Python SDKs)
#[cfg(feature = "full")]
fn completion_any_llm(
    parsed: &ParsedModel,
    messages: &[Message],
    api_key: Option<String>,
    base_url: Option<String>,
) -> PyResult<Py<PyAny>> {
    use quota_router_core::py_bridge::PyBridgeProviderFactory;

    let mut provider = PyBridgeProviderFactory::create(&parsed.provider)
        .ok_or_else(|| pyo3::exceptions::PyNotImplementedError::new_err(
            format!("Provider '{}' not supported in any-llm-mode", parsed.provider)
        ))?;

    if let Some(key) = api_key {
        provider = provider.with_api_key(key);
    }
    if let Some(base) = base_url {
        provider = provider.with_api_base(base);
    }

    let core_messages = to_core_messages(messages);
    let result = provider.completion(&parsed.model, &core_messages)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", parsed.provider, e)))?;

    Python::with_gil(|py| result.to_dict(py))
}

/// acompletion - Async completion call
///
/// Delegates to sync completion(). The blocking happens inside Python's GIL
/// (py_bridge calls Python SDKs), so spawn_blocking is not needed.
#[pyfunction]
#[pyo3(name = "acompletion")]
pub async fn acompletion(
    model: String,
    messages: Vec<Message>,
    // Optional parameters (match LiteLLM)
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    top_p: Option<f64>,
    n: Option<i32>,
    stream: Option<bool>,
    stop: Option<String>,
    presence_penalty: Option<f64>,
    frequency_penalty: Option<f64>,
    user: Option<String>,
    seed: Option<i32>,
    timeout: Option<f64>,
    _extra_headers: Option<String>,
    base_url: Option<String>,
    _api_version: Option<String>,
    // quota-router specific
    api_key: Option<String>,
    // Phase 4 parameters
    _service_tier: Option<String>,
    _background: Option<bool>,
    _prompt_cache_key: Option<String>,
    _prompt_cache_retention: Option<String>,
    _conversation: Option<String>,
    // Mode selection: "litellm" or "any-llm"
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    completion(
        model,
        messages,
        temperature,
        max_tokens,
        top_p,
        n,
        stream,
        stop,
        presence_penalty,
        frequency_penalty,
        user,
        seed,
        timeout,
        _extra_headers,
        base_url,
        _api_version,
        api_key,
        _service_tier,
        _background,
        _prompt_cache_key,
        _prompt_cache_retention,
        _conversation,
        _mode,
    )
}

/// embedding - Sync embedding call
#[pyfunction]
#[pyo3(
    name = "embedding",
    text_signature = "(input, model, api_key=None, api_base=None, **kwargs)"
)]
pub fn embedding(
    input: Py<PyAny>,
    model: String,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (input, model, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Embeddings are not yet implemented in any-llm (direct) mode. \
         Use litellm mode (via the quota-router proxy) for embedding calls, \
         or call the provider SDK directly.",
    ))
}

/// aembedding - Async embedding call (per RFC-0920 lines 4031-4043)
#[pyfunction]
#[pyo3(
    name = "aembedding",
    text_signature = "(input, model, api_key=None, api_base=None, **kwargs)"
)]
pub async fn aembedding(
    input: Py<PyAny>,
    model: String,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (input, model, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Embeddings are not yet implemented in any-llm (direct) mode. \
         Use litellm mode (via the quota-router proxy) for embedding calls, \
         or call the provider SDK directly.",
    ))
}

// =============================================================================
// Messages API (Anthropic Messages API format)
// RFC-0920: Anthropic-compatible Messages API
// =============================================================================

/// messages - Sync Anthropic Messages API call
///
/// Note: The quota-router proxy does not yet support the Anthropic Messages API endpoint.
/// Use `completion()` for chat completions. See RFC-0920 for planned support.
#[pyfunction]
#[pyo3(
    name = "messages",
    text_signature = "(model, messages, *, provider=None, **kwargs)"
)]
pub fn messages(
    model: String,
    messages: Py<PyAny>,
    max_tokens: Option<i32>,
    system: Option<String>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<i32>,
    stop: Option<Vec<String>>,
    stream: Option<bool>,
    tools: Option<Py<PyAny>>,
    tool_choice: Option<Py<PyAny>>,
    thinking: Option<Py<PyAny>>,
    metadata: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
    provider: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (
        provider,
        model,
        messages,
        max_tokens,
        system,
        temperature,
        top_p,
        top_k,
        stop,
        stream,
        tools,
        tool_choice,
        thinking,
        metadata,
        api_key,
        api_base,
    );
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Anthropic Messages API endpoint is not yet implemented in the quota-router proxy. \
         Use completion() for chat completions instead. See RFC-0920 for planned Messages API support.",
    ))
}

/// amessages - Async Anthropic Messages API call
#[pyfunction]
#[pyo3(name = "amessages")]
pub async fn amessages(
    model: String,
    messages: Py<PyAny>,
    max_tokens: Option<i32>,
    system: Option<String>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<i32>,
    stop: Option<Vec<String>>,
    stream: Option<bool>,
    tools: Option<Py<PyAny>>,
    tool_choice: Option<Py<PyAny>>,
    thinking: Option<Py<PyAny>>,
    metadata: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
    provider: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::messages(
        model,
        messages,
        max_tokens,
        system,
        temperature,
        top_p,
        top_k,
        stop,
        stream,
        tools,
        tool_choice,
        thinking,
        metadata,
        api_key,
        api_base,
        provider,
    )
}

// =============================================================================
// Responses API (OpenAI Responses API)
// RFC-0920: OpenAI-compatible Responses API
// =============================================================================

/// responses - Sync OpenAI Responses API call
///
/// Note: The quota-router proxy does not yet support the Responses API endpoint.
/// Use `completion()` for chat completions. See RFC-0920 for planned support.
#[pyfunction]
#[pyo3(
    name = "responses",
    text_signature = "(model, input, *, provider=None, **kwargs)"
)]
pub fn responses(
    model: String,
    input: Py<PyAny>,
    instructions: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    top_p: Option<f64>,
    stream: Option<bool>,
    tools: Option<Py<PyAny>>,
    tool_choice: Option<Py<PyAny>>,
    modalities: Option<Py<PyAny>>,
    audio: Option<Py<PyAny>>,
    store: Option<bool>,
    metadata: Option<Py<PyAny>>,
    user: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    provider: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (
        provider,
        model,
        input,
        instructions,
        temperature,
        max_tokens,
        top_p,
        stream,
        tools,
        tool_choice,
        modalities,
        audio,
        store,
        metadata,
        user,
        api_key,
        api_base,
    );
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "OpenAI Responses API endpoint is not yet implemented in the quota-router proxy. \
         Use completion() for chat completions instead. See RFC-0920 for planned Responses API support.",
    ))
}

/// aresponses - Async OpenAI Responses API call
#[pyfunction]
#[pyo3(name = "aresponses")]
pub async fn aresponses(
    model: String,
    input: Py<PyAny>,
    instructions: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    top_p: Option<f64>,
    stream: Option<bool>,
    tools: Option<Py<PyAny>>,
    tool_choice: Option<Py<PyAny>>,
    modalities: Option<Py<PyAny>>,
    audio: Option<Py<PyAny>>,
    store: Option<bool>,
    metadata: Option<Py<PyAny>>,
    user: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    provider: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::responses(
        model,
        input,
        instructions,
        temperature,
        max_tokens,
        top_p,
        stream,
        tools,
        tool_choice,
        modalities,
        audio,
        store,
        metadata,
        user,
        api_key,
        api_base,
        provider,
    )
}

/// get_response - Retrieve a response by ID
#[pyfunction]
#[pyo3(
    name = "get_response",
    text_signature = "(response_id, provider=None, **kwargs)"
)]
pub fn get_response(
    response_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (provider, response_id, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "get_response() is not yet implemented in the quota-router proxy. \
         See RFC-0920 for planned Responses API support.",
    ))
}

/// aget_response - Async retrieve a response by ID
#[pyfunction]
#[pyo3(name = "aget_response")]
pub async fn aget_response(
    response_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::get_response(response_id, provider, api_key, api_base)
}

/// delete_response - Delete a response by ID
#[pyfunction]
#[pyo3(
    name = "delete_response",
    text_signature = "(response_id, provider=None, **kwargs)"
)]
pub fn delete_response(
    response_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (provider, response_id, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "delete_response() is not yet implemented in the quota-router proxy. \
         See RFC-0920 for planned Responses API support.",
    ))
}

/// adelete_response - Async delete a response by ID
#[pyfunction]
#[pyo3(name = "adelete_response")]
pub async fn adelete_response(
    response_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::delete_response(response_id, provider, api_key, api_base)
}

// =============================================================================
// Model Listing API
// =============================================================================

/// list_models - Sync list models API
///
/// Note: Not yet implemented. Real model listing through the proxy
/// requires the model registry to be wired. See RFC-0920.
#[pyfunction]
#[pyo3(name = "list_models")]
pub fn list_models(_provider: Option<String>) -> PyResult<Py<PyAny>> {
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "list_models() is not yet implemented in the quota-router proxy. \
         See RFC-0920 for planned model registry support.",
    ))
}

/// alist_models - Async list models API
#[pyfunction]
#[pyo3(name = "alist_models")]
pub async fn alist_models(provider: Option<String>) -> PyResult<Py<PyAny>> {
    list_models(provider)
}

// =============================================================================
// Batch API (OpenAI Batch API)
// RFC-0920: OpenAI-compatible Batch API
// =============================================================================

/// batch_create - Sync create batch API
///
/// Note: The quota-router proxy does not yet support the Batch API endpoint.
/// Use `batch_completion()` for in-memory parallel batch processing.
/// See RFC-0920 for planned Batch API support.
#[pyfunction]
#[pyo3(
    name = "batch_create",
    text_signature = "(provider, input_file, model, **kwargs)"
)]
pub fn batch_create(
    provider: String,
    input_file: String,
    model: String,
    endpoint: Option<String>,
    completion_window: Option<String>,
    metadata: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (
        provider,
        model,
        input_file,
        endpoint,
        completion_window,
        metadata,
        api_key,
        api_base,
    );
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Batch API endpoint is not yet implemented in the quota-router proxy. \
         Use batch_completion() for in-memory parallel batch processing. \
         See RFC-0920 for planned Batch API support.",
    ))
}

/// abatch_create - Async create batch API
#[pyfunction]
#[pyo3(name = "abatch_create")]
pub async fn abatch_create(
    provider: String,
    input_file: String,
    model: String,
    endpoint: Option<String>,
    completion_window: Option<String>,
    metadata: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::batch_create(
        provider,
        input_file,
        model,
        endpoint,
        completion_window,
        metadata,
        api_key,
        api_base,
    )
}

/// batch_retrieve - Sync retrieve batch API
#[pyfunction]
#[pyo3(
    name = "batch_retrieve",
    text_signature = "(batch_id, provider=None, **kwargs)"
)]
pub fn batch_retrieve(
    batch_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (provider, batch_id, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Batch API endpoint is not yet implemented in the quota-router proxy. \
         See RFC-0920 for planned Batch API support.",
    ))
}

/// abatch_retrieve - Async retrieve batch API
#[pyfunction]
#[pyo3(name = "abatch_retrieve")]
pub async fn abatch_retrieve(
    batch_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::batch_retrieve(batch_id, provider, api_key, api_base)
}

/// batch_cancel - Sync cancel batch API
#[pyfunction]
#[pyo3(
    name = "batch_cancel",
    text_signature = "(provider, batch_id, **kwargs)"
)]
pub fn batch_cancel(
    provider: String,
    batch_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (provider, batch_id, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Batch API endpoint is not yet implemented in the quota-router proxy. \
         See RFC-0920 for planned Batch API support.",
    ))
}

/// abatch_cancel - Async cancel batch API
#[pyfunction]
#[pyo3(name = "abatch_cancel")]
pub async fn abatch_cancel(
    provider: String,
    batch_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::batch_cancel(provider, batch_id, api_key, api_base)
}

/// batch_list - Sync list batches API
#[pyfunction]
#[pyo3(name = "batch_list", text_signature = "(provider, limit=20, **kwargs)")]
pub fn batch_list(
    provider: String,
    limit: i32,
    after: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (provider, limit, after, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Batch API endpoint is not yet implemented in the quota-router proxy. \
         See RFC-0920 for planned Batch API support.",
    ))
}

/// abatch_list - Async list batches API
#[pyfunction]
#[pyo3(name = "abatch_list")]
pub async fn abatch_list(
    provider: String,
    limit: i32,
    after: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::batch_list(provider, limit, after, api_key, api_base)
}

/// batch_results - Sync retrieve batch results API
#[pyfunction]
#[pyo3(
    name = "batch_results",
    text_signature = "(batch_id, provider=None, **kwargs)"
)]
pub fn batch_results(
    batch_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    let _ = (provider, batch_id, api_key, api_base);
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "Batch API endpoint is not yet implemented in the quota-router proxy. \
         See RFC-0920 for planned Batch API support.",
    ))
}

/// abatch_results - Async retrieve batch results API
#[pyfunction]
#[pyo3(name = "abatch_results")]
pub async fn abatch_results(
    batch_id: String,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    self::batch_results(batch_id, provider, api_key, api_base)
}

// =============================================================================
// Text Completion API (LiteLLM parity)
// =============================================================================

/// text_completion - Synchronous text completion (non-chat models)
#[pyfunction]
#[pyo3(name = "text_completion")]
pub fn text_completion(
    model: String,
    prompt: String,
    frequency_penalty: Option<f64>,
    _logprobs: Option<i32>,
    max_tokens: Option<i32>,
    presence_penalty: Option<f64>,
    stop: Option<Vec<String>>,
    _stream: Option<bool>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    api_key: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Wrap prompt as a user message and delegate to completion()
    let messages = vec![Message {
        role: "user".to_string(),
        content: prompt,
    }];

    completion(
        model,
        messages,
        temperature,
        max_tokens,
        top_p,
        None, // n
        _stream,
        stop.and_then(|v| if v.is_empty() { None } else { Some(v.join(",")) }),
        presence_penalty,
        frequency_penalty,
        None, // user
        None, // seed
        None, // timeout
        None, // extra_headers
        None, // base_url
        None, // api_version
        api_key,
        None, // service_tier
        None, // background
        None, // prompt_cache_key
        None, // prompt_cache_retention
        None, // conversation
        None, // _mode
    )
}

/// atext_completion - Asynchronous text completion (non-chat models)
#[pyfunction]
#[pyo3(name = "atext_completion")]
pub async fn atext_completion(
    model: String,
    prompt: String,
    frequency_penalty: Option<f64>,
    _logprobs: Option<i32>,
    max_tokens: Option<i32>,
    presence_penalty: Option<f64>,
    stop: Option<Vec<String>>,
    _stream: Option<bool>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    timeout: Option<f64>,
    api_key: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Wrap prompt as a user message and delegate to acompletion()
    let messages = vec![Message {
        role: "user".to_string(),
        content: prompt,
    }];

    acompletion(
        model,
        messages,
        temperature,
        max_tokens,
        top_p,
        None, // n
        _stream,
        stop.and_then(|v| if v.is_empty() { None } else { Some(v.join(",")) }),
        presence_penalty,
        frequency_penalty,
        None,     // user
        None,     // seed
        timeout,  // timeout
        None,     // extra_headers
        None,     // base_url
        None,     // api_version
        api_key,
        None, // service_tier
        None, // background
        None, // prompt_cache_key
        None, // prompt_cache_retention
        None, // conversation
        None, // _mode
    )
    .await
}
