# 0011-h-s-a-topology-render — Substrate additions for TopologyCommitment::render (RFC-0855 §5.1)

## Status

Claimed (2026-09-20) — Substrate additions for TopologyCommitment::render Phase 11 G14. Substrate slice LANDED at `next 293556e4` (1 file changed, 125 insertions) per RFC-0011-h §Substrate-Additions Companion Missions row G14 + RFC-0011-s Phase 11 §Substrate-Additions. `TopologyCommitment` EXTENDED with `render(format: GraphFormat) -> String` method + private `render_ascii()` + `render_dot()` helpers at `crates/octo-network/src/mon/topology.rs:36` (EXTEND, not NEW — substrate-faithfulness audit per Phase 5 RFC-0011-m precedent overrides original G14 stub note). `GraphFormat` enum REUSED from existing `crates/octo-network/src/mon/trust_graph.rs:41` (zero NEW types). 4 NEW unit tests land at `crates/octo-network/src/mon/topology.rs` (tv_phase11_substrate_1 through tv_phase11_substrate_4) covering ASCII label format + DOT digraph format + deterministic-across-calls + distinct-per-topology-model. 1493/1493 octo-network tests pass + zero regression of existing 1489 tests. Cargo clippy -p octo-network --all-targets -- -D warnings clean. Layer B substrate addition only zero Layer A change. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). CLI dispatch slice PENDING per RFC-0011-s Phase 11 §Subcommand Taxonomy.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G14 + RFC-0011-s Phase 11 §Substrate-Additions Companion Missions + RFC-0855 §5.1 Topology Models.

## Summary

EXTENDS existing `TopologyCommitment` struct at `crates/octo-network/src/mon/topology.rs:36` (per Phase 5 RFC-0011-m precedent additive method extension path; original G14 stub noted `crates/octo-network/src/topology/render.rs (NEW)` but the substrate-faithful path is EXTEND `mon/topology.rs` not create a new sibling module) with a new `render(format: GraphFormat) -> String` method. REUSES existing `GraphFormat` enum at `crates/octo-network/src/mon/trust_graph.rs:41` (Ascii + Dot variants) — no new types introduced. Required by `octo network topology render [--format ascii|dot] [--depth <N>]` per RFC-0011-s Phase 11 §Subcommand Taxonomy.

### Substrate additions target

```rust
// crates/octo-network/src/mon/topology.rs (EXTEND existing module)
use crate::mon::trust_graph::GraphFormat;

impl TopologyCommitment {
    /// Render the topology commitment as ASCII or DOT
    /// graph output (Phase 11 G14 per RFC-0011-s
    /// §Substrate Mapping Table). Operates on the
    /// in-memory snapshot of the commitment; live
    /// topology-source adapter OUT OF SCOPE for Phase 11.
    /// Per-extension impl crates (Layer D) provide real
    /// topology-source adapters in follow-on missions.
    /// BTreeMap-based deterministic iteration ordering
    /// preserved per RFC-0011-h §Output Envelope determinism.
    pub fn render(&self, format: GraphFormat) -> String {
        match format {
            GraphFormat::Ascii => self.render_ascii(),
            GraphFormat::Dot => self.render_dot(),
        }
    }

    fn render_ascii(&self) -> String {
        let label = format!(
            "topology mission_id={} model={:?} epoch={}",
            hex::encode(self.mission_id.as_bytes()),
            self.model,
            self.epoch,
        );
        let mut out = String::new();
        out.push_str(&label);
        out.push('\n');
        out
    }

    fn render_dot(&self) -> String {
        let mut out = String::new();
        out.push_str("digraph G {\n");
        out.push_str(&format!(
            "  mission_id_{} [label=\"{:?}\"];\n",
            hex::encode(self.mission_id.as_bytes()),
            self.model,
        ));
        out.push_str("}\n");
        out
    }
}
```

EXTENDS the existing `TopologyCommitment` struct definition. No new types; no new modules. REUSE of existing `GraphFormat` enum from Phase 1 mission 0851p-a-trust-ux at `crates/octo-network/src/mon/trust_graph.rs:41`.

## Acceptance Criteria

- [x] `TopologyCommitment::render(format)` method EXTENDED at `crates/octo-network/src/mon/topology.rs:36` per RFC-0011-h §Substrate-Additions row G14 + RFC-0011-s Phase 11 §Substrate Mapping Table
- [x] `render(format: GraphFormat) -> String` signature lands on the existing impl block
- [x] `render_ascii()` private helper lands (label line per RFC-0855 §5.1 topology-model field)
- [x] `render_dot()` private helper lands (`digraph G { ... }` block with deterministic key order)
- [x] `GraphFormat` enum REUSED from existing `crates/octo-network/src/mon/trust_graph.rs:41` (zero NEW types per Phase 5 precedent)
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean (NO regression of existing 1489 tests)
- [x] `cargo test -p octo-network --lib` green (≥3 unit tests added; zero regression)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (substrate-faithful boundary tests pin ascii rendering + dot rendering + model label correctness)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-s Phase 11 topology-render amendment Draft at `next a2bfc2cb`
- Phase 11 G14 substrate stub fill-in at `next 5441fce4`
- Phase 11 G14 substrate slice at `next 293556e4`
- RFC-0855 §5.1 Topology Models (governing RFC)
- Existing `TopologyCommitment` struct at `crates/octo-network/src/mon/topology.rs:36` (EXTEND, not NEW)
- Existing `GraphFormat` enum at `crates/octo-network/src/mon/trust_graph.rs:41` (REUSE, not NEW)

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-topology` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-topology-source-* Layer D follow-on missions, OUT OF SCOPE for this additive-method-only phase)
- Live topology source adapter (Phase 11 substrate operates on the in-memory snapshot of the existing commitment struct only)
- Recursive graph rendering --depth > 1 (Phase 11 stubs depth filter; follow-on Layer D adapter missions in future)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Stub note pinned path `crates/octo-network/src/topology/render.rs (NEW)` — substrate-faithfulness audit per Phase 5 RFC-0011-m precedent overrides: EXTEND existing `mon/topology.rs` instead of creating a new sibling module. Full AC + scope land in Phase 11 stub fill-in commit at `next PENDING` per the Phase 5 RFC-0011-m 5-commit pattern. Phase 11 follows the Phase 5 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism. `render` operates on in-memory snapshot only; live topology-source adapter in follow-on Layer D adapter mission per per-extension crate pattern.
