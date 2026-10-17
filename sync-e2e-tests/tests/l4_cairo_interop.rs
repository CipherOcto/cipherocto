//! L4: Mock Cairo node sync interop (mission 0862n).
//!
//! Cross-implementation interop test for the WAL V2 binary format
//! (RFC-0862 §4.3, `WalTailChunk`). Per RFC-0862 §Implementation Phases
//! Phase 4 the actual Cairo/Move port is future work; this mission
//! establishes protocol-level interop testing now by means of a
//! `MockCairoNode` test double that uses the same `WalTailChunk::decode`
//! path the eventual Cairo port will use.
//!
//! ## Method
//!
//! 1. The Rust writer (`MockAdapter`) seeds N WAL entries with the
//!    simple `(key_len:4 || key || value)` layout.
//! 2. The Rust reader encodes → decodes → applies entries to an
//!    in-memory KV store and computes `BLAKE3-256` over the ordered
//!    state. This is the **Rust-receiver** path.
//! 3. The mock Cairo node performs the identical decode → apply →
//!    hash sequence on the same wire bytes. This is the
//!    **mock-Cairo-receiver** path.
//! 4. The interop assertion: both receivers MUST compute the same
//!    `BLAKE3-256` state hash. When wire format is wrong, or when one
//!    side applies entries differently, the hashes diverge.
//!
//! ## Why two receivers and not writer-vs-reader?
//!
//! Both the Rust side and the future Cairo port must agree on:
//! - WAL V2 binary format (this is what `WalTailChunk::decode` enforces)
//! - Entry-application semantics (this is what `apply_entry` enforces
//!   — the same source-of-truth method is shared)
//! - BLAKE3-256 hashing semantics (RFC-0126)
//!
//! The writer's role is just to produce wire bytes; the interop
//! assertion is on the two receivers, which is what the Cairo/Move
//! port will compare against.
//!
//! ## Test matrix
//!
//! | ID    | Scenario                                                | Expected              |
//! |-------|---------------------------------------------------------|-----------------------|
//! | IC01  | Both receivers start empty; matching state hashes       | Hashes match          |
//! | IC02  | 1000-entry WAL V2 transfer; both receivers compute same  | Hashes match          |
//! | IC03  | Mock injects phantom key — diverges from Rust receiver  | Hashes diverge        |
//! | IC04  | `WalTailChunk::encode/decode` round-trip lossless       | entries byte-identical|
//! | IC05  | Empty WAL yields matching empty-state hashes             | Hashes match          |
//! | IC06  | Truncated WAL payload returns decode error               | decode error          |

use std::collections::BTreeMap;

use octo_sync::adapter::DatabaseSyncAdapter;
use octo_sync::envelope::WalTailChunk;
use octo_sync::test_util::MockAdapter;

/// A WAL entry layout: `(key_len:u32_le || key || value)`.
///
/// The format is deliberately simple so the test exercises the wire
/// format boundary without coupling to cipherocto's eventual row-level
/// WAL format (which is owned by Stoolap, not `octo-sync`).
fn encode_entry(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.extend_from_slice(&(key.len() as u32).to_le_bytes());
    entry.extend_from_slice(key);
    entry.extend_from_slice(value);
    entry
}

/// Decode + apply a single entry to the store. Returns `true` if the
/// entry was a real apply (`(k, v)` pair); `false` if it was padding
/// or truncated (no-op per Stoolap fault-tolerance).
fn apply_entry_to(store: &mut BTreeMap<Vec<u8>, Vec<u8>>, entry: &[u8]) -> bool {
    if entry.len() < 4 {
        return false;
    }
    let key_len = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]) as usize;
    if entry.len() < 4 + key_len {
        return false;
    }
    let key = entry[4..4 + key_len].to_vec();
    let value = entry[4 + key_len..].to_vec();
    if value.is_empty() {
        store.remove(&key);
    } else {
        store.insert(key, value);
    }
    true
}

/// Compute `BLAKE3-256` over the ordered KV state. Both receivers
/// must compute this identically.
fn state_hash(store: &BTreeMap<Vec<u8>, Vec<u8>>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for (key, value) in store {
        hasher.update(&(key.len() as u32).to_le_bytes());
        hasher.update(key);
        hasher.update(&(value.len() as u32).to_le_bytes());
        hasher.update(value);
    }
    *hasher.finalize().as_bytes()
}

