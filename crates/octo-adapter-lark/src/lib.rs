//! Lark/Feishu adapter for DOT (RFC-0850 S8.1, PlatformType::Lark)
//!
//! Bridges DOT envelopes to Lark/Feishu via the Lark Bot API.
//! Supports both International (larksuite.com) and China (feishu.cn) regions.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "app_id": "cli_...",
//!   "app_secret": "...",
//!   "region": "international",
//!   "groups": ["oc_..."]
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

#[derive(Clone, serde::Deserialize, serde::Serialize, Debug, Default)]
pub enum LarkRegion {
    #[default]
    #[serde(rename = "international")]
    International,
    #[serde(rename = "china")]
    China,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct LarkConfig {
    pub app_id: String,
    pub app_secret: String,
    #[serde(default)]
    pub region: LarkRegion,
    pub groups: Vec<String>,
}

impl std::fmt::Debug for LarkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LarkConfig")
            .field("app_id", &self.app_id)
            .field("app_secret", &"***")
            .field("region", &self.region)
            .field("groups", &self.groups)
            .finish()
    }
}

pub struct LarkAdapter {
    config: LarkConfig,
    client: reqwest::Client,
    token_cache: Arc<Mutex<Option<(String, u64)>>>,
}

impl LarkAdapter {
    pub fn new(config: LarkConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            token_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_config_bytes(config: &[u8]) -> Result<Self, String> {
        Ok(Self::new(
            serde_json::from_slice(config).map_err(|e| format!("Invalid config: {e}"))?,
        ))
    }

    fn api_base(&self) -> &str {
        match self.config.region {
            LarkRegion::International => "https://open.larksuite.com/open-apis",
            LarkRegion::China => "https://open.feishu.cn/open-apis",
        }
    }

    pub fn encode_envelope(bytes: &[u8]) -> String {
        format!(
            "DOT/1/{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        )
    }

    pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String> {
        let b64 = text
            .trim()
            .strip_prefix("DOT/1/")
            .ok_or("Missing DOT/1/ prefix")?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .map_err(|e| format!("Base64: {e}"))
    }

    pub fn domain_hash(chat_id: &str) -> [u8; 32] {
        *blake3::hash(format!("lark:{}", chat_id.trim().to_lowercase()).as_bytes()).as_bytes()
    }

    pub const PLATFORM_TYPE: u16 = 0x0013;
    pub fn max_payload_bytes() -> usize {
        30_000
    }
    pub fn rate_limit_per_second() -> u32 {
        50
    }

    async fn get_tenant_token(&self) -> Result<String, PlatformAdapterError> {
        {
            let g = self.token_cache.lock();
            if let Some((t, exp)) = g.as_ref() {
                if (chrono::Utc::now().timestamp() as u64) < *exp {
                    return Ok(t.clone());
                }
            }
        }
        let url = format!("{}/auth/v3/tenant_access_token/internal", self.api_base());
        let body =
            serde_json::json!({"app_id": self.config.app_id, "app_secret": self.config.app_secret});
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_err(format!("Token failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Token parse: {e}")))?;
        let token = resp["tenant_access_token"]
            .as_str()
            .ok_or_else(|| transport_err("Missing token"))?
            .to_string();
        let expire = resp["expire"].as_u64().unwrap_or(7200);
        *self.token_cache.lock() = Some((
            token.clone(),
            chrono::Utc::now().timestamp() as u64 + expire,
        ));
        Ok(token)
    }

    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<String, PlatformAdapterError> {
        let token = self.get_tenant_token().await?;
        let url = format!("{}/im/v1/messages?receive_id_type=chat_id", self.api_base());
        let body = serde_json::json!({"receive_id": chat_id, "msg_type": "text", "content": serde_json::json!({"text": text}).to_string()});
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| transport_err(format!("Send failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Send parse: {e}")))?;
        let msg_id = resp["data"]["message_id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(msg_id)
    }
}

fn transport_err(msg: impl Into<String>) -> PlatformAdapterError {
    PlatformAdapterError::Unreachable {
        platform: "lark".into(),
        reason: msg.into(),
    }
}

fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[async_trait]
impl PlatformAdapter for LarkAdapter {
    async fn send_envelope(
        &self,
        domain: &BroadcastDomainId,
        envelope: &DeterministicEnvelope,
    ) -> Result<DeliveryReceipt, PlatformAdapterError> {
        let encoded = Self::encode_envelope(&envelope.to_wire_bytes());
        let chat_id = self
            .config
            .groups
            .iter()
            .find(|g| Self::domain_hash(g) == domain.domain_hash)
            .ok_or_else(|| {
                transport_err(format!("No group for domain {:?}", domain.domain_hash))
            })?;
        let msg_id = self.send_message(chat_id, &encoded).await?;
        Ok(DeliveryReceipt {
            platform_message_id: msg_id,
            delivered_at: epoch_millis(),
        })
    }

    async fn receive_messages(
        &self,
        _: &BroadcastDomainId,
    ) -> Result<Vec<RawPlatformMessage>, PlatformAdapterError> {
        Ok(vec![])
    }

    fn canonicalize(
        &self,
        raw: &RawPlatformMessage,
    ) -> Result<DeterministicEnvelope, PlatformAdapterError> {
        if raw.payload.is_empty() {
            return Err(transport_err("Empty payload"));
        }
        let wire = Self::decode_envelope(&String::from_utf8_lossy(&raw.payload)).map_err(|e| {
            PlatformAdapterError::ApiError {
                code: 400,
                message: format!("canonicalize: {e}"),
            }
        })?;
        DeterministicEnvelope::from_wire_bytes(&wire).map_err(|e| PlatformAdapterError::ApiError {
            code: 400,
            message: format!("canonicalize: {e}"),
        })
    }

