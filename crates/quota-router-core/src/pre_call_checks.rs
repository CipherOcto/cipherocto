//! Pre-call checks for deployment filtering (RFC-0936).
//!
//! Checks deployments before routing to filter out invalid options:
//! - Context window limits
//! - Tag filtering
//! - Health checks

use async_trait::async_trait;

// ============================================================================
// Types
// ============================================================================

/// Result of a pre-call check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckResult {
    /// Deployment passes the check
    Pass,
    /// Deployment fails the check
    Fail { reason: String },
}

/// Completion request for pre-call checks
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub max_tokens: Option<usize>,
    pub tags: Vec<String>,
    pub model: String,
}

/// Message in a completion request
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Deployment info for pre-call checks
#[derive(Debug, Clone)]
pub struct DeploymentInfo {
    pub deployment_id: String,
    pub model: String,
    pub max_input_tokens: Option<usize>,
    pub max_output_tokens: Option<usize>,
    pub allowed_tags: Vec<String>,
    pub blocked_tags: Vec<String>,
    pub health_endpoint: Option<String>,
    pub is_healthy: bool,
}

// ============================================================================
// PreCallCheck Trait
// ============================================================================

/// Pre-call check trait (RFC-0936)
#[async_trait]
pub trait PreCallCheck: Send + Sync {
    /// Check if a deployment is valid for the given request
    async fn check(&self, deployment: &DeploymentInfo, request: &CompletionRequest) -> CheckResult;
}

// ============================================================================
// ContextWindowCheck
// ============================================================================

/// Checks if the deployment's context window can handle the request
#[derive(Default)]
pub struct ContextWindowCheck;

impl ContextWindowCheck {
    pub fn new() -> Self {
        Self
    }

    /// Estimate token count for text using tiktoken
    fn estimate_tokens(&self, text: &str, model: &str) -> usize {
        // Try to get tokenizer for model, fallback to cl100k_base
        let bpe = tiktoken_rs::get_bpe_from_model(model)
            .unwrap_or_else(|_| tiktoken_rs::cl100k_base().unwrap());
        bpe.encode_ordinary(text).len()
    }
}

#[async_trait]
impl PreCallCheck for ContextWindowCheck {
    async fn check(&self, deployment: &DeploymentInfo, request: &CompletionRequest) -> CheckResult {
        // Skip check if no model info available
        let max_input = match deployment.max_input_tokens {
            Some(tokens) => tokens,
            None => return CheckResult::Pass,
        };

        let max_output = deployment.max_output_tokens.unwrap_or(4096);

        // Estimate input tokens
        let input_tokens = {
            let model = &request.model;
            let mut total = 0;
            for msg in &request.messages {
                total += 4; // message overhead
                total += self.estimate_tokens(&msg.role, model);
                total += self.estimate_tokens(&msg.content, model);
            }
            total += 2; // reply priming
            total
        };

        // Check input tokens against max input
        if input_tokens > max_input {
            return CheckResult::Fail {
                reason: format!(
                    "Input tokens ({}) exceeds max input tokens ({})",
                    input_tokens, max_input
                ),
            };
        }

        // Check if requested output tokens fit
        let requested_output = request.max_tokens.unwrap_or(max_output);
        if requested_output > max_output {
            return CheckResult::Fail {
                reason: format!(
                    "Requested output tokens ({}) exceeds max output tokens ({})",
                    requested_output, max_output
                ),
            };
        }

        // Check total context window
        let total_tokens = input_tokens + requested_output;
        let context_window = max_input + max_output;
        if total_tokens > context_window {
            return CheckResult::Fail {
                reason: format!(
                    "Total tokens ({}) exceeds context window ({})",
                    total_tokens, context_window
                ),
            };
        }

        CheckResult::Pass
    }
}

// ============================================================================
// TagFilterCheck
// ============================================================================

/// Filters deployments based on allowed/blocked tags
pub struct TagFilterCheck;

impl TagFilterCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TagFilterCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PreCallCheck for TagFilterCheck {
    async fn check(&self, deployment: &DeploymentInfo, request: &CompletionRequest) -> CheckResult {
        // If request has no tags, pass all deployments
        if request.tags.is_empty() {
            return CheckResult::Pass;
        }

        // Check blocked tags first
        for tag in &request.tags {
            if deployment.blocked_tags.contains(tag) {
                return CheckResult::Fail {
                    reason: format!("Tag '{}' is blocked for this deployment", tag),
                };
            }
        }

        // If allowed_tags is set, request must have at least one allowed tag
        if !deployment.allowed_tags.is_empty() {
            let has_allowed_tag = request
                .tags
                .iter()
                .any(|tag| deployment.allowed_tags.contains(tag));

            if !has_allowed_tag {
                return CheckResult::Fail {
                    reason: format!(
                        "Request tags {:?} don't match allowed tags {:?}",
                        request.tags, deployment.allowed_tags
                    ),
                };
            }
        }

        CheckResult::Pass
    }
}

// ============================================================================
// HealthCheck
// ============================================================================

/// Checks deployment health via HTTP endpoint
pub struct HealthCheck {
    client: reqwest::Client,
}

