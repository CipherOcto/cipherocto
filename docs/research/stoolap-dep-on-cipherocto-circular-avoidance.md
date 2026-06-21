# Reversing the Stoolap → CipherOcto Dependency: Avoiding Circular Dependencies

**Status:** Draft (awaiting adversarial review)
**Date:** 2026-06-21
**Author:** @cipherocto (research)
**Trigger:** RFC-0862 (Stoolap Data Sync Protocol) is Accepted; implementation requires stoolap to consume cipherocto network APIs.

## 1. Problem Statement

The `Stoolap Data Sync Protocol` (RFC-0862, Accepted 2026-06-20) defines a wire-level sub-protocol for synchronizing two Stoolap fork instances over the CipherOcto overlay network. To implement this protocol, the Stoolap fork (`/home/mmacedoeu/_w/databases/stoolap`, a separate repository) must depend on the CipherOcto network stack — specifically on `octo-network` (DOT, DGP, OCrypt, ORR) and likely the new `octo-sync` crate.

**However, the dependency graph is already partially cyclic at the git/repo level:**

```
cipherocto workspace  ──depends on──>  Stoolap fork  ──depends on──>  octo-determin
       (workspace)                    (separate repo)                  (separate workspace)
```

That is: the `cipherocto` Cargo workspace (in `/home/mmacedoeu/_w/ai/cipherocto/`) depends on the `stoolap` crate (sourced from `https://github.com/CipherOcto/stoolap?branch=feat/blockchain-sql`); the `stoolap` fork in turn depends on `octo-determin` (sourced from `https://github.com/CipherOcto/cipherocto`).

This cycle already works — but only because:

1. `octo-determin` is a **separate** Cargo workspace with a minimal dep footprint (only DFP primitives + `sha2`/`hex` for encoding; no async runtime, no AEAD or signatures).
2. Both packages are versioned and published **independently** (no shared feature graph at the workspace level).

The proposed change — adding a dependency from `stoolap` onto `octo-network` (or `octo-sync`) — threatens this arrangement. Unlike `octo-determin`, the cipherocto network stack is large (it depends on `tokio` with the `sync`, `rt-multi-thread`, `macros` features declared by `octo-network` itself — the workspace-level `full` features are pulled in via feature unification when the network is built inside the cipherocto workspace, but the declared `octo-network` dep set is narrower), `blake3`, `chacha20poly1305`, `ed25519-dalek`, `x25519-dalek`, `async-trait`, `libloading`, optional `wasmtime`, plus 23 platform-adapter crates that depend on `octo-network` (NOT the other way around — the adapters do not transitively come in with an `octo-network` dep). Importing this graph into `stoolap` would:

