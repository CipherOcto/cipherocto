# CipherOcto Memory

## Project Overview

- **Protocol** for autonomous intelligence collaboration (private AI, local infra, hybrid blockchain)
- **Ocean Stack**: 🐙 Assistant → Agent Orchestrator → 🦑 Secure Execution → 🪼 Hybrid Network
- **Branch Strategy**: trunk-based (main/next/feat/*/agent/*/research/*/hotfix/*)

## Current Focus

- RFC-0104: Deterministic Floating-Point (DFP) implementation
- quota-router-cli: Rust CLI for AI API quotas with HTTPS proxy

## Architecture

- **Determinism**: Class A (Protocol), Class B (Off-Chain), Class C (Probabilistic)

---

## Current Work: Phase 7.J — fix 401 LoggedOut reconnect bug

**Branch**: `feat/whatsapp-runtime-cli-mcp` (local-only, no push per operator 2026-07-05)
**Goal**: patch wacore@551e574 to emit modern XX-extended ClientHello so reconnect after onboarding succeeds
**Root cause**: wacore's `HandshakeUtils::build_client_hello` (line 95 of `wacore/noise/src/handshake.rs`) only sets `ephemeral` field. Chrome 150 emits ephemeral + encrypted_static + encrypted_payload + useExtended + extendedCiphertext + pqMode + extendedEphemeral. Frame[2] gap = 261B (wacore) vs 363B (Chrome) = +102B. Server 401s at `lla` after handshake completes.

**Progress** (8-session plan from `docs/plans/cryptic-percolating-octopus.md`):
- ✅ S1 (commits c6e635df, 12b54607, e40adb4b): Chrome localStorage full dump — `WANoiseInfo` (217B = privKey/pubKey/recoveryToken) + `WAWebEncKeySalt` + `WANoiseInfoIv` (4 IVs)
- ✅ S2.1 (commit cb93f1da): webhook module capture — **FAILED** (WA Web 2026 doesn't populate `webpackChunk.*` via legacy push pattern; chunkLen stays at 0)
- ✅ S2.2 (just committed): IDB enumeration + CryptoKey export — **PARTIAL/FAILED**. Chrome 150 stores noise keys as non-extractable `CryptoKey` objects (extractable=false). `crypto.subtle.exportKey('raw'/'jwk')` throws. Only Ed25519 signatures (64B raw bytes) are extractable. **`wawc_db_enc/keys[1]` master AES key ALSO non-extractable**. Conclusion: **decrypting Chrome's IndexedDB is a dead end**.
- ✅ S6 (commit c098bf7d): `whatsapp_modern_client_hello` proto scaffold — encodes modern ClientHello = 352B (Chrome 363B, diff=11B within tag-overhead tolerance). Validates the proto structure is correct.

**Pivot**: skip S3 (decrypt IndexedDB), S4 (decrypt Chrome frame[2]), S5 (field-by-field diff). The fields Chrome emits are KNOWN from the proto schema (`waproto::handshake_message::ClientHello`: ephemeral=1, static=2, payload=3, useExtended=4, extendedCiphertext=5, pqMode=9, extendedEphemeral=10). We don't need Chrome's actual identity keys — wacore has its OWN identity in `session.db`. Patch wacore's XX `build_client_hello` to populate all 4 missing fields with computed values, test against the server.

**Next sessions** (persisted as TaskList #135/#136/#137):
- S6.5: extend `whatsapp_modern_client_hello` to actually WS-connect + send + verify server verdict
- S6.7: patch wacore's `build_client_hello` to populate modern fields (upstream fork commit on `mmacedoeu/whatsapp-rust`)
- S7: full daemon live run + validate `bot_state=Connected`
- S8: land upstream + memory update

**Key source pointers**:
- `crates/octo-adapter-whatsapp/src/bin/whatsapp_xx_session_probe.rs`: WS+Noise opener that confirms server accepts our frame[0] wire shape
- `crates/octo-adapter-whatsapp/src/bin/whatsapp_decode_chrome_frame2.rs`: server hello parse + 261B gap measurement
- `crates/octo-adapter-whatsapp/src/bin/whatsapp_modern_client_hello.rs`: modern proto shape encoder (proto-only)
- `crates/whatsapp_chrome_session_extract/src/bin/whatsapp_kdf_dump.rs`: webpackChunk hook (kept for forensic record, didn't capture anything)
- `crates/whatsapp_chrome_session_extract/src/bin/whatsapp_idb_decrypt_attempt.rs`: IDB enumeration + CryptoKey export (kept for forensic record, confirmed non-extractable)
- `docs/research/2026-07-14-401-diagnosis-final.md`: full 5-theory diagnosis
- `docs/research/2026-07-14-S1-chrome-localstorage-dump.md`: S1 results
- `docs/research/2026-07-14-S2-KDF-and-pivot.md`: S2 fail + pivot justification

## CRITICAL RULES

1. **Git: Never push without authorization** — commits OK, push requires user permission
2. **Always solve ALL RFC issues** — no deferrals, fix now or formal rebuttal only
3. **Cargo fmt before commit** — run `cargo fmt -- --check` before every commit
4. **Mode Gate ≠ Interface** — HTTP proxy AND Python SDK exist in ALL modes (litellm/any-llm/full)
5. **RFC references by number only** — no version pins, no status (e.g., RFC-0917 not RFC-0917 v2.35)
6. **docs/reviews/ are scratchpads** — NEVER committed to git

---

## BLUEPRINT Governance

### The 4 Layers (never mix)
| Layer | Question | Purpose |
|-------|----------|---------|
| Research | CAN WE? | Feasibility |
| Use Cases | WHY? | Intent/Narrative |
| RFCs | WHAT? | Protocol Design |
| Missions | HOW? | Execution |

### Canonical Flow
`Idea → Research → Use Case → RFC → Mission → Agent Claims → Implementation → Merge → Protocol Evolution`

### RFC Lifecycle
`Planned → Draft → Review (7+ days) → Accepted → Final`

### Mission Rules
- REQUIRE an Accepted RFC — no RFC = no Mission
- 14-day claim timeout, 7-day PR review timeout
- Use `git mv` for status updates (preserves rename history)

### RFC Status Update Process

When updating RFC status (e.g., Draft → Accepted):

1. **Verify content first** — read both files, confirm headers/sections correct
2. **Use `git mv`** — track rename so git sees R100, not A+D
3. **Update Status header via sed** — `sed -i 's/Draft (/Accepted (/'`
4. **Stage and verify separately** — `git diff --cached --name-status` should show R100

```bash
# Verify rename tracked:
git diff --cached --name-status  # Should show R100

# Verify sections after move:
grep "^### Section:" accepted/file.md
```

**Content Swap Risk:** When moving multiple RFCs, avoid file-swapping operations.

### Human vs Agent
- Humans: Create Use Cases, Accept RFCs, Merge PRs
- Agents: Claim Missions, Implement RFCs, Write Tests
- Agents CANNOT initiate RFCs or create Use Cases

---

## Dependencies

- Rust (cargo, tokio, hyper, clap), Python (PyO3)
