//! Gossip fragment reassembly (RFC-0852 §11)

use std::collections::BTreeMap;

use super::error::DgpError;

/// Default reassembly timeout in logical time units.
const DEFAULT_REASSEMBLY_TIMEOUT: u64 = 30;

/// A single gossip fragment.
#[derive(Debug, Clone)]
pub struct GossipFragment {
    /// Object hash of the complete object
    pub object_hash: [u8; 32],
    /// Fragment index (0-based)
    pub fragment_index: u32,
    /// Total fragment count
    pub fragment_total: u32,
    /// Fragment payload bytes
    pub payload: Vec<u8>,
}

/// Tracks fragment reassembly state for a single object.
#[derive(Debug)]
pub struct FragmentAssembly {
    /// Expected total fragments
    pub total: u32,
    /// Received fragments (index -> payload)
    pub fragments: BTreeMap<u32, Vec<u8>>,
    /// When reassembly started
    pub started_at: u64,
}

/// Fragment reassembly manager.
#[derive(Debug)]
pub struct FragmentAssembler {
    /// In-progress assemblies (object_hash -> assembly)
    assemblies: BTreeMap<[u8; 32], FragmentAssembly>,
    /// Reassembly timeout
    timeout: u64,
}

impl FragmentAssembler {
    /// Create a new fragment assembler.
    pub fn new(timeout: Option<u64>) -> Self {
        Self {
            assemblies: BTreeMap::new(),
            timeout: timeout.unwrap_or(DEFAULT_REASSEMBLY_TIMEOUT),
        }
    }

    /// Add a fragment. Returns Some(payload) when all fragments are collected.
    pub fn add_fragment(
        &mut self,
        fragment: GossipFragment,
        current_time: u64,
    ) -> Result<Option<Vec<u8>>, DgpError> {
        if fragment.fragment_index >= fragment.fragment_total {
            return Err(DgpError::FragmentAssemblyFailed {
                object_hash: fragment.object_hash,
                reason: format!(
                    "fragment_index {} >= fragment_total {}",
                    fragment.fragment_index, fragment.fragment_total
                ),
            });
        }

        let assembly = self
            .assemblies
            .entry(fragment.object_hash)
            .or_insert_with(|| FragmentAssembly {
                total: fragment.fragment_total,
                fragments: BTreeMap::new(),
                started_at: current_time,
            });

        // Validate consistency
        if assembly.total != fragment.fragment_total {
            return Err(DgpError::FragmentAssemblyFailed {
                object_hash: fragment.object_hash,
                reason: format!(
                    "fragment_total mismatch: expected {}, got {}",
                    assembly.total, fragment.fragment_total
                ),
            });
        }

        assembly
            .fragments
            .insert(fragment.fragment_index, fragment.payload);

        // Check if complete
        if assembly.fragments.len() as u32 == assembly.total {
            let mut result = Vec::new();
            for i in 0..assembly.total {
                if let Some(payload) = assembly.fragments.remove(&i) {
                    result.extend(payload);
                }
            }
            self.assemblies.remove(&fragment.object_hash);
            return Ok(Some(result));
        }

        Ok(None)
    }

    /// Purge timed-out assemblies. Returns count of purged objects.
    pub fn purge_expired(&mut self, current_time: u64) -> usize {
        let cutoff = current_time.saturating_sub(self.timeout);
        let before = self.assemblies.len();
        self.assemblies.retain(|_, a| a.started_at >= cutoff);
        before - self.assemblies.len()
    }

    /// Number of in-progress assemblies.
    pub fn pending_count(&self) -> usize {
        self.assemblies.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fragment(obj_hash: [u8; 32], index: u32, total: u32, data: &[u8]) -> GossipFragment {
        GossipFragment {
            object_hash: obj_hash,
            fragment_index: index,
            fragment_total: total,
            payload: data.to_vec(),
        }
    }

    #[test]
    fn test_single_fragment() {
        let mut asm = FragmentAssembler::new(None);
        let frag = make_fragment([0xAA; 32], 0, 1, b"hello");
        let result = asm.add_fragment(frag, 100).unwrap();
        assert_eq!(result, Some(b"hello".to_vec()));
    }

    #[test]
    fn test_multi_fragment_reassembly() {
        let mut asm = FragmentAssembler::new(None);
        let hash = [0xBB; 32];
        asm.add_fragment(make_fragment(hash, 0, 3, b"aaa"), 100)
            .unwrap();
        asm.add_fragment(make_fragment(hash, 2, 3, b"ccc"), 100)
            .unwrap();
        let result = asm
            .add_fragment(make_fragment(hash, 1, 3, b"bbb"), 100)
            .unwrap();
        assert_eq!(result, Some(b"aaabbbccc".to_vec()));
    }

    #[test]
    fn test_fragment_index_out_of_range() {
        let mut asm = FragmentAssembler::new(None);
        let frag = make_fragment([0xCC; 32], 5, 3, b"bad");
        assert!(asm.add_fragment(frag, 100).is_err());
    }

    #[test]
    fn test_fragment_total_mismatch() {
        let mut asm = FragmentAssembler::new(None);
        let hash = [0xDD; 32];
        asm.add_fragment(make_fragment(hash, 0, 3, b"a"), 100)
            .unwrap();
        let result = asm.add_fragment(make_fragment(hash, 1, 2, b"b"), 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_purge_expired() {
        let mut asm = FragmentAssembler::new(Some(50));
        let hash = [0xEE; 32];
        asm.add_fragment(make_fragment(hash, 0, 2, b"a"), 100)
            .unwrap();
        assert_eq!(asm.pending_count(), 1);
        let purged = asm.purge_expired(200);
        assert_eq!(purged, 1);
        assert_eq!(asm.pending_count(), 0);
    }
}
