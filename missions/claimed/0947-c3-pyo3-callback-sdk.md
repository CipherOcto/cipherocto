# Mission: 0947-c3 — PyO3 Callback SDK

## Status

LANDED 2026-08-13. Follow-on to `0947-c-callback-executor` (CLOSED 2026-08-13, DEFERRED pattern). Wires callback surface into the PyO3 SDK (`crates/quota-router-pyo3/`) for Python consumer parity with LiteLLM.

## RFC

RFC-0947 (Economics): Callback System §Python SDK

## Dependencies

- Mission-0947-c: Callback Executor (CLOSED 2026-08-13) ✓
- Mission-0947-c1: Proxy End/Success/Failure Wiring (LANDED 2026-08-13) ✓
- Mission-0947-c2: Streaming Callback Semantics (LANDED 2026-08-13) ✓

## Acceptance Criteria

- [x] Add `input_callback`, `success_callback`, `failure_callback`, `service_callback` to PyO3 SDK (`crates/quota-router-pyo3/src/callbacks.rs`)
- [x] Each callback is a `PyObject` (`Py<PyAny>`) (Python callable) registered at SDK init
- [x] PyO3 callback target wrapper: `PyO3CallbackTarget { name, func }` implements `CallbackTarget` trait (Rust-side) — calls back into Python via `Python::with_gil`
- [x] GIL acquisition overhead bounded (one GIL acquire per event, not per target — JSON serialize OUTSIDE GIL, then GIL acquire to call user callable)
- [x] Custom callback function support: `set_custom_callback(callback_type, fn)` API in Python SDK
- [x] Match LiteLLM callback interface: documented in `python/README.md` with `log_success_event`, `log_failure_event`, `log_input_event` naming convention
- [x] Add at least 4 PyO3 callback tests: input fired before provider, success fired after response, failure fired after error, custom Python function receives event (8 tests in callbacks.rs)
- [x] Document Python SDK usage in `crates/quota-router-pyo3/python/README.md` (added full Callback section with examples + event shape)
- [x] Clippy passes with zero warnings on `callbacks.rs` (pre-existing dead-code warnings in `types.rs`/`streaming.rs` are unrelated)
- [x] All existing tests pass + new PyO3 callback tests (28/28 pass under `LD_LIBRARY_PATH=/home/mmacedoeu/.pyenv/versions/3.12.9/lib cargo test --lib`)

## Claimant

cc-cascade (auto-landed via cascade pick order)

## Pull Request

#

## Notes

Key files:

- `crates/quota-router-pyo3/src/callbacks.rs` — NEW module: `PyO3CallbackTarget` + `GLOBAL_EXECUTOR` + 8 `set_*_callback` pyfunctions
- `crates/quota-router-pyo3/src/lib.rs` — registers 9 new `set_*_callback` / `callback_*` pyfunctions
- `crates/quota-router-pyo3/python/quota_router/__init__.py` — re-exports the new pyfunctions
- `crates/quota-router-pyo3/python/README.md` — adds full Callback section with LiteLLM-compat table + event shape + delivery semantics
- `crates/quota-router-pyo3/Cargo.toml` — adds `async-trait`, `tracing`, `chrono` deps + `pyo3-extension-module` opt-in feature flag (maturin enables via `[tool.maturin] features`)
- `crates/quota-router-core/src/callbacks/mod.rs` — adds `Copy` to `CallbackType` (non-breaking; required to pass by-value into HashMap entries in pyo3 module)

Reference points:

- LiteLLM callback interface: `litellm.inputCallback`, `litellm.successCallback`, `litellm.failureCallback`, `litellm.serviceCallback`
- `crates/quota-router-core/src/callbacks/mod.rs:199` — `CallbackTarget` trait wrapped

Architecture:

- PyO3 callback wrapper holds `Py<PyAny>` (Python callable)
- Wrapped in `Arc<PyO3CallbackTarget>` for Rust-side registry via `CallbackExecutor::register`
- On `fire(event)`: serialize event to JSON outside GIL → acquire GIL → parse JSON via Python `json.loads` → invoke user callable
- One GIL acquire per event (NOT per registered target) — bounded GIL overhead
- GLOBAL_EXECUTOR singleton lazily initializes; `set_*_callback` pushes into both executor + `PY_REGISTRY` snapshot

Test environment note: `cargo test --lib` for `quota-router-pyo3` requires `LD_LIBRARY_PATH=/home/mmacedoeu/.pyenv/versions/3.12.9/lib` (matches existing `e2e_proxy` env fix documented in [[proxy-strong-scenarios-status]]). The `[tool.maturin] features = ["pyo3/extension-module"]` setting in `python/pyproject.toml` enables the extension-module feature for wheel builds; the rlib test path auto-initializes Python via the `auto-initialize` pyo3 feature.

## Version History

| Version | Date       | Change                                                                                                                            |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------- |
| v0.2    | 2026-08-13 | LANDED. 10/10 ACs closed. 8 PyO3 callback tests pass. New `crates/quota-router-pyo3/src/callbacks.rs` + README Callback section.   |
| v0.1    | 2026-08-13 | Mission filed. Follow-on to `0947-c-callback-executor` closure. PyO3 callback SDK + LiteLLM interface match. 10 ACs.              |

Last Updated: 2026-08-13
Version: 0.2
