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

/// Convert a Python object to serde_json::Value via json.dumps
fn python_to_json(py: Python<'_>, obj: &Py<PyAny>) -> PyResult<serde_json::Value> {
    let json_module = py.import("json")?;
    let serialized = json_module.call_method1("dumps", (obj,))?;
    let s: String = serialized.extract()?;
    serde_json::from_str(&s).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Convert serde_json::Value to Python object via json.loads
fn json_to_python(py: Python<'_>, val: &serde_json::Value) -> PyResult<Py<PyAny>> {
    let s = serde_json::to_string(val).unwrap_or_else(|_| "null".to_string());
    let json_module = py.import("json")?;
    let obj = json_module.call_method1("loads", (s,))?;
    Ok(obj.into())
}

/// Map ProviderError to appropriate Python exception
fn provider_error_to_py(
    e: quota_router_core::native_http::ProviderError,
    provider: &str,
) -> pyo3::PyErr {
    use quota_router_core::native_http::ProviderError;
    match e {
        ProviderError::AuthError(msg) => {
            pyo3::exceptions::PyPermissionError::new_err(format!("{}: {}", provider, msg))
        }
        ProviderError::RateLimit(msg) => {
            pyo3::exceptions::PyConnectionError::new_err(format!("{}: {}", provider, msg))
        }
        ProviderError::UnsupportedModel(msg) => {
            pyo3::exceptions::PyNotImplementedError::new_err(format!("{}: {}", provider, msg))
        }
        ProviderError::Network(msg) => {
            pyo3::exceptions::PyConnectionError::new_err(format!("{}: {}", provider, msg))
        }
        ProviderError::InvalidResponse(msg) => {
            pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", provider, msg))
        }
    }
}

// =============================================================================
// Phase 2: Native async helpers (AsyncOpenAI / AsyncAnthropic via pyo3_asyncio)
// =============================================================================

/// Check if a provider has a native async Python SDK.
#[cfg(feature = "full")]
fn supports_native_async(provider: &str) -> bool {
    matches!(provider, "openai" | "anthropic")
}

/// Create a Python async completion coroutine using AsyncOpenAI or AsyncAnthropic.
/// Returns a Python awaitable that can be converted to a Rust future via pyo3_asyncio.
#[cfg(feature = "full")]
fn create_async_completion_py<'py>(
    py: Python<'py>,
    provider: &str,
    model: &str,
    messages: &[quota_router_core::types::Message],
    api_key: Option<&str>,
    api_base: Option<&str>,
) -> PyResult<Py<PyAny>> {
    match provider {
        "openai" => {
            let openai = py.import("openai")?;
            let async_class = openai.getattr("AsyncOpenAI")?;
            let kwargs = pyo3::types::PyDict::new(py);
            if let Some(key) = api_key {
                kwargs.set_item("api_key", key)?;
            }
            if let Some(base) = api_base {
                kwargs.set_item("base_url", base)?;
            }
            let headers = pyo3::types::PyDict::new(py);
            headers.set_item("Accept-Encoding", "identity")?;
            kwargs.set_item("default_headers", headers)?;
            let client = async_class.call((), Some(kwargs))?;

            let py_messages = pyo3::types::PyList::empty(py);
            for msg in messages {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("role", &msg.role)?;
                dict.set_item("content", &msg.content)?;
                py_messages.append(dict)?;
            }

            let chat = client.getattr("chat")?;
            let completions = chat.getattr("completions")?;
            let create = completions.getattr("create")?;
            let call_kwargs = pyo3::types::PyDict::new(py);
            call_kwargs.set_item("model", model)?;
            call_kwargs.set_item("messages", py_messages)?;
            create.call((), Some(call_kwargs)).map(|r| r.into())
        }
        "anthropic" => {
            let anthropic = py.import("anthropic")?;
            let async_class = anthropic.getattr("AsyncAnthropic")?;
            let kwargs = pyo3::types::PyDict::new(py);
            if let Some(key) = api_key {
                kwargs.set_item("api_key", key)?;
            }
            if let Some(base) = api_base {
                kwargs.set_item("base_url", base)?;
            }
            let client = async_class.call((), Some(kwargs))?;

            let py_messages = pyo3::types::PyList::empty(py);
            for msg in messages {
                let dict = pyo3::types::PyDict::new(py);
                let role = if msg.role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                dict.set_item("role", role)?;
                dict.set_item("content", &msg.content)?;
                py_messages.append(dict)?;
            }

            let messages_attr = client.getattr("messages")?;
            let create = messages_attr.getattr("create")?;
            let call_kwargs = pyo3::types::PyDict::new(py);
            call_kwargs.set_item("model", model)?;
            call_kwargs.set_item("messages", py_messages)?;
            call_kwargs.set_item("max_tokens", 1024)?;
            create.call((), Some(call_kwargs)).map(|r| r.into())
        }
        _ => Err(pyo3::exceptions::PyNotImplementedError::new_err(format!(
            "Native async not available for provider '{}'",
            provider
        ))),
    }
}

/// Create a Python async embedding coroutine using AsyncOpenAI.
#[cfg(feature = "full")]
fn create_async_embedding_py<'py>(
    py: Python<'py>,
    provider: &str,
    model: &str,
    input: &str,
    api_key: Option<&str>,
    api_base: Option<&str>,
) -> PyResult<Py<PyAny>> {
    match provider {
        "openai" => {
            let openai = py.import("openai")?;
            let async_class = openai.getattr("AsyncOpenAI")?;
            let kwargs = pyo3::types::PyDict::new(py);
            if let Some(key) = api_key {
                kwargs.set_item("api_key", key)?;
            }
            if let Some(base) = api_base {
                kwargs.set_item("base_url", base)?;
            }
            let client = async_class.call((), Some(kwargs))?;

            let embeddings = client.getattr("embeddings")?;
            let create = embeddings.getattr("create")?;
            let call_kwargs = pyo3::types::PyDict::new(py);
            call_kwargs.set_item("model", model)?;
            call_kwargs.set_item("input", input)?;
            create.call((), Some(call_kwargs)).map(|r| r.into())
        }
        _ => Err(pyo3::exceptions::PyNotImplementedError::new_err(format!(
            "Native async embedding not available for provider '{}'",
            provider
        ))),
    }
}

