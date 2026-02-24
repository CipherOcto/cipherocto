# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Status

**IMPORTANT:** This is a documentation-only repository in seed stage development. There is NO implementation code yet. The repository contains architectural planning, whitepapers, and tokenomics documentation for a future decentralized AI platform.

## Project Overview

CipherOcto is a planned next-generation private AI assistant platform designed to run across local infrastructure, private cloud, edge deployments, and hybrid blockchain networks. The mission is to build a sovereign intelligence layer where AI agents can reason privately, execute autonomously, coordinate securely, and operate anywhere.

## The Ocean Stack (Conceptual Architecture)

```
User / Organization
↓
CipherOcto Assistant 🐙 (Intelligence Layer)
↓
Agent Orchestrator
↓
Secure Execution Runtime 🦑 (Execution Layer)
↓
Hybrid Network Mesh 🪼 (Network Layer)
(Local Nodes + Blockchain Verification)
```

Design philosophy: **many agents, one intelligence**

## Planned Modules

Not yet implemented - these are architectural plans:

- Assistant Core
- Agent Runtime
- Local Inference Engine
- Secure Execution Sandbox
- Node Identity System (OCTO-ID)
- Hybrid Blockchain Coordination
- Developer SDK
- Deployment Toolkit

## Documentation Structure

Key documentation is in `/docs`:

- `01-foundation/whitepaper/v0.1-draft.md` - Comprehensive whitepaper covering the trust & reputation architecture, autonomous market layer, and data sovereignty via data flagging
- `04-tokenomics/token-design.md` - Detailed multi-token economy design with role-based tokens (OCTO sovereign token + specialized role tokens)

## Core Architectural Concepts

### Data Flagging System

Every dataset/interaction is tagged with privacy levels:
- `PRIVATE` - Encrypted, local-only, never enters marketplace
- `CONFIDENTIAL` - Restricted to trusted agents
- `SHARED` - Allowed marketplace access
- `PUBLIC` - Monetizable dataset

### Proof of Reliability (PoR)

Trust emerges from:
- OCTO-ID (persistent identity)
- Stake (economic commitment)
- Performance (measurable outcomes)
- Reputation Score (long-term trust)
- Social Validation (ecosystem feedback)

### Multi-Token Economy

- `OCTO` - Sovereign token for governance, staking, settlement
- Role tokens (OCTO-A, OCTO-B, OCTO-O, OCTO-W, etc.) - For specialized providers

### Dual-Stake Model

Every participant stakes both OCTO (global alignment) + Role Token (local specialization) to prevent role tourism.

## Development Workflow

Since this is documentation-only, there are no build, test, or lint commands.

### Documentation Script

```bash
scripts/init-docs.sh
```

Creates the documentation directory structure in `/docs`.

## Repository Conventions

- Marine life emoji theme: 🐙 (assistant), 🦑 (execution), 🪼 (network)
- Project tagline: "Private intelligence, everywhere"
- Philosophy: AI should be private by default, distributed by design, sovereign by choice

## Branch Strategy

CipherOcto uses **Trunk-Based + Feature Streams**:

| Branch | Purpose | Protection |
|--------|---------|------------|
| `main` | Always releasable | PR only, all checks, 1+ approval |
| `next` | Integration lane | CI required, direct push OK |
| `feat/*` | Contributor features | CI required |
| `agent/*` | AI-generated work | CI required + extra review |
| `research/*` | Experimental | CI required |
| `hotfix/*` | Emergency fixes | PR to main |

**Golden Rule:** Nobody pushes directly to `main`.

Full documentation: `.github/BRANCH_STRATEGY.md`
Branch protection rules: `.github/branch-protection-rules.md`

<!-- gitnexus:start -->
# GitNexus MCP

This project is indexed by GitNexus as **cipherocto** (60 files, 120 symbols, 2 execution flows).

GitNexus provides a knowledge graph over this codebase — call chains, blast radius, execution flows, and semantic search.

## Always Start Here

For any task involving code understanding, debugging, impact analysis, or refactoring, you must:

1. **Read `gitnexus://repo/{name}/context`** — codebase overview + check index freshness
2. **Match your task to a skill below** and **read that skill file**
3. **Follow the skill's workflow and checklist**

> If step 1 warns the index is stale, run `npx gitnexus analyze` in the terminal first.

## Skills

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/refactoring/SKILL.md` |

## Tools Reference

| Tool | What it gives you |
|------|-------------------|
| `query` | Process-grouped code intelligence — execution flows related to a concept |
| `context` | 360-degree symbol view — categorized refs, processes it participates in |
| `impact` | Symbol blast radius — what breaks at depth 1/2/3 with confidence |
| `detect_changes` | Git-diff impact — what do your current changes affect |
| `rename` | Multi-file coordinated rename with confidence-tagged edits |
| `cypher` | Raw graph queries (read `gitnexus://repo/{name}/schema` first) |
| `list_repos` | Discover indexed repos |

## Current Codebase Structure

**Functional Areas (Clusters):**
- Cluster_1: 5 symbols, 89% cohesion

**Execution Flows (Processes):**
- Main → Open_db: 4 steps (cross-community)
- Main → Execute_agent: 3 steps (intra-community)

## Resources Reference

Lightweight reads (~100-500 tokens) for navigation:

| Resource | Content |
|----------|---------|
| `gitnexus://repo/{name}/context` | Stats, staleness check |
| `gitnexus://repo/{name}/clusters` | All functional areas with cohesion scores |
| `gitnexus://repo/{name}/cluster/{clusterName}` | Area members |
| `gitnexus://repo/{name}/processes` | All execution flows |
| `gitnexus://repo/{name}/process/{processName}` | Step-by-step trace |
| `gitnexus://repo/{name}/schema` | Graph schema for Cypher |

## Graph Schema

**Nodes:** File, Function, Class, Interface, Method, Community, Process
**Edges (via CodeRelation.type):** CALLS, IMPORTS, EXTENDS, IMPLEMENTS, DEFINES, MEMBER_OF, STEP_IN_PROCESS

```cypher
MATCH (caller)-[:CodeRelation {type: 'CALLS'}]->(f:Function {name: "myFunc"})
RETURN caller.name, caller.filePath
```

<!-- gitnexus:end -->
