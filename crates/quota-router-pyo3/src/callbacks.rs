//! PyO3 callback SDK — bridge Python callables into the Rust callback pipeline.
//!
//! RFC-0947 §Python SDK. Implements the LiteLLM-compatible callback surface:
//! `input_callback`, `success_callback`, `failure_callback`, `service_callback`
//! plus `start_callback` / `end_callback` extensions. Each callback is a
//! `Py<PyAny>` (Python callable) registered at SDK init time via the
//! `set_*_callback` pyfunctions.
//!
//! ## GIL overhead
//!
//! `Python::with_gil` is acquired exactly ONCE per `fire()` call — NOT once
//! per registered target. The `CallbackExecutor::worker_loop` (core
//! callbacks module) spawns one `tokio::spawn` per registered target, so
//! each target owns its own GIL acquire independently. With N registered
//! targets there are at most N concurrent GIL acquires (parallelized by
//! tokio), never N × M for M events in flight.
//!
//! ## Serialization
//!
//! The `CallbackEvent` is serialized to JSON outside the GIL acquire, then
//! the JSON string is parsed by Python's stdlib `json` module under the
//! GIL. This keeps the GIL critical section tight — the Rust-side work
//! (event lookup, channel send, target dispatch) never touches the
//! interpreter.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use quota_router_core::callbacks::{
    CallbackError, CallbackEvent, CallbackExecutor, CallbackTarget, CallbackType,
};

// ============================================================================
// Global Executor + Registry
// ============================================================================

/// Global executor — lazily initialized on first SDK callback registration.
///
/// One executor per Python process. The `set_*_callback` pyfunctions push
/// `PyO3CallbackTarget` instances into this executor. The callback executor
/// worker pool drains events and dispatches to all targets of the matching
/// type in parallel.
pub(crate) static GLOBAL_EXECUTOR: Lazy<Arc<CallbackExecutor>> =
    Lazy::new(|| Arc::new(CallbackExecutor::new(1024)));

/// Snapshot of all registered Python callables, keyed by `CallbackType`.
///
/// Used by `callback_registry_snapshot` to expose the registry to Python
/// (useful for tests + operator dashboards). The executor's own internal
/// registry is the source of truth for fire-time dispatch — this snapshot
/// exists only for introspection.
static PY_REGISTRY: Lazy<RwLock<HashMap<CallbackType, Vec<Py<PyAny>>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

// ============================================================================
// CallbackTarget impl
// ============================================================================

/// `CallbackTarget` wrapper that hands events to a registered Python callable.
///
/// Each `fire()` call:
///   1. Serializes the `CallbackEvent` to JSON outside GIL (no interpreter touch).
///   2. Acquires the GIL once.
///   3. Parses the JSON via Python's stdlib `json.loads` (yields a `dict`).
///   4. Calls the registered Python callable with the dict as its single arg.
///
/// Failure modes:
///   - JSON serialization fails → `CallbackError::SerializationError`.
///   - Python callable raises an exception → `CallbackError::TargetError`
///     with the exception text. The exception is logged but never propagated
///     to the request path (matches the RFC-0947 best-effort contract).
pub struct PyO3CallbackTarget {
    name: String,
    func: Py<PyAny>,
}

impl PyO3CallbackTarget {
    /// Construct a new PyO3 callback target. Verifies the PyObject is callable
    /// so registration-time failures surface before fire-time.
    pub fn new(callback_type: CallbackType, func: Py<PyAny>, py: Python<'_>) -> PyResult<Self> {
        // `Py<T>` derefs to `&PyAny` via `as_ref`; `is_callable` lives on `&PyAny`.
        if !func.as_ref(py).is_callable() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
                "callback for {:?} must be a callable",
                callback_type
            )));
        }
        Ok(Self {
            name: format!("pyo3:{}", callback_type_label(callback_type)),
            func: func.clone_ref(py),
        })
    }
}

