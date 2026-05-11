// Completion functions for PyO3 bindings

#![allow(clippy::too_many_arguments)]
#![allow(deprecated)]

use crate::model::ParsedModel;
use crate::providers::anthropic::AnthropicProvider;
use crate::providers::azure::AZUREProvider;
use crate::providers::azureanthropic::AZUREANTHROPICProvider;
use crate::providers::azureopenai::AZUREOPENAIProvider;
use crate::providers::base::LLMProvider;
use crate::providers::bedrock::BEDROCKProvider;
use crate::providers::cerebras::CEREBRASProvider;
use crate::providers::cohere::COHEREProvider;
use crate::providers::dashscope::DASHSCOPEProvider;
use crate::providers::databricks::DATABRICKSProvider;
use crate::providers::deepinfra::DEEPINFRAProvider;
use crate::providers::deepseek::DEEPSEEKProvider;
use crate::providers::fireworks::FIREWORKSProvider;
use crate::providers::gateway::GATEWAYProvider;
use crate::providers::gemini::GeminiProvider;
use crate::providers::groq::GROQProvider;
use crate::providers::huggingface::HUGGINGFACEProvider;
use crate::providers::inception::INCEPTIONProvider;
use crate::providers::llama::LLAMAProvider;
use crate::providers::llamacpp::LLAMACPPProvider;
use crate::providers::llamafile::LLAMAFILEProvider;
use crate::providers::lmstudio::LMSTUDIOProvider;
use crate::providers::minimax::MINIMAXProvider;
use crate::providers::mistral::MistralProvider;
use crate::providers::moonshot::MOONSHOTProvider;
use crate::providers::mzai::MZAIProvider;
use crate::providers::nebius::NEBIUSProvider;
use crate::providers::ollama::OLLAMAProvider;
use crate::providers::openai::OpenAIProvider;
use crate::providers::openrouter::OPENROUTERProvider;
use crate::providers::perplexity::PERPLEXITYProvider;
use crate::providers::platform::PLATFORMProvider;
use crate::providers::portkey::PORTKEYProvider;
use crate::providers::sagemaker::SAGEMAKERProvider;
use crate::providers::sambanova::SAMBANOVAProvider;
use crate::providers::together::TOGETHERProvider;
use crate::providers::vertexai::VERTEXAIProvider;
use crate::providers::vertexaianthropic::VERTEXAIANTHROPICProvider;
use crate::providers::vllm::VLLMProvider;
use crate::providers::voyage::VOYAGEProvider;
use crate::providers::watsonx::WATSONXProvider;
use crate::providers::xai::XAIProvider;
use crate::providers::zai::ZAIProvider;
use crate::streaming::{chunks_to_pylist, create_chunk_list};
use crate::types::{ChatCompletion, Choice, Message};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};

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
    _seed: Option<i32>,
    _timeout: Option<f64>,
    _extra_headers: Option<String>,
    _base_url: Option<String>,
    _api_version: Option<String>,
    // quota-router specific
    api_key: Option<String>,
    // Phase 4 parameters
    _service_tier: Option<String>,
    _background: Option<bool>,
    _prompt_cache_key: Option<String>,
    _prompt_cache_retention: Option<String>,
    _conversation: Option<String>,
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

    // For DeepInfra provider, use real SDK
    if parsed.provider == "deepinfra" {
        let provider = DEEPINFRAProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init DeepInfra client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("DeepInfra API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For DashScope provider, use real SDK
    if parsed.provider == "dashscope" {
        let provider = DASHSCOPEProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init DashScope client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("DashScope API error: {}", e.message());
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

    // For AzureAnthropic provider, use real SDK
    if parsed.provider == "azureanthropic" {
        let provider = AZUREANTHROPICProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init AzureAnthropic client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("AzureAnthropic API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For AzureOpenAI provider, use real SDK
    if parsed.provider == "azureopenai" {
        let provider = AZUREOPENAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init AzureOpenAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("AzureOpenAI API error: {}", e.message());
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

    // For Bedrock provider, use real SDK
    if parsed.provider == "bedrock" {
        let provider = BEDROCKProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Bedrock client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Bedrock API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Fireworks provider, use real SDK
    if parsed.provider == "fireworks" {
        let provider = FIREWORKSProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Fireworks client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Fireworks API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Cerebras provider, use real SDK
    if parsed.provider == "cerebras" {
        let provider = CEREBRASProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Cerebras client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Cerebras API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For OpenRouter provider, use real SDK
    if parsed.provider == "openrouter" {
        let provider = OPENROUTERProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init OpenRouter client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("OpenRouter API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For xAI provider, use real SDK
    if parsed.provider == "xai" {
        let provider = XAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init xAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("xAI API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For HuggingFace provider, use real SDK
    if parsed.provider == "huggingface" {
        let provider = HUGGINGFACEProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init HuggingFace client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("HuggingFace API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For MZAI provider, use real SDK
    if parsed.provider == "mzai" {
        let provider = MZAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init MZAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("MZAI API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For MiniMax provider, use real SDK
    if parsed.provider == "minimax" {
        let provider = MINIMAXProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init MiniMax client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("MiniMax API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Nebius provider, use real SDK
    if parsed.provider == "nebius" {
        let provider = NEBIUSProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Nebius client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Nebius API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Moonshot provider, use real SDK
    if parsed.provider == "moonshot" {
        let provider = MOONSHOTProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Moonshot client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Moonshot API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Ollama provider, use real SDK
    if parsed.provider == "ollama" {
        let provider = OLLAMAProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Ollama client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Ollama API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Voyage provider, use real SDK
    if parsed.provider == "voyage" {
        let provider = VOYAGEProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Voyage client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Voyage API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Databricks provider, use real SDK
    if parsed.provider == "databricks" {
        let provider = DATABRICKSProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Databricks client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Databricks API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For SageMaker provider, use real SDK
    if parsed.provider == "sagemaker" {
        let provider = SAGEMAKERProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init SageMaker client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("SageMaker API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For SambaNova provider, use real SDK
    if parsed.provider == "sambanova" {
        let provider = SAMBANOVAProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init SambaNova client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("SambaNova API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For VertexAI provider, use real SDK
    if parsed.provider == "vertexai" {
        let provider = VERTEXAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init VertexAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("VertexAI API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Watsonx provider, use real SDK
    if parsed.provider == "watsonx" {
        let provider = WATSONXProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Watsonx client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Watsonx API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Gateway provider, use real SDK
    if parsed.provider == "gateway" {
        let provider = GATEWAYProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Gateway client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Gateway API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Platform provider, use real SDK
    if parsed.provider == "platform" {
        let provider = PLATFORMProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Platform client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Platform API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For VertexAI Anthropic provider, use real SDK
    if parsed.provider == "vertexaianthropic" {
        let provider = VERTEXAIANTHROPICProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init VertexAI Anthropic client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("VertexAI Anthropic API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Llama provider, use real SDK
    if parsed.provider == "llama" {
        let provider = LLAMAProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Llama client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Llama API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For LlamaCPP provider, use real SDK
    if parsed.provider == "llamacpp" {
        let provider = LLAMACPPProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init LlamaCPP client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("LlamaCPP API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Llamafile provider, use real SDK
    if parsed.provider == "llamafile" {
        let provider = LLAMAFILEProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Llamafile client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Llamafile API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For LMStudio provider, use real SDK
    if parsed.provider == "lmstudio" {
        let provider = LMSTUDIOProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init LMStudio client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("LMStudio API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Inception provider, use real SDK
    if parsed.provider == "inception" {
        let provider = INCEPTIONProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Inception client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Inception API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For vLLM provider, use real SDK
    if parsed.provider == "vllm" {
        let provider = VLLMProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init vLLM client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("vLLM API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For Portkey provider, use real SDK
    if parsed.provider == "portkey" {
        let provider = PORTKEYProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Portkey client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Portkey API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For ZAI provider, use real SDK
    if parsed.provider == "zai" {
        let provider = ZAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init ZAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("ZAI API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For other providers, use mock response
    let content = messages
        .first()
        .map(|m| format!("{} Echo: {}", parsed.provider, m.content))
        .unwrap_or_default();

    let choices: Vec<Choice> = vec![Choice::new(0, Message::new("assistant", content), "stop")];

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
    stream: Option<bool>,
    _stop: Option<String>,
    _presence_penalty: Option<f64>,
    _frequency_penalty: Option<f64>,
    _user: Option<String>,
    _seed: Option<i32>,
    _timeout: Option<f64>,
    _extra_headers: Option<String>,
    _base_url: Option<String>,
    _api_version: Option<String>,
    // quota-router specific
    api_key: Option<String>,
    // Phase 4 parameters
    _service_tier: Option<String>,
    _background: Option<bool>,
    _prompt_cache_key: Option<String>,
    _prompt_cache_retention: Option<String>,
    _conversation: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Log the request parameters
    println!(
        "acompletion called: model={}, messages={}, stream={:?}",
        model,
        messages.len(),
        stream
    );

    // Parse model string to determine provider
    let parsed = match ParsedModel::parse(&model) {
        Ok(p) => p,
        Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e)),
    };

    let stream = stream.unwrap_or(false);

    // For OpenAI provider, use real SDK
    if parsed.provider == "openai" {
        let provider = crate::providers::openai::OpenAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init OpenAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
            Ok(response) => {
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
        let provider = crate::providers::anthropic::AnthropicProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Anthropic client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
            Ok(response) => {
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
        let provider = crate::providers::mistral::MistralProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Mistral client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
            Ok(response) => {
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
        let provider = crate::providers::gemini::GeminiProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Gemini client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
            Ok(response) => {
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
        let provider = crate::providers::groq::GROQProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Groq client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
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
        let provider = crate::providers::cohere::COHEREProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Cohere client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
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
        let provider = crate::providers::perplexity::PERPLEXITYProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Perplexity client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
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
        let provider = crate::providers::deepseek::DEEPSEEKProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init DeepSeek client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("DeepSeek API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For DeepInfra provider, use real SDK
    if parsed.provider == "deepinfra" {
        let provider = crate::providers::deepinfra::DEEPINFRAProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init DeepInfra client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        match provider.completion(&parsed.model, &messages, stream) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("DeepInfra API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For other providers, use mock response with provider name prefix
    let content = messages
        .first()
        .map(|m| format!("{} Echo: {}", parsed.provider, m.content))
        .unwrap_or_default();

    let choices: Vec<Choice> = vec![Choice::new(0, Message::new("assistant", content), "stop")];

    let response = ChatCompletion::new(
        format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        parsed.model,
        choices,
    );

    // Convert to Python dict
    Python::with_gil(|py| response.to_dict(py))
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
    _api_key: Option<String>,
    _api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!("embedding called: model={}", model);

    // Handle input: could be str or List[str]
    let inputs: Vec<String> = Python::with_gil(|py| {
        let py_input = input.as_ref(py);
        if py_input.is_instance_of::<PyString>() {
            vec![py_input.extract::<String>().unwrap_or_default()]
        } else if py_input.is_instance_of::<PyList>() {
            py_input
                .extract::<&PyList>()
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.extract::<String>().ok())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        }
    });

    // Mock embedding response
    let embeddings: Vec<crate::types::Embedding> = inputs
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let embedding: Vec<f32> = (0..384).map(|_| 0.1).collect();
            crate::types::Embedding::new(i as u32, embedding)
        })
        .collect();

    let response = crate::types::EmbeddingsResponse::new(model, embeddings);

    // Convert to dict
    let result = Python::with_gil(|py| {
        let dict = PyDict::new(py);
        dict.set_item("object", "list")?;

        let data_list = PyList::new(py, Vec::<&PyAny>::new());
        for emb in response.data.iter() {
            let emb_dict = PyDict::new(py);
            emb_dict.set_item("object", "embedding")?;
            emb_dict.set_item("embedding", &emb.embedding)?;
            emb_dict.set_item("index", emb.index)?;
            data_list.append(emb_dict)?;
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

/// aembedding - Async embedding call (per RFC-0920 lines 4031-4043)
#[pyfunction]
#[pyo3(
    name = "aembedding",
    text_signature = "(input, model, api_key=None, api_base=None, **kwargs)"
)]
pub async fn aembedding(
    input: Py<PyAny>,
    model: String,
    _api_key: Option<String>,
    _api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!("aembedding called: model={}", model);

    // Handle input: could be str or List[str]
    let inputs: Vec<String> = Python::with_gil(|py| {
        let py_input = input.as_ref(py);
        if py_input.is_instance_of::<PyString>() {
            vec![py_input.extract::<String>().unwrap_or_default()]
        } else if py_input.is_instance_of::<PyList>() {
            py_input
                .extract::<&PyList>()
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.extract::<String>().ok())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        }
    });

    // Mock embedding response
    let embeddings: Vec<crate::types::Embedding> = inputs
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

        let data_list = PyList::new(py, Vec::<&PyAny>::new());
        for emb in response.data.iter() {
            let emb_dict = PyDict::new(py);
            emb_dict.set_item("object", "embedding")?;
            emb_dict.set_item("embedding", &emb.embedding)?;
            emb_dict.set_item("index", emb.index)?;
            data_list.append(emb_dict)?;
        }
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
    _top_k: Option<i32>,
    _stop: Option<String>,
    _user: Option<String>,
    _system: Option<String>,
    _truncation: Option<String>,
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
    _top_k: Option<i32>,
    _stop: Option<String>,
    _user: Option<String>,
    _system: Option<String>,
    _truncation: Option<String>,
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

// =============================================================================
// Text Completion API (LiteLLM parity)
// =============================================================================

/// text_completion - Synchronous text completion (non-chat models)
#[pyfunction]
#[pyo3(name = "text_completion")]
pub fn text_completion(
    model: String,
    prompt: String,
    _frequency_penalty: Option<f64>,
    _logprobs: Option<i32>,
    _max_tokens: Option<i32>,
    _presence_penalty: Option<f64>,
    _stop: Option<Vec<String>>,
    _stream: Option<bool>,
    _temperature: Option<f64>,
    _top_p: Option<f64>,
    api_key: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!(
        "text_completion called: model={}, prompt_len={}",
        model,
        prompt.len()
    );

    // Parse model string to determine provider
    let parsed = match ParsedModel::parse(&model) {
        Ok(p) => p,
        Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e)),
    };

    // For OpenAI provider, use real SDK
    if parsed.provider == "openai" {
        let provider = OpenAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init OpenAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        // Build messages from prompt (converting text completion to chat format)
        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt,
        }];

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
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

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Anthropic client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt,
        }];

        match provider.completion(&parsed.model, &messages, false) {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Anthropic API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // For other providers, use chat completion with prompt wrapped as user message
    let messages = vec![Message {
        role: "user".to_string(),
        content: prompt,
    }];

    let chat_result = completion(
        model,
        messages,
        _temperature,
        _max_tokens,
        _top_p,
        None, // n
        _stream,
        None, // stop
        _presence_penalty,
        _frequency_penalty,
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
    )?;

    Ok(chat_result)
}

/// atext_completion - Asynchronous text completion (non-chat models)
#[pyfunction]
#[pyo3(name = "atext_completion")]
pub async fn atext_completion(
    model: String,
    prompt: String,
    _frequency_penalty: Option<f64>,
    _logprobs: Option<i32>,
    _max_tokens: Option<i32>,
    _presence_penalty: Option<f64>,
    _stop: Option<Vec<String>>,
    _stream: Option<bool>,
    _temperature: Option<f64>,
    _top_p: Option<f64>,
    _timeout: Option<f64>,
    api_key: Option<String>,
) -> PyResult<Py<PyAny>> {
    println!(
        "atext_completion called: model={}, prompt_len={}",
        model,
        prompt.len()
    );

    // Parse model string to determine provider
    let parsed = match ParsedModel::parse(&model) {
        Ok(p) => p,
        Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e)),
    };

    // For OpenAI provider, use real SDK
    if parsed.provider == "openai" {
        let provider = OpenAIProvider::new();

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init OpenAI client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt,
        }];

        match provider.acompletion(&parsed.model, &messages, false).await {
            Ok(response) => {
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

        if let Some(key) = api_key {
            if let Err(e) = provider.init_client(&key, None) {
                let err_msg = format!("Failed to init Anthropic client: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }

        let messages = vec![Message {
            role: "user".to_string(),
            content: prompt,
        }];

        match provider.acompletion(&parsed.model, &messages, false).await {
            Ok(response) => {
                return Python::with_gil(|py| response.to_dict(py));
            }
            Err(e) => {
                let err_msg = format!("Anthropic API error: {}", e.message());
                return Err(pyo3::exceptions::PyRuntimeError::new_err(err_msg));
            }
        }
    }

    // Fallback: use async completion
    let messages = vec![Message {
        role: "user".to_string(),
        content: prompt,
    }];

    acompletion(
        model,
        messages,
        _temperature,
        _max_tokens,
        _top_p,
        None, // n
        _stream,
        None, // stop
        _presence_penalty,
        _frequency_penalty,
        None,     // user
        None,     // seed
        _timeout, // timeout
        None,     // extra_headers
        None,     // base_url
        None,     // api_version
        api_key,
        None, // service_tier
        None, // background
        None, // prompt_cache_key
        None, // prompt_cache_retention
        None, // conversation
    )
    .await
}
