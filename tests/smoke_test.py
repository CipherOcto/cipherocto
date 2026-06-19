#!/usr/bin/env python3
"""
Lightweight smoke tests for quota_router Python SDK.

These tests verify the package structure, import surface, and exception
hierarchy without making any network calls or requiring API keys. They
are safe to run in CI on every commit.

Run with:
    python tests/smoke_test.py
or:
    .venv/bin/python -m pytest tests/smoke_test.py -v
"""

from __future__ import annotations

import asyncio
import inspect
import sys
from typing import Any

import pytest


# --- Top-level surface (mirrors `quota_router.__all__`) ---------------------

# Functions that must be present and callable.
EXPECTED_FUNCTIONS: tuple[str, ...] = (
    "completion",
    "acompletion",
    "text_completion",
    "atext_completion",
    "embedding",
    "aembedding",
    "messages",
    "amessages",
    "responses",
    "aresponses",
    "list_models",
    "alist_models",
    "get_response",
    "aget_response",
    "delete_response",
    "adelete_response",
    "batch_create",
    "abatch_create",
    "batch_list",
    "abatch_list",
    "batch_results",
    "abatch_results",
    "batch_retrieve",
    "abatch_retrieve",
    "batch_cancel",
    "abatch_cancel",
    "batch_completion",
    "get_budget_status",
    "get_metrics",
    "get_provider_info",
    "get_supported_providers",
    "is_provider_supported",
    "parse_model",
    "parse_model_strict",
    "set_api_key",
)

# Async functions (each must have a callable + signature, and a
# corresponding sync counterpart). The Rust `a*` functions are
# exposed as `builtin_function_or_method` (not Python coroutine
# functions), so we can't use `inspect.iscoroutinefunction`.
ASYNC_FUNCTIONS: frozenset[str] = frozenset(
    {
        "acompletion",
        "atext_completion",
        "aembedding",
        "amessages",
        "aresponses",
        "alist_models",
        "aget_response",
        "adelete_response",
        "abatch_create",
        "abatch_list",
        "abatch_results",
        "abatch_retrieve",
        "abatch_cancel",
    }
)

# Exception classes (all must inherit from QuotaRouterError).
EXPECTED_EXCEPTIONS: tuple[str, ...] = (
    "QuotaRouterError",
    "RateLimitError",
    "AuthenticationError",
    "InvalidRequestError",
    "ProviderError",
    "ContentFilterError",
    "ModelNotFoundError",
    "ContextLengthExceededError",
    "MissingApiKeyError",
    "UnsupportedProviderError",
    "UnsupportedParameterError",
    "InsufficientFundsError",
    "UpstreamProviderError",
    "GatewayTimeoutError",
    "LengthFinishReasonError",
    "ContentFilterFinishReasonError",
    "BatchNotCompleteError",
    "AllModelsFailedError",
    "BatchPartialFailureError",
    "BudgetExceededError",
    "ServiceUnavailableError",
    "APIConnectionError",
    "APIError",
    "NotFoundError",
    "ContextWindowExceededError",
    "ContentPolicyViolationError",
)

# Submodules that must be importable.
EXPECTED_SUBMODULES: tuple[str, ...] = (
    "quota_router",
    "quota_router.router",
    "quota_router.exceptions",
    "quota_router.litellm",
    "quota_router.any_llm",
    "quota_router_native",
)


# --- Fixtures ---------------------------------------------------------------


@pytest.fixture(scope="module")
def qr() -> Any:
    """Provide the quota_router module as a fixture."""
    import quota_router
    return quota_router


@pytest.fixture(scope="module")
def native() -> Any:
    """Provide the native quota_router_native module as a fixture."""
    import quota_router_native
    return quota_router_native


# --- Tests ------------------------------------------------------------------


def test_package_import():
    """Test 1: Package imports and has a version."""
    import quota_router
    assert quota_router.__version__ == "0.1.0"
    # All symbols listed in __all__ must exist.
    for name in quota_router.__all__:
        assert hasattr(quota_router, name), f"Missing symbol: {name}"


def test_native_import():
    """Test 2: Native extension is importable."""
    import quota_router_native
    # Native module must expose at least the base exception.
    assert hasattr(quota_router_native, "QuotaRouterError")


def test_submodules_importable():
    """Test 3: All expected submodules can be imported."""
    for modname in EXPECTED_SUBMODULES:
        __import__(modname)  # raises ImportError on failure


def test_functions_callable(qr):
    """Test 4: All expected top-level functions exist and are callable."""
    for name in EXPECTED_FUNCTIONS:
        assert hasattr(qr, name), f"Missing function: {name}"
        assert callable(getattr(qr, name)), f"Not callable: {name}"


def test_async_functions_have_signatures(qr):
    """Test 5: All `a`-prefixed functions are callable with a signature.

    These are exposed by pyo3 as `builtin_function_or_method`, so we
    can't use `inspect.iscoroutinefunction` (which only sees Python
    coroutines). Instead we verify the function is callable and has
    an inspectable signature.
    """
    for name in ASYNC_FUNCTIONS:
        fn = getattr(qr, name)
        assert callable(fn), f"{name} is not callable"
        try:
            sig = inspect.signature(fn)
        except (ValueError, TypeError) as e:
            raise AssertionError(f"{name} has no inspectable signature: {e}")