- Add a heavy async runtime dependency to an otherwise-`std`-only embedded database.
- Couple the two projects' release cycles.
- Risk Cargo resolver errors if both projects are ever added to the same workspace.
- Duplicate the cycle at the workspace level (Cargo's `resolver = "2"` allows *some* cyclic deps with feature unification, but only in narrow circumstances — see [the Cargo reference](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification) for the exact rules).

## 2. Current State

### 2.1 Stoolap fork (`/home/mmacedoeu/_w/databases/stoolap`)

- `Cargo.toml:1-4`: package name `stoolap`, version `0.3.2`, edition `2021` (license is at line 8: `Apache-2.0`).
- `Cargo.toml:55`: `octo-determin = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }` — only cipherocto dep.
- `src/` has 18 entries: `api`, `bin`, `common`, `consensus`, `core`, `determ`, `execution`, `executor`, `functions`, `lib.rs`, `optimizer`, `parser`, `pubsub`, `rollup`, `storage`, `trie`, `wasm.rs`, `zk` (16 subdirectories + 2 single files). All are **first-party** to the stoolap fork; there is no network sub-tree.
- No current `tokio` dep. The fork is built on a synchronous, single-threaded `std` core. The fork's `Cargo.toml` is a single-package manifest (no `[workspace]` table); it has its own `[profile.*]` and `[features]` sections that may conflict under workspace feature unification.

### 2.2 Cipherocto workspace (`/home/mmacedoeu/_w/ai/cipherocto`)

- `Cargo.toml:19-24`: `exclude = [ "determin", "crates/quota-router-pyo3", ... ]` — `determin` is its own workspace.
- `Cargo.toml:25`: `resolver = "2"` — feature-unification v2, which allows *some* cyclic members.
- The workspace has 36 active member crates; the sync-relevant ones are:
  - `crates/octo-network/` — DOT, DGP, OCrypt, ORR, DRS, DOM, MON, PoRelay (plus dc, dps, gdp, gossip, common, lib.rs as sub-modules). Depends on `tokio` (`sync`, `rt-multi-thread`, `macros`), `blake3`, `chacha20poly1305`, `ed25519-dalek`, `x25519-dalek`, `libloading`, `wasmtime` (optional), `rand`. (Note: the cipherocto **workspace** deps specify `tokio = { features = ["full"] }` for some crates; feature unification in the workspace would pull in `full` for the workspace build, but `octo-network` itself declares the narrower subset.)
  - `determin/` — pure DFP library, no async. This is the **existing** leaf crate that both projects share.
  - `crates/octo-network/src/dot/`, `dgp/`, `ocrypt/` — sub-modules implementing the relevant RFCs (RFC-0850, RFC-0852, RFC-0853).
  - 23 platform-adapter crates (`octo-adapter-telegram`, `octo-adapter-whatsapp`, etc.) — **NOT** transitive deps of `octo-network`; they depend on `octo-network`, not the other way around. They would not come along with an `octo-network` dep.
- `Cargo.lock:8074-8076`: workspace pins `stoolap` to `git+https://github.com/CipherOcto/stoolap?branch=feat%2Fblockchain-sql#0301bd6bab95ce6404e2db4cbb8b3382dc463666` (line 8074: package name, line 8076: source URL).

### 2.3 `octo-determin` (`/home/mmacedoeu/_w/ai/cipherocto/determin/`)

- Standalone workspace, not a member of `cipherocto`. Own `Cargo.toml:1`: `[workspace] members = ["cli"]`.
- Minimal dep footprint (only DFP primitives + `sha2`/`hex` for encoding; no async runtime, no AEAD or signatures). This is the **existing** leaf crate that both projects share.
- `stoolap` fork depends on this via `git = "https://github.com/CipherOcto/cipherocto", branch = "next"`. The branch tracks cipherocto `next`, so the dep follows cipherocto's trunk.
- Because the workspace is excluded from `cipherocto`'s workspace, the cycle is broken at the Cargo workspace graph level — each project sees the other as an **external** crate, versioned independently.

### 2.4 The current cycle (already working)

```
   cipherocto workspace                Stoolap fork
        │                                  │
        │  [dependencies]                    │
        ├──────────────────────────────────►│  (stoolap, git source)
        │                                  │
        │                                  │  [dependencies]
        │                                  ├──────────────►  octo-determin
        │                                  │                (git source, branch=next)
        │                                  │                     │
        │◄─────────────────────────────────┼─────────────────────┘
           (octo-determin is part of cipherocto org, but
            excluded from cipherocto workspace → not a Cargo
            cycle, just a git/source-level shared crate)
```

The current cycle is **contained** because `octo-determin` is its own workspace, versioned independently, and excluded from `cipherocto`'s workspace. Adding `octo-network` (a member of `cipherocto`'s workspace) as a `stoolap` dep would break this containment.

## 3. Approaches Considered

### Approach A — "Mirror `octo-determin`" (Extract a leaf crate / standalone workspace)

**Idea:** Create a new standalone workspace `octo-sync` (or `octo-wire`) at `/home/mmacedoeu/_w/ai/cipherocto/octo-sync/`, similar to `octo-determin`. It contains only the wire-protocol primitives needed by both projects: envelope payload discriminators, DCS encoding, BLAKE3 wrappers, Merkle segment tree, OCrypt HKDF context `"sync:v1"`, replay cache, snapshot segment types.

Both the `cipherocto` workspace and the `stoolap` fork depend on `octo-sync` via git. The cipherocto workspace excludes `octo-sync` from its members. This breaks the cycle at the Cargo workspace graph level.

**Cargo dep layout:**

```toml
# cipherocto/crates/octo-network/Cargo.toml
[dependencies]
octo-sync = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }

# cipherocto/Cargo.toml (workspace)
[workspace]
exclude = ["determin", "octo-sync", ...]   # exclude from workspace members

# stoolap fork/Cargo.toml
[dependencies]
octo-determin = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }
octo-sync     = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }
```

**Pros:**

