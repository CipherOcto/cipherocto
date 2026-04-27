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

## License

MIT OR Apache-2.0