/// Create a Python async messages coroutine using AsyncAnthropic.
#[cfg(feature = "full")]
fn create_async_messages_py<'py>(
    py: Python<'py>,
    model: &str,
    messages: &[Message],
    max_tokens: i32,
    system: Option<&str>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    stop_sequences: Option<&[String]>,
    api_key: Option<&str>,
    api_base: Option<&str>,
) -> PyResult<Py<PyAny>> {
    let anthropic = py.import("anthropic")?;
    let async_class = anthropic.getattr("AsyncAnthropic")?;
    let kwargs = pyo3::types::PyDict::new(py);
    if let Some(key) = api_key {
        kwargs.set_item("api_key", key)?;
    }
    if let Some(base) = api_base {
        kwargs.set_item("base_url", base)?;
    }
    let client = async_class.call((), Some(kwargs))?;

    let py_messages = pyo3::types::PyList::empty(py);
    for msg in messages {
        let dict = pyo3::types::PyDict::new(py);
        let role = if msg.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        dict.set_item("role", role)?;
        dict.set_item("content", &msg.content)?;
        py_messages.append(dict)?;
    }

    let messages_attr = client.getattr("messages")?;
    let create = messages_attr.getattr("create")?;
    let call_kwargs = pyo3::types::PyDict::new(py);
    call_kwargs.set_item("model", model)?;
    call_kwargs.set_item("messages", py_messages)?;
    call_kwargs.set_item("max_tokens", max_tokens)?;
    if let Some(temp) = temperature {
        call_kwargs.set_item("temperature", temp)?;
    }
    if let Some(tp) = top_p {
        call_kwargs.set_item("top_p", tp)?;
    }
    if let Some(sys) = system {
        call_kwargs.set_item("system", sys)?;
    }
    if let Some(stops) = stop_sequences {
        call_kwargs.set_item("stop_sequences", stops)?;
    }
    create.call((), Some(call_kwargs)).map(|r| r.into())
}

/// Convert async Python response to dict using the provider's convert_response.
#[cfg(feature = "full")]
fn convert_async_response(
    py: pyo3::Python<'_>,
    result_py: &pyo3::types::PyAny,
    provider: &str,
) -> PyResult<Py<PyAny>> {
    let chat_completion = match provider {
        "openai" => quota_router_core::py_bridge::openai::convert_response(result_py, py)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("openai: {}", e)))?,
        "anthropic" => quota_router_core::py_bridge::anthropic::convert_response(result_py, py)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("anthropic: {}", e)))?,
        _ => {
            return Err(pyo3::exceptions::PyNotImplementedError::new_err(format!(
                "Response conversion not supported for provider '{}'",
                provider
            )))
        }
    };
    chat_completion.to_dict(py)
}

/// Resolve the provider mode from an optional string.
fn resolve_mode(mode_str: Option<&str>) -> quota_router_core::mode::ProviderMode {
    match mode_str {
        Some(s) => quota_router_core::mode::ProviderMode::parse(s).unwrap_or_else(|| {
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
        quota_router_core::mode::ProviderMode::LiteLLM => completion_litellm(
            &parsed,
            &messages,
            temperature,
            max_tokens,
            top_p,
            n,
            stop,
            presence_penalty,
            frequency_penalty,
            user,
            seed,
            api_key,
            base_url,
            timeout,
        ),
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            completion_any_llm(&parsed, &messages, api_key, base_url)
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
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
    timeout: Option<f64>,
) -> PyResult<Py<PyAny>> {
    use quota_router_core::native_http::HttpProviderFactory;

    let provider = HttpProviderFactory::create(&parsed.provider).ok_or_else(|| {
        pyo3::exceptions::PyNotImplementedError::new_err(format!(
            "Provider '{}' not supported in litellm-mode",
            parsed.provider
        ))
    })?;

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
        timeout,
    };

    // Use tokio runtime for async call
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e)))?;

    let result = rt
        .block_on(async { provider.completion(&request, api_key.as_deref()).await })
        .map_err(|e| provider_error_to_py(e, &parsed.provider))?;

    // Convert to Python dict
    Python::with_gil(|py| {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("id", &result.id)?;
        dict.set_item("object", &result.object)?;
        dict.set_item("created", result.created)?;
        dict.set_item("model", &result.model)?;

        let choices: Vec<_> = result
            .choices
            .iter()
            .map(|c| {
                let choice_dict = pyo3::types::PyDict::new(py);
                choice_dict.set_item("index", c.index).ok();
                let msg_dict = pyo3::types::PyDict::new(py);
                msg_dict.set_item("role", &c.message.role).ok();
                msg_dict.set_item("content", &c.message.content).ok();
                choice_dict.set_item("message", msg_dict).ok();
                choice_dict.set_item("finish_reason", &c.finish_reason).ok();
                choice_dict
            })
            .collect();
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

    let mut provider = PyBridgeProviderFactory::create(&parsed.provider).ok_or_else(|| {
        pyo3::exceptions::PyNotImplementedError::new_err(format!(
            "Provider '{}' not supported in any-llm-mode",
            parsed.provider
        ))
    })?;

    if let Some(key) = api_key {
        provider = provider.with_api_key(key);
    }
    if let Some(base) = base_url {
        provider = provider.with_api_base(base);
    }

    let core_messages = to_core_messages(messages);
    let result = provider
        .completion(&parsed.model, &core_messages)
        .map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", parsed.provider, e))
        })?;

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
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let parsed = match ParsedModel::parse(&model) {
        Ok(p) => p,
        Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e)),
    };
    let mode = resolve_mode(_mode.as_deref());
    let _ = (
        _extra_headers,
        _api_version,
        _service_tier,
        _background,
        _prompt_cache_key,
        _prompt_cache_retention,
        _conversation,
        n,
        seed,
    );

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider =
                quota_router_core::native_http::HttpProviderFactory::create(&parsed.provider)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported in litellm-mode",
                            parsed.provider
                        ))
                    })?;

            let shared_messages = to_shared_messages(&messages);
            let request = quota_router_core::native_http::HttpCompletionRequest {
                model: parsed.model.clone(),
                messages: shared_messages,
                stream,
                temperature: temperature.map(|t| t as f32),
                max_tokens: max_tokens.map(|m| m as u32),
                top_p: top_p.map(|p| p as f32),
                stop: stop.map(|s| vec![s]),
                n: n.map(|n| n as u32),
                presence_penalty: presence_penalty.map(|p| p as f32),
                frequency_penalty: frequency_penalty.map(|f| f as f32),
                user,
                api_base: base_url,
                tools: None,
                tool_choice: None,
                response_format: None,
                seed: seed.map(|s| s as i64),
                logprobs: None,
                top_logprobs: None,
                parallel_tool_calls: None,
                prompt_id: None,
                prompt_variables: None,
                provider_params: None,
                timeout,
            };

            // True async: await the provider's async completion method
            let result = provider
                .completion(&request, api_key.as_deref())
                .await
                .map_err(|e| provider_error_to_py(e, &parsed.provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("object", &result.object)?;
                dict.set_item("created", result.created)?;
                dict.set_item("model", &result.model)?;
                let choices_list = pyo3::types::PyList::empty(py);
                for choice in &result.choices {
                    let choice_dict = pyo3::types::PyDict::new(py);
                    choice_dict.set_item("index", choice.index)?;
                    let message_dict = pyo3::types::PyDict::new(py);
                    message_dict.set_item("role", &choice.message.role)?;
                    message_dict.set_item("content", &choice.message.content)?;
                    choice_dict.set_item("message", message_dict)?;
                    choice_dict.set_item("finish_reason", &choice.finish_reason)?;
                    choices_list.append(choice_dict)?;
                }
                dict.set_item("choices", choices_list)?;
                let usage_dict = pyo3::types::PyDict::new(py);
                usage_dict.set_item("prompt_tokens", result.usage.prompt_tokens)?;
                usage_dict.set_item("completion_tokens", result.usage.completion_tokens)?;
                usage_dict.set_item("total_tokens", result.usage.total_tokens)?;
                dict.set_item("usage", usage_dict)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            let provider_name = parsed.provider.clone();

            if supports_native_async(&provider_name) {
                // Phase 2: Native async via AsyncOpenAI / AsyncAnthropic
                let core_messages = to_core_messages(&messages);
                let model = parsed.model.clone();
                let pn = provider_name.clone();
                let ak = api_key.clone();
                let ab = base_url.clone();

                let result_py = Python::with_gil(|py| {
                    let awaitable = create_async_completion_py(
                        py,
                        &pn,
                        &model,
                        &core_messages,
                        ak.as_deref(),
                        ab.as_deref(),
                    )?;
                    pyo3_asyncio_0_21::tokio::into_future(awaitable.into_bound(py))
                })
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "{}: Failed to create async call: {}",
                        provider_name, e
                    ))
                })?
                .await
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", provider_name, e))
                })?;

                Python::with_gil(|py| {
                    convert_async_response(py, result_py.as_ref(py), &provider_name)
                })
            } else {
                // Phase 1 fallback: spawn_blocking for providers without async SDKs
                let provider =
                    quota_router_core::py_bridge::PyBridgeProviderFactory::create(&provider_name)
                        .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported in any-llm-mode",
                            provider_name
                        ))
                    })?;

                let provider = if let Some(key) = api_key {
                    provider.with_api_key(key)
                } else {
                    provider
                };
                let provider = if let Some(base) = base_url {
                    provider.with_api_base(base)
                } else {
                    provider
                };

                let core_messages = to_core_messages(&messages);
                let model = parsed.model.clone();
                let pn = provider_name.clone();

                let result = tokio::task::spawn_blocking(move || {
                    provider.completion(&model, &core_messages)
                })
                .await
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "spawn_blocking failed: {}",
                        e
                    ))
                })?
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", pn, e)))?;

                Python::with_gil(|py| result.to_dict(py))
            }
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// embedding - Sync embedding call
#[pyfunction]
#[pyo3(
    name = "embedding",
    text_signature = "(model, input=None, inputs=None, *, provider=None, api_key=None, api_base=None, **kwargs)"
)]
pub fn embedding(
    model: String,
    input: Option<Py<PyAny>>,  // litellm convention
    inputs: Option<Py<PyAny>>, // any-llm convention
    dimensions: Option<i32>,
    encoding_format: Option<String>,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    timeout: Option<f64>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = (dimensions, encoding_format, client_args);

    // Dual-convention: accept both `input` (litellm) and `inputs` (any-llm)
    let data = input.or(inputs).ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(
            "embedding() requires either `input` or `inputs` parameter",
        )
    })?;

    // Extract input text from Python object (string or list of strings)
    let input_text: String = Python::with_gil(|py| -> PyResult<String> {
        if let Ok(s) = data.extract::<String>(py) {
            return Ok(s);
        }
        if let Ok(list) = data.downcast::<pyo3::types::PyList>(py) {
            let parts: Vec<String> = list
                .iter()
                .map(|item| item.extract::<String>().unwrap_or_default())
                .collect();
            return Ok(parts.join(" "));
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "input must be a string or list of strings",
        ))
    })?;

    let provider_name = provider.as_deref().unwrap_or("openai");
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl =
                quota_router_core::native_http::HttpProviderFactory::create(provider_name)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported in litellm-mode",
                            provider_name
                        ))
                    })?;

            let request = quota_router_core::native_http::HttpEmbeddingRequest {
                input: input_text,
                model: model.clone(),
                api_base: api_base.clone(),
                timeout,
            };

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async { provider_impl.embedding(&request, api_key.as_deref()).await })
                .map_err(|e| provider_error_to_py(e, provider_name))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("object", &result.object)?;
                dict.set_item("model", &result.model)?;

                let data_list = pyo3::types::PyList::empty(py);
                for emb in &result.data {
                    let emb_dict = pyo3::types::PyDict::new(py);
                    emb_dict.set_item("object", &emb.object)?;
                    emb_dict.set_item("index", emb.index)?;
                    let embedding_list = pyo3::types::PyList::empty(py);
                    for val in &emb.embedding {
                        embedding_list.append(*val)?;
                    }
                    emb_dict.set_item("embedding", embedding_list)?;
                    data_list.append(emb_dict)?;
                }
                dict.set_item("data", data_list)?;

                let usage_dict = pyo3::types::PyDict::new(py);
                usage_dict.set_item("prompt_tokens", result.usage.prompt_tokens)?;
                usage_dict.set_item("total_tokens", result.usage.total_tokens)?;
                dict.set_item("usage", usage_dict)?;

                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "embedding() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// aembedding - Async embedding call