- Mirrors the existing, working pattern (`octo-determin`).
- Both projects treat `octo-sync` as an external, versioned dep — no shared feature graph.
- Cargo's resolver v2 is not required; resolver v1 works.
- The `cipherocto` workspace remains unaffected — `octo-network` simply acquires a new external dep.
- Version skew is explicit (each repo pins what it needs).

**Cons:**

- Another workspace to maintain.
- Cross-workspace change coordination: when the wire format evolves, both `octo-sync` consumers must be updated.
- Branch tracking: `stoolap` would track `cipherocto/next` for `octo-sync` updates (currently does this for `octo-determin`); a `stoolap` release branch may need to pin a specific commit.
- Build graph: two-step pull (`stoolap` → `octo-sync` → maybe `octo-determin`).

**Verdict:** **Recommended** (with caveats). The pattern is proven. Cost is operational, not architectural.

---

### Approach B — Merge into a single workspace (Monorepo)

**Idea:** Restructure so that `stoolap` fork is a member of the `cipherocto` Cargo workspace, or vice versa. The two projects become a single Cargo workspace; intra-workspace deps are allowed (with feature unification via resolver v2).

**Cargo dep layout (one option):**

```
cipherocto/  (workspace, single)
  crates/
    octo-network/
    octo-determin/
    octo-sync/      # new
    ...
  vendors/
    stoolap/        # git submodule or subtree, NOT a separate workspace
```

```toml
# cipherocto/Cargo.toml
[workspace]
members = ["crates/*", "vendors/stoolap"]
resolver = "2"   # already set
```

**Pros:**

- Single Cargo build graph; no cross-workspace coordination.
- Cycle (if any) handled at compile time by resolver v2.
- One CI, one set of release artifacts.

**Cons:**

- **Massive refactor.** Stoolap fork is a separate repository with its own version, CI, governance, and release cadence. Merging it into the cipherocto monorepo requires:
  - Migrating the fork's commit history (e.g., `git subtree add`).
  - Resolving any Cargo workspace conflicts (the fork has its own `[profile.*]` tables and `[features]` section that may conflict under feature unification).
  - Negotiating governance: who owns the release? Which LICENSE applies? (stoolap fork is `Apache-2.0`; cipherocto is `MIT OR Apache-2.0`.)
  - Updating CI/CD, version tagging, and 3rd-party consumers of the fork.
  - The fork has its own downstream users (per `stoolap-research.md`, multiple "agent memory" projects depend on it). Breaking the repo would be a BC issue for them.
  - The fork's `Cargo.toml` is a single-package manifest (no `[workspace]` table) but has its own `[profile.*]` tables and `[features]` section that may conflict under feature unification.
  - Cargo's `resolver = "2"` allows feature unification, but only in narrow conditions; not all cyclic dep patterns are supported.

**Verdict:** **Not recommended** for this use case. The refactor cost is enormous and the benefit (eliminating a single workspace-level dep cycle) is small. Reserve for a future, top-down consolidation.

---

### Approach C — Publish to crates.io

**Idea:** Publish `octo-sync` (or a stripped-down `octo-network` feature) to crates.io. `stoolap` depends on it via `version = "x.y.z"`. No git dep.

**Cargo dep layout:**

```toml
# stoolap fork/Cargo.toml
[dependencies]
octo-sync = "0.1"
octo-determin = "0.1"
```

```toml
# cipherocto/crates/octo-network/Cargo.toml
[dependencies]
octo-sync = "0.1"  # same version, from crates.io
```

**Pros:**

- Standard Cargo pattern; no cycles, no git deps, no cross-workspace feature graph.
- Decoupled release cadence (semver is enforced).
- Smaller clone size (no git history).

**Cons:**

- Publishing is a one-way door; once `0.1.0` is on crates.io, breaking changes require `0.2.0`.
- cipherocto has a high release velocity (per recent git log: 7+ commits/day). A 6-week crates.io release cadence is too slow.
- Crates.io is a public, immutable registry; mistakes are recoverable only via `yank` + new version.
- Doesn't help with cipherocto's existing `git` dep on `stoolap fork`; the fork itself is not on crates.io (`repository = "https://github.com/stoolap/stoolap"` per `Cargo.toml:7-9`).

**Verdict:** **Partial fit.** Could work for **stable** sub-crates (e.g., `octo-determin` is a good candidate; a future `octo-wire-encoding` would be too). Not suitable for the rapidly-evolving network protocol.

---

### Approach D — Refactor: protocol-as-trait, no Cargo dep

