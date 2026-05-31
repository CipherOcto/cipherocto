//! Mission Route Table (RFC-0855 §13)
//!
//! Per-mission route tables with route isolation enforcement,
//! route commitment, and Merkle commitment over route tables.

use std::collections::BTreeMap;

/// A route entry in the mission route table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteEntry {
    /// The destination gateway ID.
    pub destination: [u8; 32],
    /// Next-hop gateway ID.
    pub next_hop: [u8; 32],
    /// Cost metric for this route.
    pub cost: u64,
    /// Sequence number for route freshness.
    pub sequence: u64,
}

/// Per-mission route table.
///
/// Stores route entries keyed by destination gateway ID,
/// scoped to a single mission ID.
#[derive(Clone, Debug)]
pub struct MissionRouteTable {
    /// The mission this route table belongs to.
    pub mission_id: [u8; 32],
    /// Route entries keyed by destination.
    pub routes: BTreeMap<[u8; 32], RouteEntry>,
}

impl MissionRouteTable {
    /// Create a new empty route table for a mission.
    pub fn new(mission_id: [u8; 32]) -> Self {
        Self {
            mission_id,
            routes: BTreeMap::new(),
        }
    }

    /// Insert or update a route entry.
    ///
    /// Only updates if the new entry's sequence is higher than the existing one,
    /// preventing stale routes from overwriting fresh ones.
    pub fn upsert(&mut self, entry: RouteEntry) {
        if let Some(existing) = self.routes.get(&entry.destination) {
            if entry.sequence <= existing.sequence {
                return; // Reject stale update
            }
        }
        self.routes.insert(entry.destination, entry);
    }

    /// Remove a route by destination.
    pub fn remove(&mut self, destination: &[u8; 32]) -> Option<RouteEntry> {
        self.routes.remove(destination)
    }

    /// Look up a route by destination.
    pub fn lookup(&self, destination: &[u8; 32]) -> Option<&RouteEntry> {
        self.routes.get(destination)
    }

    /// Number of routes in the table.
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Whether the route table is empty.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

/// Route isolation guard — enforces that only authorized gateways
/// can participate in a given mission's routing.
#[derive(Clone, Debug)]
pub struct RouteIsolationGuard {
    /// The mission ID this guard protects.
    pub mission_id: [u8; 32],
    /// Set of authorized gateway IDs.
    pub authorized_gateways: std::collections::BTreeSet<[u8; 32]>,
}

impl RouteIsolationGuard {
    /// Create a new guard for a mission with the given authorized gateways.
    pub fn new(mission_id: [u8; 32], authorized_gateways: Vec<[u8; 32]>) -> Self {
        Self {
            mission_id,
            authorized_gateways: authorized_gateways.into_iter().collect(),
        }
    }

    /// Check if a gateway is authorized to participate in the given mission's routing.
    ///
    /// Returns `true` if the gateway is in the authorized set for this mission.
    pub fn is_authorized(&self, mission_id: &[u8; 32], gateway_id: &[u8; 32]) -> bool {
        if self.mission_id != *mission_id {
            return false;
        }
        self.authorized_gateways.contains(gateway_id)
    }
}

/// Compute a route commitment hash.
///
/// Commitment = BLAKE3-256("route_commitment:v1" || mission_id || route_sequence_be || epoch_be)
///
/// This binds a specific route sequence and epoch to a mission,
/// enabling verifiable route commitments.
pub fn compute_route_commitment(
    mission_id: &[u8; 32],
    route_sequence: u64,
    epoch: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"route_commitment:v1");
    hasher.update(mission_id);
    hasher.update(&route_sequence.to_be_bytes());
    hasher.update(&epoch.to_be_bytes());
    *hasher.finalize().as_bytes()
}

