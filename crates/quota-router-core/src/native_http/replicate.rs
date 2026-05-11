// replicate — Replicate via reqwest (native_http, LiteLLM mode)

use super::{HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse, ProviderError};
use async_trait::async_trait;
use reqwest::Client;

pub struct ReplicateProvider {
    client: Client,
    api_base: String,
}

impl ReplicateProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.replicate.com/v1".to_string(),
        }
    }
}

impl Default for ReplicateProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for ReplicateProvider {
    fn name(&self) -> &str {
        "replicate"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "meta/llama-3-70b-instruct", "meta/llama-3-8b-instruct",
            "mistralai/mixtral-8x22b", "mistralai/pixtral-12b",
            "deepseek-ai/deepseek-v3",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        // Replicate uses a predictions API - first create a prediction, then poll
        let create_url = format!("{}/predictions", self.api_base);

        let last_msg = request.messages.last()
            .ok_or_else(|| ProviderError::InvalidResponse("No messages provided".to_string()))?;

        let create_body = serde_json::json!({
            "version": request.model,
            "input": {
                "prompt": last_msg.content,
                "max_tokens": request.max_tokens.unwrap_or(1024),
            }
        });

        let create_resp = self.client
            .post(&create_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&create_body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !create_resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", create_resp.status())));
        }

        let prediction: ReplicatePrediction = create_resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        // Poll for completion
        let output = loop {
            let status_url = prediction.urls.status.as_ref().or(prediction.urls.cancel.as_ref());
            let poll_url = status_url.cloned().unwrap_or_else(|| prediction.urls.get.as_deref().unwrap_or("").to_string());

            let poll_resp = self.client
                .get(poll_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;

            let status: ReplicateStatus = poll_resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

            match status.status.as_str() {
                "succeeded" => break status.output,
                "failed" => return Err(ProviderError::InvalidResponse("Prediction failed".to_string())),
                "canceled" => return Err(ProviderError::InvalidResponse("Prediction canceled".to_string())),
                _ => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        };

        let output_text = output.as_str().unwrap_or("").to_string();

        Ok(HttpCompletionResponse {
            id: format!("replicate-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("assistant", output_text),
                "stop".to_string(),
            )],
            usage: crate::shared_types::Usage::new(0, 0, 0),
        })
    }

    async fn embedding(
        &self,
        _request: &HttpEmbeddingRequest,
        _api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        Err(ProviderError::UnsupportedModel("Replicate does not support embeddings".to_string()))
    }

    fn routing_weight(&self) -> u32 {
        3
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ReplicatePrediction {
    id: String,
    urls: ReplicateUrls,
}

#[derive(serde::Deserialize)]
struct ReplicateUrls {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    cancel: Option<String>,
    #[serde(default)]
    get: Option<String>,
}

#[derive(serde::Deserialize)]
struct ReplicateStatus {
    status: String,
    output: serde_json::Value,
}
