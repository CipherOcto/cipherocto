# CipherOcto Documentation

Welcome to the official CipherOcto documentation repository.

---

## Quick Links

| Audience | Start Here |
| -------- | ---------- |
| **Newcomers** | [Litepaper](../01-foundation/litepaper.md) — 10-minute read |
| **Investors** | [Investor Portal](../08-investors/README.md) |
| **Developers** | [Getting Started](../07-developers/getting-started.md) |
| **Partners** | [Partnership Strategy](../05-growth/partnerships.md) |
| **Everyone** | [Glossary](./GLOSSARY.md) |

---

## Documentation Structure

```
docs/
├── 00-meta/              # This folder
│   ├── GLOSSARY.md        # Terminology definitions
│   ├── STYLE-GUIDE.md     # Writing guidelines
│   └── README.md          # This file
│
├── 01-foundation/         # Core documents
│   ├── litepaper.md       # 10-minute overview
│   ├── manifesto.md       # Our philosophy
│   ├── roadmap.md         # Development timeline
│   └── whitepaper/
│       ├── v1.0-whitepaper.md    # Comprehensive whitepaper
│       ├── v0.1-draft-formatted.md
│       └── v0.1-draft.md
│
├── 02-product/           # Product information
│   ├── overview.md       # Product overview
│   ├── competitive-analysis.md  # Competition analysis
│   └── user-personas.md  # User profiles
│
├── 03-technology/        # Technical documentation
│   ├── ai-stack.md       # AI infrastructure
│   ├── system-architecture.md  # Ocean Stack
│   └── blockchain-integration.md  # Blockchain layer
│
├── 04-tokenomics/        # Economic model
│   ├── token-design.md   # Token system design
│   ├── distribution-schedule.md  # Token release schedule
│   └── governance.md     # Governance model
│
├── 05-growth/            # Growth & partnerships
│   ├── partnerships.md   # Partnership strategy
│   └── content-strategy.md  # Content & community
│
├── 06-operations/        # Operational documents
│   └── team/
│       └── org-chart.md  # Team structure
│
├── 07-developers/        # Developer resources
│   ├── getting-started.md  # Quick start
│   ├── local-setup.md    # Development environment
│   └── contributing.md   # Contribution guidelines
│
└── 08-investors/         # Investor resources
    └── README.md         # Investment overview
```

---

## Reading Order

### For Everyone

1. [Litepaper](../01-foundation/litepaper.md) — 10 minutes
2. [Manifesto](../01-foundation/manifesto.md) — 5 minutes
3. [Glossary](./GLOSSARY.md) — Reference as needed

### For Investors

1. [Litepaper](../01-foundation/litepaper.md)
2. [Whitepaper](../01-foundation/whitepaper/v1.0-whitepaper.md) — Executive Summary sections
3. [Investor Portal](../08-investors/README.md)
4. [Tokenomics](../04-tokenomics/token-design.md)

### For Developers

1. [Getting Started](../07-developers/getting-started.md)
2. [Product Overview](../02-product/overview.md)
3. [AI Stack](../03-technology/ai-stack.md)
4. [System Architecture](../03-technology/system-architecture.md)
5. [Contributing](../07-developers/contributing.md)

### For Partners

1. [Litepaper](../01-foundation/litepaper.md)
2. [Partnership Strategy](../05-growth/partnerships.md)
3. [User Personas](../02-product/user-personas.md)
4. [Competitive Analysis](../02-product/competitive-analysis.md)

### For Enterprise

1. [Product Overview](../02-product/overview.md)
2. [User Personas: Enterprise CTO](../02-product/user-personas.md)
3. [System Architecture](../03-technology/system-architecture.md)
4. [Blockchain Integration](../03-technology/blockchain-integration.md)

---

## Key Documents

| Document | Length | Audience | Summary |
| ---------- | ------ | -------- | -------- |
| **Litepaper** | 10 min | All | Quick overview |
| **Whitepaper** | 2-3 hours | Investors, Developers | Comprehensive |
| **Manifesto** | 5 min | All | Philosophy |
| **Roadmap** | 10 min | All | Timeline |
| **Token Design** | 30 min | Investors | Economics |
| **Governance** | 20 min | Token holders | Voting |

---

## Conventions

### Emojis

| Emoji | Meaning |
| ----- | ------- |
| 🐙 | Intelligence Layer |
| 🦑 | Execution Layer |
| 🪼 | Network Layer |
| ✅ | Complete, positive |
| ❌ | Incomplete, negative |
| 🔄 | In progress |
| 📅 | Planned |

### Status Indicators

| Status | Meaning |
| ------ | ------- |
| **Published** | Final, approved |
| **Draft** | Work in progress |
| **Deprecated** | Outdated, do not use |
| ** Confidential** | Restricted access |

---

## Contributing

### How to Contribute

1. **Report issues** — Open a GitHub issue
2. **Suggest improvements** — Start a discussion
3. **Submit changes** — Open a pull request
4. **Join the community** — [Discord](https://discord.gg/cipherocto)

### Style Guide

All documentation should follow the [Style Guide](./STYLE-GUIDE.md).

Key points:
- Use clear, concise language
- Write for accessibility
- Include examples where helpful
- Follow markdown conventions
- Test code examples

---

## Versioning

### Document Versions

| Document | Version | Last Updated |
| ---------- | ------- | ------------- |
| Litepaper | 1.0 | February 2026 |
| Whitepaper | 1.0 | February 2026 |
| Manifesto | 1.0 | February 2026 |
| Roadmap | 1.0 | February 2026 |
| Token Design | 1.0 | February 2026 |

### Update Policy

| Document Type | Update Frequency |
| ------------- | ---------------- |
| Whitepaper | Quarterly |
| Litepaper | Quarterly |
| Roadmap | Monthly |
| Technical docs | Per release |
| API docs | Continuous |

---

## Formatting

### Build

```bash
# Install dependencies
npm install

# Build documentation
npm run docs:build

# Serve locally
npm run docs:serve
```

### Lint

```bash
# Lint markdown files
npm run docs:lint

# Fix auto-fixable issues
npm run docs:lint:fix
```

---

## Search

### Finding Content

Use the built-in search or check these indexes:

- [Glossary](./GLOSSARY.md) — Term definitions
- [Style Guide](./STYLE-GUIDE.md) — Writing conventions
- [GitHub Issues](https://github.com/cipherocto/cipherocto/issues) — Known issues

---

## Support

| Channel | Best For | Response Time |
| ------- | ----------------- | -------------- |
| **Documentation** | Self-service | Immediate |
| **Discord #docs** | Questions | Hours |
| **GitHub Issues** | Bugs, suggestions | Days |
| **Email** | Private inquiries | 1-2 days |

---

## License

All documentation is licensed under the same license as the CipherOcto protocol.

---

## Acknowledgments

Documentation is maintained by the CipherOcto team with contributions from the community.

Special thanks to all contributors who improve these documents.

---

**Quick Start:** Read the [Litepaper](../01-foundation/litepaper.md) (10 minutes)

🐙 **Private intelligence, everywhere.**

---

*Last updated: February 2026*
