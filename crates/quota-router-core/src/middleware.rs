// Key validation middleware - validates API keys from HTTP requests

use crate::keys::{validate_key, ApiKey, KeyError};
use crate::KeyStorage;
use http;
use std::sync::Arc;

/// Middleware state containing key storage
pub struct KeyMiddleware<S: KeyStorage> {
    storage: Arc<S>,
}

impl<S: KeyStorage> KeyMiddleware<S> {
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    /// Extract API key from request
    /// Supports: Authorization header (Bearer token), X-API-Key header
    pub fn extract_key_from_request<B>(&self, request: &http::Request<B>) -> Result<Option<String>, KeyError> {
        // Check Authorization header
        if let Some(auth) = request.headers().get("authorization") {
            if let Ok(auth_str) = auth.to_str() {
                if auth_str.starts_with("Bearer ") {
                    return Ok(Some(auth_str[7..].to_string()));
                }
            }
        }

        // Check X-API-Key header
        if let Some(api_key) = request.headers().get("x-api-key") {
            return Ok(Some(api_key.to_str().unwrap_or("").to_string()));
        }

        Ok(None)
    }

    /// Validate key and return ApiKey if valid
    pub fn validate_request_key(&self, key_string: &str) -> Result<ApiKey, KeyError> {
        use crate::keys::compute_key_hash;

        let key_hash = compute_key_hash(key_string);
        let key_prefix = key_string.chars().take(7).collect::<String>();

        let mut key = self.storage.lookup_by_hash(&key_hash)?
            .ok_or(KeyError::NotFound)?;

        // Set the key_prefix from the request
        key.key_prefix = key_prefix;

        // Validate expiry and revoked status
        validate_key(&key)?;

        Ok(key)
    }

    /// Extract and validate key from request in one step
    pub fn extract_and_validate<B>(&self, request: &http::Request<B>) -> Result<ApiKey, KeyError> {
        let key_string = self.extract_key_from_request(request)?
            .ok_or(KeyError::MissingKey)?;

        self.validate_request_key(&key_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyType;

    fn create_test_middleware() -> KeyMiddleware<crate::storage::StoolapKeyStorage> {
        let db = stoolap::Database::open_in_memory().unwrap();
        crate::schema::init_database(&db).unwrap();
        let storage = crate::storage::StoolapKeyStorage::new(db);
        KeyMiddleware::new(Arc::new(storage))
    }

    #[test]
    fn test_extract_key_from_bearer_header() {
        let middleware = create_test_middleware();

        let req = http::Request::builder()
            .header("authorization", "Bearer sk-qr-test123")
            .body(())
            .unwrap();

        let key = middleware.extract_key_from_request(&req).unwrap();
        assert!(key.is_some());
        assert_eq!(key.unwrap(), "sk-qr-test123");
    }

    #[test]
    fn test_extract_key_from_api_key_header() {
        let middleware = create_test_middleware();

        let req = http::Request::builder()
            .header("x-api-key", "sk-qr-test456")
            .body(())
            .unwrap();

        let key = middleware.extract_key_from_request(&req).unwrap();
        assert!(key.is_some());
        assert_eq!(key.unwrap(), "sk-qr-test456");
    }

    #[test]
    fn test_extract_key_no_header() {
        let middleware = create_test_middleware();

        let req = http::Request::builder()
            .body(())
            .unwrap();

        let key = middleware.extract_key_from_request(&req).unwrap();
        assert!(key.is_none());
    }

    #[test]
    fn test_extract_key_bearer_takes_precedence() {
        let middleware = create_test_middleware();

        let req = http::Request::builder()
            .header("authorization", "Bearer from-bearer")
            .header("x-api-key", "from-header")
            .body(())
            .unwrap();

        let key = middleware.extract_key_from_request(&req).unwrap();
        assert_eq!(key.unwrap(), "from-bearer");
    }

    #[test]
    fn test_validate_request_key_not_found() {
        let middleware = create_test_middleware();

        let result = middleware.validate_request_key("sk-qr-nonexistentkey12345678901234567890123456789012345678901234");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), KeyError::NotFound));
    }

    #[test]
    fn test_validate_request_key_expired() {
        let middleware = create_test_middleware();

        // Create an expired key directly in storage
        let storage = middleware.storage.clone();
        let key = ApiKey {
            key_id: "expired-key".to_string(),
            key_hash: vec![1, 2, 3],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 100,
            expires_at: Some(1), // Expired in past
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };
        storage.create_key(&key).unwrap();

        // Try to validate - should fail
        let result = middleware.validate_request_key("sk-qr-expired");
        assert!(result.is_err());
    }
}