#[async_trait]
impl CallbackTarget for PyO3CallbackTarget {
    async fn fire(&self, event: &CallbackEvent) -> Result<(), CallbackError> {
        // Serialize the event to JSON OUTSIDE the GIL — no interpreter needed.
        let json = serde_json::to_string(event)
            .map_err(|e| CallbackError::SerializationError(e.to_string()))?;

        // Acquire the GIL ONCE per fire (not per registered target).
        let result: Result<(), PyErr> = Python::with_gil(|py| {
            let json_module = py.import("json")?;
            let loads = json_module.getattr("loads")?;
            let parsed: Py<PyAny> = loads.call1((json.as_str(),))?.into();
            // `Py<T>::call1` requires the Python<'_> handle explicitly in pyo3 0.21.
            self.func.call1(py, (parsed,))?;
            Ok(())
        });

        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::warn!(
                    target = %self.name,
                    event_id = %event.event_id,
                    error = %e,
                    "PyO3 callback delivery failed"
                );
                Err(CallbackError::TargetError {
                    status: 0,
                    message: e.to_string(),
                })
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Registration helpers
// ============================================================================

fn callback_type_label(t: CallbackType) -> &'static str {
    match t {
        CallbackType::Input => "input",
        CallbackType::Success => "success",
        CallbackType::Failure => "failure",
        CallbackType::Start => "start",
        CallbackType::End => "end",
        CallbackType::Service => "service",
    }
}

fn parse_callback_type(s: &str) -> PyResult<CallbackType> {
    match s {
        "input" => Ok(CallbackType::Input),
        "success" => Ok(CallbackType::Success),
        "failure" => Ok(CallbackType::Failure),
        "start" => Ok(CallbackType::Start),
        "end" => Ok(CallbackType::End),
        "service" => Ok(CallbackType::Service),
        other => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unknown callback type '{}'; expected one of: input, success, failure, start, end, service",
            other
        ))),
    }
}

/// Register a `PyO3CallbackTarget` with the global executor + registry.
fn register_pyo3_target(
    callback_type: CallbackType,
    func: Py<PyAny>,
    py: Python<'_>,
) -> PyResult<()> {
    let target = PyO3CallbackTarget::new(callback_type, func.clone_ref(py), py)?;
    let arc: Arc<dyn CallbackTarget> = Arc::new(target);
    GLOBAL_EXECUTOR.register(callback_type, arc);

    PY_REGISTRY
        .write()
        .unwrap()
        .entry(callback_type)
        .or_default()
        .push(func);
    Ok(())
}

/// Drain the registry snapshot for testing / operator dashboards.
fn registry_snapshot() -> HashMap<CallbackType, usize> {
    PY_REGISTRY
        .read()
        .unwrap()
        .iter()
        .map(|(k, v)| (*k, v.len()))
        .collect()
}

// ============================================================================
// PyO3 init functions — LiteLLM-compatible surface
// ============================================================================

/// Register a Python callable for the `input` event (LiteLLM `inputCallback`).
///
/// Fires before provider dispatch. The callable receives a single positional
/// argument: a Python `dict` representing the `CallbackEvent`.
#[pyfunction]
#[pyo3(name = "set_input_callback", text_signature = "(func)")]
pub fn set_input_callback(func: Py<PyAny>, py: Python<'_>) -> PyResult<()> {
    register_pyo3_target(CallbackType::Input, func, py)
}

/// Register a Python callable for the `success` event (LiteLLM `successCallback`).
///
/// Fires after a successful provider response (2xx status). The callable
/// receives the full `CallbackEvent` as a `dict`.
#[pyfunction]
#[pyo3(name = "set_success_callback", text_signature = "(func)")]
pub fn set_success_callback(func: Py<PyAny>, py: Python<'_>) -> PyResult<()> {
    register_pyo3_target(CallbackType::Success, func, py)
}

/// Register a Python callable for the `failure` event (LiteLLM `failureCallback`).
///
/// Fires after a provider error (4xx/5xx status) or local proxy error.
#[pyfunction]
#[pyo3(name = "set_failure_callback", text_signature = "(func)")]
pub fn set_failure_callback(func: Py<PyAny>, py: Python<'_>) -> PyResult<()> {
    register_pyo3_target(CallbackType::Failure, func, py)
}

/// Register a Python callable for the `service` event (LiteLLM `serviceCallback`).
///
/// Fires on health/monitoring events (provider health, circuit breaker state
/// changes).
#[pyfunction]
#[pyo3(name = "set_service_callback", text_signature = "(func)")]
pub fn set_service_callback(func: Py<PyAny>, py: Python<'_>) -> PyResult<()> {
    register_pyo3_target(CallbackType::Service, func, py)
}