def test_async_functions_have_sync_counterparts(qr):
    """Test 6: Every `a`-prefixed function has a sync counterpart."""
    for name in ASYNC_FUNCTIONS:
        sync_name = name[1:]  # strip leading `a`
        assert hasattr(qr, sync_name), (
            f"Async function {name} has no sync counterpart {sync_name}"
        )
        assert callable(getattr(qr, sync_name))


def test_exception_hierarchy(qr):
    """Test 7: All exceptions inherit from QuotaRouterError and Exception."""
    base = qr.QuotaRouterError
    assert issubclass(base, Exception)
    for name in EXPECTED_EXCEPTIONS:
        if name == "QuotaRouterError":
            continue
        cls = getattr(qr, name)
        assert inspect.isclass(cls), f"{name} is not a class"
        assert issubclass(cls, base), (
            f"{name} does not inherit from QuotaRouterError"
        )
        assert issubclass(cls, Exception)


def test_router_class_exists(qr):
    """Test 8: Router class is present and is a class."""
    assert inspect.isclass(qr.Router)


def test_litellm_alias_surface():
    """Test 9: Drop-in LiteLLM alias works (import + attribute access only)."""
    import quota_router as litellm
    # The drop-in replacement contract: key names match LiteLLM's surface.
    for name in ("completion", "acompletion", "embedding", "aembedding"):
        assert hasattr(litellm, name), f"litellm alias missing: {name}"


def test_any_llm_alias_surface():
    """Test 10: Drop-in any-llm alias works (import + attribute access only)."""
    import quota_router as any_llm
    for name in ("completion", "acompletion"):
        assert hasattr(any_llm, name), f"any_llm alias missing: {name}"


def test_completion_signature(qr):
    """Test 11: `completion` exposes a `model` parameter (LiteLLM-compatible)."""
    sig = inspect.signature(qr.completion)
    assert "model" in sig.parameters, "completion() must accept a `model` parameter"


def test_no_network_at_import(qr, monkeypatch):
    """Test 12: Importing the package does not perform any network I/O."""
    # Patch socket.socket to detect any network attempt during a no-op call.
    import socket

    network_attempted = False
    original_socket = socket.socket

    def guard(*args, **kwargs):
        nonlocal network_attempted
        network_attempted = True
        raise AssertionError("No network calls allowed during smoke test")

    monkeypatch.setattr(socket, "socket", guard)
    # Re-import in case of cached state.
    import importlib
    importlib.reload(qr)
    # Touch a few attributes to trigger any lazy init paths.
    _ = qr.__version__
    _ = qr.Router
    _ = qr.QuotaRouterError
    assert not network_attempted, "importing quota_router triggered a network call"


# --- Standalone runner ------------------------------------------------------


async def _run_async_checks() -> None:
    """Async-side checks that don't make network calls."""
    import quota_router as qr

    # Verify each async function is callable and has a signature.
    for name in ASYNC_FUNCTIONS:
        fn = getattr(qr, name)
        assert callable(fn), name
        try:
            inspect.signature(fn)
        except (ValueError, TypeError) as e:
            raise AssertionError(f"{name} has no signature: {e}")


def main() -> int:
    """Run all checks directly (no pytest required)."""
    print("Running lightweight smoke tests for quota_router...\n")

    # Checks that do NOT take a module argument.
    no_arg_checks: list[tuple[str, Any]] = [
        ("test_package_import", test_package_import),
        ("test_native_import", test_native_import),
        ("test_submodules_importable", test_submodules_importable),
        ("test_litellm_alias_surface", test_litellm_alias_surface),
        ("test_any_llm_alias_surface", test_any_llm_alias_surface),
    ]

    # Checks that take the quota_router module as a positional argument.
    qr_checks: list[tuple[str, Any]] = [
        ("test_functions_callable", test_functions_callable),
        ("test_async_functions_have_signatures", test_async_functions_have_signatures),
        ("test_async_functions_have_sync_counterparts", test_async_functions_have_sync_counterparts),
        ("test_exception_hierarchy", test_exception_hierarchy),
        ("test_router_class_exists", test_router_class_exists),
        ("test_completion_signature", test_completion_signature),
    ]

    try:
        import quota_router as _qr
        import quota_router_native as _native  # noqa: F401

        for name, fn in no_arg_checks:
            print(f"  • {name} ...", end=" ", flush=True)
            fn()
            print("ok")

        for name, fn in qr_checks:
            print(f"  • {name} ...", end=" ", flush=True)
            fn(_qr)
            print("ok")

        # Async checks
        print("  • _run_async_checks ...", end=" ", flush=True)
        asyncio.run(_run_async_checks())
        print("ok")

        # The network guard test uses pytest's monkeypatch fixture, so it
        # only runs under pytest. Skip it in standalone mode.
        print(
            "  • test_no_network_at_import ... SKIPPED "
            "(requires pytest's monkeypatch)"
        )

        print("\n✅ All lightweight smoke tests passed!")
        return 0

    except Exception as e:
        print(f"\n❌ Smoke test failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
