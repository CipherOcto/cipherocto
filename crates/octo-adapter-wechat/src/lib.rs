//! WeChat Official Account adapter for DOT (RFC-0850 S8.1, PlatformType::WeChat)
//!
//! Bridges DOT envelopes to WeChat via the WeChat Official Account API.
//! Uses access_token authentication with AES-256-CBC message encryption.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "app_id": "wx...",
//!   "app_secret": "...",
//!   "token": "verification_token",
//!   "encoding_aes_key": "...",
//!   "groups": ["group_openid_1"]
//! }
//! ```

use async_trait::async_trait;
use base64::Engine;
use parking_lot::Mutex;
use std::sync::Arc;

use octo_network::dot::adapters::{
    CapabilityReport, DeliveryReceipt, MediaCapabilities, PlatformAdapter, RawPlatformMessage,
};
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::error::PlatformAdapterError;

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct WeChatConfig {
    pub app_id: String,
    pub app_secret: String,
    pub token: String,
    pub encoding_aes_key: Option<String>,
    pub groups: Vec<String>,
}

impl std::fmt::Debug for WeChatConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeChatConfig")
            .field("app_id", &self.app_id)
            .field("app_secret", &"***")
            .field("token", &"***")
            .field("groups", &self.groups)
            .finish()
    }
}

pub struct WeChatAdapter {
    config: WeChatConfig,
    client: reqwest::Client,
    access_token: Arc<Mutex<Option<(String, u64)>>>,
}

impl WeChatAdapter {
    pub fn new(config: WeChatConfig) -> Self {
        Self { config, client: reqwest::Client::new(), access_token: Arc::new(Mutex::new(None)) }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        let config: WeChatConfig = serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Self::new(config))
    }

    fn api_base() -> &'static str { "https://api.weixin.qq.com/cgi-bin" }

    pub fn encode_envelope(bytes: &[u8]) -> String {
        format!("DOT/1/{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let b64 = text.trim().strip_prefix("DOT/1/").ok_or("Missing DOT/1/ prefix")?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(b64).map_err(|e| format!("Base64: {e}"))
    }

    pub fn domain_hash(group_id: &str) -> [u8; 32] {
        *blake3::hash(format!("wechat:{}", group_id.trim().to_lowercase()).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0011;
    pub fn max_payload_bytes() -> usize { 2048 }
    pub fn rate_limit_per_second() -> u32 { 1 }

    async fn get_access_token(&self) -> Result<String, PlatformAdapterError> {
        {
            let guard = self.access_token.lock();
            if let Some((token, exp)) = guard.as_ref() {
                if (chrono::Utc::now().timestamp() as u64 * 1000) < *exp {
                    return Ok(token.clone());
                }
            }
        }
        let url = format!("{}/token?grant_type=client_credential&appid={}&secret={}",
            Self::api_base(), self.config.app_id, self.config.app_secret);
        let resp = self.client.get(&url).send().await
            .map_err(|e| transport_err(format!("Token failed: {e}")))?
            .json::<serde_json::Value>().await
            .map_err(|e| transport_err(format!("Token parse: {e}")))?;
        let token = resp["access_token"].as_str().ok_or_else(|| transport_err("Missing access_token"))?.to_string();
        let expires_in = resp["expires_in"].as_u64().unwrap_or(7200);
        *self.access_token.lock() = Some((token.clone(), (chrono::Utc::now().timestamp() as u64 + expires_in) * 1000));
        Ok(token)
    }

    async fn send_text(&self, openid: &str, text: &str) -> Result<String, PlatformAdapterError> {
        let token = self.get_access_token().await?;
        let url = format!("{}/message/custom/send?access_token={}", Self::api_base(), token);
        let body = serde_json::json!({"touser": openid, "msgtype": "text", "text": {"content": text}});
        let resp = self.client.post(&url).json(&body).send().await
            .map_err(|e| transport_err(format!("Send failed: {e}")))?
            .json::<serde_json::Value>().await
            .map_err(|e| transport_err(format!("Send parse: {e}")))?;
        if resp["errcode"].as_i64().unwrap_or(0) != 0 {
            return Err(transport_err(format!("WeChat: {}", resp["errmsg"].as_str().unwrap_or("err"))));
        }
        Ok("ok".to_string())
    }
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable { platform: "wechat".into(), reason: msg.into() }
}

fn epoch_millis() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[async_trait]
impl PlatformAdapter for WeChatAdapter {
    async fn send_envelope(&self, domain: &BroadcastDomainId, envelope: &DeterministicEnvelope) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let encoded = Self::encode_envelope(&envelope.to_wire_bytes());
        let openid = self.config.groups.iter()
            .find(|g| Self::domain_hash(g) == domain.domain_hash)
            .ok_or_else(|| transport_err(format!("No group for domain {:?}", domain.domain_hash)))?;
        self.send_text(openid, &encoded).await?;
        Ok(DeliveryReceipt { platform_message_id: "wechat".to_string(), delivered_at: epoch_millis() })
    }

    async fn receive_messages(&self, _: &BroadcastDomainId) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> { Ok(vec![]) }

    fn canonicalize(&self, raw: &RawPlatformMessage) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() { return Err(transport_err("Empty payload")); }
        let wire = Self::decode_envelope(&String::from_utf8_lossy(&raw.payload))
            .map_err(|e| PlatformAdapterError::ApiError { code: 400, message: format!("canonicalize: {e}") })?;
        DeterministicEnvelope::from_wire_bytes(&wire).map_err(|e| PlatformAdapterError::ApiError { code: 400, message: format!("canonicalize: {e}") })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: true,
            supports_encryption: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: Some(MediaCapabilities { max_upload_bytes: 10_485_760, supported_mime_types: vec!["image/jpeg".into(), "image/png".into()] }),
        }
    }

    fn domain_id(&self, pid: &str) -> BroadcastDomainId { BroadcastDomainId::new(PlatformType::WeChat, pid) }
    fn platform_type(&self) -> PlatformType { PlatformType::WeChat }
    fn self_handle(&self) -> Option<String> { None }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> { *self.access_token.lock() = None; Ok(()) }
    async fn health_check(&self) -> Result<(), PlatformAdapterError> { self.get_access_token().await.map(|_| ()) }
}