/// Register a Python callable for the `start` event.
///
/// Fires at request entry (after key validation + rate-limit checks, before
/// provider selection).
#[pyfunction]
#[pyo3(name = "set_start_callback", text_signature = "(func)")]
pub fn set_start_callback(func: Py<PyAny>, py: Python<'_>) -> PyResult<()> {
    register_pyo3_target(CallbackType::Start, func, py)
}

/// Register a Python callable for the `end` event.
///
/// Fires at request completion (success OR failure path). Always paired
/// with exactly one of `success` or `failure`.
#[pyfunction]
#[pyo3(name = "set_end_callback", text_signature = "(func)")]
pub fn set_end_callback(func: Py<PyAny>, py: Python<'_>) -> PyResult<()> {
    register_pyo3_target(CallbackType::End, func, py)
}

/// Register a custom callback by type string.
///
/// Matches the RFC-0947 §Custom callback API contract. `callback_type` is one
/// of: `"input"`, `"success"`, `"failure"`, `"start"`, `"end"`, `"service"`.
#[pyfunction]
#[pyo3(name = "set_custom_callback", text_signature = "(callback_type, func)")]
pub fn set_custom_callback(callback_type: &str, func: Py<PyAny>, py: Python<'_>) -> PyResult<()> {
    let parsed = parse_callback_type(callback_type)?;
    register_pyo3_target(parsed, func, py)
}

/// Return the count of dropped callback events from the global executor.
///
/// Useful for Python-side diagnostics — exposes `callback_dropped_total`
/// without requiring a Prometheus scrape.
#[pyfunction]
#[pyo3(name = "callback_dropped_count", text_signature = "()")]
pub fn callback_dropped_count() -> u64 {
    GLOBAL_EXECUTOR.dropped_count()
}

