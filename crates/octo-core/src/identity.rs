use serde::{Deserialize, Serialize};

/// User identity in the CipherOcto network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub public_key: String,
}

impl Identity {
    pub fn new(id: String) -> Self {
        Self {
            id,
            public_key: String::new(), // In MVP: placeholder
        }
    }
}
