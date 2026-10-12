---
name: 0968-stoolap-reputation-backend-is-never-exercised-by-ci
description: 'octo-reputation declares default = [] and keeps the stoolap feature off by default. With the feature off, StoolapReputationStore methods return the stub error ChainRefInvalid("stoolap_backend_unimplemented:<name>") rather than doing any SQL, so the CI workspace run tests the stub. 69 integration test functions across two files are gated behind the same feature and never compile in CI. All 69 pass locally when the feature is enabled.'
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0968
---

# Mission `0968-stoolap-reputation-backend-is-never-exercised-by-ci`

## Status

Open. No workflow has been edited. The finding and the proposed step are
recorded here for a maintainer.

## The finding

`crates/octo-reputation/Cargo.toml` declares:

```toml
[features]
default = []
```

and documents what the empty default means:

> With `stoolap` enabled, every method is wired against real SQL. Without it
> (CI default build), the type remains constructible as a marker; methods
> return the historical stub `ChainRefInvalid("stoolap_backend_unimplemented:<name>")`
> errors so the `ReputationStore` trait is still nameable.

`ci.yml` runs the workspace under default features:

```
cargo test --verbose --workspace \
  --exclude whatsapp_chrome_reconnect_observer \
  --exclude whatsapp_chrome_driver \
  --exclude whatsapp_chrome_session_extract
```

So CI compiles and exercises the **stub** backend. The SQL implementation is
never called by CI at all.

## How much is gated

Two integration files carry a whole-file `#![cfg(feature = "stoolap")]`:

| File                                 | Test functions |
| ------------------------------------ | -------------- |
| `tests/stoolap_integration.rs`       | 48             |
| `tests/cross_backend_integration.rs` | 21             |
| **Total**                            | **69**         |

When the feature is off, each file compiles to an empty test binary. `ci.yml`
already knows this pattern — it says so about the `hex-ids` vectors and adds a
dedicated step for them.

## Measured

Running the feature locally:

```
cargo test -p octo-reputation --features stoolap
```

| Target                               | Tests   | Result       |
| ------------------------------------ | ------- | ------------ |
| lib                                  | 267     | ok           |
| `tests/canonical_blobs.rs`           | 9       | ok           |
| `tests/cross_backend_integration.rs` | 21      | ok           |
| `tests/stoolap_integration.rs`       | 48      | ok           |
| **Total**                            | **345** | **0 failed** |

The backend works. Nothing here suggests a defect — this is a coverage gap,
not a bug.

## The other feature-gated tests are correctly excluded

A census of test functions behind a whole-file feature gate:

| Feature         | Test functions | Assessment                                           |
| --------------- | -------------- | ---------------------------------------------------- |
| `real-network`  | 73             | Correctly excluded — needs live Telegram credentials |
| `stoolap`       | 69             | **Gap — needs no credentials, only a feature flag**  |
| `live-matrix`   | 14             | Correctly excluded — needs a live Matrix homeserver  |
| `live-whatsapp` | 3              | Correctly excluded — needs a live session            |
| `hex-ids`       | 3              | Already covered by a dedicated `ci.yml` step         |

`stoolap` is the only one of these that is excluded for a build-budget reason
rather than an external-dependency reason. It is the cheapest of the five to
recover.

## Proposed step

Mirroring the `hex-ids` step that already exists:

```yaml
- name: Test stoolap backend (octo-reputation)
  run: |
    if [ -f Cargo.toml ]; then
      cargo test -p octo-reputation --features stoolap;
    fi
```

## Why this was not applied

Two reasons, both substantive.

**It is workflow policy.** Earlier in this arc a workflow edit was made only
where the change was an unambiguous defect — a path filter that omitted a
directory the workflow already claimed to test. This is different in kind: it
adds CI work, and the right amount of CI work is an owner decision.

**The disk risk is documented and cannot be validated locally.** `ci.yml`
carries a long comment explaining that the `build-test` job has already run the
hosted runner out of disk twice:

> The last of those ran the runner out of disk twice in a row, failing to
> archive libwaproto.rlib and libwacore.rlib with "No space left on device".

The job sets `CARGO_INCREMENTAL: "0"` and zeroes debug info specifically to stay
inside that budget. Adding a second feature-gated build of a crate that pulls
`octo-storage-core` moves the job in exactly the direction that has already
failed twice. Whether it fits cannot be established from a workstation.

## Acceptance Criteria

- [ ] The `stoolap` step is added to `ci.yml`, or the decision not to is
      recorded with a reason
- [ ] If added, the added build cost is measured against the runner disk
      budget, and the job's existing disk comment is updated to reflect the
      new total
- [ ] `StoolapReputationStore` stops returning
      `ChainRefInvalid("stoolap_backend_unimplemented:<name>")` under the
      default feature set, or the stub is explicitly documented as
      intentional and permanent

## Related

The third acceptance criterion is the deeper one. A stub that silently answers
in the default build is indistinguishable from an implementation to anything
that only runs the default configuration. Closing the CI gap makes the real
backend visible; retiring the stub removes the possibility of it being mistaken
for one.
