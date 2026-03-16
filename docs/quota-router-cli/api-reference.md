# API Reference

## CLI Commands

### quota-router init

Initialize the router configuration.

**Arguments:** None

**Exit Codes:**

- `0` - Success
- `1` - Error

---

### quota-router add-provider

Add a new AI provider.

**Arguments:**

- `name` (required) - Provider name

**Example:**

```bash
quota-router add-provider openai
```

---

### quota-router balance

Display current OCTO-W balance.

**Arguments:** None

**Output:**

```
OCTO-W Balance: <number>
```

---

### quota-router list

List quota for sale.

**Arguments:**

- `--prompts`, `-p` (optional) - Number of prompts (default: 100)
- `--price` (optional) - Price per prompt (default: 1)

**Example:**

```bash
quota-router list --prompts 100 --price 1
```

---

### quota-router proxy

Start the transparent proxy server.

**Arguments:**

- `--port`, `-p` (optional) - Port to listen on (default: 8080)

**Environment Variables:**

- Provider API keys (e.g., `OPENAI_API_KEY`)

**Example:**

```bash
quota-router proxy --port 8080
```

---

### quota-router route

Test routing to a provider.

**Arguments:**

- `--provider` (required) - Provider name
- `-p`, `--prompt` (required) - Test prompt

**Example:**

```bash
quota-router route --provider openai -p "Hello"
```

---

## Configuration Schema

### Config

```json
{
  "balance": "u64 - OCTO-W balance",
  "providers": "Vec<Provider> - Configured providers",
  "proxy_port": "u16 - Default proxy port"
}
```

### Provider

```json
{
  "name": "String - Provider identifier",
  "endpoint": "String - Provider API endpoint"
}
```

---

## Error Codes

| Code | Meaning           |
| ---- | ----------------- |
| 0    | Success           |
| 1    | General error     |
| 2    | Invalid arguments |

### Proxy HTTP Responses

| Status | Meaning                        |
| ------ | ------------------------------ |
| 200    | Request forwarded successfully |
| 401    | API key not set in environment |
| 402    | Insufficient OCTO-W balance    |

---

## Environment Variables

| Variable            | Description       |
| ------------------- | ----------------- |
| `OPENAI_API_KEY`    | OpenAI API key    |
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `GOOGLE_API_KEY`    | Google API key    |

---

## File Locations

| File   | Location                             |
| ------ | ------------------------------------ |
| Config | `~/.config/quota-router/config.json` |
