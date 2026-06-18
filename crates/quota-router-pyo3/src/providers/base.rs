// Base provider trait and types for quota-router-pyo3
// Inspired by any-llm's AnyLLM abstract base class

/// Provider feature flags
#[derive(Debug, Clone)]
pub struct ProviderFeatures {
    pub supports_completion: bool,
    pub supports_completion_streaming: bool,
    pub supports_embedding: bool,
    pub supports_responses: bool,
    pub supports_list_models: bool,
    pub supports_batch: bool,
    pub supports_messages: bool,
}

/// Static provider info - shared across all instances of a provider
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: &'static str,
    pub doc_url: &'static str,
    pub env_api_key: &'static str,
    pub env_api_base: Option<&'static str>,
    pub api_base: Option<&'static str>,
    pub features: ProviderFeatures,
}

impl ProviderInfo {
    pub const fn new(
        name: &'static str,
        doc_url: &'static str,
        env_api_key: &'static str,
        env_api_base: Option<&'static str>,
        api_base: Option<&'static str>,
        features: ProviderFeatures,
    ) -> Self {
        Self {
            name,
            doc_url,
            env_api_key,
            env_api_base,
            api_base,
            features,
        }
    }
}

/// All supported providers
#[allow(clippy::upper_case_acronyms)]
pub struct Providers;

impl Providers {
    /// Get provider info by name
    pub fn get(name: &str) -> Option<&'static ProviderInfo> {
        match name.to_lowercase().as_str() {
            "openai" => Some(&OPENAI_INFO),
            "anthropic" => Some(&ANTHROPIC_INFO),
            "mistral" => Some(&MISTRAL_INFO),
            "ollama" => Some(&OLLAMA_INFO),
            "gemini" => Some(&GEMINI_INFO),
            "deepinfra" => Some(&DEEPINFRA_INFO),
            "azure" => Some(&AZURE_INFO),
            "azureopenai" => Some(&AZUREOPENAI_INFO),
            "azureanthropic" => Some(&AZUREANTHROPIC_INFO),
            "bedrock" => Some(&BEDROCK_INFO),
            "cerebras" => Some(&CEREBRAS_INFO),
            "cohere" => Some(&COHERE_INFO),
            "dashscope" => Some(&DASHSCOPE_INFO),
            "databricks" => Some(&DATABRICKS_INFO),
            "deepseek" => Some(&DEEPSEEK_INFO),
            "fireworks" => Some(&FIREWORKS_INFO),
            "gateway" => Some(&GATEWAY_INFO),
            "groq" => Some(&GROQ_INFO),
            "huggingface" => Some(&HUGGINGFACE_INFO),
            "inception" => Some(&INCEPTION_INFO),
            "llama" => Some(&LLAMA_INFO),
            "llamacpp" => Some(&LLAMACPP_INFO),
            "llamafile" => Some(&LLAMAFILE_INFO),
            "lmstudio" => Some(&LMSTUDIO_INFO),
            "minimax" => Some(&MINIMAX_INFO),
            "moonshot" => Some(&MOONSHOT_INFO),
            "mzai" => Some(&MZAI_INFO),
            "nebius" => Some(&NEB_IUS_INFO),
            "openrouter" => Some(&OPENROUTER_INFO),
            "perplexity" => Some(&PERPLEXITY_INFO),
            "platform" => Some(&PLATFORM_INFO),
            "portkey" => Some(&PORTKEY_INFO),
            "sagemaker" => Some(&SAGEMAKER_INFO),
            "sambanova" => Some(&SAMBANOVA_INFO),
            "together" => Some(&TOGETHER_INFO),
            "vertexai" => Some(&VERTEXAI_INFO),
            "vertexaianthropic" => Some(&VERTEXAIANTHROPIC_INFO),
            "vllm" => Some(&VLLM_INFO),
            "voyage" => Some(&VOYAGE_INFO),
            "watsonx" => Some(&WATSONX_INFO),
            "xai" => Some(&XAI_INFO),
            "zai" => Some(&ZAI_INFO),
            _ => None,
        }
    }

    /// List all supported provider names
    pub fn list_names() -> Vec<&'static str> {
        vec![
            "openai",
            "anthropic",
            "mistral",
            "ollama",
            "gemini",
            "azure",
            "azureopenai",
            "azureanthropic",
            "bedrock",
            "cerebras",
            "cohere",
            "dashscope",
            "databricks",
            "deepinfra",
            "deepseek",
            "fireworks",
            "gateway",
            "groq",
            "huggingface",
            "inception",
            "llama",
            "llamacpp",
            "llamafile",
            "lmstudio",
            "minimax",
            "moonshot",
            "mzai",
            "nebius",
            "openrouter",
            "perplexity",
            "platform",
            "portkey",
            "sagemaker",
            "sambanova",
            "together",
            "vertexai",
            "vertexaianthropic",
            "vllm",
            "voyage",
            "watsonx",
            "xai",
            "zai",
        ]
    }
}

