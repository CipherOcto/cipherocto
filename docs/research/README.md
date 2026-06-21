# Research & Feasibility Studies

This folder contains research reports and feasibility studies that inform CipherOcto's technical decisions.

## Purpose

Research reports come **before** Use Cases in the development workflow. They investigate whether a technology or approach is worth pursuing before committing to a full specification.

## How It Works

```
Idea
  ↓
Research Report (feasibility, technology analysis)
  ↓
Use Case (if research shows viability)
  ↓
RFC (technical specification)
  ↓
Mission (implementation)
```

## Contents

| Report                                                       | Status   | Summary                                  |
| ------------------------------------------------------------ | -------- | ---------------------------------------- |
| [ZKP_Research_Report.md](./ZKP_Research_Report.md)           | Complete | Zero-knowledge proofs landscape analysis |
| [cairo-ai-research-report.md](./cairo-ai-research-report.md) | Complete | Cairo AI integration feasibility         |
| [litellm-analysis-and-quota-router-comparison.md](./litellm-analysis-and-quota-router-comparison.md) | **Approved** | LiteLLM analysis and quota-router gaps   |
| [stoolap-research.md](./stoolap-research.md) | Complete | Original Stoolap embedded-SQL capability catalogue |
| [stoolap-integration-research.md](./stoolap-integration-research.md) | Complete | Stoolap × AI Quota Marketplace integration |
| [stoolap-determinism-analysis.md](./stoolap-determinism-analysis.md) | Complete | Stoolap determinism (RFC-0104) compliance |
| [stoolap-data-sync-via-cipherocto-network.md](./stoolap-data-sync-via-cipherocto-network.md) | **Draft** | Two-node data sync for the Stoolap fork via the CipherOcto network (this is the missing feature) |
| [stoolap-dep-on-cipherocto-circular-avoidance.md](./stoolap-dep-on-cipherocto-circular-avoidance.md) | **Draft** | Reversing the Stoolap → CipherOcto dependency: how to avoid Cargo workspace cycles when adding cipherocto network as a dep of the Stoolap fork (extracts an `octo-sync` leaf workspace, mirroring the `octo-determin` pattern) |
| [octo-sync-database-adapter-trait.md](./octo-sync-database-adapter-trait.md) | **Draft** | Phase 2 of the dep-avoidance research: the `DatabaseSyncAdapter` trait that abstracts the database operations the cipherocto sync engine needs (WAL read/apply, snapshot read/write, backpressure, identity). Sync (not async) per the cipherocto convention for compute/state traits. 8 methods, `Send + Sync`, `Result<T, SyncError>`. |
| [deterministic-overlay-transport.md](./deterministic-overlay-transport.md) | In progress | Source scratch pad for the networking RFC family (DOT, GDP, DGP, OCrypt, MON, DRS, DOM, ORR) |
| [networking-rfc-cross-reference-analysis.md](./networking-rfc-cross-reference-analysis.md) | Complete | Audit of the 11 networking RFCs and their dependencies |
| [2026-06-21-telegram-pure-rust-mtproto-adapter.md](./2026-06-21-telegram-pure-rust-mtproto-adapter.md) | Complete | Pure-Rust MTProto Telegram adapter (grammers) to replace TDLib C++ dependency |

## Research vs RFC

| Research Report          | RFC (Request for Comments) |
| ------------------------ | -------------------------- |
| Investigates feasibility | Specifies solution         |
| Explores options         | Makes decisions            |
| Informs direction        | Defines implementation     |
| Pre-decision             | Post-decision              |

## Contributing

To create a new research report:

1. Create a new markdown file in this folder
2. Follow the research template below
3. Submit as PR for review
4. If accepted → informs Use Case creation

## Template

```markdown
# Research: [Technology/Approach Name]

## Executive Summary

Brief overview of what this research investigates.

## Problem Statement

What challenge are we investigating solutions for?

## Research Scope

- What's included
- What's excluded

## Findings

### Technology A

### Technology B

### Analysis

## Recommendations

- Recommended approach
- Risks and mitigations

## Next Steps

- Create Use Case? (Yes/No)
- Related technologies to explore
```

---

_Research drives informed decisions. The Blueprint ensures research leads to action._