#[pyfunction]
#[pyo3(
    name = "aembedding",
    text_signature = "(model, input, *, provider=None, api_key=None, api_base=None, **kwargs)"
)]
pub async fn aembedding(
    model: String,
    input: Option<Py<PyAny>>,  // litellm convention
    inputs: Option<Py<PyAny>>, // any-llm convention
    dimensions: Option<i32>,
    encoding_format: Option<String>,
    provider: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    timeout: Option<f64>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch for litellm-mode
    ensure_providers_initialized();
    let _ = (&dimensions, &encoding_format, &client_args);
    let data = input.or(inputs).ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err("embedding() requires either `input` or `inputs`")
    })?;
    let input_text: String = Python::with_gil(|py| -> PyResult<String> {
        if let Ok(s) = data.extract::<String>(py) {
            return Ok(s);
        }
        if let Ok(list) = data.downcast::<pyo3::types::PyList>(py) {
            let parts: Vec<String> = list
                .iter()
                .map(|i| i.extract::<String>().unwrap_or_default())
                .collect();
            return Ok(parts.join(" "));
        }
        Err(pyo3::exceptions::PyTypeError::new_err(
            "input must be a string or list of strings",
        ))
    })?;
    let provider_name = provider.as_deref().unwrap_or("openai");
    let mode = resolve_mode(_mode.as_deref());
    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl =
                quota_router_core::native_http::HttpProviderFactory::create(provider_name)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported",
                            provider_name
                        ))
                    })?;
            let request = quota_router_core::native_http::HttpEmbeddingRequest {
                input: input_text,
                model: model.clone(),
                api_base: api_base.clone(),
                timeout,
            };
            let result = provider_impl
                .embedding(&request, api_key.as_deref())
                .await
                .map_err(|e| provider_error_to_py(e, provider_name))?;
            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("object", &result.object)?;
                dict.set_item("model", &result.model)?;
                let data_list = pyo3::types::PyList::empty(py);
                for emb in &result.data {
                    let emb_dict = pyo3::types::PyDict::new(py);
                    emb_dict.set_item("object", &emb.object)?;
                    emb_dict.set_item("index", emb.index)?;
                    let el = pyo3::types::PyList::empty(py);
                    for v in &emb.embedding {
                        el.append(*v)?;
                    }
                    emb_dict.set_item("embedding", el)?;
                    data_list.append(emb_dict)?;
                }
                dict.set_item("data", data_list)?;
                let u = pyo3::types::PyDict::new(py);
                u.set_item("prompt_tokens", result.usage.prompt_tokens)?;
                u.set_item("total_tokens", result.usage.total_tokens)?;
                dict.set_item("usage", u)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            let pn = provider_name.to_string();

            if supports_native_async(&pn) {
                // Phase 2: Native async embedding via AsyncOpenAI
                let model_clone = model.clone();
                let input_clone = input_text.clone();
                let ak = api_key.clone();
                let ab = api_base.clone();

                let result_py = Python::with_gil(|py| {
                    let awaitable = create_async_embedding_py(
                        py,
                        &pn,
                        &model_clone,
                        &input_clone,
                        ak.as_deref(),
                        ab.as_deref(),
                    )?;
                    pyo3_asyncio_0_21::tokio::into_future(awaitable.into_bound(py))
                })
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "{}: Failed to create async call: {}",
                        pn, e
                    ))
                })?
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", pn, e)))?;

                Python::with_gil(|py| {
                    let result_ref = result_py.as_ref(py);
                    let dict = pyo3::types::PyDict::new(py);
                    let data_attr: &pyo3::types::PyAny =
                        result_ref.getattr("data").map_err(|e| {
                            pyo3::exceptions::PyRuntimeError::new_err(format!(
                                "Failed to get data: {}",
                                e
                            ))
                        })?;
                    let data_list = pyo3::types::PyList::empty(py);
                    for item_result in data_attr.iter()? {
                        let item = item_result?;
                        let emb_dict = pyo3::types::PyDict::new(py);
                        let obj: String = item.getattr("object")?.extract().unwrap_or_default();
                        let index: u32 = item.getattr("index")?.extract().unwrap_or(0);
                        emb_dict.set_item("object", obj)?;
                        emb_dict.set_item("index", index)?;
                        let embedding_list = pyo3::types::PyList::empty(py);
                        let emb_data = item.getattr("embedding")?;
                        for v in emb_data.iter()? {
                            let val: f64 = v?.extract()?;
                            embedding_list.append(val)?;
                        }
                        emb_dict.set_item("embedding", embedding_list)?;
                        data_list.append(emb_dict)?;
                    }
                    dict.set_item("data", data_list)?;
                    if let Ok(model_val) = result_ref.getattr("model") {
                        dict.set_item("model", model_val)?;
                    }
                    if let Ok(usage) = result_ref.getattr("usage") {
                        let u = pyo3::types::PyDict::new(py);
                        if let Ok(pt) = usage.getattr("prompt_tokens") {
                            u.set_item("prompt_tokens", pt)?;
                        }
                        if let Ok(tt) = usage.getattr("total_tokens") {
                            u.set_item("total_tokens", tt)?;
                        }
                        dict.set_item("usage", u)?;
                    }
                    Ok(dict.into())
                })
            } else {
                // Phase 1 fallback
                let model_clone = model.clone();
                let provider_name_clone = provider_name.to_string();
                let api_key_clone = api_key.clone();
                let api_base_clone = api_base.clone();
                let mode_clone = _mode.clone();
                tokio::task::spawn_blocking(move || {
                    embedding(
                        model_clone,
                        None,
                        Some(data),
                        dimensions,
                        encoding_format,
                        Some(provider_name_clone),
                        api_key_clone,
                        api_base_clone,
                        client_args,
                        timeout,
                        mode_clone,
                    )
                })
                .await
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "spawn_blocking failed: {}",
                        e
                    ))
                })?
            }
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