// Provider metadata constants
pub const OPENAI_INFO: ProviderInfo = ProviderInfo::new(
    "openai",
    "https://platform.openai.com/docs/api-reference",
    "OPENAI_API_KEY",
    Some("OPENAI_BASE_URL"),
    Some("https://api.openai.com/v1"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: true,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const ANTHROPIC_INFO: ProviderInfo = ProviderInfo::new(
    "anthropic",
    "https://docs.anthropic.com/en/api/reference",
    "ANTHROPIC_API_KEY",
    Some("ANTHROPIC_BASE_URL"),
    Some("https://api.anthropic.com"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const MISTRAL_INFO: ProviderInfo = ProviderInfo::new(
    "mistral",
    "https://docs.mistral.com/api/",
    "MISTRAL_API_KEY",
    Some("MISTRAL_BASE_URL"),
    Some("https://api.mistral.ai/v1"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const OLLAMA_INFO: ProviderInfo = ProviderInfo::new(
    "ollama",
    "https://github.com/ollama/ollama",
    "OLLAMA_API_KEY", // No standard env var for ollama, often uses no auth
    Some("OLLAMA_BASE_URL"),
    Some("http://localhost:11434"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const GEMINI_INFO: ProviderInfo = ProviderInfo::new(
    "gemini",
    "https://ai.google.dev/api/rest",
    "GOOGLE_API_KEY",
    Some("GOOGLE_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const DEEPINFRA_INFO: ProviderInfo = ProviderInfo::new(
    "deepinfra",
    "https://deepinfra.com/docs",
    "DEEPINFRA_API_KEY",
    Some("DEEPINFRA_BASE_URL"),
    Some("https://api.deepinfra.com/v1"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const AZURE_INFO: ProviderInfo = ProviderInfo::new(
    "azure",
    "https://learn.microsoft.com/en-us/azure/ai-services/openai/",
    "AZURE_OPENAI_KEY",
    Some("AZURE_OPENAI_ENDPOINT"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: true,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const AZUREOPENAI_INFO: ProviderInfo = ProviderInfo::new(
    "azureopenai",
    "https://learn.microsoft.com/en-us/azure/ai-services/openai/",
    "AZURE_OPENAI_KEY",
    Some("AZURE_OPENAI_ENDPOINT"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: true,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const AZUREANTHROPIC_INFO: ProviderInfo = ProviderInfo::new(
    "azureanthropic",
    "https://learn.microsoft.com/en-us/azure/ai-services/",
    "AZURE_ANTHROPIC_KEY",
    Some("AZURE_ANTHROPIC_ENDPOINT"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const BEDROCK_INFO: ProviderInfo = ProviderInfo::new(
    "bedrock",
    "https://docs.aws.amazon.com/bedrock/",
    "AWS_ACCESS_KEY_ID",
    Some("AWS_DEFAULT_REGION"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const CEREBRAS_INFO: ProviderInfo = ProviderInfo::new(
    "cerebras",
    "https://inference.cerebras.ai/",
    "CEREBRAS_API_KEY",
    Some("CEREBRAS_BASE_URL"),
    Some("https://inference.cerebras.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const COHERE_INFO: ProviderInfo = ProviderInfo::new(
    "cohere",
    "https://docs.cohere.com/",
    "COHERE_API_KEY",
    Some("COHERE_BASE_URL"),
    Some("https://api.cohere.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const DASHSCOPE_INFO: ProviderInfo = ProviderInfo::new(
    "dashscope",
    "https://help.aliyun.com/document_detail/2512448.html",
    "DASHSCOPE_API_KEY",
    Some("DASHSCOPE_BASE_URL"),
    Some("https://dashscope.aliyuncs.com"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const DATABRICKS_INFO: ProviderInfo = ProviderInfo::new(
    "databricks",
    "https://docs.databricks.com/en/generative-ai/index.html",
    "DATABRICKS_API_KEY",
    Some("DATABRICKS_HOST"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const DEEPSEEK_INFO: ProviderInfo = ProviderInfo::new(
    "deepseek",
    "https://platform.deepseek.com/",
    "DEEPSEEK_API_KEY",
    Some("DEEPSEEK_BASE_URL"),
    Some("https://api.deepseek.com"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const FIREWORKS_INFO: ProviderInfo = ProviderInfo::new(
    "fireworks",
    "https://docs.fireworks.ai/",
    "FIREWORKS_API_KEY",
    Some("FIREWORKS_BASE_URL"),
    Some("https://api.fireworks.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const GATEWAY_INFO: ProviderInfo = ProviderInfo::new(
    "gateway",
    "https://docs.gateway.dev/",
    "GATEWAY_API_KEY",
    Some("GATEWAY_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const GROQ_INFO: ProviderInfo = ProviderInfo::new(
    "groq",
    "https://console.groq.com/docs/",
    "GROQ_API_KEY",
    Some("GROQ_BASE_URL"),
    Some("https://api.groq.com"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const HUGGINGFACE_INFO: ProviderInfo = ProviderInfo::new(
    "huggingface",
    "https://huggingface.co/docs/huggingface",
    "HF_API_KEY",
    Some("HF_BASE_URL"),
    Some("https://api.endpoints.huggingface.cloud"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const INCEPTION_INFO: ProviderInfo = ProviderInfo::new(
    "inception",
    "https://docs.inception.ai/",
    "INCEPTION_API_KEY",
    Some("INCEPTION_BASE_URL"),
    Some("https://api.inception.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const LLAMA_INFO: ProviderInfo = ProviderInfo::new(
    "llama",
    "https://docs.llama.ai/",
    "LLAMA_API_KEY",
    Some("LLAMA_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const LLAMACPP_INFO: ProviderInfo = ProviderInfo::new(
    "llamacpp",
    "https://github.com/ggerganov/llama.cpp",
    "LLAMACPP_API_KEY",
    Some("LLAMACPP_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const LLAMAFILE_INFO: ProviderInfo = ProviderInfo::new(
    "llamafile",
    "https://github.com/Mozilla-Ocho/llamafile",
    "LLAMAFILE_API_KEY",
    Some("LLAMAFILE_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const LMSTUDIO_INFO: ProviderInfo = ProviderInfo::new(
    "lmstudio",
    "https://lmstudio.ai/docs",
    "LMSTUDIO_API_KEY",
    Some("LMSTUDIO_BASE_URL"),
    Some("http://localhost:1234"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const MINIMAX_INFO: ProviderInfo = ProviderInfo::new(
    "minimax",
    "https://www.minimaxi.com/docs",
    "MINIMAX_API_KEY",
    Some("MINIMAX_BASE_URL"),
    Some("https://api.minimax.chat"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const MOONSHOT_INFO: ProviderInfo = ProviderInfo::new(
    "moonshot",
    "https://docs.moonshot.cn/",
    "MOONSHOT_API_KEY",
    Some("MOONSHOT_BASE_URL"),
    Some("https://api.moonshot.cn"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const MZAI_INFO: ProviderInfo = ProviderInfo::new(
    "mzai",
    "https://mz.ai/docs",
    "MZAI_API_KEY",
    Some("MZAI_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const NEB_IUS_INFO: ProviderInfo = ProviderInfo::new(
    "nebius",
    "https://docs.nebius.ai/",
    "NEB_IUS_API_KEY",
    Some("NEB_IUS_BASE_URL"),
    Some("https://api.nebius.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const OPENROUTER_INFO: ProviderInfo = ProviderInfo::new(
    "openrouter",
    "https://openrouter.ai/docs",
    "OPENROUTER_API_KEY",
    Some("OPENROUTER_BASE_URL"),
    Some("https://openrouter.ai/api"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const PERPLEXITY_INFO: ProviderInfo = ProviderInfo::new(
    "perplexity",
    "https://docs.perplexity.ai/",
    "PERPLEXITY_API_KEY",
    Some("PERPLEXITY_BASE_URL"),
    Some("https://api.perplexity.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const PLATFORM_INFO: ProviderInfo = ProviderInfo::new(
    "platform",
    "https://platform.ai/docs",
    "PLATFORM_API_KEY",
    Some("PLATFORM_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: true,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const PORTKEY_INFO: ProviderInfo = ProviderInfo::new(
    "portkey",
    "https://docs.portkey.ai/",
    "PORTKEY_API_KEY",
    Some("PORTKEY_BASE_URL"),
    Some("https://api.portkey.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const SAGEMAKER_INFO: ProviderInfo = ProviderInfo::new(
    "sagemaker",
    "https://docs.aws.amazon.com/sagemaker/",
    "AWS_ACCESS_KEY_ID",
    Some("AWS_DEFAULT_REGION"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const SAMBANOVA_INFO: ProviderInfo = ProviderInfo::new(
    "sambanova",
    "https://docs.sambanova.ai/",
    "SAMBANOVA_API_KEY",
    Some("SAMBANOVA_BASE_URL"),
    Some("https://api.sambanova.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const TOGETHER_INFO: ProviderInfo = ProviderInfo::new(
    "together",
    "https://docs.together.ai/",
    "TOGETHER_API_KEY",
    Some("TOGETHER_BASE_URL"),
    Some("https://api.together.xyz"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: true,
        supports_messages: true,
    },
);

pub const VERTEXAI_INFO: ProviderInfo = ProviderInfo::new(
    "vertexai",
    "https://cloud.google.com/vertex-ai/docs",
    "GOOGLE_CLOUD_API_KEY",
    Some("VERTEXAI_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const VERTEXAIANTHROPIC_INFO: ProviderInfo = ProviderInfo::new(
    "vertexaianthropic",
    "https://cloud.google.com/vertex-ai/docs",
    "GOOGLE_CLOUD_API_KEY",
    Some("VERTEXAI_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const VLLM_INFO: ProviderInfo = ProviderInfo::new(
    "vllm",
    "https://docs.vllm.ai/",
    "VLLM_API_KEY",
    Some("VLLM_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const VOYAGE_INFO: ProviderInfo = ProviderInfo::new(
    "voyage",
    "https://docs.voyageai.com/",
    "VOYAGE_API_KEY",
    Some("VOYAGE_BASE_URL"),
    Some("https://api.voyageai.com"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const WATSONX_INFO: ProviderInfo = ProviderInfo::new(
    "watsonx",
    "https://cloud.ibm.com/docs/watsonx",
    "WATSONX_API_KEY",
    Some("WATSONX_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const XAI_INFO: ProviderInfo = ProviderInfo::new(
    "xai",
    "https://docs.x.ai/",
    "XAI_API_KEY",
    Some("XAI_BASE_URL"),
    Some("https://api.x.ai"),
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: false,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);

pub const ZAI_INFO: ProviderInfo = ProviderInfo::new(
    "zai",
    "https://z.ai/docs",
    "ZAI_API_KEY",
    Some("ZAI_BASE_URL"),
    None,
    ProviderFeatures {
        supports_completion: true,
        supports_completion_streaming: true,
        supports_embedding: true,
        supports_responses: false,
        supports_list_models: true,
        supports_batch: false,
        supports_messages: true,
    },
);