**Idea:** Define a Rust trait `DatabaseSyncAdapter` in a new RFC (e.g., RFC-0863). The trait abstracts the operations that the sync protocol needs from the underlying database (read WAL range, apply WAL entry, write snapshot segment, etc.). `stoolap` provides a `DatabaseSyncAdapter` implementation; cipherocto's `octo-network` consumes it. **No Cargo dep is required** — only a trait definition.

**Cargo dep layout:** None. The dep is replaced by a trait bound.

```rust
// cipherocto/crates/octo-sync/src/adapter.rs
pub trait DatabaseSyncAdapter: Send + Sync + 'static {
    fn read_wal_range(&self, from_lsn: u64, to_lsn: u64) -> Result<Vec<WALEntry>>;
    fn apply_wal_entry(&self, entry: &WALEntry) -> Result<()>;
    fn read_snapshot_segment(&self, table_id: u32, segment_index: u32) -> Result<SnapshotSegment>;
    fn write_snapshot_segment(&self, table_id: u32, segment_index: u32, payload: &[u8]) -> Result<()>;
    fn current_lsn(&self) -> u64;
    fn last_ack_lsn(&self) -> u64;
    // ... more methods
}

// stoolap (new) crates/sync-adapter/src/lib.rs
impl DatabaseSyncAdapter for StoolapAdapter { ... }

// cipherocto (new) crates/octo-sync-bridge/src/lib.rs
pub struct StoolapSyncBridge<A: DatabaseSyncAdapter> { adapter: A, ... }
```

**Pros:**

- **No Cargo dep cycle at all.** This is the cleanest architectural solution.
- The trait is the *interface*; the Cargo dep is the *implementation*. The two are decoupled.
- Other databases (e.g., a future PostgreSQL adapter) can implement the trait and use the same cipherocto sync.
- Fully testable: cipherocto can test against a mock `DatabaseSyncAdapter` without stoolap.

**Cons:**

- Requires designing the trait boundary carefully; mistakes are expensive to refactor later.
- Adds a new RFC (RFC-0863 or similar) — one more artifact to maintain.
- `stoolap` no longer "uses cipherocto" in the Cargo sense; instead, the two are linked at the application integration level (e.g., via a binary crate that wires them together). This may make the deployment story slightly more complex.

**Verdict:** **Architecturally purest, but heavyweight.** The trait approach is the right *eventual* shape (per Separation of Concerns). It can be adopted incrementally: even if we use Approach A now, the `octo-sync` leaf crate should expose its API as a trait, not as a concrete struct, so that future databases can plug in.

---

### Approach E — No dep at all: cross-process wire protocol (stub)

**Idea:** Don't add a Cargo dep. Run the cipherocto sync process externally; let it speak the RFC-0862 wire protocol to the stoolap process over a network socket. The two are linked at the **network** level, not the Cargo level.

**Cargo dep layout:** None. The two communicate over the wire format already specified by RFC-0862.

**Pros:**

- **No coupling at all.** Either project can be replaced without touching the other.
- Aligns with the "RFC-0862 is a wire protocol" intent.

**Cons:**

- The wire format is designed for **two nodes over a network**, not for **two crates in the same process**. The current RFC-0862 wire format assumes adversarial environment, AEAD encryption, mission-binding, etc. Using it as the in-process boundary adds unnecessary overhead.
- The original RFC-0862 design is for a v1 **single-leader** deployment; running two processes adds operational complexity.
- Latency overhead (network round-trip) is fine for eventual consistency but wasteful for in-process sync.

**Verdict:** **Mismatch.** The wire format is the right boundary *between* nodes, not *within* a node. Approach A (or D) is correct for the in-process case.

---

### Approach F — `[patch.crates-io]` / git-redirect

**Idea:** Add a Cargo `[patch]` section to redirect a transitive dep. E.g., stoolap declares `[patch."https://github.com/CipherOcto/cipherocto"] octo-network = { path = "../cipherocto/crates/octo-network" }`.

**Pros:**

- No actual code change to cipherocto.

**Cons:**

- `[patch]` with `path` requires a local filesystem layout (e.g., stoolap fork must be cloned next to cipherocto). Doesn't work in distributed CI.
- The Cargo book explicitly warns: "Patches can only be used with the same semver-major version" — so this constrains how cipherocto and stoolap can evolve.
- Doesn't help if the dep is `octo-network` and the `cipherocto` workspace is also in the build (true in CI but not in stoolap's standalone build).

