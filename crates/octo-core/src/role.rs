use serde::{Deserialize, Serialize};

/// Ecosystem roles available in CipherOcto
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    #[serde(rename = "builder")]
    Builder,
    #[serde(rename = "provider")]
    Provider,
    #[serde(rename = "storage")]
    Storage,
    #[serde(rename = "bandwidth")]
    Bandwidth,
    #[serde(rename = "orchestrator")]
    Orchestrator,
}

impl Role {
    pub fn token_symbol(&self) -> &str {
        match self {
            Role::Builder => "OCTO-D",
            Role::Provider => "OCTO-A",
            Role::Storage => "OCTO-S",
            Role::Bandwidth => "OCTO-B",
            Role::Orchestrator => "OCTO-O",
        }
    }
}
