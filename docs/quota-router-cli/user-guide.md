# User Guide

## Installation

### From Source

```bash
cargo install --path crates/quota-router-cli
```

### Verify Installation

```bash
quota-router --help
```

## Configuration

### Initial Setup

Run the init command to create the default configuration:

```bash
quota-router init
```

This creates `~/.config/quota-router/config.json` with default values:

```json
{
  "balance": 100,
  "providers": [],
  "proxy_port": 8080
}
```

### Adding Providers

Add an AI provider:

```bash
quota-router add-provider openai
quota-router add-provider anthropic
```

Known providers automatically get their default endpoints:
- OpenAI: `https://api.openai.com/v1`
- Anthropic: `https://api.anthropic.com`
- Google: `https://generativelanguage.googleapis.com`

## Commands

### init

Initialize the router configuration:

```bash
quota-router init
```

### add-provider

Add a new AI provider:

```bash
quota-router add-provider <name>
```

Example:
```bash
quota-router add-provider openai
```

### balance

Check your OCTO-W balance:

```bash
quota-router balance
```

Output:
```
OCTO-W Balance: 100
```

### list

List quota for sale on the marketplace:

```bash
quota-router list --prompts 100 --price 1
```

Arguments:
- `--prompts` - Number of prompts to sell (default: 100)
- `--price` - Price per prompt in OCTO-W (default: 1)

### proxy

Start the transparent proxy server:

```bash
quota-router proxy --port 8080
```

The proxy:
1. Listens on localhost
2. Checks your OCTO-W balance before each request
3. Injects your API key from environment variable
4. Forwards request to the provider
5. Deducts 1 OCTO-W per request

**Required Environment Variable:**

Set your provider's API key:

```bash
# For OpenAI
export OPENAI_API_KEY=sk-...

# For Anthropic
export ANTHROPIC_API_KEY=sk-...
```

**Error Responses:**

- `402 Payment Required` - Insufficient balance
- `401 Unauthorized` - API key not set

### route

Test routing to a provider:

```bash
quota-router route --provider openai -p "Hello, world!"
```

Arguments:
- `--provider` - Provider name
- `-p`, `--prompt` - Test prompt

## Environment Variables

The proxy reads API keys from these environment variables:

| Provider | Variable |
|----------|----------|
| OpenAI | `OPENAI_API_KEY` |
| Anthropic | `ANTHROPIC_API_KEY` |
| Google | `GOOGLE_API_KEY` |

## Troubleshooting

### "API key not set in environment"

Make sure the environment variable is set:

```bash
export OPENAI_API_KEY=your-key-here
```

### "Insufficient OCTO-W balance"

The proxy requires OCTO-W balance to forward requests. Check your balance:

```bash
quota-router balance
```

### Port already in use

Choose a different port:

```bash
quota-router proxy --port 8081
```

## Configuration File

Location: `~/.config/quota-router/config.json`

```json
{
  "balance": 100,
  "providers": [
    {
      "name": "openai",
      "endpoint": "https://api.openai.com/v1"
    }
  ],
  "proxy_port": 8080
}
```
