//! DOT configuration (YAML/TOML deserialization)

use serde::Deserialize;

/// Top-level DOT configuration
#[derive(Clone, Debug, Deserialize)]
pub struct DotConfig {
    /// Network identifier
    pub network_id: u32,
    /// Gateway configuration
    pub gateway: GatewayConfig,
    /// Replay cache configuration
    pub replay_cache: ReplayCacheConfig,
    /// Platform-specific configurations
    pub platforms: PlatformsConfig,
}

/// Gateway configuration
#[derive(Clone, Debug, Deserialize)]
pub struct GatewayConfig {
    /// Gateway class (edge, relay, consensus, archive, stealth, translation)
    pub class: String,
    /// Creation epoch
    pub creation_epoch: u64,
}

/// Replay cache configuration
#[derive(Clone, Debug, Deserialize)]
pub struct ReplayCacheConfig {
    /// Maximum entries in the replay cache
    pub max_entries: u32,
    /// Window duration in seconds
    pub window_duration_secs: u64,
}

/// Platform-specific configurations
#[derive(Clone, Debug, Deserialize)]
pub struct PlatformsConfig {
    /// Telegram adapter configuration
    pub telegram: Option<PlatformEntry>,
    /// Discord adapter configuration
    pub discord: Option<PlatformEntry>,
    /// Matrix adapter configuration
    pub matrix: Option<PlatformEntry>,
    /// Native P2P configuration
    pub native_p2p: Option<NativeP2PConfig>,
}

/// Single platform configuration entry
#[derive(Clone, Debug, Deserialize)]
pub struct PlatformEntry {
    /// Whether this platform is enabled
    pub enabled: bool,
}

/// Native P2P specific configuration
#[derive(Clone, Debug, Deserialize)]
pub struct NativeP2PConfig {
    /// Whether native P2P is enabled
    pub enabled: bool,
    /// Listen address (e.g., "/ip4/0.0.0.0/tcp/4001")
    pub listen_addr: String,
}

impl Default for DotConfig {
    fn default() -> Self {
        Self {
            network_id: 1,
            gateway: GatewayConfig {
                class: "edge".to_string(),
                creation_epoch: 0,
            },
            replay_cache: ReplayCacheConfig {
                max_entries: 100_000,
                window_duration_secs: 3600,
            },
            platforms: PlatformsConfig {
                telegram: None,
                discord: None,
                matrix: None,
                native_p2p: Some(NativeP2PConfig {
                    enabled: true,
                    listen_addr: "/ip4/0.0.0.0/tcp/4001".to_string(),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot_config_default() {
        let config = DotConfig::default();
        assert_eq!(config.network_id, 1);
        assert_eq!(config.gateway.class, "edge");
        assert_eq!(config.replay_cache.max_entries, 100_000);
        assert!(config.platforms.native_p2p.is_some());
    }
}
