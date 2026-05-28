//! Mission Economics (RFC-0855 §17)

use serde::{Deserialize, Serialize};

/// Slashing conditions (RFC-0855 §17)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum SlashingCondition {
    InvalidTaskResult = 0x0001,
    EnvelopeForgery = 0x0002,
    IsolationBreach = 0x0003,
    FreeRiding = 0x0004,
    CoordinatorMisbehavior = 0x0005,
}

/// Token types used in mission economics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum TokenType {
    OctoO = 0x0001, // Coordinator/orchestration
    OctoA = 0x0002, // Compute/proof
    OctoB = 0x0003, // Bandwidth/relay
    OctoN = 0x0004, // Node operations
    OctoS = 0x0005, // Storage
}

/// Economic incentive record for a mission operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct IncentiveRecord {
    pub peer_id: [u8; 32],
    pub token_type: TokenType,
    pub amount: u64,
    pub epoch: u64,
    pub reason: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slashing_condition_repr() {
        assert_eq!(SlashingCondition::InvalidTaskResult as u16, 0x0001);
        assert_eq!(SlashingCondition::CoordinatorMisbehavior as u16, 0x0005);
    }

    #[test]
    fn test_token_type_repr() {
        assert_eq!(TokenType::OctoO as u16, 0x0001);
        assert_eq!(TokenType::OctoS as u16, 0x0005);
    }
}