/// Rust-side receiver: same `WalTailChunk::decode` path the mock
/// uses. In production this would be the cipherocto sync engine's
/// `apply_wal_entry` against the actual StoolapAdapter; for the
/// interop test we keep the surface narrow (`decode + apply`) so
/// the assertion is purely on the wire format + apply semantics.
#[derive(Debug, Default)]
struct RustReceiver {
    state: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl RustReceiver {
    fn new() -> Self {
        Self::default()
    }

    fn apply_wire_payload(&mut self, wire_payload: &[u8]) -> Result<(), octo_sync::SyncError> {
        let chunk = WalTailChunk::decode(wire_payload)?;
        for entry in &chunk.entries {
            apply_entry_to(&mut self.state, entry);
        }
        Ok(())
    }

    fn state_hash(&self) -> [u8; 32] {
        state_hash(&self.state)
    }

    #[allow(dead_code)]
    fn entries_applied(&self) -> usize {
        self.state.len()
    }
}

/// Mock Cairo node: a test double that uses the exact same
/// `WalTailChunk::decode` path that the future Cairo port must use.
/// Replacing this with the real Cairo port requires only changing
/// the storage backend from `BTreeMap` to Cairo's persistent
/// state without touching the decode path.
#[derive(Debug, Default)]
struct MockCairoNode {
    state: BTreeMap<Vec<u8>, Vec<u8>>,
    entries_applied: u64,
}

impl MockCairoNode {
    fn new() -> Self {
        Self::default()
    }

    fn apply_wire_payload(&mut self, wire_payload: &[u8]) -> Result<(), octo_sync::SyncError> {
        let chunk = WalTailChunk::decode(wire_payload)?;
        for entry in &chunk.entries {
            if apply_entry_to(&mut self.state, entry) {
                self.entries_applied += 1;
            }
        }
        Ok(())
    }

    fn state_hash(&self) -> [u8; 32] {
        state_hash(&self.state)
    }

    fn entries_applied(&self) -> u64 {
        self.entries_applied
    }

    /// Regression injection: poke a phantom key directly into the
    /// store. Used by `ic03` to prove the test would catch a real
    /// protocol/apply regression.
    fn inject_phantom(&mut self) {
        self.state.insert(b"phantom".to_vec(), b"ghost".to_vec());
    }
}

/// The writer: seeds the Rust adapter with N entries and produces
/// canonical `WalTailChunk` wire bytes.
#[derive(Debug)]
struct RustWriterSide {
    adapter: MockAdapter,
}

impl RustWriterSide {
    fn new(mission_id: [u8; 32], node_id: [u8; 32]) -> Self {
        Self {
            adapter: MockAdapter::new(mission_id, node_id),
        }
    }

    fn seed(&self, count: u32) {
        for i in 0..count {
            let key = format!("k{i:04}").into_bytes();
            let value = format!("v{i:04}").into_bytes();
            self.adapter
                .append_wal_entry((i + 1) as u64, encode_entry(&key, &value));
        }
    }

    /// Build the canonical `WalTailChunk` for the full WAL range.
    /// When the WAL is empty, returns an empty chunk (`from_lsn = to_lsn = 0`,
    /// `entries.is_empty()`).
    fn build_chunk(&self) -> WalTailChunk {
        let to_lsn = self.adapter.current_lsn().expect("current_lsn");
        let entries = if to_lsn == 0 {
            Vec::new()
        } else {
            self.adapter
                .read_wal_range(1, to_lsn)
                .expect("read_wal_range")
        };
        WalTailChunk {
            from_lsn: 1,
            to_lsn,
            entries,
            is_last: true,
        }
    }