    fn capabilities(&self) -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: Self::max_payload_bytes(),
            supports_fragmentation: false,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: Self::rate_limit_per_second(),
            media_capabilities: Some(MediaCapabilities {
                max_upload_bytes: 52_428_800,
                supported_mime_types: vec![
                    "image/jpeg".into(),
                    "image/png".into(),
                    "application/pdf".into(),
                ],
            }),

            ..Default::default()
        }
    }

    fn domain_id(&self, pid: &str) -> BroadcastDomainId {
        BroadcastDomainId::new(PlatformType::Lark, pid)
    }
    fn platform_type(&self) -> PlatformType {
        PlatformType::Lark
    }
    fn self_handle(&self) -> Option<String> {
        None
    }

    async fn shutdown(&self) -> Result<(), PlatformAdapterError> {
        *self.token_cache.lock() = None;
        Ok(())
    }
    async fn health_check(&self) -> Result<(), PlatformAdapterError> {
        self.get_tenant_token().await.map(|_| ())
    }

    async fn upload_media(
        &self,
        filename: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, PlatformAdapterError> {
        let token = self.get_tenant_token().await?;
        let url = format!("{}/im/v1/images", self.api_base());
        let file_part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .map_err(|e| transport_err(format!("MIME: {e}")))?;
        let form = reqwest::multipart::Form::new().part("image", file_part);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| transport_err(format!("Upload failed: {e}")))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| transport_err(format!("Parse: {e}")))?;
        let image_key = resp["data"]["image_key"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(image_key)
    }
    async fn download_media(&self, media_id: &str) -> Result<Vec<u8>, PlatformAdapterError> {
        let token = self.get_tenant_token().await?;
        let url = format!("{}/im/v1/images/{}", self.api_base(), media_id);
        let bytes = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| transport_err(format!("Download failed: {e}")))?
            .bytes()
            .await
            .map_err(|e| transport_err(format!("Download read: {e}")))?;
        Ok(bytes.to_vec())
    }
}

#[no_mangle]
pub extern "C" fn adapter_version() -> u32 {
    1
}
#[no_mangle]
pub extern "C" fn platform_type() -> u16 {
    0x0013
}
#[no_mangle]
/// # Safety
///
/// `config` must point to a valid buffer of at least `len` bytes.
pub unsafe extern "C" fn create_adapter(config: *const u8, len: usize) -> *mut () {
    if config.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    match LarkAdapter::from_config_bytes(std::slice::from_raw_parts(config, len)) {
        Ok(a) => Box::into_raw(Box::new(a)) as *mut (),
        Err(_) => std::ptr::null_mut(),
    }
}
#[no_mangle]
/// # Safety
///
/// `ptr` must be a pointer previously returned by `create_adapter`.
pub unsafe extern "C" fn destroy_adapter(ptr: *mut ()) {
    if !ptr.is_null() {
        let _ = Box::from_raw(ptr as *mut LarkAdapter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_domain_hash() {
        assert_eq!(
            LarkAdapter::domain_hash("oc123"),
            LarkAdapter::domain_hash("oc123")
        );
        assert_ne!(
            LarkAdapter::domain_hash("oc123"),
            LarkAdapter::domain_hash("oc456")
        );
    }
    #[test]
    fn test_encode_decode() {
        let d = b"test";
        let e = LarkAdapter::encode_envelope(d);
        assert!(e.starts_with("DOT/1/"));
        assert_eq!(LarkAdapter::decode_envelope(&e).unwrap(), d);
    }
    #[test]
    fn test_platform_type() {
        assert_eq!(LarkAdapter::PLATFORM_TYPE, 0x0013);
    }
    #[test]
    fn test_abi() {
        assert_eq!(adapter_version(), 1);
        assert_eq!(platform_type(), 0x0013);
    }
    #[test]
    fn test_capabilities() {
        let a = LarkAdapter::new(LarkConfig {
            app_id: "".into(),
            app_secret: "".into(),
            region: LarkRegion::International,
            groups: vec![],
        });
        assert_eq!(a.capabilities().max_payload_bytes, 30_000);
        assert!(!a.capabilities().supports_fragmentation);
        assert!(a.capabilities().media_capabilities.is_some());
    }
    #[test]
    fn test_region_api_base() {
        let intl = LarkAdapter::new(LarkConfig {
            app_id: "".into(),
            app_secret: "".into(),
            region: LarkRegion::International,
            groups: vec![],
        });
        assert!(intl.api_base().contains("larksuite"));
        let cn = LarkAdapter::new(LarkConfig {
            app_id: "".into(),
            app_secret: "".into(),
            region: LarkRegion::China,
            groups: vec![],
        });
        assert!(cn.api_base().contains("feishu"));
    }
    #[test]
    fn test_self_handle_none() {
        assert!(LarkAdapter::new(LarkConfig {
            app_id: "".into(),
            app_secret: "".into(),
            region: LarkRegion::International,
            groups: vec![]
        })
        .self_handle()
        .is_none());
    }
    #[test]
    fn test_decode_missing_prefix() {
        assert!(LarkAdapter::decode_envelope("hello").is_err());
    }
    #[test]
    fn test_config_from_json() {
        let a = LarkAdapter::from_config_bytes(serde_json::to_vec(&serde_json::json!({"app_id":"cli_123","app_secret":"s","region":"china","groups":["oc_1"]})).unwrap().as_slice()).unwrap();
        assert_eq!(a.config.groups.len(), 1);
    }
}
