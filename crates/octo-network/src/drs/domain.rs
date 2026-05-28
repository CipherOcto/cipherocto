//! Route domains and scope flags (RFC-0856 §3)

use serde::{Deserialize, Serialize};

/// Route scope flag — u64 bitmask for route domain isolation (RFC-0856 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u64)]
pub enum RouteScopeFlag {
    Global = 0x0001,
    Regional = 0x0002,
    Mission = 0x0004,
    Private = 0x0008,
    Local = 0x0010,
    Consensus = 0x0020,
}

impl RouteScopeFlag {
    /// Convert to u64 bitmask value.
    pub fn to_u64(self) -> u64 {
        self as u64
    }

    /// Check if a bitmask contains this flag.
    pub fn is_set(self, mask: u64) -> bool {
        (mask & self as u64) != 0
    }
}

/// Route domain — scopes routes to overlay domains (RFC-0856 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct RouteDomain {
    /// Network identifier
    pub network_id: u32,
    /// Mission identifier (zero for global routes)
    pub mission_id: [u8; 32],
    /// Scope flags (bitmask of RouteScopeFlag)
    pub scope_flags: u64,
}

impl RouteDomain {
    /// Create a global route domain.
    pub fn global(network_id: u32) -> Self {
        Self {
            network_id,
            mission_id: [0u8; 32],
            scope_flags: RouteScopeFlag::Global as u64,
        }
    }

    /// Create a mission-scoped route domain.
    pub fn mission(network_id: u32, mission_id: [u8; 32]) -> Self {
        Self {
            network_id,
            mission_id,
            scope_flags: RouteScopeFlag::Mission as u64,
        }
    }

    /// Compute domain hash for deterministic identification.
    pub fn domain_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.network_id.to_be_bytes());
        hasher.update(&self.mission_id);
        hasher.update(&self.scope_flags.to_be_bytes());
        *hasher.finalize().as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_scope_flag_values() {
        assert_eq!(RouteScopeFlag::Global as u64, 0x0001);
        assert_eq!(RouteScopeFlag::Regional as u64, 0x0002);
        assert_eq!(RouteScopeFlag::Mission as u64, 0x0004);
        assert_eq!(RouteScopeFlag::Private as u64, 0x0008);
        assert_eq!(RouteScopeFlag::Local as u64, 0x0010);
        assert_eq!(RouteScopeFlag::Consensus as u64, 0x0020);
    }

    #[test]
    fn test_route_scope_is_set() {
        let mask = RouteScopeFlag::Global as u64 | RouteScopeFlag::Regional as u64;
        assert!(RouteScopeFlag::Global.is_set(mask));
        assert!(RouteScopeFlag::Regional.is_set(mask));
        assert!(!RouteScopeFlag::Mission.is_set(mask));
    }

    #[test]
    fn test_route_domain_global() {
        let domain = RouteDomain::global(1);
        assert_eq!(domain.network_id, 1);
        assert_eq!(domain.mission_id, [0u8; 32]);
        assert_eq!(domain.scope_flags, RouteScopeFlag::Global as u64);
    }

    #[test]
    fn test_route_domain_mission() {
        let mission_id = [0xAA; 32];
        let domain = RouteDomain::mission(1, mission_id);
        assert_eq!(domain.mission_id, mission_id);
        assert_eq!(domain.scope_flags, RouteScopeFlag::Mission as u64);
    }

    #[test]
    fn test_route_domain_hash_deterministic() {
        let d1 = RouteDomain::global(1);
        let d2 = RouteDomain::global(1);
        assert_eq!(d1.domain_hash(), d2.domain_hash());
    }

    #[test]
    fn test_route_domain_hash_different() {
        let d1 = RouteDomain::global(1);
        let d2 = RouteDomain::global(2);
        assert_ne!(d1.domain_hash(), d2.domain_hash());
    }
}
