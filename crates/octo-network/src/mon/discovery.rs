//! Mission Discovery Scopes (RFC-0855 §8.1)

use serde::{Deserialize, Serialize};

/// Mission discovery scopes — distinct from GDP's DiscoveryScope.
/// Discriminants start at 0x0100 to avoid collision with GDP (0x0001-0x0006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum MissionDiscoveryScope {
    Public = 0x0100,
    InviteOnly = 0x0101,
    Stealth = 0x0102,
    Federated = 0x0103,
    Ephemeral = 0x0104,
}

/// Map MissionDiscoveryScope to GDP DiscoveryScope (RFC-0855 §8.2).
pub fn scope_to_gdp_scope(scope: MissionDiscoveryScope) -> u16 {
    match scope {
        MissionDiscoveryScope::Public => 0x0004,     // Global
        MissionDiscoveryScope::InviteOnly => 0x0005, // Private
        MissionDiscoveryScope::Stealth => 0x0005,    // Private + stealth flag
        MissionDiscoveryScope::Federated => 0x0002,  // Regional
        MissionDiscoveryScope::Ephemeral => 0x0003,  // Mission
    }
}

/// Ephemeral mission TTL (hops) for advertisements.
pub const EPHEMERAL_ADVERTISEMENT_TTL: u16 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_repr_values() {
        assert_eq!(MissionDiscoveryScope::Public as u16, 0x0100);
        assert_eq!(MissionDiscoveryScope::Ephemeral as u16, 0x0104);
    }

    #[test]
    fn test_scope_no_collision_with_gdp() {
        // GDP DiscoveryScope uses 0x0001-0x0006
        // MissionDiscoveryScope uses 0x0100-0x0104
        assert!(MissionDiscoveryScope::Public as u16 > 0x00FF);
    }

    #[test]
    fn test_scope_to_gdp_mapping() {
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Public), 0x0004);
        assert_eq!(
            scope_to_gdp_scope(MissionDiscoveryScope::InviteOnly),
            0x0005
        );
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Stealth), 0x0005);
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Federated), 0x0002);
        assert_eq!(scope_to_gdp_scope(MissionDiscoveryScope::Ephemeral), 0x0003);
    }
}
