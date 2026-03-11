# Design: Quota Router CLI (MVE)

## Overview

Minimum Viable Edition of the Quota Router CLI tool for managing AI API quotas.

## Decisions Made

| Decision      | Choice                   | Rationale                       |
| ------------- | ------------------------ | ------------------------------- |
| Wallet        | Mock/placeholder balance | Focus on CLI, wallet in Phase 2 |
| Proxy         | Transparent HTTP/HTTPS   | Realistic developer workflow    |
| Auth          | Environment variable     | Secure, simple for MVE          |
| HTTPS         | Self-signed cert         | Realistic production behavior   |
| Balance check | Hard block               | Clear failure mode              |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    quota-router-cli                          │
├─────────────────────────────────────────────────────────────┤
│  CLI Commands  │  Proxy Server  │  Config Store           │
└────────────────┴────────────────┴──────────────────────────┘
                              │
                       ┌──────▼──────┐
                       │    Core     │
                       │ - Balance   │
                       │ - Routing   │
                       │ - Providers │
                       └─────────────┘
```

## Data Flow

```
Developer App → Proxy (localhost:8080) → Balance Check → Forward + Inject API Key
                     │                          │
                     │                          └── OK → Forward
                     └── Insufficient → HTTP 402
```

## CLI Commands

```bash
quota-router init                    # Create ~/.quota-router/
quota-router provider add --name openai   # Add provider
quota-router balance                 # Show OCTO-W balance
quota-router list --prompts 100 --price 1  # List quota
quota-router proxy --port 8080      # Start HTTPS proxy
quota-router route --provider openai --prompt "Hello"  # Test route
```

## Key Implementation Details

### Balance Check

- Local config file stores balance (e.g., `config.yaml`)
- Check before every proxied request
- Decrement on success (mock - no real transaction)

### Proxy

- HTTPS with self-signed certificate
- Reads API key from `PROVIDER_NAME_API_KEY` env var
- Forwards to actual provider endpoint
- Returns response to client

### Config Location

- `~/.quota-router/config.yaml` - Main config
- `~/.quota-router/listings/` - Quota listings

## Error Handling

| Scenario             | Response                     |
| -------------------- | ---------------------------- |
| Balance < required   | HTTP 402 Payment Required    |
| Provider unreachable | HTTP 503 Service Unavailable |
| Invalid config       | Error with path              |
| Port in use          | Error with suggestion        |

## Testing

- Unit tests for balance logic
- Integration tests for proxy forwarding
- Mock provider for CLI tests

## Acceptance Criteria

- [ ] CLI tool installable via cargo
- [ ] HTTPS proxy intercepts requests
- [ ] API key from environment variable
- [ ] Balance display command
- [ ] Single provider routing
- [ ] Balance check before request
- [ ] Manual quota listing
- [ ] Unit tests

## RFC References

- RFC-0900 (Economics): AI Quota Marketplace Protocol
- RFC-0901 (Economics): Quota Router Agent Specification
- RFC-0102 (Numeric/Math): Wallet Cryptography Specification

---

**Created:** 2026-03-04
**Status:** Approved