/// Compute a Merkle root over the route table using BLAKE3.
///
/// The Merkle tree is built over sorted (destination, entry_hash) pairs.
/// Entry hash = BLAKE3(destination || next_hop || cost_be || sequence_be).
///
/// Returns the BLAKE3 Merkle root of the route table, or zero hash if empty.
pub fn compute_route_table_merkle(routes: &BTreeMap<[u8; 32], RouteEntry>) -> [u8; 32] {
    if routes.is_empty() {
        return [0u8; 32];
    }

    // BTreeMap is already sorted by key (destination), which gives deterministic ordering.
    let leaf_hashes: Vec<[u8; 32]> = routes
        .values()
        .map(|entry| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"route_entry:v1");
            hasher.update(&entry.destination);
            hasher.update(&entry.next_hop);
            hasher.update(&entry.cost.to_be_bytes());
            hasher.update(&entry.sequence.to_be_bytes());
            *hasher.finalize().as_bytes()
        })
        .collect();

    compute_merkle_root(&leaf_hashes)
}

/// Compute a BLAKE3-256 Merkle root from a list of hashes.
///
/// Uses domain separation per RFC 6962:
/// - Leaf hash: BLAKE3(0x00 || hash)
/// - Internal hash: BLAKE3(0x01 || left || right)
///
/// If the number of leaves is odd, the last element is duplicated for pairing.
/// Returns zero hash for empty input.
fn compute_merkle_root(hashes: &[[u8; 32]]) -> [u8; 32] {
    if hashes.is_empty() {
        return [0u8; 32];
    }

    // Compute leaf hashes with domain separation
    let mut level: Vec<[u8; 32]> = hashes
        .iter()
        .map(|h| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&[0x00]);
            hasher.update(h);
            *hasher.finalize().as_bytes()
        })
        .collect();

    // Build tree bottom-up
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() {
                level[i + 1]
            } else {
                level[i]
            };
            let mut hasher = blake3::Hasher::new();
            hasher.update(&[0x01]);
            hasher.update(&left);
            hasher.update(&right);
            next.push(*hasher.finalize().as_bytes());
            i += 2;
        }
        level = next;
    }

    level[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(dest_byte: u8, hop_byte: u8, cost: u64, seq: u64) -> RouteEntry {
        RouteEntry {
            destination: [dest_byte; 32],
            next_hop: [hop_byte; 32],
            cost,
            sequence: seq,
        }
    }

    // -- MissionRouteTable tests --

    #[test]
    fn test_route_table_new_empty() {
        let table = MissionRouteTable::new([0x01; 32]);
        assert_eq!(table.mission_id, [0x01; 32]);
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn test_route_table_upsert_and_lookup() {
        let mut table = MissionRouteTable::new([0x01; 32]);
        table.upsert(make_entry(0xAA, 0xBB, 10, 1));
        assert_eq!(table.len(), 1);
        let entry = table.lookup(&[0xAA; 32]).unwrap();
        assert_eq!(entry.next_hop, [0xBB; 32]);
        assert_eq!(entry.cost, 10);
    }

    #[test]
    fn test_route_table_update_existing() {
        let mut table = MissionRouteTable::new([0x01; 32]);
        table.upsert(make_entry(0xAA, 0xBB, 10, 1));
        table.upsert(make_entry(0xAA, 0xCC, 20, 2));
        assert_eq!(table.len(), 1);
        let entry = table.lookup(&[0xAA; 32]).unwrap();
        assert_eq!(entry.next_hop, [0xCC; 32]);
        assert_eq!(entry.cost, 20);
    }

    #[test]
    fn test_route_table_remove() {
        let mut table = MissionRouteTable::new([0x01; 32]);
        table.upsert(make_entry(0xAA, 0xBB, 10, 1));
        let removed = table.remove(&[0xAA; 32]);
        assert!(removed.is_some());
        assert!(table.is_empty());
    }

    #[test]
    fn test_route_table_lookup_missing() {
        let table = MissionRouteTable::new([0x01; 32]);
        assert!(table.lookup(&[0xFF; 32]).is_none());
    }

    #[test]
    fn test_route_table_rejects_stale_update() {
        let mut table = MissionRouteTable::new([0x01; 32]);
        table.upsert(make_entry(0xAA, 0xBB, 10, 5));
        // Try to update with lower sequence — should be rejected
        table.upsert(make_entry(0xAA, 0xCC, 20, 3));
        let entry = table.lookup(&[0xAA; 32]).unwrap();
        assert_eq!(entry.next_hop, [0xBB; 32], "stale update must be rejected");
        assert_eq!(entry.cost, 10);
        assert_eq!(entry.sequence, 5);
    }

    #[test]
    fn test_route_table_accepts_same_sequence_higher() {
        let mut table = MissionRouteTable::new([0x01; 32]);
        table.upsert(make_entry(0xAA, 0xBB, 10, 5));
        // Same sequence — should be rejected (not strictly greater)
        table.upsert(make_entry(0xAA, 0xCC, 20, 5));
        let entry = table.lookup(&[0xAA; 32]).unwrap();
        assert_eq!(entry.next_hop, [0xBB; 32]);
    }

    // -- RouteIsolationGuard tests --

    #[test]
    fn test_isolation_guard_authorized() {
        let guard = RouteIsolationGuard::new([0x01; 32], vec![[0xAA; 32], [0xBB; 32]]);
        assert!(guard.is_authorized(&[0x01; 32], &[0xAA; 32]));
        assert!(guard.is_authorized(&[0x01; 32], &[0xBB; 32]));
    }

    #[test]
    fn test_isolation_guard_unauthorized_gateway() {
        let guard = RouteIsolationGuard::new([0x01; 32], vec![[0xAA; 32]]);
        assert!(!guard.is_authorized(&[0x01; 32], &[0xFF; 32]));
    }

    #[test]
    fn test_isolation_guard_wrong_mission() {
        let guard = RouteIsolationGuard::new([0x01; 32], vec![[0xAA; 32]]);
        assert!(!guard.is_authorized(&[0x02; 32], &[0xAA; 32]));
    }

    // -- Route commitment tests --

    #[test]
    fn test_route_commitment_deterministic() {
        let c1 = compute_route_commitment(&[0x01; 32], 1, 100);
        let c2 = compute_route_commitment(&[0x01; 32], 1, 100);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_route_commitment_varies_by_sequence() {
        let c1 = compute_route_commitment(&[0x01; 32], 1, 100);
        let c2 = compute_route_commitment(&[0x01; 32], 2, 100);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_route_commitment_varies_by_epoch() {
        let c1 = compute_route_commitment(&[0x01; 32], 1, 100);
        let c2 = compute_route_commitment(&[0x01; 32], 1, 200);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_route_commitment_varies_by_mission() {
        let c1 = compute_route_commitment(&[0x01; 32], 1, 100);
        let c2 = compute_route_commitment(&[0x02; 32], 1, 100);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_route_commitment_length() {
        let c = compute_route_commitment(&[0x01; 32], 1, 100);
        assert_eq!(c.len(), 32);
    }

    // -- Route table Merkle tests --

    #[test]
    fn test_route_table_merkle_empty() {
        let routes = BTreeMap::new();
        let root = compute_route_table_merkle(&routes);
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn test_route_table_merkle_deterministic() {
        let mut routes = BTreeMap::new();
        routes.insert([0xAA; 32], make_entry(0xAA, 0xBB, 10, 1));
        routes.insert([0xCC; 32], make_entry(0xCC, 0xDD, 20, 2));
        let r1 = compute_route_table_merkle(&routes);
        let r2 = compute_route_table_merkle(&routes);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_route_table_merkle_changes_with_entry() {
        let mut routes1 = BTreeMap::new();
        routes1.insert([0xAA; 32], make_entry(0xAA, 0xBB, 10, 1));

        let mut routes2 = BTreeMap::new();
        routes2.insert([0xAA; 32], make_entry(0xAA, 0xBB, 99, 1));

        let r1 = compute_route_table_merkle(&routes1);
        let r2 = compute_route_table_merkle(&routes2);
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_route_table_merkle_nonzero_for_nonempty() {
        let mut routes = BTreeMap::new();
        routes.insert([0xAA; 32], make_entry(0xAA, 0xBB, 10, 1));
        let root = compute_route_table_merkle(&routes);
        assert_ne!(root, [0u8; 32]);
    }
}
