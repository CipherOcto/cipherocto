# Research: ZeroClaw Architecture

**Date:** 2026-05-26
**Status:** Research (v2 -- post CocoIndex verification)
**Source:** Source code analysis of `zeroclaw` v0.8.0-beta-1 (zeroclaw-labs/zeroclaw)
**Index Stats:** 1,132 files, 29,325 chunks, 1,787 symbols, 4,666 import edges, 772 Rust files (418,710 LOC)

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [System Architecture](#2-system-architecture)
3. [Crate Topology](#3-crate-topology)
4. [Agent Runtime](#4-agent-runtime)
5. [Tools System](#5-tools-system)
6. [Provider Layer](#6-provider-layer)
7. [Channel System](#7-channel-system)
8. [Security Model](#8-security-model)
9. [Memory System](#9-memory-system)
10. [Plugin System (WASM)](#10-plugin-system-wasm)
11. [Hardware & Peripherals](#11-hardware--peripherals)
12. [Skills & SkillForge](#12-skills--skillforge)
13. [Observability](#13-observability)
14. [Gateway & Frontend](#14-gateway--frontend)
15. [Codebase Analysis](#15-codebase-analysis)
16. [Data Flow Diagrams](#16-data-flow-diagrams)

---

## 1. Project Overview

ZeroClaw is a Rust-first autonomous agent runtime optimized for performance, efficiency, stability, extensibility, sustainability, and security. It runs on your own devices, communicates across 35 messaging channels, supports 14 LLM providers, has 95+ built-in tools, and extends through WASM plugins and hardware peripherals.

| Property | Value | Evidence |
|----------|-------|----------|
| **Version** | 0.8.0-beta-1 | `Cargo.toml:24` |
| **Language** | Rust (edition 2024, rustc >=1.87) | `Cargo.toml:25-27` |
| **License** | MIT OR Apache-2.0 | `Cargo.toml:26` |
| **Architecture** | Cargo workspace (17 crates + root + apps/tools/xtask) | `Cargo.toml:1-21` |
| **Total Rust Files** | 772 | `find` analysis |
| **Total Rust LOC** | 418,710 | `find + wc` analysis |
| **Code Chunks** | 29,325 | CocoIndex |
| **Extracted Symbols** | 1,787 | CocoIndex (regex-based) |
| **Import Edges** | 4,666 | CocoIndex |
| **Channels** | 35 implementations (34 files) | `impl Channel for` grep in `crates/zeroclaw-channels/src/` |
| **LLM Providers** | 14 implementations | `crates/zeroclaw-providers/src/` (excl. factory/reliable/router) |
| **Tools** | 95+ (69 in `src/tools/`, 26 in `crates/zeroclaw-runtime/src/tools/`) | Direct file count |
| **Memory Backend Kinds** | 7 kinds + 16 feature modules | `MemoryBackendKind` enum in `backend.rs` |
| **Security Modules** | 15 + 6 infrastructure | `crates/zeroclaw-runtime/src/security/` |
| **Firmware Targets** | 8 (Arduino, ESP32, ESP32-UI, Pico, Nucleo, Uno Q Bridge, FW Protocol, Nucleo FW) | `firmware/` |
| **Native App** | Tauri desktop app | `apps/tauri/` |

### 1.1 Design Philosophy

```mermaid
graph LR
    A["Performance<br/>Zero-cost abstractions, async Tokio"] --> B["Security-First<br/>Landlock, bubblewrap, prompt guard"]
    B --> C["Trait-Driven<br/>Modular, plugin architecture"]
    C --> D["Hardware-Aware<br/>GPIO, serial, USB discovery"]
    D --> E["Open Source<br/>MIT / Apache-2.0"]

    style A fill:#e3f2fd
    style B fill:#fce4ec
    style C fill:#fff3e0
    style D fill:#e8f5e9
    style E fill:#f3e5f5
```

### 1.2 Key Differentiators

| Dimension | ZeroClaw | IronClaw | OpenClaw |
|-----------|----------|----------|----------|
| **Language** | Rust | Rust | TypeScript |
| **Channels** | 35 (incl. WeChat, WeCom, Lark, DingTalk, QQ) | ~6 | 25+ |
| **Providers** | 14 (OpenAI, Anthropic, Gemini, Ollama, Bedrock, Azure, OpenRouter, Copilot, GLM, Telnyx, Gemini CLI, OpenAI Codex, Compatible, KiloCLI) | 10+ | Multi via adapters |
| **Tools** | 95+ built-in tools | ~20 | ~30 |
| **Security** | 15 modules (Landlock, bubblewrap, Docker, firejail, seatbelt, Nevis, prompt guard, leak detector, secrets, OTP, WebAuthn, pairing, IAM, E-stop, audit) | WASM sandbox + capabilities | Safe defaults |
| **Hardware** | GPIO, serial, USB, firmware (Pico/Nucleo/ESP32) | None | None |
| **Plugins** | WASM component model | WASM modules + MCP | npm + plugin SDK |
| **Memory** | 7 backend kinds + 16 feature modules (knowledge graph, consolidation, decay, embeddings) | libSQL/Postgres | SQLite + LanceDB |
| **Sandboxing** | Landlock + bubblewrap + firejail + seatbelt + Docker + Nevis | Wasmtime WASM | Docker/Podman |

---

## 2. System Architecture

### 2.1 High-Level Architecture

```mermaid
graph TB
    subgraph UserLayer["User Interfaces"]
        direction LR
        CLI[CLI<br/>zeroclaw binary]
        TUI[TUI<br/>zeroclaw-tui crate]
        WebUI[Web Gateway<br/>zeroclaw-gateway crate]
        ACP[IDE Bridge<br/>zeroclaw-acp-bridge]
        Tauri[Tauri Desktop<br/>apps/tauri]
    end

    subgraph ChannelLayer["Channel System (35 channels)"]
        direction LR
        subgraph CNA["China/Asia"]
            WX[WeChat]
            WC[WeCom + WeCom WS]
            LK[Lark + DingTalk]
            QQ[QQ]
            LN[LINE]
        end
        subgraph Western["Western"]
            TG[Telegram]
            DC[Discord]
            SL[Slack]
            WA[WhatsApp + WA Web]
            SG[Signal]
            IM[iMessage]
            EM[Email + Gmail Push]
        end
        subgraph Other["Other"]
            MT[Matrix]
            MM[Mattermost]
            IRC[IRC]
            NS[Nostr]
            BS[Bluesky]
            RD[Reddit]
            TW[Twitter]
            NT[Notion]
            WH[Webhook]
            NC[Nextcloud Talk]
            WT[Wati]
            CD[ClawdTalk + MochaTalk]
            LQ[Linq]
            ACP2[ACP + CLI]
            VC[Voice Call + Voice Wake]
        end
    end

    subgraph AgentCore["Agent Runtime"]
        direction TB
        AG[Agent Loop<br/>21 internal modules]
        DP[Dispatcher]
        SL2[Scheduler<br/>cron + routines]
        SOP[SOP Engine<br/>routines/]
        SUB[Subagent<br/>spawn + manage]
        SF[SkillForge<br/>skill generation]
        HB[Heartbeat<br/>proactive execution]
        TV[Trust + Verifiable Intent]
    end

    subgraph Security["Security Layer (15 modules)"]
        direction LR
        LL[Landlock]
        BW[Bubblewrap]
        FJ[Firejail]
        SB[Seatbelt]
        NV[Nevis]
        DK[Docker Sandbox]
        PG[Prompt Guard]
        LD[Leak Detector]
        SK[Secrets]
        OTP[OTP + WebAuthn]
        PR[Pairing]
        IAM[IAM Policy]
        EST[E-Stop]
        AUD[Audit]
    end

    subgraph Providers["LLM Providers (14)"]
        direction LR
        OA[OpenAI]
        OAC[OpenAI Codex]
        AN[Anthropic]
        GM[Gemini]
        GC[Gemini CLI]
        OL[Ollama]
        BD[Bedrock]
        AZ[Azure OpenAI]
        OR[OpenRouter]
        CP[Copilot]
        GL[GLM]
        TK[Telnyx]
        CM[Compatible<br/>Generic OpenAI-compat]
        KC[KiloCLI]
    end

    subgraph ProviderInfra["Provider Infrastructure"]
        direction LR
        FC[Factory<br/>Instantiation]
        RL[Reliable<br/>Circuit breaker]
        RT[Router<br/>Smart routing]
        CAT[Catalog + Models.dev]
    end

    subgraph Storage["Memory & Storage"]
        direction LR
        SQ[(SQLite)]
        MD[(Markdown)]
        PG2[(PostgreSQL + pgvector)]
        QD[(Qdrant)]
        LUC[Lucid<br/>Consolidation]
        KG[Knowledge Graph]
        EMB[Embeddings + Vector]
        DEC[Decay + Importance]
    end

    subgraph HW["Hardware Layer"]
        direction LR
        USB[USB Discovery]
        GPIO[GPIO]
        SER[Serial]
        FW[Firmware<br/>Pico/Nucleo/ESP32]
        RK[Robot Kit]
    end

    subgraph ToolsLayer["Tools (95+)"]
        direction LR
        FS[File/Shell<br/>edit, read, write, glob]
        BW2[Browser<br/>browse, screenshot, fetch]
        MM[MCP<br/>client, protocol, transport]
        HW2[Hardware<br/>board info, memory]
        CL[Cloud<br/>ops, patterns, git]
        SO[Social<br/>discord, linkedin, notion]
        AG2[Agent<br/>subagent, delegate, escalate]
        SK2[Skills + SOP]
    end

    UserLayer --> ChannelLayer
    ChannelLayer --> AgentCore
    AgentCore --> Security
    AgentCore --> Providers
    ProviderInfra --> Providers
    AgentCore --> Storage
    AgentCore --> HW
    AgentCore --> ToolsLayer

    style UserLayer fill:#e3f2fd
    style ChannelLayer fill:#e8f5e9
    style AgentCore fill:#fce4ec
    style Security fill:#fff3e0
    style Providers fill:#f3e5f5
    style ProviderInfra fill:#f3e5f5
    style Storage fill:#e0f2f1
    style ToolsLayer fill:#fce4ec
    style HW fill:#ffebee
```

### 2.2 Component Map

```mermaid
graph LR
    subgraph Entry["Entry Points"]
        E1["src/main.rs<br/>CLI dispatch"]
        E2["src/bin/zeroclaw-acp-bridge.rs<br/>IDE integration"]
        E3["crates/zeroclaw-gateway/<br/>Webhook daemon"]
    end

    subgraph Core["Core Runtime (src/ -- 45+ modules)"]
        G1[agent/<br/>Agent loop]
        G2[channels/<br/>Channel orchestration]
        G3[commands/<br/>CLI commands]
        G4[config/<br/>Configuration]
        G5[cron/<br/>Scheduled tasks]
        G6[heartbeat/<br/>Proactive execution]
        G7[hooks/<br/>Event hooks]
        G8[memory/<br/>Memory CLI + tests]
        G9[nodes/<br/>Node management]
        G10[observability/<br/>Tracing + metrics]
        G11[onboard/<br/>Onboarding wizard]
        G12[platform/<br/>Platform detection]
        G13[security/<br/>Security orchestration]
        G14[skillforge/<br/>Skill generation]
        G15[skills/<br/>Skill management]
        G16[sop/<br/>Standard operating procedures]
        G17[routines/<br/>Routine execution]
        G18[approval/<br/>Human-in-the-loop]
        G19[trust/<br/>Trust policy]
        G20[verifiable_intent/<br/>Intent verification]
        G21[rag/<br/>RAG pipeline]
        G22[tools/<br/>69 tool implementations]
        G23[integrations/<br/>External integrations]
        G24[multimodal.rs<br/>Multimodal support]
    end

    subgraph Crates["Workspace Crates"]
        C1[zeroclaw-api<br/>Public trait definitions]
        C2[zeroclaw-config<br/>Schema + loading]
        C3[zeroclaw-log<br/>Unified log surface]
        C4[zeroclaw-providers<br/>14 LLM providers + infra]
        C5[zeroclaw-channels<br/>35 channels + orchestrator]
        C6[zeroclaw-tools<br/>Tool execution]
        C7[zeroclaw-memory<br/>7 backends + 16 features]
        C8[zeroclaw-runtime<br/>Agent loop + security + tools]
        C9[zeroclaw-infra<br/>Shared infra]
        C10[zeroclaw-gateway<br/>Webhook server]
        C11[zeroclaw-tui<br/>TUI wizard]
        C12[zeroclaw-plugins<br/>WASM plugins]
        C13[zeroclaw-hardware<br/>USB/GPIO/serial]
        C14[zeroclaw-tool-call-parser<br/>Tool call parsing]
        C15[zeroclaw-macros<br/>Derive macros]
        C16[robot-kit<br/>Robot abstraction]
        C17[aardvark-sys<br/>Native bindings]
    end

    E1 --> Core
    E2 --> Core
    E3 --> Core
    Core --> Crates
```

---

## 3. Crate Topology

ZeroClaw organizes its codebase into 17 workspace crates with a trait-driven, layered architecture. Extension points are defined in `zeroclaw-api` and implemented by domain crates.

```mermaid
graph TB
    subgraph Layer0["Layer 0: Foundation"]
        API[zeroclaw-api<br/>Public trait definitions<br/>Provider, Channel, Tool, Memory, Observer, Peripheral]
        INFRA[zeroclaw-infra<br/>Debounce, session, stall watchdog]
        CFG[zeroclaw-config<br/>Schema, config loading/merging]
        LOG[zeroclaw-log<br/>record! macro, JSONL, broadcast hook]
        MACROS[zeroclaw-macros<br/>Configurable derive macro]
    end

    subgraph Layer1["Layer 1: Domain Crates"]
        direction LR
        PROV[zeroclaw-providers<br/>14 LLM providers + resilient wrapper + smart router]
        CHAN[zeroclaw-channels<br/>35 channels + orchestrator]
        TOOLS2[zeroclaw-tools<br/>Shell, file, memory, browser tools]
        MEM[zeroclaw-memory<br/>7 backends + 16 feature modules]
    end

    subgraph Layer2["Layer 2: Composition"]
        direction LR
        RT[zeroclaw-runtime<br/>Agent loop, security, cron, SOP, skills, tools, onboarding]
        GW[zeroclaw-gateway<br/>Webhook/gateway server]
        HW[zeroclaw-hardware<br/>USB, peripherals, serial, GPIO]
        PLUG[zeroclaw-plugins<br/>WASM plugin system]
    end

    subgraph Layer3["Layer 3: User-Facing"]
        direction LR
        TUI[zeroclaw-tui<br/>TUI onboarding wizard]
        TCP[zeroclaw-tool-call-parser<br/>Tool call parsing]
        RK[robot-kit<br/>Robot abstraction]
        ASYS[aardvark-sys<br/>Native bindings]
    end

    Layer0 --> Layer1
    Layer1 --> Layer2
    Layer2 --> Layer3

    style Layer0 fill:#e3f2fd
    style Layer1 fill:#e8f5e9
    style Layer2 fill:#fff3e0
    style Layer3 fill:#fce4ec
```

### 3.1 Crate Details

| Crate | Tier | LOC (lib.rs) | Role |
|-------|------|-------------|------|
| `zeroclaw-api` | Experimental | 47 | Trait definitions (Provider, Channel, Tool, Memory, Observer, Peripheral) |
| `zeroclaw-config` | Beta | 44 | Schema, config loading/merging, TOML-based |
| `zeroclaw-log` | Beta | 82 | Unified log emission, JSONL persistence, broadcast hook |
| `zeroclaw-macros` | Beta | 2,205 | `#[derive(Configurable)]` and other procedural macros |
| `zeroclaw-infra` | Beta | 163 | Debounce, session management, stall watchdog |
| `zeroclaw-providers` | Beta | 3,314 | 14 LLM providers, factory, resilient wrapper, smart router, catalog |
| `zeroclaw-channels` | Experimental | 89 (lib) | 35 channel adapters, orchestrator (lifecycle, media pipeline, MQTT) |
| `zeroclaw-tools` | Experimental | 73 | Shell, file, memory, browser tool execution |
| `zeroclaw-memory` | Beta | 907 | 7 backend kinds + 16 feature modules (KG, consolidation, decay, embeddings) |
| `zeroclaw-runtime` | Experimental | 42 (lib) | Agent loop (21 modules), security (15+6), tools (26), cron, SOP, skills, onboarding |
| `zeroclaw-gateway` | Experimental | 5,211 | Webhook/gateway server, separate binary |
| `zeroclaw-tui` | Experimental | 10 | TUI onboarding wizard |
| `zeroclaw-plugins` | Experimental | 91 | WASM plugin system (host, runtime, signature, WASM channel/tool) |
| `zeroclaw-hardware` | Experimental | 747 | USB discovery, peripherals, serial, GPIO, firmware flashing |
| `zeroclaw-tool-call-parser` | Beta | 3,834 | Tool call parsing from LLM output |
| `robot-kit` | Experimental | 154 | Robot hardware abstraction |
| `aardvark-sys` | Experimental | 483 | Native system bindings |

---

## 4. Agent Runtime

### 4.1 Agent Loop Architecture

The agent runtime lives in `crates/zeroclaw-runtime/` and `src/agent/`. It orchestrates the full lifecycle from message receipt to response delivery.

```mermaid
graph TB
    subgraph Input["Input Sources"]
        CH[Channel Message]
        CR[Cron Trigger]
        HB[Heartbeat]
        SOP2[SOP Trigger]
        CLI2[CLI Input]
    end

    subgraph Runtime["Agent Runtime"]
        direction TB
        AL[Agent Loop<br/>21 internal modules]
        DP[Dispatcher]
        AP[Approval Gate<br/>Human-in-the-loop]
        SEC[Security Orchestrator<br/>15 modules]
    end

    subgraph Processing["Processing Pipeline"]
        direction TB
        CLS[Classifier]
        CTX[Context Analyzer + Compressor]
        PC[Prompt Construction]
        LLM[LLM Provider Call]
        TC[Tool Call Parsing]
        TE[Tool Execution]
        RL[Response Loop]
        LD[Loop Detector]
    end

    subgraph Output["Output"]
        RESP[Response Delivery]
        MEM2[Memory Storage]
        OBS[Observability Log]
        HOOK[Hook Trigger]
    end

    Input --> Runtime
    Runtime --> Processing
    Processing --> Output
    Processing -->|tool calls| Processing

    style Input fill:#e3f2fd
    style Runtime fill:#fce4ec
    style Processing fill:#fff3e0
    style Output fill:#e8f5e9
```

### 4.2 Runtime Modules (Top-Level)

The runtime crate (`crates/zeroclaw-runtime/src/`) contains 36 top-level modules:

| Module | Path | Role |
|--------|------|------|
| **Agent** | `agent/` | Core agent loop with 21 internal modules (see 4.3) |
| **Approval** | `approval/` | Human-in-the-loop approval workflows |
| **Browse** | `browse.rs` | Browser automation |
| **CLI Input** | `cli_input.rs` | CLI input handling |
| **Cost** | `cost/` | Cost tracking |
| **Cron** | `cron/` | Scheduled task execution |
| **Daemon** | `daemon/` | Background daemon management |
| **Doctor** | `doctor/` | Health diagnostics |
| **Firmware** | `firmware/` | Firmware management |
| **Health** | `health/` | Health check endpoints |
| **Heartbeat** | `heartbeat/` | Proactive agent execution on intervals |
| **Hooks** | `hooks/` | Event hook system |
| **i18n** | `i18n.rs` | Internationalization (Fluent strings) |
| **Identity** | `identity.rs` | Agent identity management |
| **Integrations** | `integrations/` | External integrations |
| **Migration** | `migration.rs` | Data migration |
| **Nodes** | `nodes/` | Node management |
| **Observability** | `observability/` | Tracing + metrics (7 backends) |
| **Onboard** | `onboard/` | First-run onboarding wizard |
| **Peers** | `peers.rs` | Peer management |
| **Platform** | `platform/` | Platform detection |
| **Process Stats** | `process_stats.rs` | Process statistics |
| **RAG** | `rag/` | Retrieval-augmented generation |
| **Routines** | `routines/` | Routine execution engine |
| **Security** | `security/` | 15 security modules + 6 infrastructure |
| **Service** | `service/` | Service lifecycle management |
| **SkillForge** | `skillforge/` | Autonomous skill generation from experience |
| **Skills** | `skills/` | Skill management (16 submodules) |
| **SOP** | `sop/` | Standard Operating Procedures |
| **Subagent** | `subagent/` | Subagent spawning and management |
| **Tools** | `tools/` | 26 runtime tools (shell, cron, SOP, skills, etc.) |
| **Trust** | `trust/` | Trust policy enforcement |
| **Tunnel** | `tunnel/` | Tunnel management |
| **Util** | `util.rs` | Utilities |
| **Verifiable Intent** | `verifiable_intent/` | Cryptographic intent verification (crypto, issuance, verification) |

**Note:** The root `src/` package has additional modules not in the runtime crate: `auth/`, `bin/`, `channels/`, `commands/`, `config/`, `gateway/`, `hardware/`, `memory/`, `multimodal.rs`, `peripherals/`, `plugins/`, `providers/`, `schema_markdown.rs`, `tools/` (69 tools).

### 4.3 Agent Internals

The agent loop (`crates/zeroclaw-runtime/src/agent/`) contains 21 modules:

| Module | File | Role |
|--------|------|------|
| **Agent** | `agent.rs` | Core agent struct and lifecycle |
| **Loop** | `loop_.rs` | Main execution loop |
| **Loop Detector** | `loop_detector.rs` | Detect infinite tool call loops |
| **Dispatcher** | `dispatcher.rs` | Message dispatch logic |
| **Classifier** | `classifier.rs` | Message classification |
| **Context Analyzer** | `context_analyzer.rs` | Context window analysis |
| **Context Compressor** | `context_compressor.rs` | Context compression for long conversations |
| **History** | `history.rs` | Conversation history management |
| **History Pruner** | `history_pruner.rs` | History pruning for context limits |
| **Memory Loader** | `memory_loader.rs` | Load memory into context |
| **Prompt** | `prompt.rs` | Prompt construction |
| **System Prompt** | `system_prompt.rs` | System prompt generation |
| **Personality** | `personality.rs` | Agent personality configuration |
| **Personality Templates** | `personality_templates/` | Built-in personality templates |
| **Thinking** | `thinking.rs` | Extended thinking / chain-of-thought |
| **Tool Execution** | `tool_execution.rs` | Tool call execution pipeline |
| **Tool Receipts** | `tool_receipts.rs` | Tool result processing |
| **Cost** | `cost.rs` | Per-turn cost tracking |
| **Eval** | `eval.rs` | Agent evaluation |
| **Tests** | `tests.rs` | Agent tests |

---

## 5. Tools System

### 5.1 Tool Catalog

ZeroClaw has 95+ tool implementations across `src/tools/` (69 files) and `crates/zeroclaw-runtime/src/tools/` (26 files).

```mermaid
graph TB
    subgraph CoreTools["Core Tools (src/tools/)"]
        direction TB
        subgraph FileOps["File & Shell"]
            FE[file_edit]
            FR[file_read]
            FW2[file_write]
            GS[glob_search]
            CS[content_search]
        end
        subgraph BrowserOps["Browser & Web"]
            BR[browser]
            BRD[browser_delegate]
            BRO[browser_open]
            TB2[text_browser]
            WF[web_fetch]
            WSR[web_search_tool]
            SS[screenshot]
            PDF[pdf_read]
        end
        subgraph MemOps["Memory"]
            MR[memory_recall]
            MS[memory_store]
            ME[memory_export]
            MF[memory_forget]
            MP2[memory_purge]
            KT[knowledge_tool]
        end
        subgraph MCPOps["MCP"]
            MCP[mcp_tool]
            MCPC[mcp_client]
            MCPD[mcp_deferred]
            MCPP[mcp_protocol]
            MCPT[mcp_transport]
        end
        subgraph CloudOps["Cloud & DevOps"]
            CO[cloud_ops]
            CP2[cloud_patterns]
            GO[git_operations]
            CC[claude_code]
            CCR[claude_code_runner]
            OC[opencode_cli]
            GC2[gemini_cli]
            CC2[codex_cli]
            PP[proxy_config]
            COMP[composio]
        end
        subgraph SocialOps["Social & Productivity"]
            DS[discord_search]
            LI[linkedin]
            NT2[notion_tool]
            JI[jira_tool]
            GW2[google_workspace]
            PU[pushover]
        end
        subgraph HWTools["Hardware"]
            HBI[hardware_board_info]
            HMM[hardware_memory_map]
            HMR[hardware_memory_read]
        end
        subgraph UtilTools["Utilities"]
            CA[calculator]
            IM[image_gen]
            II[image_info]
            WE[weather_tool]
            PO[poll]
            RE[reaction]
            SE[sessions]
            TO[tool_search]
            PS[project_intel]
            DM[data_management]
            RT[report_templates]
            LP[pipeline]
            MC2[model_routing_config]
            ESC[escalate]
            AU[ask_user]
            BK[backup_tool]
            LL2[llm_task]
            CV[canvas]
            NC2[node_capabilities]
        end
    end

    subgraph RuntimeTools["Runtime Tools (crates/zeroclaw-runtime/src/tools/)"]
        direction TB
        SH[shell]
        FR2[file_read]
        ATT[attribution]
        MS2[model_switch]
        SCH[schedule]
        SEC2[security_ops]
        SMP[send_message_to_peer]
        SPAWN[spawn_subagent]
        RS[read_skill]
        SH2[skill_http]
        ST[skill_tool]
        SO2[sop_advance/approve/execute/list/status]
        CRON[cron_add/list/remove/run/runs/update]
        VI[verifiable_intent]
        DL[delegate]
    end

    style CoreTools fill:#e8f5e9
    style RuntimeTools fill:#fff3e0
```

### 5.2 Tool Categories

| Category | Count | Key Tools |
|----------|-------|-----------|
| **File & Shell** | 6 | file_edit, file_read, file_write, glob_search, content_search, shell |
| **Browser & Web** | 9 | browser, browser_delegate, browser_open, text_browser, web_fetch, web_search_tool, web_search_provider_routing, screenshot, pdf_read |
| **Memory** | 6 | memory_recall, memory_store, memory_export, memory_forget, memory_purge, knowledge_tool |
| **MCP** | 5 | mcp_tool, mcp_client, mcp_deferred, mcp_protocol, mcp_transport |
| **Cloud & DevOps** | 10 | cloud_ops, cloud_patterns, git_operations, claude_code, claude_code_runner, opencode_cli, gemini_cli, codex_cli, proxy_config, composio |
| **Social & Productivity** | 7 | discord_search, linkedin, linkedin_client, notion_tool, jira_tool, google_workspace, pushover |
| **Hardware** | 3 | hardware_board_info, hardware_memory_map, hardware_memory_read |
| **Scheduling** | 8 | schedule, cron_add, cron_list, cron_remove, cron_run, cron_runs, cron_update, poll |
| **Agent** | 5 | spawn_subagent, send_message_to_peer, escalate, ask_user, delegate |
| **Skills** | 4 | read_skill, skill_http, skill_tool, attribution |
| **SOP** | 5 | sop_advance, sop_approve, sop_execute, sop_list, sop_status |
| **Security** | 1 | security_ops |
| **Utilities** | 16 | calculator, image_gen, image_info, weather_tool, model_routing_config, model_switch, data_management, report_templates, pipeline, project_intel, sessions, reaction, tool_search, backup_tool, canvas, llm_task |
| **Other** | 4 | verifiable_intent, node_capabilities, cli_discovery, schema |

---

## 6. Provider Layer

### 6.1 Provider Architecture

ZeroClaw supports 14 LLM providers through a unified trait-based abstraction with circuit breaker patterns, failover, and smart routing.

```mermaid
graph TB
    subgraph Trait["Provider Trait (zeroclaw-api)"]
        PT[Provider<br/>trait definition]
    end

    subgraph Providers["14 Provider Implementations"]
        direction TB
        subgraph Tier1["Tier 1: Full Support"]
            OA[OpenAI<br/>GPT-4, o1, o3]
            AN[Anthropic<br/>Claude 3.5/4]
            GM[Gemini<br/>Gemini 2.x]
            OL[Ollama<br/>Local models]
        end
        subgraph Tier2["Tier 2: Cloud"]
            AZ[Azure OpenAI]
            BD[Bedrock]
            OR[OpenRouter]
            CP[Copilot]
        end
        subgraph Tier3["Tier 3: Specialized"]
            GL[GLM<br/>Zhipu AI]
            TK[Telnyx]
            KC[KiloCLI]
            GC2[Gemini CLI]
            OAC[OpenAI Codex]
            CM[Compatible<br/>Generic OpenAI-compatible]
        end
    end

    subgraph Infra["Provider Infrastructure"]
        direction LR
        FC[Factory<br/>Instantiation + registration]
        RL[Reliable<br/>Resilient wrapper + circuit breaker]
        RT[Router<br/>Smart routing]
        CAT[Catalog<br/>Model registry]
        MD[Models.dev<br/>External model catalog]
        MM[Multimodal<br/>Cross-provider multimodal]
    end

    Trait --> Providers
    Providers --> Infra

    style Trait fill:#e3f2fd
    style Providers fill:#e8f5e9
    style Infra fill:#fff3e0
```

### 6.2 Provider Catalog

| Provider | File | Key Features |
|----------|------|-------------|
| OpenAI | `openai.rs` | GPT-4, o1, o3, native tools, streaming |
| Anthropic | `anthropic.rs` | Claude 3.5/4, extended thinking, tool use |
| Gemini | `gemini.rs` | Gemini 2.x, multimodal, grounding |
| Ollama | `ollama.rs` | Local model serving, no API key |
| Azure OpenAI | `azure_openai.rs` | Enterprise Azure deployments |
| Bedrock | `bedrock.rs` | AWS Bedrock, cross-region |
| OpenRouter | `openrouter.rs` | Multi-provider aggregator |
| Copilot | `copilot.rs` | GitHub Copilot models |
| GLM | `glm.rs` | Zhipu AI GLM-4 series |
| Telnyx | `telnyx.rs` | Telnyx AI platform |
| KiloCLI | `kilocli.rs` | Kilo CLI provider |
| Gemini CLI | `gemini_cli.rs` | Gemini CLI integration |
| OpenAI Codex | `openai_codex.rs` | Codex-specific adapter |
| Compatible | `compatible.rs` | Generic OpenAI-compatible endpoint |

### 6.3 Provider Infrastructure

| Component | File | Role |
|-----------|------|------|
| Factory | `factory.rs` | Provider instantiation and registration (66 Provider-related references) |
| Reliable | `reliable.rs` | Resilient wrapper with circuit breaker (12 references) |
| Router | `router.rs` | Smart routing across providers (15 references) |
| Catalog | `catalog.rs` | Model registry and capabilities |
| Models.dev | `models_dev.rs` | External model catalog integration |
| Multimodal | `multimodal.rs` | Cross-provider multimodal support |

---

## 7. Channel System

### 7.1 Channel Architecture

ZeroClaw connects to 35 messaging platforms through a unified Channel trait and an orchestrator that manages lifecycle, routing, and media pipeline.

```mermaid
graph TB
    subgraph Trait["Channel Trait (zeroclaw-api)"]
        CT[Channel<br/>trait definition]
    end

    subgraph Orchestrator["Orchestrator (crates/zeroclaw-channels/src/orchestrator/)"]
        direction TB
        LC[Lifecycle Manager]
        RT[Message Router]
        MP[Media Pipeline]
        AL[Allowlist<br/>Access control]
        MQTT[MQTT<br/>IoT messaging]
    end

    subgraph Channels["35 Channel Adapters"]
        direction TB
        subgraph China["China/Asia"]
            WX[WeChat]
            WC[WeCom + WeCom WS]
            LK[Lark]
            DT[DingTalk]
            QQ[QQ]
            LN[LINE]
        end
        subgraph Messaging["Messaging"]
            TG[Telegram]
            DC[Discord]
            SL[Slack]
            WA[WhatsApp + WhatsApp Web]
            SG[Signal]
            IM[iMessage]
            MT[Matrix]
            MM[Mattermost]
            IRC[IRC]
        end
        subgraph Social["Social & Other"]
            NS[Nostr]
            BS[Bluesky]
            RD[Reddit]
            TW[Twitter]
            NT[Notion]
            EM[Email + Gmail Push]
            WH[Webhook]
            ACP2[ACP Channel]
            CLI3[CLI Channel]
            CD[ClawdTalk]
            MC[MochaTalk]
            LQ[Linq]
            NC[Nextcloud Talk]
            WT[Wati]
            VC[Voice Call]
            VW[Voice Wake]
        end
    end

    Trait --> Orchestrator
    Orchestrator --> Channels

    style Trait fill:#e3f2fd
    style Orchestrator fill:#e8f5e9
    style Channels fill:#fff3e0
```

### 7.2 Channel List

| Category | Channels | Count |
|----------|----------|-------|
| **China/Asia** | WeChat, WeCom, WeCom WS, Lark, DingTalk, QQ, LINE | 7 |
| **Western Messaging** | Telegram, Discord, Slack, WhatsApp, WhatsApp Web, Signal, iMessage | 7 |
| **Enterprise** | Matrix, Mattermost, IRC, Nextcloud Talk, Email, Gmail Push | 6 |
| **Social** | Nostr, Bluesky, Reddit, Twitter | 4 |
| **Productivity** | Notion, Webhook, Wati | 3 |
| **Internal** | ACP (IDE), CLI, ClawdTalk, MochaTalk, Linq | 5 |
| **Voice** | Voice Call, Voice Wake | 2 |
| **Infrastructure** | Allowlist, Orchestrator (lifecycle, media pipeline, MQTT), Link Enricher, Listing, Transcription, Util | -- |
| **Total** | 35 `impl Channel for` across 34 files (whatsapp_web has 2 impls) | **35** |

**Source:** `grep -c 'impl Channel for' crates/zeroclaw-channels/src/*.rs` -- 34 files with Channel implementations.

---

## 8. Security Model

### 8.1 Security Architecture

ZeroClaw implements 15 security modules providing defense-in-depth from OS-level sandboxing to prompt injection detection.

```mermaid
graph TB
    subgraph OSLevel["OS-Level Sandboxing"]
        LL[Landlock<br/>Linux filesystem restriction]
        BW[Bubblewrap<br/>Namespace isolation]
        FJ[Firejail<br/>Sandbox profiles]
        SB[Seatbelt<br/>macOS sandbox]
        NV[Nevis<br/>Additional isolation]
        DK[Docker<br/>Container sandbox]
    end

    subgraph AppLevel["Application Security"]
        PG[Prompt Guard<br/>Injection detection]
        LD[Leak Detector<br/>Output scanning]
        SK[Secrets<br/>Encrypted credential store]
        OTP2[OTP<br/>One-time passwords]
        WA2[WebAuthn<br/>FIDO2 authentication]
        PR2[Pairing<br/>Device pairing protocol]
        IAM2[IAM Policy<br/>Capability-based access]
        EST[E-Stop<br/>Emergency shutdown]
        AUD2[Audit<br/>Security event logging]
    end

    subgraph Policy["Policy & Detection"]
        direction TB
        PL[Policy<br/>Configurable rules]
        TR[Traits<br/>Security trait definitions]
        DET[Detect<br/>Threat detection]
        VULN[Vulnerability<br/>Vulnerability scanning]
        DM[Domain Matcher<br/>Domain-based rules]
        PLAY[Playbook<br/>Incident response]
    end

    OSLevel --> Policy
    AppLevel --> Policy

    style OSLevel fill:#fce4ec
    style AppLevel fill:#fff3e0
    style Policy fill:#e3f2fd
```

### 8.2 Security Modules

| Module | File | Platform | Role |
|--------|------|----------|------|
| **Landlock** | `landlock.rs` | Linux | Filesystem access restriction via Landlock LSM |
| **Bubblewrap** | `bubblewrap.rs` | Linux | Namespace isolation (bwrap) |
| **Firejail** | `firejail.rs` | Linux | Application sandboxing profiles |
| **Seatbelt** | `seatbelt.rs` | macOS | macOS sandbox profiles |
| **Nevis** | `nevis.rs` | Cross-platform | Additional isolation layer |
| **Docker** | `docker.rs` | Cross-platform | Container-based sandboxing |
| **Prompt Guard** | `prompt_guard.rs` | Cross-platform | LLM prompt injection detection |
| **Leak Detector** | `leak_detector.rs` | Cross-platform | Credential/data leak scanning in outputs |
| **Secrets** | `secrets.rs` | Cross-platform | Encrypted credential storage |
| **OTP** | `otp.rs` | Cross-platform | One-time password authentication |
| **WebAuthn** | `webauthn.rs` | Cross-platform | WebAuthn FIDO2 authentication |
| **Pairing** | `pairing.rs` | Cross-platform | Device pairing protocol |
| **IAM Policy** | `iam_policy.rs` | Cross-platform | Capability-based access control |
| **E-Stop** | `estop.rs` | Cross-platform | Emergency agent shutdown |
| **Audit** | `audit.rs` | Cross-platform | Security event logging |

**Infrastructure modules** (not direct security actions): `detect.rs` (threat detection), `domain_matcher.rs` (domain rules), `playbook.rs` (incident response), `policy.rs` (configurable rules), `traits.rs` (security trait definitions), `vulnerability.rs` (vulnerability scanning).

---

## 9. Memory System

### 9.1 Memory Architecture

ZeroClaw implements 7 storage backend kinds with 16+ feature modules for advanced memory management.

```mermaid
graph TB
    subgraph Trait["Memory Trait (zeroclaw-api)"]
        MT[Memory<br/>trait definition]
    end

    subgraph Backends["7 Storage Backend Kinds"]
        direction TB
        subgraph Local["Local Storage"]
            MD[Markdown<br/>File-based memory]
            SQ[SQLite<br/>Structured storage]
        end
        subgraph Vector["Vector Storage"]
            QD[Qdrant<br/>Vector database]
            PG3[PostgreSQL + pgvector]
        end
        subgraph Advanced["Advanced"]
            LUC[Lucid<br/>Dream-like consolidation]
        end
        subgraph None["Null"]
            NO[None<br/>No-op backend]
        end
    end

    subgraph Features["16+ Feature Modules"]
        direction LR
        CH[Chunker<br/>Text splitting]
        EMB2[Embeddings<br/>Vector generation]
        KG[Knowledge Graph<br/>Entity relationships]
        KGP[KG Postgres<br/>Graph on PostgreSQL]
        CON2[Consolidation<br/>Memory merging]
        DEC2[Decay<br/>Relevance decay]
        IMP[Importance<br/>Scoring]
        HY[Hygiene<br/>Memory cleanup]
        CONF[Conflict<br/>Resolution]
        ASM[Agent-Scoped<br/>Per-agent isolation]
        ASM2[Agent-Scoped MD<br/>Per-agent markdown]
        AUD3[Audit<br/>Memory audit log]
        POL[Policy<br/>Retention rules]
        RC[Response Cache<br/>Cache responses]
        RET[Retrieval<br/>Memory retrieval]
        SNAP[Snapshot<br/>Memory snapshots]
        VEC[Vector<br/>Vector operations]
    end

    Trait --> Backends
    Backends --> Features

    style Trait fill:#e3f2fd
    style Backends fill:#e8f5e9
    style Features fill:#fff3e0
```

### 9.2 Storage Backend Kinds

| Backend | Enum Variant | Storage | Use Case |
|---------|-------------|---------|----------|
| SQLite | `MemoryBackendKind::Sqlite` | Local DB | Default structured storage |
| Markdown | `MemoryBackendKind::Markdown` | Local files | Simple file-based memory |
| Lucid | `MemoryBackendKind::Lucid` | In-process | Dream-like memory consolidation |
| PostgreSQL | `MemoryBackendKind::Postgres` | Remote | Enterprise storage + pgvector |
| Qdrant | `MemoryBackendKind::Qdrant` | Remote | Production vector DB |
| None | `MemoryBackendKind::None` | N/A | No-op for testing |
| Unknown | `MemoryBackendKind::Unknown` | Custom | Custom/third-party backends |

**Source:** `crates/zeroclaw-memory/src/backend.rs` -- `MemoryBackendKind` enum with 7 variants.

### 9.3 Memory Feature Modules

| Feature | File | Role |
|---------|------|------|
| Agent-Scoped | `agent_scoped.rs` | Per-agent memory isolation |
| Agent-Scoped Markdown | `agent_scoped_markdown.rs` | Per-agent markdown memory |
| Audit | `audit.rs` | Memory access audit trail |
| Chunker | `chunker.rs` | Text splitting for storage |
| Conflict | `conflict.rs` | Memory conflict resolution |
| Consolidation | `consolidation.rs` | Memory merging and dedup |
| Decay | `decay.rs` | Time-based relevance decay |
| Embeddings | `embeddings.rs` | Vector embedding generation |
| Hygiene | `hygiene.rs` | Memory cleanup and maintenance |
| Importance | `importance.rs` | Memory importance scoring |
| Knowledge Graph | `knowledge_graph.rs` | Entity relationship mapping |
| Knowledge Graph PG | `knowledge_graph_pg.rs` | Knowledge graph on PostgreSQL |
| Policy | `policy.rs` | Retention and access policies |
| Response Cache | `response_cache.rs` | Cache LLM responses |
| Retrieval | `retrieval.rs` | Memory retrieval strategies |
| Snapshot | `snapshot.rs` | Memory state snapshots |
| Vector | `vector.rs` | Vector operations and search |

---

## 10. Plugin System (WASM)

### 10.1 WASM Plugin Architecture

ZeroClaw extends through WASM components using the component model for secure, sandboxed plugin execution.

```mermaid
graph TB
    subgraph Plugin["Plugin System (zeroclaw-plugins)"]
        direction TB
        HOST[Host<br/>Plugin host environment]
        RT[Runtime<br/>WASM runtime]
        SIG[Signature<br/>Plugin signing + verification]
    end

    subgraph PluginTypes["Plugin Types"]
        direction LR
        WC[WASM Channel<br/>Custom channel plugins]
        WT[WASM Tool<br/>Custom tool plugins]
    end

    subgraph Lifecycle["Plugin Lifecycle"]
        direction TB
        DISC[Discovery<br/>Find plugins]
        LOAD[Load<br/>Instantiate WASM]
        EXEC[Execute<br/>Sandboxed call]
        UNLOAD[Unload<br/>Cleanup]
    end

    Plugin --> PluginTypes
    Plugin --> Lifecycle

    style Plugin fill:#e3f2fd
    style PluginTypes fill:#e8f5e9
    style Lifecycle fill:#fff3e0
```

---

## 11. Hardware & Peripherals

### 11.1 Hardware Architecture

ZeroClaw has a full hardware abstraction layer for IoT and robotics use cases.

```mermaid
graph TB
    subgraph Discovery["Hardware Discovery"]
        USB[USB Discovery<br/>Device enumeration]
        INT[Introspect<br/>Device capability detection]
        REG[Registry<br/>Device registry]
    end

    subgraph Peripherals["Peripheral Support"]
        direction TB
        GPIO[GPIO<br/>Pin control]
        SER[Serial<br/>UART/SPI/I2C]
        RPI[Raspberry Pi<br/>RPi GPIO]
        ARD[Arduino<br/>Serial protocol]
        PICO[Pico<br/>UF2 flashing]
        NUC[Nucleo<br/>STM32 support]
        ESP[ESP32<br/>WiFi firmware]
    end

    subgraph Firmware["Firmware Targets"]
        direction LR
        F1[Arduino<br/>firmware/arduino]
        F2[ESP32<br/>firmware/esp32]
        F3[ESP32-UI<br/>firmware/esp32-ui]
        F4[Pico<br/>firmware/pico]
        F5[Nucleo<br/>firmware/nucleo]
        F6[Uno Q Bridge<br/>firmware/uno-q-bridge]
        F7[FW Protocol<br/>zeroclaw-fw-protocol]
        F8[Nucleo FW<br/>zeroclaw-nucleo]
    end

    subgraph Robot["Robot Kit"]
        RK2[robot-kit<br/>Safety, tests, abstractions]
    end

    Discovery --> Peripherals
    Peripherals --> Firmware
    Peripherals --> Robot

    style Discovery fill:#e3f2fd
    style Peripherals fill:#e8f5e9
    style Firmware fill:#fff3e0
    style Robot fill:#fce4ec
```

---

## 12. Skills & SkillForge

### 12.1 Skills System

ZeroClaw manages skills as reusable capability bundles that can be created, tested, improved, and shared.

```mermaid
graph TB
    subgraph Skills["Skills System (16 submodules)"]
        direction TB
        BUNDLE[Bundle<br/>Package skills]
        SCAFFOLD[Scaffold<br/>Generate skill skeleton]
        TEST[Test<br/>Skill testing + symlink tests]
        IMPROVE[Improve<br/>Iterative improvement]
        DOC[Document<br/>Skill documentation]
        REF[Reference<br/>Skill references]
        SUGGEST[Suggestions<br/>Skill recommendations]
        FRONT[Frontmatter<br/>Skill metadata]
        HTTP[Skill HTTP<br/>HTTP-based skills]
        TOOL[Skill Tool<br/>Tool-based skills]
        SERVE[Service<br/>Skill serving]
        CONST[Constants<br/>Skill constants]
    end

    subgraph SkillForge["SkillForge (Autonomous)"]
        direction TB
        CREATOR[Creator<br/>Generate from experience]
        ANALYZE[Analyze<br/>Pattern recognition]
        ITERATE[Iterate<br/>Self-improvement]
    end

    Skills --> SkillForge

    style Skills fill:#e8f5e9
    style SkillForge fill:#fff3e0
```

---

## 13. Observability

### 13.1 Observability Stack

ZeroClaw provides unified observability through 7 backends.

```mermaid
graph TB
    subgraph Emit["Emission Layer"]
        REC[record! macro<br/>zeroclaw-log]
        TR[Tracing<br/>tracing crate spans]
    end

    subgraph Backends["7 Observability Backends"]
        direction LR
        LOG2[Log<br/>Structured logging]
        OTEL[OpenTelemetry<br/>OTLP export]
        PROM[Prometheus<br/>Metrics]
        DORA[Dora<br/>Distributed tracing]
        VERB[Verbose<br/>Console output]
        MULTI[Multi<br/>Fan-out to multiple]
        NOOP2[Noop<br/>No-op for testing]
    end

    subgraph Data["Data"]
        direction LR
        RT2[Runtime Trace<br/>Execution traces]
        JSONL[JSONL<br/>Persistent logs]
        BROADCAST[Broadcast<br/>Real-time log stream]
    end

    Emit --> Backends
    Backends --> Data

    style Emit fill:#e3f2fd
    style Backends fill:#e8f5e9
    style Data fill:#fff3e0
```

---

## 14. Gateway & Frontend

### 14.1 Gateway Architecture

The `zeroclaw-gateway` crate (5,211 LOC in lib.rs) provides a webhook/gateway server for external integrations.

```mermaid
graph TB
    subgraph Gateway["Gateway (zeroclaw-gateway)"]
        direction TB
        WH2[Webhook Server<br/>HTTP endpoint]
        Tauri2[Tauri App<br/>Desktop GUI]
    end

    subgraph External["External Integrations"]
        direction LR
        EXT1[Webhooks]
        EXT2[REST API]
        EXT3[ACP Bridge]
    end

    Gateway --> External

    style Gateway fill:#e8f5e9
    style External fill:#e3f2fd
```

---

## 15. Codebase Analysis

### 15.1 CocoIndex Statistics

| Metric | Value |
|--------|-------|
| **Total Files Indexed** | 1,132 |
| **Code Chunks** | 29,325 |
| **Extracted Symbols** | 1,787 |
| **Import Edges** | 4,666 |
| **Embedding Coverage** | 100% (29,325/29,325) |

### 15.2 Code Metrics

| Metric | Value | Evidence |
|--------|-------|----------|
| **Rust Files** | 772 | `find . -name '*.rs'` |
| **Total Rust LOC** | 418,710 | `find + wc -l` |
| **Workspace Crates** | 17 | `Cargo.toml` workspace members |
| **Channel Implementations** | 35 | `impl Channel for` grep (34 files, whatsapp_web has 2) |
| **Provider Implementations** | 14 | Provider files excl. infrastructure |
| **Provider Infrastructure** | 6 | factory, reliable, router, catalog, models_dev, multimodal |
| **Tools** | 95+ | 69 in `src/tools/`, 26 in `crates/zeroclaw-runtime/src/tools/` |
| **Memory Backend Kinds** | 7 | `MemoryBackendKind` enum |
| **Memory Feature Modules** | 16 | Feature files in `crates/zeroclaw-memory/src/` |
| **Security Modules** | 15 | Active security modules |
| **Security Infrastructure** | 6 | Policy, detection, playbook, traits, vulnerability, domain_matcher |
| **Observability Backends** | 7 | Log, OTEL, Prometheus, Dora, Verbose, Multi, Noop |
| **Firmware Targets** | 8 | Arduino, ESP32, ESP32-UI, Pico, Nucleo, Uno Q, FW Protocol, Nucleo FW |

### 15.3 Largest Crates (by lib.rs)

| Crate | lib.rs LOC | Role |
|-------|-----------|------|
| `zeroclaw-gateway` | 5,211 | Webhook server |
| `zeroclaw-tool-call-parser` | 3,834 | Tool call parsing |
| `zeroclaw-providers` | 3,314 | LLM providers |
| `zeroclaw-macros` | 2,205 | Derive macros |
| `zeroclaw-memory` | 907 | Memory backends |
| `zeroclaw-hardware` | 747 | Hardware abstraction |
| `aardvark-sys` | 483 | Native bindings |
| `zeroclaw-infra` | 163 | Shared infra |

### 15.4 Stability Tiers

```mermaid
graph LR
    subgraph Stable["Stable (planned)"]
        S1[zeroclaw-api @ v1.0.0]
        S2[zeroclaw-config @ v0.8.0]
        S3[zeroclaw-tool-call-parser @ v0.8.0]
    end

    subgraph Beta["Beta"]
        B1[zeroclaw-config]
        B2[zeroclaw-log]
        B3[zeroclaw-providers]
        B4[zeroclaw-memory]
        B5[zeroclaw-infra]
        B6[zeroclaw-macros]
    end

    subgraph Experimental["Experimental"]
        E1[zeroclaw-api]
        E2[zeroclaw-channels]
        E3[zeroclaw-tools]
        E4[zeroclaw-runtime]
        E5[zeroclaw-gateway]
        E6[zeroclaw-tui]
        E7[zeroclaw-plugins]
        E8[zeroclaw-hardware]
    end

    style Stable fill:#e8f5e9
    style Beta fill:#fff3e0
    style Experimental fill:#fce4ec
```

---

## 16. Data Flow Diagrams

### 16.1 Message Processing Flow

```mermaid
sequenceDiagram
    participant User
    participant Channel as Channel (e.g. Telegram)
    participant Orch as Orchestrator
    participant Security as Security Layer
    participant Agent as Agent Loop
    participant Provider as LLM Provider
    participant Tools as Tool Execution
    participant Memory as Memory System

    User->>Channel: Send message
    Channel->>Orch: MessageEvent
    Orch->>Orch: Allowlist check
    Orch->>Security: Security scan
    Security-->>Orch: Cleared
    Orch->>Agent: Dispatch message

    Agent->>Agent: Classify message
    Agent->>Memory: Load context
    Memory-->>Agent: Context window

    Agent->>Provider: LLM request + tools
    Provider-->>Agent: Response / tool calls

    loop Tool Execution
        Agent->>Tools: Execute tool call
        Tools-->>Agent: Tool result
        Agent->>Agent: Loop detector check
        Agent->>Provider: Continue with result
        Provider-->>Agent: Response / more tool calls
    end

    Agent->>Agent: Context compress if needed
    Agent->>Agent: History prune if needed
    Agent->>Memory: Store conversation
    Agent->>Orch: Final response
    Orch->>Channel: Deliver response
    Channel->>User: Message delivered
```

### 16.2 Security Pipeline Flow

```mermaid
sequenceDiagram
    participant Input as Incoming Message
    participant PG as Prompt Guard
    participant Policy as Policy Engine
    participant Agent as Agent
    participant LD as Leak Detector
    participant Output as Outgoing Response

    Input->>PG: Scan for injection
    PG->>Policy: Threat assessment

    alt Threat Detected
        Policy-->>Output: Block + audit log
    else Clean
        Policy->>Agent: Pass through
        Agent->>Agent: Process
        Agent->>LD: Scan output
        LD->>Policy: Leak check

        alt Leak Detected
            Policy-->>Output: Sanitize + audit log
        else Clean
            Policy-->>Output: Deliver
        end
    end
```

### 16.3 Plugin Execution Flow

```mermaid
sequenceDiagram
    participant Agent as Agent
    participant PluginHost as Plugin Host
    participant WASM as WASM Runtime
    participant Sandbox as Sandbox

    Agent->>PluginHost: Invoke plugin
    PluginHost->>PluginHost: Signature verification
    PluginHost->>WASM: Instantiate component
    WASM->>Sandbox: Create sandbox
    Sandbox->>WASM: Sandboxed execution
    WASM-->>PluginHost: Result
    PluginHost->>Sandbox: Cleanup
    PluginHost-->>Agent: Plugin result
```

### 16.4 Memory Consolidation Flow

```mermaid
sequenceDiagram
    participant Agent as Agent
    participant Memory as Memory System
    participant Consolidate as Consolidation
    participant Decay as Decay Engine
    participant KG as Knowledge Graph

    Agent->>Memory: Store memory
    Memory->>Consolidate: Check for duplicates
    Consolidate->>Consolidate: Merge similar memories
    Consolidate->>KG: Update entity graph
    Memory->>Decay: Schedule decay

    Note over Decay: Periodic: decay relevance scores

    Agent->>Memory: Recall
    Memory->>KG: Query relationships
    Memory->>Memory: Rank by importance + recency
    Memory-->>Agent: Relevant memories
```

### 16.5 Agent Loop Detail

```mermaid
sequenceDiagram
    participant Input as Input Source
    participant Agent as Agent Loop
    participant Classifier as Classifier
    participant Context as Context Analyzer
    participant LLM as LLM Provider
    participant Tools as Tool Executor
    participant Memory as Memory System
    participant History as History Pruner

    Input->>Agent: Message
    Agent->>Classifier: Classify message
    Classifier-->>Agent: Classification result

    Agent->>Context: Analyze context window
    Context-->>Agent: Context strategy

    Agent->>Memory: Load relevant memories
    Memory-->>Agent: Memory context

    Agent->>Agent: Build prompt (system + personality + context)
    Agent->>LLM: Send request
    LLM-->>Agent: Response / tool calls

    loop Tool Execution
        Agent->>Tools: Execute tool call
        Tools-->>Agent: Tool result
        Agent->>Agent: Loop detector check
        Agent->>LLM: Continue with result
        LLM-->>Agent: Response / more calls
    end

    Agent->>Agent: Context compress if needed
    Agent->>History: Prune if needed
    Agent->>Memory: Store conversation

    Note over Agent,History: Loop detector monitors for infinite tool call cycles
```

---

## Appendix A: Repository Structure

```
zeroclaw/
├── Cargo.toml                    # Workspace root
├── src/
│   ├── main.rs                   # CLI entrypoint
│   ├── lib.rs                    # Module re-exports
│   ├── agent/                    # Agent loop
│   ├── approval/                 # Human-in-the-loop
│   ├── auth/                     # Authentication
│   ├── channels/                 # Channel orchestration
│   ├── commands/                 # CLI commands
│   ├── config/                   # Configuration
│   ├── cost/                     # Cost tracking
│   ├── cron/                     # Scheduled tasks
│   ├── daemon/                   # Background daemon
│   ├── doctor/                   # Health diagnostics
│   ├── gateway/                  # Gateway integration
│   ├── hardware/                 # Hardware abstraction
│   ├── health/                   # Health checks
│   ├── heartbeat/                # Proactive execution
│   ├── hooks/                    # Event hooks
│   ├── i18n.rs                   # Internationalization
│   ├── identity.rs               # Agent identity
│   ├── integrations/             # External integrations
│   ├── memory/                   # Memory CLI + tests
│   ├── migration.rs              # Data migration
│   ├── multimodal.rs             # Multimodal support
│   ├── nodes/                    # Node management
│   ├── observability/            # Tracing + metrics
│   ├── onboard/                  # Onboarding wizard
│   ├── peripherals/              # Peripheral management
│   ├── platform/                 # Platform detection
│   ├── plugins/                  # Plugin integration
│   ├── providers/                # Provider integration
│   ├── rag/                      # RAG pipeline
│   ├── schema_markdown.rs        # Schema documentation
│   ├── security/                 # Security orchestration
│   ├── service/                  # Service management
│   ├── skillforge/               # Skill generation
│   ├── skills/                   # Skill management
│   ├── sop/                      # Standard operating procedures
│   ├── tools/                    # 69 tool implementations
│   ├── trust/                    # Trust policy
│   ├── tunnel/                   # Tunnel management
│   ├── util.rs                   # Utilities
│   └── verifiable_intent/        # Intent verification
├── crates/
│   ├── zeroclaw-api/             # Public traits
│   ├── zeroclaw-config/          # Config schema
│   ├── zeroclaw-log/             # Logging
│   ├── zeroclaw-macros/          # Derive macros
│   ├── zeroclaw-infra/           # Shared infra
│   ├── zeroclaw-providers/       # 14 LLM providers + 6 infra
│   ├── zeroclaw-channels/        # 35 channels + orchestrator
│   ├── zeroclaw-tools/           # Tool execution
│   ├── zeroclaw-memory/          # 7 backends + 16 features
│   ├── zeroclaw-runtime/         # Agent runtime (36 modules)
│   ├── zeroclaw-gateway/         # Webhook server
│   ├── zeroclaw-tui/             # TUI wizard
│   ├── zeroclaw-plugins/         # WASM plugins
│   ├── zeroclaw-hardware/        # Hardware abstraction
│   ├── zeroclaw-tool-call-parser/# Tool call parsing
│   ├── robot-kit/                # Robot abstraction
│   └── aardvark-sys/             # Native bindings
├── apps/tauri/                   # Tauri desktop app
├── firmware/                     # MCU firmware
│   ├── arduino/
│   ├── esp32/
│   ├── esp32-ui/
│   ├── pico/
│   ├── nucleo/
│   ├── uno-q-bridge/
│   ├── zeroclaw-fw-protocol/
│   └── zeroclaw-nucleo/
├── tools/fill-translations/      # i18n tooling
├── xtask/                        # Build automation
└── docs/                         # Documentation
```

## Appendix B: Extension Points

| Extension Point | Trait/Location | How to Extend |
|----------------|----------------|---------------|
| **Provider** | `crates/zeroclaw-api/src/provider.rs` | Implement `Provider` trait |
| **Channel** | `crates/zeroclaw-api/src/channel.rs` | Implement `Channel` trait |
| **Tool** | `crates/zeroclaw-api/src/tool.rs` | Implement `Tool` trait |
| **Memory** | `crates/zeroclaw-api/src/memory_traits.rs` | Implement `Memory` trait |
| **Observer** | `crates/zeroclaw-api/src/observability_traits.rs` | Implement `Observer` trait |
| **Runtime Adapter** | `crates/zeroclaw-api/src/runtime_traits.rs` | Implement `RuntimeAdapter` trait |
| **Peripheral** | `crates/zeroclaw-api/src/peripherals_traits.rs` | Implement `Peripheral` trait |
| **WASM Plugin** | `crates/zeroclaw-plugins/` | Build WASM component |
