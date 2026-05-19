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
use crate::types::Message;
use pyo3::prelude::*;

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

    // Provider not yet supported in any-llm (direct PyO3) mode
    Err(pyo3::exceptions::PyNotImplementedError::new_err(format!(
        "Provider '{}' is not yet supported in any-llm (direct) mode. \
         Use litellm mode (via the quota-router proxy) for this provider, \
         or switch to a supported provider (openai, anthropic, mistral, gemini, groq, \
         cohere, perplexity, deepseek, deepinfra, dashscope, azure, azureanthropic, \
         azureopenai, together, bedrock, fireworks, cerebras, openrouter, xai, \
         huggingface, mzai, minimax, nebius, moonshot, ollama, voyage, databricks, \
         sagemaker, sambanova, vertexai, watsonx, gateway, platform, vertexaianthropic, \
         llama, llamacpp, llamafile, lmstudio, inception, vllm, portkey, zai).",
        parsed.provider
    )))
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

    // Provider not yet supported in any-llm (direct PyO3) mode
    Err(pyo3::exceptions::PyNotImplementedError::new_err(format!(
        "Provider '{}' is not yet supported in any-llm (direct) mode. \
         Use litellm mode (via the quota-router proxy) for this provider, \
         or switch to a supported provider (openai, anthropic, mistral, gemini, groq, \
         cohere, perplexity, deepseek, deepinfra, dashscope, azure, azureanthropic, \
         azureopenai, together, bedrock, fireworks, cerebras, openrouter, xai, \
         huggingface, mzai, minimax, nebius, moonshot, ollama, voyage, databricks, \
         sagemaker, sambanova, vertexai, watsonx, gateway, platform, vertexaianthropic, \
         llama, llamacpp, llamafile, lmstudio, inception, vllm, portkey, zai).",
        parsed.provider
    )))
}

/// embedding - Sync embedding call
#[pyfunction]
#[pyo3(
    name = "embedding",
    text_signature = "(input, model, api_key=None, api_base=None, **kwargs)"
)]
pub fn embedding(
    _input: Py<PyAny>,
    _model: String,
    _api_key: Option<String>,
    _api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Embeddings are not yet implemented in any-llm (direct PyO3) mode.
    // Use litellm mode (via the quota-router proxy) or call provider SDKs directly.
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
    _input: Py<PyAny>,
    _model: String,
    _api_key: Option<String>,
    _api_base: Option<String>,
) -> PyResult<Py<PyAny>> {
    // Embeddings are not yet implemented in any-llm (direct PyO3) mode.
    // Use litellm mode (via the quota-router proxy) or call provider SDKs directly.
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