// =============================================================================
// Messages API (Anthropic Messages API format)
// RFC-0920: Anthropic-compatible Messages API
// =============================================================================

/// messages - Sync Anthropic Messages API call
///
/// messages - Anthropic Messages API call (delegates to native_http anthropic provider)
#[pyfunction]
#[pyo3(
    name = "messages",
    text_signature = "(model, messages, max_tokens, *, provider=None, **kwargs)"
)]
pub fn messages(
    model: String,
    messages: Py<PyAny>,
    max_tokens: i32,
    system: Option<String>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    _top_k: Option<i32>,
    stop_sequences: Option<Vec<String>>,
    stream: Option<bool>,
    _tools: Option<Py<PyAny>>,
    _tool_choice: Option<Py<PyAny>>,
    _thinking: Option<Py<PyAny>>,
    _metadata: Option<Py<PyAny>>,
    _cache_control: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    provider: Option<String>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = (
        client_args,
        _top_k,
        _tools,
        _tool_choice,
        _thinking,
        _metadata,
        _cache_control,
    );

    let provider_name = provider.as_deref().unwrap_or("anthropic");
    let mode = resolve_mode(_mode.as_deref());

    // Extract messages from Python object
    let py_messages: Vec<Message> = Python::with_gil(|py| -> PyResult<Vec<Message>> {
        let list: &pyo3::types::PyList = messages
            .downcast(py)
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("messages must be a list"))?;
        let mut result = Vec::new();
        for item in list.iter() {
            let dict: &pyo3::types::PyDict = item.downcast().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("each message must be a dict")
            })?;
            let role = match dict.get_item("role")? {
                Some(v) => v.extract::<String>().unwrap_or_else(|_| "user".to_string()),
                None => "user".to_string(),
            };
            let content = match dict.get_item("content")? {
                Some(v) => v.extract::<String>().unwrap_or_default(),
                None => String::new(),
            };
            result.push(Message { role, content });
        }
        Ok(result)
    })?;

    // Prepend system message if provided
    let mut all_messages = Vec::new();
    if let Some(ref sys) = system {
        all_messages.push(Message {
            role: "system".to_string(),
            content: sys.clone(),
        });
    }
    all_messages.extend(py_messages);

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl =
                quota_router_core::native_http::HttpProviderFactory::create(provider_name)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported in litellm-mode",
                            provider_name
                        ))
                    })?;

            let shared_messages = to_shared_messages(&all_messages);
            let request = quota_router_core::native_http::HttpCompletionRequest {
                model: model.clone(),
                messages: shared_messages,
                stream,
                temperature: temperature.map(|t| t as f32),
                max_tokens: Some(max_tokens as u32),
                top_p: top_p.map(|p| p as f32),
                stop: stop_sequences,
                n: None,
                presence_penalty: None,
                frequency_penalty: None,
                user: None,
                api_base: api_base.clone(),
                tools: None,
                tool_choice: None,
                response_format: None,
                seed: None,
                logprobs: None,
                top_logprobs: None,
                parallel_tool_calls: None,
                prompt_id: None,
                prompt_variables: None,
                provider_params: None,
                timeout: None,
            };

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async { provider_impl.completion(&request, api_key.as_deref()).await })
                .map_err(|e| provider_error_to_py(e, provider_name))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("type", "message")?;
                dict.set_item("role", "assistant")?;
                let content_list = pyo3::types::PyList::empty(py);
                let content_block = pyo3::types::PyDict::new(py);
                content_block.set_item("type", "text")?;
                if let Some(choice) = result.choices.first() {
                    content_block.set_item("text", &choice.message.content)?;
                }
                content_list.append(content_block)?;
                dict.set_item("content", content_list)?;
                dict.set_item("model", &result.model)?;
                if let Some(choice) = result.choices.first() {
                    dict.set_item("stop_reason", &choice.finish_reason)?;
                }
                let usage_dict = pyo3::types::PyDict::new(py);
                usage_dict.set_item("input_tokens", result.usage.prompt_tokens)?;
                usage_dict.set_item("output_tokens", result.usage.completion_tokens)?;
                dict.set_item("usage", usage_dict)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "messages() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// amessages - Async Anthropic Messages API call
