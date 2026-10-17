# quota-router

Drop-in replacement for LiteLLM with quota routing and cost management.

## Installation

```bash
pip install quota-router
```

## Quick Start

```python
import quota_router

# Set your API key
quota_router.set_api_key("openai", "sk-...")

# Make a completion call
response = quota_router.completion(
    model="gpt-4o",
    messages=[{"role": "user", "content": "Hello!"}]
)
print(response["choices"][0]["message"]["content"])

# Or stream responses
for chunk in quota_router.completion(
    model="gpt-4o",
    messages=[{"role": "user", "content": "Count to 5"}],
    stream=True
):
    print(chunk["choices"][0]["delta"]["content"], end="")
```

## Features

- **41 Providers**: OpenAI, Anthropic, Mistral, Gemini, and 37 more
- **Quota Management**: Track spend across providers
- **Budget Controls**: Set budget limits per provider
- **Metrics**: Prometheus-compatible metrics
- **Streaming**: Support for streaming responses
- **Callbacks**: LiteLLM-compatible callback surface (`input_callback`,
  `success_callback`, `failure_callback`, `service_callback`)

## Callbacks (RFC-0947)

quota-router exposes a LiteLLM-compatible callback surface. Each callback
is a Python callable that receives a single argument — a `dict`
representation of the `CallbackEvent`. Names follow the LiteLLM
convention (`log_success_event`, `log_failure_event`, `log_input_event`)
so existing LiteLLM callback code is drop-in.

### Available callback types

| Type                | LiteLLM equivalent       | When it fires                                                |
|---------------------|--------------------------|--------------------------------------------------------------|
| `input_callback`    | `litellm.inputCallback`  | Before provider dispatch (key validation, rate limit check). |
| `success_callback`  | `litellm.successCallback`| After a successful provider response (2xx status).           |
| `failure_callback`  | `litellm.failureCallback`| After a provider error (4xx/5xx) or local proxy error.        |
| `service_callback`  | `litellm.serviceCallback`| Health / monitoring events (provider health, circuit breaker).|
| `start_callback`    | (extension)              | At request entry, after key validation + rate-limit checks.  |
| `end_callback`      | (extension)              | At request completion (always paired with success/failure).  |

### Example

```python
import quota_router

# Define a callback. The signature is `def fn(event)` — the single
# `event` argument is a dict with the CallbackEvent shape.
def log_success_event(event):
    print(f"OK: {event['request']['model']} → {event['response']['usage']['total_tokens']} tokens")

def log_failure_event(event):
    print(f"FAIL: {event['error']['error_type']} ({event['error']['status_code']})")

# Register the callbacks. Each `set_*` accepts a single callable.
quota_router.set_success_callback(log_success_event)
quota_router.set_failure_callback(log_failure_event)

# Make a completion call. The callbacks fire after the response (or error).
response = quota_router.completion(
    model="gpt-4o",
    messages=[{"role": "user", "content": "Hello!"}]
)

# Or use the custom registration API to register by string type.
quota_router.set_custom_callback("success", log_success_event)

# Inspect diagnostics: how many events were dropped (channel full).
print(quota_router.callback_dropped_count())

# Snapshot the registered callback counts: {type: count}.
print(quota_router.callback_registry_snapshot())
```

### Callback event shape

The dict passed to your callback has the following top-level keys:

- `event_id`: unique UUIDv4 (always present)
- `callback_type`: `"input" | "success" | "failure" | "start" | "end" | "service"`
- `timestamp`: ISO-8601 UTC string
- `request`: `{model, messages, temperature, max_tokens, stream, provider, key_id, team_id, user_id}`
- `response`: `{id, model, response_summary, usage, latency_ms, provider, cached}` — None for non-Success events
- `error`: `{error_type, message, status_code, provider}` — None for non-Failure events
- `key_metadata`: virtual-key metadata (spend, budget) — None if not applicable
- `timing`: `{request_start, request_end, total_ms, provider_latency_ms, queue_time_ms}`

### Delivery semantics

- **Async + non-blocking** — callback dispatch happens on a background
  worker pool. The request path never blocks on a callback.
- **Bounded channel** — if the channel fills, events are dropped and
  counted in `callback_dropped_count()`. Backpressure is observable.
- **Best-effort** — Python exceptions raised inside the callback are
  logged and counted as drops; they do NOT propagate to the request path.
- **One GIL acquire per event** — multiple registered targets each
  spawn their own dispatch task. GIL contention scales with target
  count, not with event count.

## License

MIT OR Apache-2.0