**Verdict:** **Workaround, not a solution.** Reject.

---

### Approach G — Cargo `[features]` and resolver-v2 cyclic support

**Idea:** Make `octo-network` an *optional* dep of `stoolap` behind a Cargo feature `sync`. The cipherocto workspace also activates `sync` only when the stoolap feature is needed. Cargo's `resolver = "2"` with `resolver-features = ["feature-allow-some-cycles"]` (planned) might allow this.

**Pros:**

- Acyclic in the default build; cyclic only when both features are active.

**Cons:**

- Cargo `resolver-features` is **not yet stable** as of 2026-06 (still in nightly / unstable). Using unstable features in both projects would block their stable release.
- Even with the feature, the cycle only works in the workspace build, not in the standalone stoolap fork build.
- Doesn't address the version-skew problem.

**Verdict:** **Not yet viable.** Reject for now; revisit when `feature-allow-some-cycles` stabilizes.

## 4. Comparison Matrix

| # | Approach | Cargo cycle? | Version-skew risk? | Refactor cost? | Long-term architectural fit? |
|---|----------|--------------|-------------------|----------------|-----------------------------|
| A | Extract `octo-sync` leaf workspace (mirror `octo-determin`) | No (cycle broken at workspace level) | Medium (git branch tracking) | Low (~1 PR per repo) | Good (follows existing pattern) |
| B | Merge into single monorepo | No (single workspace) | Low | **Very High** (fork migration, governance) | Best (one source of truth) |
| C | Publish to crates.io | No | Low (semver) | Medium (publishing workflow) | Partial (only for stable APIs) |
| D | Protocol-as-trait (no Cargo dep) | No | Low (trait is stable) | Medium (new RFC) | **Best** (separation of concerns) |
| E | Cross-process wire protocol | No (network only) | Low | Low | Mismatch (overkill for in-process) |
| F | `[patch]` redirect | No (path-based) | High (semver lock) | Low | Poor (workaround) |
| G | Resolver-v2 cyclic features | Conditional (nightly only) | Medium | Low | Not yet viable |

## 5. Recommended Approach (Hybrid: A + D)

**Recommendation:** Combine **Approach A (extract `octo-sync` as a leaf crate)** with **Approach D's trait boundary (have `octo-sync` expose a `DatabaseSyncAdapter` trait)**.

### 5.1 Phase 1 — Extract `octo-sync` (immediate, low-risk)

1. Create a new standalone workspace at `/home/mmacedoeu/_w/ai/cipherocto/octo-sync/`, with its own `Cargo.toml` declaring `[workspace] members = ["."]`.
2. Move the wire-protocol primitives from `cipherocto/crates/octo-network/src/` to `cipherocto/octo-sync/src/`:
   - Envelope payload discriminators (0xA0-0xC2) and their DCS encoding.
   - `SyncSummary`, `SyncSegment`, `WalTailChunk`, `NodeStatus`, `SyncNodeId`, `SyncPeerId` structs (from RFC-0862 §4.2).
   - `MerkleSegmentTree` (from mission 0862b).
   - `MissionKeyRing` and the `"sync:v1"` HKDF context (from mission 0862d, with the RFC-0853 amendment).
   - `ReplayCache` (in-memory + persistent from mission 0862e).
   - `SegmentIndexer` and the `create_snapshot_for_table` interface (from mission 0862c).
3. **Strip out** anything that depends on `tokio` (use `std`-only where possible, or gate async via a feature flag). The cipherocto workspace adds an internal adapter crate `octo-sync-bridge` that wraps `octo-sync` with the cipherocto async runtime.
4. Update the `cipherocto` workspace:
   - Add `octo-sync` to the workspace `exclude` list.
   - Update `crates/octo-network/Cargo.toml` to add `octo-sync = { path = "../../octo-sync" }` (internal workspace path; `octo-network` lives at `crates/octo-network/`, `octo-sync` lives at the repo root, so the relative path from one to the other requires two `..` levels).
   - Adjust the cipherocto workspace's `Cargo.lock` accordingly.
5. Update the `stoolap` fork's `Cargo.toml`:
   - Add `octo-sync = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }`.
   - Wrap it in a Cargo feature `sync` (default off; the existing in-process sync design from RFC-0862 only activates when `--features sync` is passed).
6. Verify: `cargo build -p stoolap` (no `sync` feature) succeeds; `cargo build -p stoolap --features sync` succeeds; `cargo build` in the cipherocto workspace succeeds.