#[pyfunction]
#[pyo3(name = "amessages")]
pub async fn amessages(
    model: String,
    messages: Py<PyAny>,
    max_tokens: i32,
    system: Option<String>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<i32>,
    stop_sequences: Option<Vec<String>>,
    stream: Option<bool>,
    tools: Option<Py<PyAny>>,
    tool_choice: Option<Py<PyAny>>,
    thinking: Option<Py<PyAny>>,
    metadata: Option<Py<PyAny>>,
    cache_control: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    provider: Option<String>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = (
        &top_k,
        &tools,
        &tool_choice,
        &thinking,
        &metadata,
        &cache_control,
        &client_args,
    );

    let provider_name = provider.as_deref().unwrap_or("anthropic");
    let mode = resolve_mode(_mode.as_deref());

    // Extract messages from Python object
    let py_messages: Vec<Message> = Python::with_gil(|py| -> PyResult<Vec<Message>> {
        let list: &pyo3::types::PyList = messages
            .downcast(py)
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("messages must be a list"))?;
        let mut result = Vec::new();
        for item in list.iter() {
            let dict: &pyo3::types::PyDict = item.downcast().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("each message must be a dict")
            })?;
            let role = match dict.get_item("role")? {
                Some(v) => v.extract::<String>().unwrap_or_else(|_| "user".to_string()),
                None => "user".to_string(),
            };
            let content = match dict.get_item("content")? {
                Some(v) => v.extract::<String>().unwrap_or_default(),
                None => String::new(),
            };
            result.push(Message { role, content });
        }
        Ok(result)
    })?;

    // Prepend system message if provided
    let mut all_messages = Vec::new();
    if let Some(ref sys) = system {
        all_messages.push(Message {
            role: "system".to_string(),
            content: sys.clone(),
        });
    }
    all_messages.extend(py_messages);

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl =
                quota_router_core::native_http::HttpProviderFactory::create(provider_name)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported in litellm-mode",
                            provider_name
                        ))
                    })?;

            let shared_messages = to_shared_messages(&all_messages);
            let request = quota_router_core::native_http::HttpCompletionRequest {
                model: model.clone(),
                messages: shared_messages,
                stream,
                temperature: temperature.map(|t| t as f32),
                max_tokens: Some(max_tokens as u32),
                top_p: top_p.map(|p| p as f32),
                stop: stop_sequences,
                n: None,
                presence_penalty: None,
                frequency_penalty: None,
                user: None,
                api_base: api_base.clone(),
                tools: None,
                tool_choice: None,
                response_format: None,
                seed: None,
                logprobs: None,
                top_logprobs: None,
                parallel_tool_calls: None,
                prompt_id: None,
                prompt_variables: None,
                provider_params: None,
                timeout: None,
            };

            // True async: await the provider's async completion method
            let result = provider_impl
                .completion(&request, api_key.as_deref())
                .await
                .map_err(|e| provider_error_to_py(e, provider_name))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("type", "message")?;
                dict.set_item("role", "assistant")?;
                let content_list = pyo3::types::PyList::empty(py);
                let content_block = pyo3::types::PyDict::new(py);
                content_block.set_item("type", "text")?;
                if let Some(choice) = result.choices.first() {
                    content_block.set_item("text", &choice.message.content)?;
                }
                content_list.append(content_block)?;
                dict.set_item("content", content_list)?;
                dict.set_item("model", &result.model)?;
                if let Some(choice) = result.choices.first() {
                    dict.set_item("stop_reason", &choice.finish_reason)?;
                }
                let usage_dict = pyo3::types::PyDict::new(py);
                usage_dict.set_item("input_tokens", result.usage.prompt_tokens)?;
                usage_dict.set_item("output_tokens", result.usage.completion_tokens)?;
                dict.set_item("usage", usage_dict)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            let pn = provider_name.to_string();

            if supports_native_async(&pn) {
                // Phase 2: Native async messages via AsyncAnthropic
                let model_clone = model.clone();
                let ak = api_key.clone();
                let ab = api_base.clone();
                let stop_ref = stop_sequences.as_deref();
                let sys_ref = system.as_deref();

                let result_py = Python::with_gil(|py| {
                    let awaitable = create_async_messages_py(
                        py,
                        &model_clone,
                        &all_messages,
                        max_tokens,
                        sys_ref,
                        temperature,
                        top_p,
                        stop_ref,
                        ak.as_deref(),
                        ab.as_deref(),
                    )?;
                    pyo3_asyncio_0_21::tokio::into_future(awaitable.into_bound(py))
                })
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "{}: Failed to create async call: {}",
                        pn, e
                    ))
                })?
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{}: {}", pn, e)))?;

                // Convert Anthropic async response to dict format
                Python::with_gil(|py| {
                    let result_ref = result_py.as_ref(py);
                    let dict = pyo3::types::PyDict::new(py);
                    if let Ok(id) = result_ref.getattr("id") {
                        dict.set_item("id", id)?;
                    }
                    dict.set_item("type", "message")?;
                    if let Ok(role) = result_ref.getattr("role") {
                        dict.set_item("role", role)?;
                    }
                    if let Ok(content) = result_ref.getattr("content") {
                        let content_list = pyo3::types::PyList::empty(py);
                        for block_result in content.iter()? {
                            let block = block_result?;
                            if let Ok(text) = block.getattr("text") {
                                let content_block = pyo3::types::PyDict::new(py);
                                content_block.set_item("type", "text")?;
                                content_block.set_item("text", text)?;
                                content_list.append(content_block)?;
                            }
                        }
                        dict.set_item("content", content_list)?;
                    }
                    if let Ok(model_val) = result_ref.getattr("model") {
                        dict.set_item("model", model_val)?;
                    }
                    if let Ok(stop_reason) = result_ref.getattr("stop_reason") {
                        dict.set_item("stop_reason", stop_reason)?;
                    }
                    if let Ok(usage) = result_ref.getattr("usage") {
                        let usage_dict = pyo3::types::PyDict::new(py);
                        if let Ok(it) = usage.getattr("input_tokens") {
                            usage_dict.set_item("input_tokens", it)?;
                        }
                        if let Ok(ot) = usage.getattr("output_tokens") {
                            usage_dict.set_item("output_tokens", ot)?;
                        }
                        dict.set_item("usage", usage_dict)?;
                    }
                    Ok(dict.into())
                })
            } else {
                // Phase 1 fallback: spawn_blocking
                let messages_clone = messages.clone();
                let model_clone = model.clone();
                let system_clone = system.clone();
                let stop_sequences_clone = stop_sequences.clone();
                let tools_clone = tools.clone();
                let tool_choice_clone = tool_choice.clone();
                let thinking_clone = thinking.clone();
                let metadata_clone = metadata.clone();
                let cache_control_clone = cache_control.clone();
                let api_key_clone = api_key.clone();
                let api_base_clone = api_base.clone();
                let provider_name_clone = provider_name.to_string();
                let mode_clone = _mode.clone();
                tokio::task::spawn_blocking(move || {
                    self::messages(
                        model_clone,
                        messages_clone,
                        max_tokens,
                        system_clone,
                        temperature,
                        top_p,
                        top_k,
                        stop_sequences_clone,
                        stream,
                        tools_clone,
                        tool_choice_clone,
                        thinking_clone,
                        metadata_clone,
                        cache_control_clone,
                        api_key_clone,
                        api_base_clone,
                        client_args,
                        Some(provider_name_clone),
                        mode_clone,
                    )
                })
                .await
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "spawn_blocking failed: {}",
                        e
                    ))
                })?
            }
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

