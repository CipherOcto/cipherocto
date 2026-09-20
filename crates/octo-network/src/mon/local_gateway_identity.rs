//! Local gateway identity state storage (RFC-0011-i companion G5 substrate)
//!
//! Persists the 3 fields that `GatewayIdentity::new` requires but the
//! CLI source (octo-wallet `IdentityKey`) does NOT provide
//! (`network_id`, `gateway_class`, `creation_epoch`). Without this
//! substrate, `octo network identity show` cannot construct a
//! deterministic `GatewayIdentity` matching the network's view of the
//! local gateway — it would derive a deterministic-but-wrong
//! `gateway_id` using zero-defaults.
//!
//! Layer B per [[cipherocto-design-principles]] §Stable Abstractions
//! Principle. Zero Layer A change.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::dot::gateway::GatewayClass;

/// File name under `<octo_home>/network/`
const FILE_NAME: &str = "local-gateway-identity.toml";

/// Local gateway identity state persisted on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalGatewayIdentity {
    pub network_id: u32,
    pub gateway_class: GatewayClass,
    pub creation_epoch: u64,
}

#[derive(Debug)]
pub enum LocalGatewayIdentityError {
    NotInitialized,
    IoError(std::io::Error),
    TomlParseError(toml::de::Error),
    TomlSerializeError(toml::ser::Error),
}

impl std::fmt::Display for LocalGatewayIdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInitialized => write!(
                f,
                "local gateway identity not initialized; run octo network bootstrap first"
            ),
            Self::IoError(e) => write!(f, "io error reading local gateway identity: {e}"),
            Self::TomlParseError(e) => write!(f, "toml parse error: {e}"),
            Self::TomlSerializeError(e) => write!(f, "toml serialize error: {e}"),
        }
    }
}

impl std::error::Error for LocalGatewayIdentityError {}

impl From<std::io::Error> for LocalGatewayIdentityError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Resolve the canonical on-disk path under `<octo_home>/network/`.
fn state_path(octo_home: &Path) -> PathBuf {
    octo_home.join("network").join(FILE_NAME)
}

impl LocalGatewayIdentity {
    /// Load local identity state from `<octo_home>/network/local-gateway-identity.toml`.
    /// Returns `NotInitialized` if the file does not exist (zero-defaults are
    /// semantically wrong; the operator must bootstrap first).
    pub fn load(octo_home: &Path) -> Result<Self, LocalGatewayIdentityError> {
        let path = state_path(octo_home);
        if !path.exists() {
            return Err(LocalGatewayIdentityError::NotInitialized);
        }
        let body = std::fs::read_to_string(&path)?;
        let parsed: Self =
            toml::from_str(&body).map_err(LocalGatewayIdentityError::TomlParseError)?;
        Ok(parsed)
    }

    /// Persist local identity state. Creates the parent `network/` directory
    /// if missing. Overwrites the existing file.
    pub fn save(&self, octo_home: &Path) -> Result<(), LocalGatewayIdentityError> {
        let path = state_path(octo_home);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string(self).map_err(LocalGatewayIdentityError::TomlSerializeError)?;
        std::fs::write(&path, body)?;
        Ok(())
    }

    /// Check whether the local identity state file exists at `<octo_home>/network/`.
    pub fn exists(octo_home: &Path) -> bool {
        state_path(octo_home).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_octo_home(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "octo-lgi-test-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn load_returns_not_initialized_when_missing() {
        let home = temp_octo_home("missing");
        match LocalGatewayIdentity::load(&home) {
            Err(LocalGatewayIdentityError::NotInitialized) => {}
            other => panic!("expected NotInitialized, got {other:?}"),
        }
    }

    #[test]
    fn save_then_load_round_trip() {
        let home = temp_octo_home("roundtrip");
        let original = LocalGatewayIdentity {
            network_id: 42,
            gateway_class: GatewayClass::Relay,
            creation_epoch: 1700,
        };
        original.save(&home).expect("save should succeed");
        let loaded = LocalGatewayIdentity::load(&home).expect("load should succeed");
        assert_eq!(original, loaded);
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn exists_returns_true_after_save_false_before() {
        let home = temp_octo_home("exists");
        assert!(!LocalGatewayIdentity::exists(&home));
        let state = LocalGatewayIdentity {
            network_id: 1,
            gateway_class: GatewayClass::Edge,
            creation_epoch: 0,
        };
        state.save(&home).expect("save should succeed");
        assert!(LocalGatewayIdentity::exists(&home));
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn load_returns_not_initialized_for_nonexistent_state_dir() {
        // When <octo_home>/network/ does not exist at all, load must
        // surface NotInitialized (NOT an io error). The contract is:
        // missing state file = NotInitialized; otherwise the io layer
        // surfaces the error verbatim.
        let home = temp_octo_home("nonexistent-dir");
        std::fs::remove_dir_all(&home).ok();
        match LocalGatewayIdentity::load(&home) {
            Err(LocalGatewayIdentityError::NotInitialized) => {}
            other => panic!("expected NotInitialized, got {other:?}"),
        }
    }
}