### 5.2 Phase 2 — Trait boundary (deferred, after Phase 1 stabilizes)

1. In the new `octo-sync` crate, define a `DatabaseSyncAdapter` trait (per Approach D):
   ```rust
   pub trait DatabaseSyncAdapter: Send + Sync + 'static {
       fn read_wal_range(&self, from_lsn: u64, to_lsn: u64) -> Result<Vec<WALEntry>>;
       fn apply_wal_entry(&self, entry: &WALEntry) -> Result<()>;
       fn read_snapshot_segment(&self, table_id: u32, segment_index: u32) -> Result<SnapshotSegment>;
       fn write_snapshot_segment(&self, table_id: u32, segment_index: u32, payload: &[u8]) -> Result<()>;
       fn current_lsn(&self) -> u64;
   }
   ```
2. `stoolap` (when `sync` feature is enabled) provides a `StoolapAdapter` implementation.
3. cipherocto's `octo-network` consumes any `A: DatabaseSyncAdapter` (e.g., `stoolap`'s, or a future PostgreSQL adapter).
4. Document the trait in a new RFC (proposed: RFC-0863 "Sync Adapter Interface").

### 5.3 Phase 3 — Optional future work

- Once `octo-sync` has stabilized (3-6 months of releases), publish it to crates.io (Approach C) for downstream consumers who don't need the full cipherocto monorepo.
- Long-term, consider merging `stoolap` fork into the cipherocto monorepo (Approach B) — but only if governance and licensing align.

## 6. Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| `stoolap` fork's git dep on cipherocto `next` branch causes unexpected breakage | Medium | Pin a specific commit or tag in `stoolap/Cargo.toml` for each release; document the update procedure. |
| `octo-sync` needs to be a `std`-only library (no `tokio`) but the cipherocto `octo-network` is async-first | Medium | `octo-sync` exposes a sync API; the `octo-sync-bridge` adapter in cipherocto adds `tokio`. The trait in Phase 2 is `Send + Sync`, not `Future`-based. |
| The fork and cipherocto are versioned at different cadences | Medium | Use semver in `octo-sync`. Document a public API stability promise (e.g., "no breaking changes within 0.x"). |
| Cargo's `[patch]` could be misused | Low | Reject Approach F explicitly; do not add `[patch]` to either `Cargo.toml`. |
| RFC-0862 wire format evolves, breaking backward compatibility | Medium | Version the envelope discriminators (already designed with discriminator-based encoding, so adding new codes is non-breaking). Use a 2-bit version field in the envelope header (planned for the wire upgrade). |

## 7. Decision

**Proceed with Phase 1 (Approach A) immediately.** The pattern is proven, the cost is low, and the architectural risk is minimal. Phase 2 (Approach D's trait) is the long-term direction and can be adopted incrementally without breaking Phase 1.

**Next BLUEPRINT artifacts to create:**

- A new Use Case `docs/use-cases/octo-sync-leaf-crate.md` describing the `octo-sync` extraction.
- A new RFC `rfcs/draft/networking/0863-sync-adapter-trait.md` for the Phase 2 trait.
- Followed by missions for the `octo-sync` leaf crate (one mission per file) and the cipherocto-side `octo-sync-bridge`.

## 8. Cross-References

- RFC-0853 §1 (HKDF-BLAKE3), §6 (Mission Cryptography), §7 (Replay Protection), §12 (Key Rotation) — defines the cryptography and key-management primitives the new `octo-sync` crate must use.
- RFC-0850 (DOT) — defines the envelope wire format.
- RFC-0852 (DGP) §7 (anti-entropy Merkle summary) — the algorithm the `MerkleSegmentTree` in `octo-sync` implements.
- RFC-0855 (Mission Overlay Networks) — defines the mission-binding precondition.
- RFC-0862 (Stoolap Data Sync Protocol) — the wire protocol that motivates this research.
- Mission 0862-base + 0862a–0862i — the 10 missions that depend on the new `octo-sync` leaf crate.
- `docs/research/stoolap-data-sync-via-cipherocto-network.md` — the upstream research that this work implements.
- `docs/BLUEPRINT.md` §"Canonical Workflow" — the Research → Use Case → RFC → Mission pipeline that this document feeds.

---

**Review note:** This document is Draft. It must pass the BLUEPRINT Research Review Gate (minimum 2 maintainer reviewers) before promoting to Use Case.
