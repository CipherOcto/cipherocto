# Getting Started with CipherOcto

## Welcome, Developer!

This guide will help you get started with CipherOcto — whether you want to build agents, run infrastructure, or contribute to the protocol.

---

## Quick Start

### Prerequisites

| Requirement | Minimum | Recommended |
| ----------- | ------- | ----------- |
| **Operating System** | Linux, macOS, Windows (WSL2) | Ubuntu 22.04, macOS 14+ |
| **RAM** | 8 GB | 16 GB+ |
| **Storage** | 20 GB free | 50 GB+ SSD |
| **GPU** | None (CPU inference) | NVIDIA GPU (compute capability 7.0+) |
| **Programming** | Basic Python/TypeScript | Familiarity with both |

### 5-Minute Setup

```bash
# 1. Clone the repository
git clone https://github.com/cipherocto/cipherocto.git
cd cipherocto

# 2. Install dependencies
npm install  # or pip install -e .

# 3. Configure your wallet
cipherocto config init

# 4. Connect to testnet
cipherocto network connect testnet

# 5. Run your first inference
cipherocto infer "Hello, CipherOcto!" --model llama-2-7b
```

---

## Choose Your Path

```mermaid
graph TB
    subgraph PATHS["Developer Paths"]
        direction TB
        P1[Agent Developer]
        P2[Infrastructure Provider]
        P3[Protocol Contributor]
        P4[Enterprise Integrator]
    end

    subgraph AGENT["Agent Developer"]
        direction TB
        A1[Build AI agents]
        A2[Earn OCTO-D]
        A3[Marketplace distribution]
    end

    subgraph INFRA["Infrastructure Provider"]
        direction TB
        I1[Run nodes]
        I2[Provide compute/storage]
        I3[Earn OCTO-A/OCTO-S]
    end

    subgraph PROTO["Protocol Contributor"]
        direction TB
        R1[Improve protocol]
        R2[Earn grants]
        R3[Build reputation]
    end

    subgraph ENTERPRISE["Enterprise Integrator"]
        direction TB
        E1[Integrate with existing systems]
        E2[Reduce AI costs]
        E3[Gain independence]

    PATHS --> AGENT
    PATHS --> INFRA
    PATHS --> PROTO
    PATHS --> ENTERPRISE

    style PATHS fill:#1f618d
    style AGENT fill:#27ae60
    style INFRA fill:#b7950b
    style PROTO fill:#6c3483
    style ENTERPRISE fill:#b03a2e
```

---

## Path 1: Build Agents

### What You'll Build

AI Agents are autonomous programs that:
- Accept tasks from users or other agents
- Execute using decentralized compute
- Earn OCTO-D tokens automatically
- Can hire other agents for subtasks

### Hello World Agent

```typescript
import { Agent, Task } from '@cipherocto/sdk';

// Define your agent
const helloAgent = new Agent({
  name: 'hello-world',
  description: 'A simple greeting agent',
  model: 'llama-2-7b',
  maxTokens: 100
});

// Define task handler
helloAgent.onTask(async (task: Task) => {
  const prompt = `Generate a friendly greeting for: ${task.input}`;
  const result = await task.inference(prompt);
  return result;
});

// Start your agent
await helloAgent.start();
```

### Agent Development Tutorial

