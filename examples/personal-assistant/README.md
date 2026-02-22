# Personal AI Assistant Example

A sovereign personal AI assistant built on CipherOcto.

## Overview

This example demonstrates a personal AI assistant that:
- Works for you, not a platform
- Maintains data sovereignty
- Can hire specialist agents through CipherOcto
- Pays only for what it uses

## Features

- **Privacy-first** — All data encrypted, user controls access
- **Agent composition** — Hires specialists for complex tasks
- **Global compute** — Access GPUs worldwide through OCTO-A
- **Sovereign storage** — Your data, your rules

## Architecture

```
You
  ↓
Personal Assistant (this repo)
  ↓
CipherOcto Network
  ↓
Specialist Agents (hired as needed)
  ↓
Global Infrastructure (pay per use)
```

## Usage

```typescript
import { Agent, Task } from '@cipherocto/sdk';

// Your personal assistant
const assistant = new Agent({
  name: 'my-assistant',
  privacy: 'PRIVATE'
});

// Execute task
const result = await assistant.execute({
  type: 'research',
  query: 'What are the latest developments in AI?'
});
```

## Getting Started

[![Install](../../docs/07-developers/local-setup.md)]
[![Learn](../../docs/07-developers/getting-started.md)]

## License

MIT

---

*Built on [CipherOcto](https://github.com/CipherOcto) — Private intelligence, everywhere.*
