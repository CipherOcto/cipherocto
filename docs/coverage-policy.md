# Coverage Policy

## Goal

Every production `.rs` file in the workspace must have **100% line coverage** as measured by `cargo tarpaulin`. This policy ensures no production code path is untested.

## Measurement

```sh
cargo tarpaulin --workspace --skip-clean --out stdout
```

Tarpaulin measures line coverage of production code. `#[cfg(test)]` modules are excluded by default.

## Test Layers

| Layer | Scope | Runs in CI | Gate |
|-------|-------|------------|------|
| **L1: Unit** | Module-level tests, trait impls, helpers | Yes | Always |
| **L2: In-process mesh** | `InMemoryChannelAdapter` + `PlatformAdapterBridge` | Yes | Always |
| **L3: Cross-process TCP** | `std::process::Command` → real TCP | No | `#[ignore]` |
| **L4: Docker compose** | Real containers, real network | No | `#[ignore]` |

**CI runs L1 + L2 only.** L3 and L4 are manual (`--ignored`).

## Coverage Thresholds

| Crate | Current | Target | Status |
|-------|---------|--------|--------|
| `quota-router-core` | ~50% (lib) | 100% | In progress |
| `octo-transport` | ~30% | 100% | Pending |
| `octo-network` | ~20% | 100% | Pending |
| `octo-adapter-tcp` | ~60% | 100% | In progress |
| `octo-adapter-matrix` | ~20% | 100% | In progress |
| `octo-adapter-whatsapp` | ~25% | 100% | Pending |

## Rules

1. **No production code without tests.** Every `pub fn` must have at least one test that exercises its body.
2. **No fake tests.** Tests must run production code, not parallel implementations. Mocks exist only at external boundaries (HTTP APIs, third-party services).
3. **No opt-outs.** `#[allow(coverage)]` or equivalent is forbidden.
4. **New code requires coverage.** PRs that add production code must include tests. Coverage must not decrease.
5. **L3/L4 are manual.** These tests require infrastructure (TCP ports, Docker) and are not run in CI. They are `#[ignore]`-gated.

## Running Coverage

```sh
# Full workspace coverage
cargo tarpaulin --workspace --skip-clean --out stdout

# Single crate
cargo tarpaulin -p quota-router-core --lib --skip-clean --out stdout

# With HTML report
cargo tarpaulin --workspace --skip-clean --out Html --output-dir coverage/
```

## CI Integration

Add to `.github/workflows/ci.yml`:

```yaml
- name: Check coverage
  run: |
    cargo tarpaulin --workspace --skip-clean --out stdout 2>&1 | tee coverage.txt
    COVERAGE=$(grep "^|| " coverage.txt | tail -1 | grep -oP '\d+\.\d+%')
    echo "Coverage: $COVERAGE"
    # Fail if below threshold (adjust as coverage improves)
    # if (( $(echo "$COVERAGE < 80.0" | bc -l) )); then
    #   echo "Coverage below threshold"
    #   exit 1
    # fi
```

## Related

- RFC-0870: Distributed Quota Router Network — Test Policy section
- `docs/plans/2026-06-30-quota-router-100-percent-coverage.md` — Implementation plan
