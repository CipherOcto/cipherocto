// ollama — Ollama via reqwest (native_http, LiteLLM mode)

use super::{HttpCompletionRequest, HttpCompletionResponse, HttpEmbeddingRequest, HttpEmbeddingResponse, ProviderError};
use async_trait::async_trait;
use reqwest::Client;

pub struct OllamaProvider {
    client: Client,
    api_base: String,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
        }
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "llama3", "llama3.1", "llama3.2",
            "mistral", "mixtral",
            "phi3", "qwen2", "codellama",
        ]
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        _api_key: &str,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        let url = format!("{}/api/chat", self.api_base);

        let messages: Vec<_> = request.messages.iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": false
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["options"]["num_predict"] = serde_json::json!(max_tokens);
        }

        let resp = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let data: OllamaResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpCompletionResponse {
            id: format!("ollama-{}", uuid::Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            model: request.model.clone(),
            choices: vec![crate::shared_types::Choice::new(
                0,
                crate::shared_types::Message::new("assistant", data.message.content),
                "stop".to_string(),
            )],
            usage: crate::shared_types::Usage::new(0, 0, 0),
        })
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        _api_key: &str,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        let url = format!("{}/api/embeddings", self.api_base);

        let body = serde_json::json!({
            "model": request.model,
            "prompt": request.input
        });

        let resp = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProviderError::InvalidResponse(format!("HTTP {}", resp.status())));
        }

        let data: OllamaEmbeddingsResponse = resp.json().await.map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpEmbeddingResponse {
            object: "list".to_string(),
            data: vec![crate::shared_types::Embedding {
                object: "embedding".to_string(),
                embedding: data.embedding,
                index: 0,
            }],
            model: request.model.clone(),
            usage: crate::shared_types::Usage::new(0, 0, 0),
        })
    }

    fn routing_weight(&self) -> u32 {
        3 // Lower priority for local ollama
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct OllamaResponse {
    model: String,
    message: OllamaMessage,
    #[allow(dead_code)]
    done: bool,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct OllamaEmbeddingsResponse {
    embedding: Vec<f32>,
}
