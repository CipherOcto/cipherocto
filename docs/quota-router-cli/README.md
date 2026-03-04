# Quota Router CLI

A CLI tool for managing AI API quotas with a transparent HTTPS proxy.

## Quick Start

```bash
# Install
cargo install --path crates/quota-router-cli

# Initialize
quota-router init

# Add a provider
quota-router add-provider openai

# Check balance
quota-router balance

# Start proxy (requires API key in environment)
OPENAI_API_KEY=sk-... quota-router proxy --port 8080
```

## Features

- **Transparent Proxy** - Intercepts API requests, checks balance, injects API key
- **Multi-Provider** - Support for OpenAI, Anthropic, Google and custom providers
- **Local Balance** - Mock OCTO-W balance for testing
- **Quota Listing** - List quota for sale on the marketplace

## Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize configuration |
| `add-provider` | Add an AI provider |
| `balance` | Show OCTO-W balance |
| `list` | List quota for sale |
| `proxy` | Start the proxy server |
| `route` | Test routing to a provider |

## Configuration

Config stored at: `~/.config/quota-router/config.json`

## Links

- [User Guide](./user-guide.md)
- [API Reference](./api-reference.md)
