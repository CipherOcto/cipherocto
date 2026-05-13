// gemini — Google Gemini via reqwest (native_http, LiteLLM mode)

use super::{
    HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse,
    ProviderError,
};
use async_trait::async_trait;
use reqwest::Client;

pub struct GeminiProvider {
    client: Client,
    api_base: String,
}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-1.5-pro",
            "gemini-1.5-flash",
            "gemini-1.5-flash-8b",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        // Gemini uses generate_content endpoint, not chat completions
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.api_base, request.model, api_key
        );

        // Build contents for Gemini - combine messages into a single text prompt
        let prompt = request
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let body = serde_json::json!({
            "contents": [{
                "parts": [{ "text": prompt }],
                "role": "user"
            }],
            "generationConfig": {
                "temperature": request.temperature.unwrap_or(0.9),
                "maxOutputTokens": request.max_tokens.unwrap_or(2048),
                "topP": request.top_p.unwrap_or(0.95),
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        let data: GeminiResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let text = data
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.as_ref())
            .unwrap_or(&String::new())
            .clone();

        Ok(HttpCompletionResponse {
            id: format!("gemini-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("model", text),
                data.candidates
                    .first()
                    .and_then(|c| c.finish_reason.as_ref())
                    .unwrap_or(&"stop".to_string())
                    .clone(),
            )],
            usage: crate::shared_types::Usage::new(
                data.usage_metadata.prompt_token_count.unwrap_or(0),
                data.usage_metadata.candidates_token_count.unwrap_or(0),
                data.usage_metadata.total_token_count.unwrap_or(0),
            ),
        })
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let url = format!(
            "{}/models/{}:embedContent?key={}",
            self.api_base, request.model, api_key
        );

        let body = serde_json::json!({
            "content": { "parts": [{ "text": request.input }] }
        });

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}",
                resp.status()
            )));
        }

        let data: GeminiEmbeddingsResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpEmbeddingResponse {
            object: "list".to_string(),
            data: vec![crate::shared_types::Embedding {
                object: "embedding".to_string(),
                embedding: data.embedding.values,
                index: 0,
            }],
            model: request.model.clone(),
            usage: crate::shared_types::Usage::new(0, 0, 0),
        })
    }

    fn routing_weight(&self) -> u32 {
        6
    }
}

#[derive(serde::Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: GeminiUsage,
}

#[derive(serde::Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(serde::Deserialize)]
struct GeminiPart {
    #[serde(default)]
    text: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct GeminiUsage {
    #[serde(default)]
    prompt_token_count: Option<u32>,
    #[serde(default)]
    candidates_token_count: Option<u32>,
    #[serde(default)]
    total_token_count: Option<u32>,
}

#[derive(serde::Deserialize)]
struct GeminiEmbeddingsResponse {
    embedding: GeminiEmbeddingValues,
}

#[derive(serde::Deserialize)]
struct GeminiEmbeddingValues {
    values: Vec<f32>,
}