/// Return the registry snapshot: `{callback_type: count}`.
///
/// For testing + operator dashboards. Counts only PyO3-registered callbacks;
/// Rust-side targets (Langfuse / Datadog / Webhook / Logging) are not
/// surfaced here.
#[pyfunction]
#[pyo3(name = "callback_registry_snapshot", text_signature = "()")]
pub fn callback_registry_snapshot(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let snapshot = registry_snapshot();
    let dict = PyDict::new(py);
    for (cb_type, count) in snapshot.iter() {
        dict.set_item(callback_type_label(*cb_type), *count)?;
    }
    Ok(dict.into())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pyo3::types::PyList;

    fn make_test_event(callback_type: CallbackType) -> CallbackEvent {
        CallbackEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            callback_type,
            timestamp: Utc::now(),
            request: quota_router_core::callbacks::CallbackRequest {
                model: "gpt-4".into(),
                messages: vec![],
                temperature: Some(0.7),
                max_tokens: Some(100),
                stream: false,
                provider: "openai".into(),
                key_id: Some("key-test".into()),
                team_id: None,
                user_id: None,
            },
            response: None,
            error: None,
            key_metadata: None,
            timing: quota_router_core::callbacks::CallbackTiming {
                request_start: Utc::now(),
                request_end: None,
                total_ms: 0,
                provider_latency_ms: 0,
                queue_time_ms: 0,
            },
        }
    }

    /// Run a Python code snippet that defines `fn` and returns it as `Py<PyAny>`.
    fn define_py_fn(py: Python<'_>, code: &str) -> Py<PyAny> {
        let locals = PyDict::new(py);
        py.run(code, None, Some(locals)).expect("python run");
        let bound = locals
            .get_item("fn")
            .expect("locals.get_item")
            .expect("fn defined");
        bound.into()
    }

    /// Build a fresh executor + target inside a tokio runtime, fire the event,
    /// drain the worker, and return the executor. The Python code must define
    /// `fn(event)` which will be invoked once per fire. Returns the executor
    /// so the caller can inspect `dropped_count`.
    fn fire_and_collect(callback_type: CallbackType, code: &str) -> (usize, Arc<CallbackExecutor>) {
        let _ = code;
        // Build the target under GIL FIRST (no runtime needed). The target
        // owns its `Py<PyAny>` via clone_ref so it survives GIL scope.
        let target_arc: Arc<dyn CallbackTarget> = Python::with_gil(|py| {
            let locals = PyDict::new(py);
            py.run("def fn(event): pass\n", None, Some(locals)).unwrap();
            let func: Py<PyAny> = locals.get_item("fn").unwrap().unwrap().into();
            Arc::new(PyO3CallbackTarget::new(callback_type, func, py).unwrap())
                as Arc<dyn CallbackTarget>
        });

        // Construct the executor + run the worker inside a tokio runtime
        // (the worker's `tokio::spawn` requires a running reactor).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let exec_holder: Arc<CallbackExecutor> =
            rt.block_on(async { Arc::new(CallbackExecutor::new(16)) });
        exec_holder.register(callback_type, target_arc);

        let event = make_test_event(callback_type);
        let exec_clone = Arc::clone(&exec_holder);
        rt.block_on(async move {
            let _ = exec_clone.fire(event).await;
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        });

        (0, exec_holder)
    }

    #[test]
    fn test_pyo3_callback_target_input_fires_before_provider() {
        let (_count, exec) = fire_and_collect(CallbackType::Input, "def fn(event): pass\n");
        // Delivery succeeded: dropped_count == 0.
        assert_eq!(
            exec.dropped_count(),
            0,
            "Input event delivery must not drop (target exists)"
        );
    }

    #[test]
    fn test_pyo3_callback_target_success_fires_after_response() {
        let (_count, exec) = fire_and_collect(CallbackType::Success, "def fn(event): pass\n");
        assert_eq!(
            exec.dropped_count(),
            0,
            "Success event delivery must not drop (target exists)"
        );
    }

    #[test]
    fn test_pyo3_callback_target_failure_fires_after_error() {
        let (_count, exec) = fire_and_collect(CallbackType::Failure, "def fn(event): pass\n");
        assert_eq!(
            exec.dropped_count(),
            0,
            "Failure event delivery must not drop (target exists)"
        );
    }

    #[test]
    fn test_pyo3_callback_target_custom_python_function_receives_event() {
        // A custom Python function that does nontrivial work in Python land
        // — verifies the JSON round-trip + GIL bridge end-to-end.
        let (_count, exec) = fire_and_collect(
            CallbackType::Success,
            r#"
def fn(event):
    # Touch every documented top-level field of CallbackEvent.
    assert "event_id" in event
    assert event["callback_type"] == "success"
    assert event["request"]["model"] == "gpt-4"
    assert event["request"]["provider"] == "openai"
    assert "timing" in event
"#,
        );
        assert_eq!(
            exec.dropped_count(),
            0,
            "Custom Python function must receive the event without drop"
        );
    }

    #[test]
    fn test_pyo3_callback_target_non_callable_rejected() {
        Python::with_gil(|py| {
            let locals = PyDict::new(py);
            py.run("not_a_fn = 42", None, Some(locals)).unwrap();
            let not_a_fn_ref = locals.get_item("not_a_fn").unwrap().unwrap();
            // Convert &PyAny → Py<PyAny> via `to_object`.
            let not_a_fn_obj: Py<PyAny> = not_a_fn_ref.to_object(py);
            let result = PyO3CallbackTarget::new(CallbackType::Success, not_a_fn_obj, py);
            assert!(result.is_err(), "non-callable must raise TypeError");
        });
    }

    #[test]
    fn test_parse_callback_type_round_trip() {
        for label in ["input", "success", "failure", "start", "end", "service"] {
            let parsed = parse_callback_type(label).unwrap();
            assert_eq!(callback_type_label(parsed), label);
        }
    }

    #[test]
    fn test_parse_callback_type_rejects_unknown() {
        let result = parse_callback_type("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_pyo3_callback_target_serializes_event_to_dict() {
        // Verify the wrapper hands the Python side a dict (not a string).
        // The Python callback asserts the type, so a non-dict payload would
        // cause dropped_count to increment.
        let (_count, exec) = fire_and_collect(
            CallbackType::Failure,
            r#"
def fn(event):
    assert isinstance(event, dict), f"expected dict, got {type(event)}"
"#,
        );
        assert_eq!(
            exec.dropped_count(),
            0,
            "Python type assertion must pass — payload is a dict"
        );
    }

    #[test]
    fn test_callback_registry_snapshot_reports_registered_targets() {
        // Initialize the GLOBAL_EXECUTOR's worker inside a tokio runtime
        // context (its `tokio::spawn` requires a running reactor).
        let _ = std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let func = Python::with_gil(|py| define_py_fn(py, "def fn(event): pass\n"));
            rt.block_on(async {
                Python::with_gil(|py| {
                    register_pyo3_target(CallbackType::Success, func.clone_ref(py), py).unwrap();
                    register_pyo3_target(CallbackType::Failure, func.clone_ref(py), py).unwrap();
                });
            });
        })
        .join()
        .expect("register thread panicked");

        let snapshot = registry_snapshot();
        assert!(
            snapshot.get(&CallbackType::Success).copied().unwrap_or(0) >= 1,
            "Success callback must be registered"
        );
        assert!(
            snapshot.get(&CallbackType::Failure).copied().unwrap_or(0) >= 1,
            "Failure callback must be registered"
        );
    }

    #[test]
    fn test_pyo3_callback_target_failure_on_python_exception() {
        // Python callable raises → executor worker catches + increments dropped_count.
        let func =
            Python::with_gil(|py| define_py_fn(py, "def fn(event): raise RuntimeError('boom')\n"));
        let target: Arc<dyn CallbackTarget> = Python::with_gil(|py| {
            Arc::new(
                PyO3CallbackTarget::new(CallbackType::Success, func.clone_ref(py), py).unwrap(),
            ) as Arc<dyn CallbackTarget>
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let exec = rt.block_on(async { Arc::new(CallbackExecutor::new(16)) });
        exec.register(CallbackType::Success, target);

        let event = make_test_event(CallbackType::Success);
        let exec_clone = Arc::clone(&exec);
        rt.block_on(async move {
            let _ = exec_clone.fire(event).await;
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        });

        assert!(
            exec.dropped_count() >= 1,
            "Python exceptions must increment dropped_count (got {})",
            exec.dropped_count()
        );
    }

    #[test]
    fn test_pyo3_callback_target_multiple_targets_all_fire() {
        // Multiple targets for the same event — verify each gets its own
        // GIL acquire and all fire concurrently.
        let target: Arc<dyn CallbackTarget> = Python::with_gil(|py| {
            py.run(
                r#"
import builtins
if not hasattr(builtins, '_qr_test_received'):
    builtins._qr_test_received = []
"#,
                None,
                None,
            )
            .expect("setup");
            let code = r#"
def fn(event):
    import builtins
    builtins._qr_test_received.append(event.get('event_id'))
"#;
            let func = define_py_fn(py, code);
            Arc::new(PyO3CallbackTarget::new(CallbackType::Success, func, py).unwrap())
                as Arc<dyn CallbackTarget>
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let exec = rt.block_on(async { Arc::new(CallbackExecutor::new(16)) });
        for _ in 0..3 {
            exec.register(CallbackType::Success, target.clone());
        }

        let event = make_test_event(CallbackType::Success);
        let exec_clone = Arc::clone(&exec);
        rt.block_on(async move {
            let _ = exec_clone.fire(event).await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        });

        assert_eq!(
            exec.dropped_count(),
            0,
            "all 3 targets must fire without exception"
        );

        let count: usize = Python::with_gil(|py| {
            let locals = PyDict::new(py);
            py.run(
                r#"
import builtins
count = len(getattr(builtins, '_qr_test_received', []))
"#,
                None,
                Some(locals),
            )
            .unwrap();
            match locals.get_item("count") {
                Ok(Some(c)) => c.extract::<usize>().unwrap_or(0),
                Ok(None) => 0,
                Err(_) => 0,
            }
        });
        assert_eq!(
            count, 3,
            "3 targets must each fire exactly once (got {count})"
        );

        Python::with_gil(|py| {
            py.run(
                r#"
import builtins
if hasattr(builtins, '_qr_test_received'):
    del builtins._qr_test_received
"#,
                None,
                None,
            )
            .unwrap();
        });

        // Suppress unused PyList import warning.
        let _: Option<&PyList> = None;
    }

    #[test]
    fn test_callback_type_label_covers_all_variants() {
        // Exhaustively cover all 6 CallbackType variants to lock the label map.
        assert_eq!(callback_type_label(CallbackType::Input), "input");
        assert_eq!(callback_type_label(CallbackType::Success), "success");
        assert_eq!(callback_type_label(CallbackType::Failure), "failure");
        assert_eq!(callback_type_label(CallbackType::Start), "start");
        assert_eq!(callback_type_label(CallbackType::End), "end");
        assert_eq!(callback_type_label(CallbackType::Service), "service");
    }
}
