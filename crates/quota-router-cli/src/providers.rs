use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub endpoint: String,
}

impl Provider {
    pub fn new(name: &str, endpoint: &str) -> Self {
        Self {
            name: name.to_string(),
            endpoint: endpoint.to_string(),
        }
    }

    /// Get API key from environment variable
    /// Format: {PROVIDER_NAME}_API_KEY (uppercase)
    pub fn get_api_key(&self) -> Option<String> {
        let env_var = format!("{}_API_KEY", self.name.to_uppercase());
        env::var(env_var).ok()
    }
}

/// Known provider endpoints
pub fn default_endpoint(name: &str) -> Option<String> {
    match name.to_lowercase().as_str() {
        "openai" => Some("https://api.openai.com/v1".to_string()),
        "anthropic" => Some("https://api.anthropic.com".to_string()),
        "google" => Some("https://generativelanguage.googleapis.com".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_api_key_env_var() {
        std::env::set_var("OPENAI_API_KEY", "test-key-123");
        let provider = Provider::new("openai", "https://api.openai.com/v1");
        let key = provider.get_api_key();
        assert_eq!(key, Some("test-key-123".to_string()));
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_get_api_key_missing() {
        std::env::remove_var("ANTHROPIC_API_KEY");
        let provider = Provider::new("anthropic", "https://api.anthropic.com");
        let key = provider.get_api_key();
        assert_eq!(key, None);
    }

    #[test]
    fn test_default_endpoint_openai() {
        let endpoint = default_endpoint("openai");
        assert_eq!(endpoint, Some("https://api.openai.com/v1".to_string()));
    }

    #[test]
    fn test_default_endpoint_anthropic() {
        let endpoint = default_endpoint("anthropic");
        assert_eq!(endpoint, Some("https://api.anthropic.com".to_string()));
    }

    #[test]
    fn test_default_endpoint_unknown() {
        let endpoint = default_endpoint("unknown");
        assert_eq!(endpoint, None);
    }
}