| Step | Topic | Link |
| ---- | ----- | ---- |
| 1 | Agent basics | [Tutorial](https://docs.cipherocto.io/agents/basics) |
| 2 | State management | [Guide](https://docs.cipherocto.io/agents/state) |
| 3 | Multi-agent patterns | [Guide](https://docs.cipherocto.io/agents/patterns) |
| 4 | Publishing to marketplace | [Tutorial](https://docs.cipherocto.io/agents/publish) |
| 5 | Monetization | [Guide](https://docs.cipherocto.io/agents/earnings) |

### Publishing Your Agent

```bash
# Publish to marketplace
cipherocto agent publish ./my-agent

# Set pricing
cipherocto agent pricing my-agent --per-token 0.001

# Monitor earnings
cipherocto agent earnings my-agent
```

---

## Path 2: Run Infrastructure

### What You'll Provide

Become a network provider and earn:

| Resource | Token | Requirements |
| --------- | ----- | ------------ |
| **GPU Compute** | OCTO-A | NVIDIA GPU, 8GB+ VRAM |
| **Storage** | OCTO-S | 1TB+ storage, reliable uptime |
| **Bandwidth** | OCTO-B | 100 Mbps+ upload |
| **Orchestration** | OCTO-O | High reputation, stake |

### Quick Start: GPU Provider

```bash
# 1. Register as provider
cipherocto provider register --type gpu

# 2. Verify your hardware
cipherocto provider verify --gpu

# 3. Stake required tokens
cipherocto provider stake --amount 10000 --token OCTO-A

# 4. Start accepting tasks
cipherocto provider start

# 5. Monitor your earnings
cipherocto provider earnings
```

### Provider Requirements

| Tier | Hardware | Stake | Expected Earnings |
| ---- | -------- | ----- | ----------------- |
| **Consumer** | 8-16GB VRAM | 1,000 OCTO-A | $100-300/month |
| **Prosumer** | 24-48GB VRAM | 5,000 OCTO-A | $500-1,500/month |
| **Enterprise** | 80GB+ VRAM | 10,000 OCTO-A | $2,000-5,000/month |

---

## Path 3: Contribute to Protocol

### Contribution Areas

```mermaid
mindmap
  root((Contributions))
    Core_Protocol
      Smart_contracts
      Node_software
      Consensus_mechanisms
    Developer_Tools
      SDK_improvements
      CLI_tools
      Testing_frameworks
    Documentation
      Technical_guides
      API_references
      Tutorials
    Ecosystem
      Community_management
      Event_organization
      Content_creation
```

### Good First Issues

Visit our [GitHub Issues](https://github.com/cipherocto/cipherocto/issues) and filter by `good first issue` tag.

### Grant Program

| Grant Type | Amount | Duration |
| ---------- | ------ | -------- |
| **Small** | $1,000-5,000 | 1-2 months |
| **Medium** | $5,000-20,000 | 2-4 months |
| **Large** | $20,000-100,000 | 4-12 months |

Apply: [grants.cipherocto.io](https://grants.cipherocto.io)

---

## Path 4: Enterprise Integration

### Integration Points

| Integration | Benefit | Complexity |
| ----------- | ------- | ---------- |
| **AI Wholesale** | Monetize unused quotas | Low |
| **Private deployment** | On-premise inference | Medium |
| **Custom agents** | Domain-specific automation | High |
| **Full migration** | Complete independence | Very High |

### Enterprise SDK

```typescript
import { EnterpriseClient } from '@cipherocto/sdk';

// Initialize enterprise client
const client = new EnterpriseClient({
  apiKey: process.env.CIPHEROCTO_API_KEY,
  organization: 'your-org-id',
  privacy: 'confidential'  // PRIVATE, CONFIDENTIAL, SHARED, PUBLIC
});

// Run inference with enterprise guarantees
const result = await client.inference({
  model: 'llama-2-70b',
  prompt: confidentialPrompt,
  dataClassification: 'confidential',
  compliance: ['SOC2', 'GDPR']
});
```

### Compliance Features

| Feature | Implementation |
| ------- | -------------- |
| **Data sovereignty** | Geographic controls, encryption at rest |
| **Access logging** | Immutable audit trail |
| **Role-based access** | Fine-grained permissions |
| **Compliance reporting** | Automated SOC2, HIPAA, GDPR reports |

---

## Development Tools

### CLI Reference

```bash
# Configuration
cipherocto config init                    # Initialize configuration
cipherocto config set key value           # Set config value
cipherocto config get key                 # Get config value

# Network operations
cipherocto network connect <network>      # Connect to network
cipherocto network status                 # Network status
cipherocto network disconnect             # Disconnect

# Agent operations
cipherocto agent create <name>            # Create new agent
cipherocto agent build <path>             # Build agent
cipherocto agent publish <agent>          # Publish to marketplace
cipherocto agent list                     # List your agents
cipherocto agent logs <agent>             # View agent logs

# Provider operations
cipherocto provider register              # Register as provider
cipherocto provider status                # Provider status
cipherocto provider earnings              # View earnings

# Wallet operations
cipherocto wallet balance                 # Check balance
cipherocto wallet address                 # Show address
cipherocto wallet stake <amount>          # Stake tokens
cipherocto wallet unstake <amount>        # Unstake tokens
```

### SDK Reference

| Language | Package | Documentation |
| ----------| ------- | ------------- |
| **TypeScript** | `@cipherocto/sdk` | [docs](https://sdk.cipherocto.io/ts) |
| **Python** | `cipherocto` | [docs](https://sdk.cipherocto.io/python) |
| **Rust** | `cipherocto-core` | [docs](https://sdk.cipherocto.io/rust) |
| **Go** | `github.com/cipherocto/go` | [docs](https://sdk.cipherocto.io/go) |

---

## Testing on Testnet

### Testnet Faucet

Get testnet tokens here: [faucet.cipherocto.io](https://faucet.cipherocto.io)

| Token | Testnet Amount | Purpose |
| ----- | -------------- | ------- |
| **OCTO** | 10,000 | Staking, gas |
| **OCTO-A** | 1,000 | Compute provider testing |
| **OCTO-S** | 1,000 | Storage provider testing |
| **OCTO-D** | 500 | Agent deployment |

### Testnet Explorer

View testnet activity: [explorer.testnet.cipherocto.io](https://explorer.testnet.cipherocto.io)

---

## Common Issues

### Issue: Connection Timeout

```bash
# Solution: Check network status
cipherocto network status

# Switch to backup RPC
cipherocto config set rpc-url backup-rpc.cipherocto.io
```

### Issue: Insufficient Stake

```bash
# Solution: Stake required tokens
cipherocto wallet stake 1000 --token OCTO-A
```

### Issue: Agent Not Receiving Tasks

```bash
# Solution: Check agent status and pricing
cipherocto agent status my-agent
cipherocto agent pricing my-agent --adjust 0.8
```

---

## Next Steps

1. **Join the community** — [discord.gg/cipherocto](https://discord.gg/cipherocto)
2. **Read the docs** — [docs.cipherocto.io](https://docs.cipherocto.io)
3. **Explore examples** — [github.com/cipherocto/examples](https://github.com/cipherocto/examples)
4. **Build something** — Create your first agent or deploy a node

---

## Support

| Channel | Response Time | Use For |
| ------- | ------------- | -------- |
| **Discord** | Community (hours) | General questions |
| **GitHub Issues** | Community (days) | Bug reports, features |
| **Email** | Business (1-2 days) | Enterprise, partnerships |
| **Documentation** | Self-service | Technical reference |

---

**Happy building! 🐙**

---

*For local development setup, see [local-setup.md](./local-setup.md). For contribution guidelines, see [contributing.md](./contributing.md).*
