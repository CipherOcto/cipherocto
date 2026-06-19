"""Centralized skip logic for tests/ that need external resources.

These tests are integration tests that make real API calls or depend on
optional packages. They are kept in the repo for local development and
ad-hoc runs, but are skipped in CI when their dependencies are missing.

The lightweight unit-level checks live in `tests/smoke_test.py`, which
runs in <1s without any network or API key and is the authoritative CI
signal for the quota_router SDK surface.
"""

import os

import pytest


def _has_openai() -> bool:
    """True if the optional `openai` package is importable."""
    try:
        import openai  # noqa: F401
        return True
    except ImportError:
        return False


def _has_api_key() -> bool:
    """True if a usable API key is present in the environment.

    Accepts any of the common names: OPENAI_API_KEY,
    QUOTA_ROUTER_API_KEY, or any *_API_KEY where the prefix maps to a
    provider supported by `quota_router.get_supported_providers()`.
    """
    if os.environ.get("OPENAI_API_KEY"):
        return True
    if os.environ.get("QUOTA_ROUTER_API_KEY"):
        return True
    # Any generic *_API_KEY
    return any(k.endswith("_API_KEY") for k in os.environ)


# Test classes that require live API calls or the optional `openai`
# package. They are skipped in CI when their dependencies are missing.
_ANY_LLM_NEEDS_OPENAI = {
    "TestCompletionAnyLLMStyle",
    "TestAcompletionAnyLLMStyle",
    "TestTypicalUsagePatterns",
}

_LITELLM_NEEDS_API_KEY = {
    "TestCompletionLiteLLMStyle",
    "TestAcompletionLiteLLMStyle",
    "TestTypicalUsagePatterns",
}


def pytest_collection_modifyitems(config, items):
    """Auto-skip integration test classes when their deps are missing."""
    for item in items:
        fspath = str(item.fspath)
        cls_name = item.cls.__name__ if item.cls is not None else ""

        if fspath.endswith("test_drop_in_any_llm.py"):
            if cls_name in _ANY_LLM_NEEDS_OPENAI and not _has_openai():
                item.add_marker(
                    pytest.mark.skip(
                        reason="openai package not installed (required "
                        "for any_llm-mode drop-in integration tests)"
                    )
                )

        elif fspath.endswith("test_drop_in_litellm.py"):
            if cls_name in _LITELLM_NEEDS_API_KEY and not _has_api_key():
                item.add_marker(
                    pytest.mark.skip(
                        reason="no API key in env (required for "
                        "litellm-mode drop-in integration tests)"
                    )
                )
