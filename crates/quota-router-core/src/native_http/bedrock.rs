// bedrock — AWS Bedrock via reqwest (native_http, LiteLLM mode)

use super::{HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse, ProviderError};
use async_trait::async_trait;
use reqwest::Client;

pub struct BedrockProvider {
    client: Client,
    region: String,
}

impl BedrockProvider {
    pub fn new() -> Self {
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        Self {
            client: Client::new(),
            region,
        }
    }

    pub fn with_region(mut self, region: String) -> Self {
        self.region = region;
        self
    }
}

impl Default for BedrockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for BedrockProvider {
    fn name(&self) -> &str {
        "bedrock"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "anthropic.claude-3-5-sonnet-latest",
            "anthropic.claude-3-opus-latest",
            "anthropic.claude-3-sonnet-latest",
            "anthropic.claude-3-haiku-latest",
            "meta.llama3-1-70b-instruct",
            "meta.llama3-1-8b-instruct",
            "mistral.mistral-large-2407",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let url = format!(
            "https://bedrock.{}.amazonaws.com/model/{}",
            self.region, request.model
        );

        // Build request body for Bedrock (varies by provider)
        let body = serde_json::json!({
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>(),
            "anthropic_version": "bedrock-2023-05-31"
        });

        let resp = self.client
            .post(&url)
            .header("x-amz-client-id", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let data: BedrockResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: format!("bedrock-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("assistant", data.content.first().and_then(|c| c.text.as_ref()).unwrap_or(&String::new()).clone()),
                data.stop_reason.unwrap_or_else(|| "stop".to_string()),
            )],
            usage: crate::shared_types::Usage::new(data.usage.input_tokens, data.usage.output_tokens, data.usage.input_tokens + data.usage.output_tokens),
        })
    }

    async fn embedding(
        &self,
        _request: &HttpEmbeddingRequest,
        _api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel("Bedrock embeddings not implemented".to_string()))
    }

    fn routing_weight(&self) -> u32 {
        4
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct BedrockResponse {
    id: String,
    #[allow(dead_code)]
    type_: String,
    content: Vec<BedrockContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
    #[allow(dead_code)]
    stop_sequence: Option<String>,
    usage: BedrockUsage,
}

#[derive(serde::Deserialize)]
struct BedrockContentBlock {
    #[allow(dead_code)]
    r#type: String,
    text: Option<String>,
}

#[derive(serde::Deserialize)]
struct BedrockUsage {
    #[allow(dead_code)]
    input_tokens: u32,
    #[allow(dead_code)]
    output_tokens: u32,
}
