# Mission: 0947-c3 — PyO3 Callback SDK

## Status

Open. Follow-on to `0947-c-callback-executor` (CLOSED 2026-08-13, DEFERRED pattern). Wires callback surface into the PyO3 SDK (`crates/quota-router-pyo3/`) for Python consumer parity with LiteLLM.

## RFC

RFC-0947 (Economics): Callback System §Python SDK

## Dependencies

- Mission-0947-c: Callback Executor (CLOSED 2026-08-13)
- Mission-0947-c1: Proxy End/Success/Failure Wiring (recommended — full semantics surface)

## Acceptance Criteria

- [ ] Add `input_callback`, `success_callback`, `failure_callback`, `service_callback` to PyO3 SDK (`crates/quota-router-pyo3/src/sdk.rs` or new `callbacks.rs`)
- [ ] Each callback is a `PyObject` (Python callable) registered at SDK init
- [ ] PyO3 callback target wrapper: `PyO3CallbackTarget { target: PyObject }` implements `CallbackTarget` trait (Rust-side) — calls back into Python via `Python::with_gil`
- [ ] GIL acquisition overhead bounded (one GIL acquire per event, not per target)
- [ ] Custom callback function support: `set_custom_callback(callback_type, fn)` API in Python SDK
- [ ] Match LiteLLM callback interface: parameter names `log_success_event`, `log_failure_event`, `log_input_event` (LitellmInputCallback, LitellmServiceCallback, etc.)
- [ ] Add at least 4 PyO3 callback tests: input fired before provider, success fired after response, failure fired after error, custom Python function receives event
- [ ] Document Python SDK usage in `crates/quota-router-pyo3/python/README.md` or equivalent
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new PyO3 callback tests (≥4)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:

- `crates/quota-router-pyo3/src/sdk.rs` — main SDK surface; add callback registration + dispatch
- `crates/quota-router-pyo3/src/callbacks.rs` — NEW module for PyO3 callback target wrapper
- `crates/quota-router-pyo3/python/quota_router/__init__.py` — Python SDK init; add callback setters
- `crates/quota-router-pyo3/python/README.md` — NEW or update with callback usage examples

Reference points:

- LiteLLM callback interface: `litellm.inputCallback`, `litellm.successCallback`, `litellm.failureCallback`, `litellm.serviceCallback`
- `crates/quota-router-core/src/callbacks/mod.rs:160` — `CallbackTarget` trait to wrap

Architecture:

- PyO3 callback wrapper holds `PyObject` (Python callable)
- Wraps it in `Arc<PyO3CallbackTarget>` for Rust-side registry
- On `fire(event)`: acquire GIL, call Python function with event kwargs
- One GIL acquire per event (not per registered target) to minimize GIL thrash

## Version History

| Version | Date       | Change                                                                                                               |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Follow-on to `0947-c-callback-executor` closure. PyO3 callback SDK + LiteLLM interface match. 10 ACs. |

Last Updated: 2026-08-13
Version: 0.1
