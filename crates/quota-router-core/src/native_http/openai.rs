// openai — OpenAI via reqwest (native_http, LiteLLM mode)

use super::{
    HttpBatchCreateRequest, HttpBatchListResponse, HttpBatchObject, HttpBatchResultsResponse,
    HttpCompletionRequest, HttpCompletionResponse, HttpDeletedObject, HttpEmbeddingRequest,
    HttpEmbeddingResponse, HttpListModelsResponse, HttpResponseObject, HttpResponsesRequest,
    ProviderError, StreamingResponse,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

pub struct OpenAIProvider {
    client: Client,
    api_base: String,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }
}

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::HttpProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn supported_models(&self) -> Vec<&str> {
        vec![
            "gpt-4",
            "gpt-4-turbo",
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-3.5-turbo",
            "gpt-4-0613",
            "gpt-4-32k",
        ]
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<HttpCompletionResponse, ProviderError> {
        // Use api_base from request if provided, otherwise fall back to provider's default
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/chat/completions", base_url);

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>()
        });

        if let Some(stream) = request.stream {
            body["stream"] = serde_json::json!(stream);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &request.stop {
            body["stop"] = serde_json::json!(stop);
        }
        if let Some(n) = request.n {
            body["n"] = serde_json::json!(n);
        }
        if let Some(p) = request.presence_penalty {
            body["presence_penalty"] = serde_json::json!(p);
        }
        if let Some(p) = request.frequency_penalty {
            body["frequency_penalty"] = serde_json::json!(p);
        }
        if let Some(user) = &request.user {
            body["user"] = serde_json::json!(user);
        }

        // Function calling fields (RFC-0939)
        if let Some(tools) = &request.tools {
            body["tools"] = serde_json::to_value(tools).unwrap_or_default();
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
        }
        if let Some(response_format) = &request.response_format {
            body["response_format"] = serde_json::to_value(response_format).unwrap_or_default();
        }
        if let Some(seed) = request.seed {
            body["seed"] = serde_json::json!(seed);
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = request.timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }

        let status = resp.status();
        let data: OpenAIResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(convert_response(data, status.as_u16()))
    }

    async fn embedding(
        &self,
        request: &HttpEmbeddingRequest,
        api_key: Option<&str>,
    ) -> Result<HttpEmbeddingResponse, ProviderError> {
        // Use api_base from request if provided, otherwise fall back to provider's default
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/embeddings", base_url);

        let body = serde_json::json!({
            "input": request.input,
            "model": request.model
        });

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = request.timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }

        let data: OpenAIEmbeddingsResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        Ok(HttpEmbeddingResponse {
            object: "list".to_string(),
            data: data
                .data
                .into_iter()
                .map(|e| crate::shared_types::Embedding {
                    object: e.object,
                    embedding: e.embedding,
                    index: e.index,
                })
                .collect(),
            model: data.model,
            usage: crate::shared_types::Usage::new(
                data.usage.prompt_tokens,
                0,
                data.usage.total_tokens,
            ),
        })
    }

    fn routing_weight(&self) -> u32 {
        10 // Higher weight for OpenAI as primary provider
    }

    async fn streaming_completion(
        &self,
        request: &HttpCompletionRequest,
        api_key: Option<&str>,
    ) -> Result<StreamingResponse, ProviderError> {
        // Use api_base from request if provided, otherwise fall back to provider's default
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/chat/completions", base_url);

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content
                })
            }).collect::<Vec<_>>(),
            "stream": true
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &request.stop {
            body["stop"] = serde_json::json!(stop);
        }

        // Function calling fields (RFC-0939)
        if let Some(tools) = &request.tools {
            body["tools"] = serde_json::to_value(tools).unwrap_or_default();
        }
        if let Some(tool_choice) = &request.tool_choice {
            body["tool_choice"] = serde_json::to_value(tool_choice).unwrap_or_default();
        }
        if let Some(response_format) = &request.response_format {
            body["response_format"] = serde_json::to_value(response_format).unwrap_or_default();
        }
        if let Some(seed) = request.seed {
            body["seed"] = serde_json::json!(seed);
        }

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }

        // For OpenAI, we pass raw SSE bytes through
        // The proxy will forward SSE bytes directly to the client
        let (tx, rx) = mpsc::channel(100);

        // Spawn task to read SSE bytes and forward them
        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        // Send raw SSE bytes to proxy for direct forwarding
                        if tx
                            .send(Ok(super::StreamingChunk::RawSSE(bytes.to_vec())))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(ProviderError::Network(e.to_string()))).await;
                        break;
                    }
                }
            }
        });

        Ok(StreamingResponse {
            receiver: rx,
            content_type: "text/event-stream",
        })
    }

    async fn get_response(
        &self,
        response_id: &str,
        api_key: Option<&str>,
        api_base: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<HttpResponseObject, ProviderError> {
        let base_url = api_base.unwrap_or(&self.api_base);
        let url = format!("{}/responses/{}", base_url, response_id);

        let mut req_builder = self
            .client
            .get(&url)
            .header("Content-Type", "application/json");
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }

        resp.json::<HttpResponseObject>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn delete_response(
        &self,
        response_id: &str,
        api_key: Option<&str>,
        api_base: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<HttpDeletedObject, ProviderError> {
        let base_url = api_base.unwrap_or(&self.api_base);
        let url = format!("{}/responses/{}", base_url, response_id);

        let mut req_builder = self
            .client
            .delete(&url)
            .header("Content-Type", "application/json");
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }

        resp.json::<HttpDeletedObject>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn list_models(
        &self,
        api_key: Option<&str>,
        api_base: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<HttpListModelsResponse, ProviderError> {
        let base_url = api_base.unwrap_or(&self.api_base);
        let url = format!("{}/models", base_url);

        let mut req_builder = self
            .client
            .get(&url)
            .header("Content-Type", "application/json");
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }

        resp.json::<HttpListModelsResponse>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn create_response(
        &self,
        request: &HttpResponsesRequest,
        api_key: Option<&str>,
    ) -> Result<HttpResponseObject, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/responses", base_url);

        let body = serde_json::to_value(request)
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = request.timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            if status == 401 || status == 403 {
                return Err(ProviderError::AuthError(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            if status == 429 {
                return Err(ProviderError::RateLimit(format!(
                    "HTTP {}: {}",
                    status, err_body
                )));
            }
            return Err(ProviderError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, err_body
            )));
        }

        resp.json::<HttpResponseObject>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn batch_create(
        &self,
        request: &HttpBatchCreateRequest,
        api_key: Option<&str>,
    ) -> Result<HttpBatchObject, ProviderError> {
        let base_url = request.api_base.as_deref().unwrap_or(&self.api_base);
        let url = format!("{}/batches", base_url);

        let body = serde_json::to_value(request)
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = request.timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(self::status_error(status, &err_body));
        }

        resp.json::<HttpBatchObject>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn batch_retrieve(
        &self,
        batch_id: &str,
        api_key: Option<&str>,
        api_base: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<HttpBatchObject, ProviderError> {
        let base_url = api_base.unwrap_or(&self.api_base);
        let url = format!("{}/batches/{}", base_url, batch_id);

        let mut req_builder = self.client.get(&url);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(self::status_error(status, &err_body));
        }

        resp.json::<HttpBatchObject>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn batch_cancel(
        &self,
        batch_id: &str,
        api_key: Option<&str>,
        api_base: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<HttpBatchObject, ProviderError> {
        let base_url = api_base.unwrap_or(&self.api_base);
        let url = format!("{}/batches/{}/cancel", base_url, batch_id);

        let mut req_builder = self.client.post(&url);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(self::status_error(status, &err_body));
        }

        resp.json::<HttpBatchObject>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn batch_list(
        &self,
        api_key: Option<&str>,
        api_base: Option<&str>,
        limit: Option<u32>,
        timeout: Option<f64>,
    ) -> Result<HttpBatchListResponse, ProviderError> {
        let base_url = api_base.unwrap_or(&self.api_base);
        let mut url = format!("{}/batches", base_url);
        if let Some(l) = limit {
            url = format!("{}?limit={}", url, l);
        }

        let mut req_builder = self.client.get(&url);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(self::status_error(status, &err_body));
        }

        resp.json::<HttpBatchListResponse>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    async fn batch_results(
        &self,
        batch_id: &str,
        api_key: Option<&str>,
        api_base: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<HttpBatchResultsResponse, ProviderError> {
        let base_url = api_base.unwrap_or(&self.api_base);
        let url = format!("{}/batches/{}/results", base_url, batch_id);

        let mut req_builder = self.client.get(&url);
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }
        if let Some(t) = timeout {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }
        let resp = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(self::status_error(status, &err_body));
        }

        // Results are returned as JSONL, parse each line
        let text = resp
            .text()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        let results: Vec<serde_json::Value> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(HttpBatchResultsResponse { results })
    }
}

fn status_error(status: reqwest::StatusCode, body: &str) -> ProviderError {
    if status == 401 || status == 403 {
        ProviderError::AuthError(format!("HTTP {}: {}", status, body))
    } else if status == 429 {
        ProviderError::RateLimit(format!("HTTP {}: {}", status, body))
    } else {
        ProviderError::InvalidResponse(format!("HTTP {}: {}", status, body))
    }
}

#[derive(Deserialize)]
struct OpenAIResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: OpenAIUsage,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    index: u32,
    message: OpenAIMessage,
    finish_reason: String,
}

#[derive(Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct OpenAIEmbeddingsResponse {
    object: String,
    data: Vec<OpenAIEmbedding>,
    model: String,
    usage: OpenAIUsage,
}

#[derive(Deserialize)]
struct OpenAIEmbedding {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}

fn convert_response(data: OpenAIResponse, _status: u16) -> HttpCompletionResponse {
    let choices = data
        .choices
        .into_iter()
        .map(|c| {
            crate::shared_types::Choice::new(
                c.index,
                crate::shared_types::Message::new(c.message.role, c.message.content),
                c.finish_reason,
            )
        })
        .collect();

    HttpCompletionResponse {
        id: data.id,
        object: data.object,
        created: data.created,
        model: data.model,
        choices,
        usage: crate::shared_types::Usage::new(
            data.usage.prompt_tokens,
            data.usage.completion_tokens,
            data.usage.total_tokens,
        ),
        metadata: None,
    }
}