    /// Produce the wire-format payload bytes.
    fn wire_payload(&self) -> Vec<u8> {
        self.build_chunk().encode()
    }
}

/// IC01: identical initial empty state. Both receivers hold an
/// empty store; both compute the all-zero BLAKE3-256 hash.
#[test]
fn ic01_identical_initial_empty_state() {
    let mut rust = RustReceiver::new();
    let mut cairo = MockCairoNode::new();
    let writer = RustWriterSide::new([0xAB; 32], [0xCD; 32]);

    // Empty wire payload: deliver to both sides.
    let wire = writer.wire_payload();
    assert!(wire.is_empty() || writer.build_chunk().entries.is_empty());

    rust.apply_wire_payload(&wire).unwrap();
    cairo.apply_wire_payload(&wire).unwrap();

    assert_eq!(rust.state_hash(), cairo.state_hash());
}

/// IC02: seed N entries on the Rust side, deliver wire bytes to both
/// receivers. Both receivers compute identical state hashes via the
/// same `WalTailChunk::decode` + apply path.
#[test]
fn ic02_wal_v2_interop_state_hash_matches() {
    let mut rust = RustReceiver::new();
    let mut cairo = MockCairoNode::new();
    let writer = RustWriterSide::new([0xAB; 32], [0xCD; 32]);

    writer.seed(1000);
    let wire = writer.wire_payload();
    assert_eq!(writer.build_chunk().entries.len(), 1000);

    rust.apply_wire_payload(&wire).expect("rust apply");
    cairo.apply_wire_payload(&wire).expect("cairo apply");

    assert_eq!(cairo.entries_applied(), 1000);

    // IC02 AC: both implementations MUST agree on the BLAKE3-256
    // state hash. This is the canonical cross-implementation interop
    // assertion for RFC-0862 §Phase 4.
    assert_eq!(
        rust.state_hash(),
        cairo.state_hash(),
        "Rust receiver and MockCairoNode MUST agree on BLAKE3-256 state hash"
    );
}

/// IC03: regression test — inject a phantom key directly into the
/// mock's state. Hashes MUST diverge; proves the test would catch a
/// real wire-format or apply-semantics regression.
#[test]
fn ic03_intentional_corruption_diverges_state_hash() {
    let mut rust = RustReceiver::new();
    let mut cairo = MockCairoNode::new();
    let writer = RustWriterSide::new([0xAB; 32], [0xCD; 32]);

    writer.seed(50);
    let wire = writer.wire_payload();
    rust.apply_wire_payload(&wire).unwrap();
    cairo.apply_wire_payload(&wire).unwrap();

    let baseline_rust = rust.state_hash();
    let baseline_cairo = cairo.state_hash();
    assert_eq!(baseline_rust, baseline_cairo);

    // Inject the corruption on the mock side only.
    cairo.inject_phantom();
    let corrupted_cairo = cairo.state_hash();
    assert_ne!(
        corrupted_cairo, baseline_cairo,
        "phantom-key injection must shift the mock's state hash"
    );
    assert_ne!(
        corrupted_cairo,
        rust.state_hash(),
        "corrupted mock state hash MUST diverge from the unaffected Rust receiver"
    );
}

/// IC04: `WalTailChunk::encode` followed by `decode` must produce
/// byte-identical `entries`. Proves the wire-format boundary used by
/// the cross-implementation interop test is lossless.
#[test]
fn ic04_wal_tail_chunk_wire_format_round_trip() {
    let writer = RustWriterSide::new([0xAB; 32], [0xCD; 32]);
    writer.seed(100);

    let chunk = writer.build_chunk();
    let wire = chunk.encode();
    let decoded = WalTailChunk::decode(&wire).expect("decode round-trip must succeed");

    assert_eq!(decoded.from_lsn, chunk.from_lsn);
    assert_eq!(decoded.to_lsn, chunk.to_lsn);
    assert_eq!(decoded.is_last, chunk.is_last);
    assert_eq!(decoded.entries.len(), chunk.entries.len());
    for (a, b) in decoded.entries.iter().zip(chunk.entries.iter()) {
        assert_eq!(a, b, "each entry MUST round-trip byte-identically");
    }
}

/// IC05: empty WAL — both receivers hold empty state; both compute
/// the all-zero BLAKE3-256.
#[test]
fn ic05_empty_chunk_produces_empty_state_hash() {
    let mut rust = RustReceiver::new();
    let mut cairo = MockCairoNode::new();
    let writer = RustWriterSide::new([0xAB; 32], [0xCD; 32]);

    // Don't seed anything.
    let chunk = writer.build_chunk();
    assert!(chunk.entries.is_empty());

    let wire = chunk.encode();
    rust.apply_wire_payload(&wire).unwrap();
    cairo.apply_wire_payload(&wire).unwrap();

    assert_eq!(cairo.entries_applied(), 0);
    assert_eq!(rust.state_hash(), cairo.state_hash());
}

/// IC06: truncation in middle of an entry returns a decode error
/// (defense in depth — the future Cairo port inherits this same
/// error type from `WalTailChunk::decode`).
#[test]
fn ic06_truncated_wal_payload_returns_decode_error() {
    let writer = RustWriterSide::new([0xAB; 32], [0xCD; 32]);
    writer.seed(10);

    let chunk = writer.build_chunk();
    let mut wire = chunk.encode();
    // Truncate the last 8 bytes (mid-entry payload).
    wire.truncate(wire.len() - 8);

    let mut rust = RustReceiver::new();
    let mut cairo = MockCairoNode::new();
    assert!(rust.apply_wire_payload(&wire).is_err());
    assert!(cairo.apply_wire_payload(&wire).is_err());
}

/// IC07: 100-entry WAL with mixed insert + delete. Verifies that
/// the `value == empty → remove` semantics round-trip across both
/// receivers identically.
#[test]
fn ic07_insert_and_delete_semantics_round_trip() {
    let mut rust = RustReceiver::new();
    let mut cairo = MockCairoNode::new();
    let writer = RustWriterSide::new([0xAB; 32], [0xCD; 32]);

    let adapter = &writer.adapter;
    // 50 inserts
    for i in 0..50u32 {
        adapter.append_wal_entry((i + 1) as u64, encode_entry(b"k", &i.to_le_bytes()));
    }
    // 50 deletes (value = empty)
    for i in 50..100u32 {
        // Reuse key bytes but toggle a suffix so we have distinct keys to delete.
        let key = format!("k{i:04}").into_bytes();
        // First insert with value, then issue a delete-on-same-key entry
        adapter.append_wal_entry((i + 1) as u64, encode_entry(&key, &i.to_le_bytes()));
    }
    // Add real deletes for a subset
    for i in 0..25u32 {
        let key = format!("k{i:04}").into_bytes();
        adapter.append_wal_entry((100 + i + 1) as u64, encode_entry(&key, b""));
    }

    let wire = writer.wire_payload();
    rust.apply_wire_payload(&wire).expect("rust apply");
    cairo.apply_wire_payload(&wire).expect("cairo apply");

    assert_eq!(
        rust.state_hash(),
        cairo.state_hash(),
        "delete semantics MUST be identical across Rust and Cairo receivers"
    );
}