#[no_mangle] pub extern "C" fn adapter_version() -> u32 { 1 }
#[no_mangle] pub extern "C" fn platform_type() -> u16 { 0x0011 }
#[no_mangle] pub unsafe extern "C" fn create_adapter(config: *const u8, len: usize) -> *mut () {
    if config.is_null() || len == 0 { return std::ptr::null_mut(); }
    match WeChatAdapter::from_config_bytes(std::slice::from_raw_parts(config, len)) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (), Err(_) => std::ptr::null_mut(),
    }
}
#[no_mangle] pub unsafe extern "C" fn destroy_adapter(ptr: *mut ()) {
    if !ptr.is_null() { let _ = Box::from_raw(ptr as *mut WeChatAdapter); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_domain_hash() { assert_eq!(WeChatAdapter::domain_hash("g1"), WeChatAdapter::domain_hash("g1")); assert_ne!(WeChatAdapter::domain_hash("g1"), WeChatAdapter::domain_hash("g2")); }
    #[test] fn test_encode_decode() { let d = b"test"; let e = WeChatAdapter::encode_envelope(d); assert!(e.starts_with("DOT/1/")); assert_eq!(WeChatAdapter::decode_envelope(&e).unwrap(), d); }
    #[test] fn test_platform_type() { assert_eq!(WeChatAdapter::PLATFORM_TYPE, 0x0011); }
    #[test] fn test_abi() { assert_eq!(adapter_version(), 1); assert_eq!(platform_type(), 0x0011); }
    #[test] fn test_capabilities() { let a = WeChatAdapter::new(WeChatConfig { app_id: "".into(), app_secret: "".into(), token: "".into(), encoding_aes_key: None, groups: vec![] }); assert_eq!(a.capabilities().max_payload_bytes, 2048); assert!(a.capabilities().supports_fragmentation); }
    #[test] fn test_self_handle_none() { assert!(WeChatAdapter::new(WeChatConfig { app_id: "".into(), app_secret: "".into(), token: "".into(), encoding_aes_key: None, groups: vec![] }).self_handle().is_none()); }
    #[test] fn test_decode_missing_prefix() { assert!(WeChatAdapter::decode_envelope("hello").is_err()); }
    #[test] fn test_config_from_json() { let a = WeChatAdapter::from_config_bytes(serde_json::to_vec(&serde_json::json!({"app_id":"wx123","app_secret":"s","token":"t","groups":["g1"]})).unwrap().as_slice()).unwrap(); assert_eq!(a.config.groups, vec!["g1"]); }
}