// =============================================================================
// Responses API (OpenAI Responses API)
// RFC-0920: OpenAI-compatible Responses API
// =============================================================================

/// responses - Sync OpenAI Responses API call
///
/// Note: The quota-router proxy does not yet support the Responses API endpoint.
/// responses - OpenAI Responses API call
#[pyfunction]
#[pyo3(
    name = "responses",
    text_signature = "(model, input=None, input_data=None, *, provider=None, **kwargs)"
)]
pub fn responses(
    model: String,
    input: Option<Py<PyAny>>,      // litellm convention
    input_data: Option<Py<PyAny>>, // any-llm convention
    instructions: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i32>,
    top_p: Option<f64>,
    stream: Option<bool>,
    tools: Option<Py<PyAny>>,
    tool_choice: Option<Py<PyAny>>,
    store: Option<bool>,
    metadata: Option<Py<PyAny>>,
    user: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    provider: Option<String>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;

    // Dual-convention: accept both `input` (litellm) and `input_data` (any-llm)
    let data = input.or(input_data).ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(
            "responses() requires either `input` or `input_data` parameter",
        )
    })?;

    // Convert input to JSON value
    let input_json: serde_json::Value = Python::with_gil(|py| -> PyResult<serde_json::Value> {
        if let Ok(s) = data.extract::<String>(py) {
            return Ok(serde_json::Value::String(s));
        }
        // For complex types, serialize via json module
        python_to_json(py, &data)
    })?;

    let provider_name = provider.as_deref().unwrap_or("openai");
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl =
                quota_router_core::native_http::HttpProviderFactory::create(provider_name)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported in litellm-mode",
                            provider_name
                        ))
                    })?;

            // Convert tools/tool_choice to JSON if provided
            let tools_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
                if let Some(ref t) = tools {
                    return Ok(Some(python_to_json(py, t)?));
                }
                Ok(None)
            })?;
            let tool_choice_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
                if let Some(ref tc) = tool_choice {
                    return Ok(Some(python_to_json(py, tc)?));
                }
                Ok(None)
            })?;
            let metadata_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
                if let Some(ref m) = metadata {
                    return Ok(Some(python_to_json(py, m)?));
                }
                Ok(None)
            })?;

            let request = quota_router_core::native_http::HttpResponsesRequest {
                model: model.clone(),
                input: input_json,
                instructions,
                temperature,
                max_output_tokens: max_output_tokens.map(|v| v as u32),
                top_p,
                stream,
                tools: tools_json,
                tool_choice: tool_choice_json,
                store,
                metadata: metadata_json,
                user,
                api_base: api_base.clone(),
                timeout: None,
            };

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .create_response(&request, api_key.as_deref())
                        .await
                })
                .map_err(|e| provider_error_to_py(e, provider_name))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("object", &result.object)?;
                dict.set_item("model", &result.model)?;
                dict.set_item("status", &result.status)?;
                let output_obj =
                    json_to_python(py, &serde_json::Value::Array(result.output.clone()))?;
                dict.set_item("output", output_obj)?;
                if let Some(usage) = &result.usage {
                    let usage_obj = json_to_python(py, usage)?;
                    dict.set_item("usage", usage_obj)?;
                }
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "responses() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// aresponses - Async OpenAI Responses API call
#[pyfunction]
#[pyo3(name = "aresponses")]
pub async fn aresponses(
    model: String,
    input: Option<Py<PyAny>>,      // litellm convention
    input_data: Option<Py<PyAny>>, // any-llm convention
    instructions: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i32>,
    top_p: Option<f64>,
    stream: Option<bool>,
    tools: Option<Py<PyAny>>,
    tool_choice: Option<Py<PyAny>>,
    store: Option<bool>,
    metadata: Option<Py<PyAny>>,
    user: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    provider: Option<String>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = &client_args;

    // Dual-convention: accept both `input` (litellm) and `input_data` (any-llm)
    let data = input.or(input_data).ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(
            "responses() requires either `input` or `input_data` parameter",
        )
    })?;

    // Clone data for potential any-llm fallback before it's consumed by the JSON closure
    let data_clone = Python::with_gil(|py| data.clone_ref(py));

    // Convert input to JSON value
    let input_json: serde_json::Value = Python::with_gil(|py| -> PyResult<serde_json::Value> {
        if let Ok(s) = data.extract::<String>(py) {
            return Ok(serde_json::Value::String(s));
        }
        python_to_json(py, &data)
    })?;

    let provider_name = provider.as_deref().unwrap_or("openai");
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl =
                quota_router_core::native_http::HttpProviderFactory::create(provider_name)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyNotImplementedError::new_err(format!(
                            "Provider '{}' not supported in litellm-mode",
                            provider_name
                        ))
                    })?;

            // Convert tools/tool_choice/metadata to JSON if provided
            let tools_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
                if let Some(ref t) = tools {
                    return Ok(Some(python_to_json(py, t)?));
                }
                Ok(None)
            })?;
            let tool_choice_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
                if let Some(ref tc) = tool_choice {
                    return Ok(Some(python_to_json(py, tc)?));
                }
                Ok(None)
            })?;
            let metadata_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
                if let Some(ref m) = metadata {
                    return Ok(Some(python_to_json(py, m)?));
                }
                Ok(None)
            })?;

            let request = quota_router_core::native_http::HttpResponsesRequest {
                model: model.clone(),
                input: input_json,
                instructions,
                temperature,
                max_output_tokens: max_output_tokens.map(|v| v as u32),
                top_p,
                stream,
                tools: tools_json,
                tool_choice: tool_choice_json,
                store,
                metadata: metadata_json,
                user,
                api_base: api_base.clone(),
                timeout: None,
            };

            // True async: await the provider's async create_response method
            let result = provider_impl
                .create_response(&request, api_key.as_deref())
                .await
                .map_err(|e| provider_error_to_py(e, provider_name))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("object", &result.object)?;
                dict.set_item("model", &result.model)?;
                dict.set_item("status", &result.status)?;
                let output_obj =
                    json_to_python(py, &serde_json::Value::Array(result.output.clone()))?;
                dict.set_item("output", output_obj)?;
                if let Some(usage) = &result.usage {
                    let usage_obj = json_to_python(py, usage)?;
                    dict.set_item("usage", usage_obj)?;
                }
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let model_clone = model.clone();
            let instructions_clone = instructions.clone();
            let tools_clone = tools.clone();
            let tool_choice_clone = tool_choice.clone();
            let metadata_clone = metadata.clone();
            let user_clone = user.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let provider_name_clone = provider_name.to_string();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::responses(
                    model_clone,
                    Some(data_clone),
                    None,
                    instructions_clone,
                    temperature,
                    max_output_tokens,
                    top_p,
                    stream,
                    tools_clone,
                    tool_choice_clone,
                    store,
                    metadata_clone,
                    user_clone,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    Some(provider_name_clone),
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

// =============================================================================
// Responses API Sub-Methods (per RFC-0920 / RFC-0953)

/// get_response - Retrieve a response by ID from provider storage (OpenAI Responses API)
#[pyfunction]
#[pyo3(
    name = "get_response",
    text_signature = "(provider, response_id, **kwargs)"
)]
pub fn get_response(
    provider: String,
    response_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .get_response(&response_id, api_key.as_deref(), api_base.as_deref(), None)
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("object", &result.object)?;
                dict.set_item("model", &result.model)?;
                dict.set_item("status", &result.status)?;
                // Convert serde_json::Value output to Python object
                let output_obj =
                    json_to_python(py, &serde_json::Value::Array(result.output.clone()))?;
                dict.set_item("output", output_obj)?;
                if let Some(usage) = &result.usage {
                    let usage_obj = json_to_python(py, usage)?;
                    dict.set_item("usage", usage_obj)?;
                }
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "get_response() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// aget_response - Async retrieve a response by ID
#[pyfunction]
#[pyo3(name = "aget_response")]
pub async fn aget_response(
    provider: String,
    response_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch for litellm-mode
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());
    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;
            let result = provider_impl
                .get_response(&response_id, api_key.as_deref(), api_base.as_deref(), None)
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;
            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("object", &result.object)?;
                dict.set_item("model", &result.model)?;
                dict.set_item("status", &result.status)?;
                let output_obj =
                    json_to_python(py, &serde_json::Value::Array(result.output.clone()))?;
                dict.set_item("output", output_obj)?;
                if let Some(usage) = &result.usage {
                    dict.set_item("usage", json_to_python(py, usage)?)?;
                }
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::get_response(
                    provider_clone,
                    response_id,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// delete_response - Delete a response by ID from provider storage (OpenAI Responses API)
#[pyfunction]
#[pyo3(
    name = "delete_response",
    text_signature = "(provider, response_id, **kwargs)"
)]
pub fn delete_response(
    provider: String,
    response_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .delete_response(
                            &response_id,
                            api_key.as_deref(),
                            api_base.as_deref(),
                            None,
                        )
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("object", &result.object)?;
                dict.set_item("deleted", result.deleted)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "delete_response() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// adelete_response - Async delete a response by ID
#[pyfunction]
#[pyo3(name = "adelete_response")]
pub async fn adelete_response(
    provider: String,
    response_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch for litellm-mode
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());
    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;
            let result = provider_impl
                .delete_response(&response_id, api_key.as_deref(), api_base.as_deref(), None)
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;
            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("id", &result.id)?;
                dict.set_item("object", &result.object)?;
                dict.set_item("deleted", result.deleted)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::delete_response(
                    provider_clone,
                    response_id,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
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
pub fn list_models(
    provider: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .list_models(api_key.as_deref(), api_base.as_deref(), None)
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("object", &result.object)?;
                let data_list = pyo3::types::PyList::empty(py);
                for model in &result.data {
                    let model_dict = pyo3::types::PyDict::new(py);
                    model_dict.set_item("id", &model.id)?;
                    model_dict.set_item("object", &model.object)?;
                    model_dict.set_item("created", model.created)?;
                    model_dict.set_item("owned_by", &model.owned_by)?;
                    data_list.append(model_dict)?;
                }
                dict.set_item("data", data_list)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "list_models() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// alist_models - Async list models API
#[pyfunction]
#[pyo3(name = "alist_models")]
pub async fn alist_models(
    provider: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = &client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            // True async: await the provider's async list_models method
            let result = provider_impl
                .list_models(api_key.as_deref(), api_base.as_deref(), None)
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("object", &result.object)?;
                let data_list = pyo3::types::PyList::empty(py);
                for model in &result.data {
                    let model_dict = pyo3::types::PyDict::new(py);
                    model_dict.set_item("id", &model.id)?;
                    model_dict.set_item("object", &model.object)?;
                    model_dict.set_item("created", model.created)?;
                    model_dict.set_item("owned_by", &model.owned_by)?;
                    data_list.append(model_dict)?;
                }
                dict.set_item("data", data_list)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                list_models(
                    provider_clone,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

// =============================================================================
// Batch API (OpenAI Batch API)
// RFC-0920: OpenAI-compatible Batch API
// =============================================================================

/// Helper: Convert HttpBatchObject to Python dict
fn batch_to_dict(batch: &quota_router_core::native_http::HttpBatchObject) -> PyResult<Py<PyAny>> {
    Python::with_gil(|py| {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("id", &batch.id)?;
        dict.set_item("object", &batch.object)?;
        dict.set_item("endpoint", &batch.endpoint)?;
        dict.set_item("status", &batch.status)?;
        dict.set_item("input_file_id", &batch.input_file_id)?;
        if let Some(ref ofi) = batch.output_file_id {
            dict.set_item("output_file_id", ofi)?;
        }
        if let Some(ref efi) = batch.error_file_id {
            dict.set_item("error_file_id", efi)?;
        }
        if let Some(ref cw) = batch.completion_window {
            dict.set_item("completion_window", cw)?;
        }
        if let Some(ca) = batch.created_at {
            dict.set_item("created_at", ca)?;
        }
        if let Some(ref rc) = batch.request_counts {
            let rc_obj = json_to_python(py, rc)?;
            dict.set_item("request_counts", rc_obj)?;
        }
        if let Some(ref md) = batch.metadata {
            let md_obj = json_to_python(py, md)?;
            dict.set_item("metadata", md_obj)?;
        }
        Ok(dict.into())
    })
}

/// batch_create - Create a batch job via provider Batch API
#[pyfunction]
#[pyo3(
    name = "batch_create",
    text_signature = "(provider, input_file, endpoint, **kwargs)"
)]
pub fn batch_create(
    provider: String,
    input_file: String,
    endpoint: String,
    completion_window: Option<String>,
    metadata: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());

    let metadata_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
        if let Some(ref m) = metadata {
            return Ok(Some(python_to_json(py, m)?));
        }
        Ok(None)
    })?;

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let request = quota_router_core::native_http::HttpBatchCreateRequest {
                input_file,
                endpoint,
                completion_window: completion_window.unwrap_or_else(|| "24h".to_string()),
                metadata: metadata_json,
                api_base: api_base.clone(),
                timeout: None,
            };

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .batch_create(&request, api_key.as_deref())
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            batch_to_dict(&result)
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "batch_create() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// abatch_create - Async create batch API
#[pyfunction]
#[pyo3(name = "abatch_create")]
pub async fn abatch_create(
    provider: String,
    input_file: String,
    endpoint: String,
    completion_window: Option<String>,
    metadata: Option<Py<PyAny>>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = &client_args;
    let mode = resolve_mode(_mode.as_deref());

    let metadata_json = Python::with_gil(|py| -> PyResult<Option<serde_json::Value>> {
        if let Some(ref m) = metadata {
            return Ok(Some(python_to_json(py, m)?));
        }
        Ok(None)
    })?;

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let request = quota_router_core::native_http::HttpBatchCreateRequest {
                input_file,
                endpoint,
                completion_window: completion_window.unwrap_or_else(|| "24h".to_string()),
                metadata: metadata_json,
                api_base: api_base.clone(),
                timeout: None,
            };

            // True async: await the provider's async batch_create method
            let result = provider_impl
                .batch_create(&request, api_key.as_deref())
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;

            batch_to_dict(&result)
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let input_file_clone = input_file.clone();
            let endpoint_clone = endpoint.clone();
            let completion_window_clone = completion_window.clone();
            let metadata_clone = metadata.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::batch_create(
                    provider_clone,
                    input_file_clone,
                    endpoint_clone,
                    completion_window_clone,
                    metadata_clone,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// batch_retrieve - Retrieve a batch job
#[pyfunction]
#[pyo3(
    name = "batch_retrieve",
    text_signature = "(provider, batch_id, **kwargs)"
)]
pub fn batch_retrieve(
    provider: String,
    batch_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .batch_retrieve(&batch_id, api_key.as_deref(), api_base.as_deref(), None)
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            batch_to_dict(&result)
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "batch_retrieve() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// abatch_retrieve - Async retrieve batch API
#[pyfunction]
#[pyo3(name = "abatch_retrieve")]
pub async fn abatch_retrieve(
    provider: String,
    batch_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = &client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            // True async: await the provider's async batch_retrieve method
            let result = provider_impl
                .batch_retrieve(&batch_id, api_key.as_deref(), api_base.as_deref(), None)
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;

            batch_to_dict(&result)
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::batch_retrieve(
                    provider_clone,
                    batch_id,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// batch_cancel - Cancel a batch job
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
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .batch_cancel(&batch_id, api_key.as_deref(), api_base.as_deref(), None)
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            batch_to_dict(&result)
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "batch_cancel() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// abatch_cancel - Async cancel batch API
#[pyfunction]
#[pyo3(name = "abatch_cancel")]
pub async fn abatch_cancel(
    provider: String,
    batch_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = &client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            // True async: await the provider's async batch_cancel method
            let result = provider_impl
                .batch_cancel(&batch_id, api_key.as_deref(), api_base.as_deref(), None)
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;

            batch_to_dict(&result)
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::batch_cancel(
                    provider_clone,
                    batch_id,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// batch_list - Sync list batches API
#[pyfunction]
#[pyo3(name = "batch_list", text_signature = "(provider, **kwargs)")]
pub fn batch_list(
    provider: String,
    limit: Option<i32>,
    _after: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = (client_args, _after);
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .batch_list(
                            api_key.as_deref(),
                            api_base.as_deref(),
                            limit.map(|l| l as u32),
                            None,
                        )
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("object", &result.object)?;
                dict.set_item("has_more", result.has_more)?;
                let data_list = pyo3::types::PyList::empty(py);
                for batch in &result.data {
                    data_list.append(batch_to_dict(batch)?)?;
                }
                dict.set_item("data", data_list)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "batch_list() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// abatch_list - Async list batches API
#[pyfunction]
#[pyo3(name = "abatch_list")]
pub async fn abatch_list(
    provider: String,
    limit: Option<i32>,
    after: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = (&client_args, &after);
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            // True async: await the provider's async batch_list method
            let result = provider_impl
                .batch_list(
                    api_key.as_deref(),
                    api_base.as_deref(),
                    limit.map(|l| l as u32),
                    None,
                )
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("object", &result.object)?;
                dict.set_item("has_more", result.has_more)?;
                let data_list = pyo3::types::PyList::empty(py);
                for batch in &result.data {
                    data_list.append(batch_to_dict(batch)?)?;
                }
                dict.set_item("data", data_list)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let after_clone = after.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::batch_list(
                    provider_clone,
                    limit,
                    after_clone,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// batch_results - Sync retrieve batch results API
#[pyfunction]
#[pyo3(
    name = "batch_results",
    text_signature = "(provider, batch_id, **kwargs)"
)]
pub fn batch_results(
    provider: String,
    batch_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    ensure_providers_initialized();
    let _ = client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            let rt = tokio::runtime::Runtime::new().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e))
            })?;

            let result = rt
                .block_on(async {
                    provider_impl
                        .batch_results(&batch_id, api_key.as_deref(), api_base.as_deref(), None)
                        .await
                })
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                let results_obj =
                    json_to_python(py, &serde_json::Value::Array(result.results.clone()))?;
                dict.set_item("results", results_obj)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            Err(pyo3::exceptions::PyNotImplementedError::new_err(
                "batch_results() is not yet supported in any-llm-mode. Use litellm-mode.",
            ))
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
}

/// abatch_results - Async retrieve batch results API
#[pyfunction]
#[pyo3(name = "abatch_results")]
pub async fn abatch_results(
    provider: String,
    batch_id: String,
    api_key: Option<String>,
    api_base: Option<String>,
    client_args: Option<Py<PyAny>>,
    _mode: Option<String>,
) -> PyResult<Py<PyAny>> {
    // True async dispatch — no rt.block_on(), no blocking the event loop
    ensure_providers_initialized();
    let _ = &client_args;
    let mode = resolve_mode(_mode.as_deref());

    match mode {
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::LiteLLM => {
            let provider_impl = quota_router_core::native_http::HttpProviderFactory::create(
                &provider,
            )
            .ok_or_else(|| {
                pyo3::exceptions::PyNotImplementedError::new_err(format!(
                    "Provider '{}' not supported in litellm-mode",
                    provider
                ))
            })?;

            // True async: await the provider's async batch_results method
            let result = provider_impl
                .batch_results(&batch_id, api_key.as_deref(), api_base.as_deref(), None)
                .await
                .map_err(|e| provider_error_to_py(e, &provider))?;

            Python::with_gil(|py| {
                let dict = pyo3::types::PyDict::new(py);
                let results_obj =
                    json_to_python(py, &serde_json::Value::Array(result.results.clone()))?;
                dict.set_item("results", results_obj)?;
                Ok(dict.into())
            })
        }
        #[cfg(feature = "full")]
        quota_router_core::mode::ProviderMode::AnyLlm => {
            // Clone args that need to move into spawn_blocking
            let provider_clone = provider.clone();
            let api_key_clone = api_key.clone();
            let api_base_clone = api_base.clone();
            let mode_clone = _mode.clone();
            tokio::task::spawn_blocking(move || {
                self::batch_results(
                    provider_clone,
                    batch_id,
                    api_key_clone,
                    api_base_clone,
                    client_args,
                    mode_clone,
                )
            })
            .await
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("spawn_blocking failed: {}", e))
            })?
        }
        #[cfg(not(feature = "full"))]
        _ => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "No mode compiled",
        )),
    }
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
        stop.and_then(|v| {
            if v.is_empty() {
                None
            } else {
                Some(v.join(","))
            }
        }),
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
        stop.and_then(|v| {
            if v.is_empty() {
                None
            } else {
                Some(v.join(","))
            }
        }),
        presence_penalty,
        frequency_penalty,
        None,    // user
        None,    // seed
        timeout, // timeout
        None,    // extra_headers
        None,    // base_url
        None,    // api_version
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
