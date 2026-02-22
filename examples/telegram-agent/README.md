# Telegram AI Agent Example

An autonomous Telegram bot powered by CipherOcto infrastructure.

## Overview

This example demonstrates an AI agent that:
- Integrates with Telegram
- Runs on decentralized compute (OCTO-A)
- Stores conversations privately (OCTO-S)
- Pays only for resources used

## Features

- **24/7 availability** — No server management required
- **Privacy** — Encrypted message storage
- **Scalable** — Handles thousands of concurrent users
- **Cost efficient** — Pay per message, not per server

## Quick Start

```typescript
import { TelegramAgent } from '@cipherocto/sdk';

// Create Telegram bot
const bot = new TelegramAgent({
  token: process.env.TELEGRAM_BOT_TOKEN,
  model: 'llama-2-70b',
  storage: 'ENCRYPTED',  // OCTO-S
  compute: 'ON_DEMAND'  // OCTO-A
});

// Bot handles messages automatically
bot.start();
```

## Architecture

```
Telegram
  ↓
Bot Agent (this repo)
  ↓
CipherOcto Orchestrator (OCTO-O)
  ↓
Compute Providers (OCTO-A)
```

## Cost Comparison

| Model | Monthly Cost | CipherOcto |
|-------|-------------|------------|
| **VPS Server** | $20-100/month | $0 (pay per use) |
| **GPU Instance** | $300-1000/month | $0 (pay per use) |
| **Scaling** | Manual | Automatic |

## Getting Started

[![Install](../../docs/07-developers/local-setup.md)]
[![Learn](../../docs/07-developers/getting-started.md)]

## License

MIT

---

*Built on [CipherOcto](https://github.com/CipherOcto) — Private intelligence, everywhere.*