impl HealthCheck {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PreCallCheck for HealthCheck {
    async fn check(
        &self,
        deployment: &DeploymentInfo,
        _request: &CompletionRequest,
    ) -> CheckResult {
        // If no health endpoint, use cached health state
        let endpoint = match &deployment.health_endpoint {
            Some(ep) => ep,
            None => {
                return if deployment.is_healthy {
                    CheckResult::Pass
                } else {
                    CheckResult::Fail {
                        reason: "Deployment marked unhealthy".to_string(),
                    }
                };
            }
        };

        // Check health endpoint
        match self.client.get(endpoint).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    CheckResult::Pass
                } else {
                    CheckResult::Fail {
                        reason: format!("Health check failed: HTTP {}", resp.status()),
                    }
                }
            }
            Err(e) => CheckResult::Fail {
                reason: format!("Health check error: {}", e),
            },
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request(
        messages: Vec<Message>,
        max_tokens: Option<usize>,
        tags: Vec<String>,
    ) -> CompletionRequest {
        CompletionRequest {
            messages,
            max_tokens,
            tags,
            model: "gpt-4".to_string(),
        }
    }

    fn test_deployment(
        max_input: Option<usize>,
        max_output: Option<usize>,
        allowed: Vec<String>,
        blocked: Vec<String>,
    ) -> DeploymentInfo {
        DeploymentInfo {
            deployment_id: "test-deploy".to_string(),
            model: "gpt-4".to_string(),
            max_input_tokens: max_input,
            max_output_tokens: max_output,
            allowed_tags: allowed,
            blocked_tags: blocked,
            health_endpoint: None,
            is_healthy: true,
        }
    }

    #[tokio::test]
    async fn test_context_window_pass() {
        let check = ContextWindowCheck::new();
        let deployment = test_deployment(Some(8192), Some(4096), vec![], vec![]);
        let request = test_request(
            vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            Some(100),
            vec![],
        );

        assert_eq!(check.check(&deployment, &request).await, CheckResult::Pass);
    }

    #[tokio::test]
    async fn test_context_window_fail_input() {
        let check = ContextWindowCheck::new();
        let deployment = test_deployment(Some(10), Some(4096), vec![], vec![]);
        let request = test_request(
            vec![Message {
                role: "user".to_string(),
                content: "Hello, this is a longer message for testing".to_string(),
            }],
            Some(100),
            vec![],
        );

        match check.check(&deployment, &request).await {
            CheckResult::Fail { reason } => assert!(reason.contains("Input tokens")),
            _ => panic!("Expected Fail"),
        }
    }

    #[tokio::test]
    async fn test_context_window_no_model_info() {
        let check = ContextWindowCheck::new();
        let deployment = test_deployment(None, None, vec![], vec![]);
        let request = test_request(vec![], Some(100), vec![]);

        assert_eq!(check.check(&deployment, &request).await, CheckResult::Pass);
    }

    #[tokio::test]
    async fn test_tag_filter_pass_no_tags() {
        let check = TagFilterCheck::new();
        let deployment = test_deployment(None, None, vec![], vec![]);
        let request = test_request(vec![], None, vec![]);

        assert_eq!(check.check(&deployment, &request).await, CheckResult::Pass);
    }

    #[tokio::test]
    async fn test_tag_filter_pass_allowed() {
        let check = TagFilterCheck::new();
        let deployment = test_deployment(
            None,
            None,
            vec!["gpu".to_string(), "fast".to_string()],
            vec![],
        );
        let request = test_request(vec![], None, vec!["gpu".to_string()]);

        assert_eq!(check.check(&deployment, &request).await, CheckResult::Pass);
    }

    #[tokio::test]
    async fn test_tag_filter_fail_blocked() {
        let check = TagFilterCheck::new();
        let deployment = test_deployment(None, None, vec![], vec!["slow".to_string()]);
        let request = test_request(vec![], None, vec!["slow".to_string()]);

        match check.check(&deployment, &request).await {
            CheckResult::Fail { reason } => assert!(reason.contains("blocked")),
            _ => panic!("Expected Fail"),
        }
    }

    #[tokio::test]
    async fn test_tag_filter_fail_no_allowed_match() {
        let check = TagFilterCheck::new();
        let deployment = test_deployment(None, None, vec!["gpu".to_string()], vec![]);
        let request = test_request(vec![], None, vec!["cpu".to_string()]);

        match check.check(&deployment, &request).await {
            CheckResult::Fail { reason } => assert!(reason.contains("don't match")),
            _ => panic!("Expected Fail"),
        }
    }

    #[tokio::test]
    async fn test_health_check_healthy() {
        let check = HealthCheck::new();
        let deployment = DeploymentInfo {
            deployment_id: "test".to_string(),
            model: "gpt-4".to_string(),
            max_input_tokens: None,
            max_output_tokens: None,
            allowed_tags: vec![],
            blocked_tags: vec![],
            health_endpoint: None,
            is_healthy: true,
        };
        let request = test_request(vec![], None, vec![]);

        assert_eq!(check.check(&deployment, &request).await, CheckResult::Pass);
    }

    #[tokio::test]
    async fn test_health_check_unhealthy() {
        let check = HealthCheck::new();
        let deployment = DeploymentInfo {
            deployment_id: "test".to_string(),
            model: "gpt-4".to_string(),
            max_input_tokens: None,
            max_output_tokens: None,
            allowed_tags: vec![],
            blocked_tags: vec![],
            health_endpoint: None,
            is_healthy: false,
        };
        let request = test_request(vec![], None, vec![]);

        match check.check(&deployment, &request).await {
            CheckResult::Fail { reason } => assert!(reason.contains("unhealthy")),
            _ => panic!("Expected Fail"),
        }
    }
}
