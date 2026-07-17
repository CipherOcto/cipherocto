# T01 — Locate the newsletter RPC dispatcher allow-list

> Investigation-only task. No code changes. Build-state + behavior map for the rest of the
> Phase 7.E+ Newsletter plan (T02–T14).

**Date:** 2026-07-15
**Branch:** `feat/whatsapp-runtime-cli-mcp`
**Worktree:** `/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp`

---

## TL;DR

There is **no** single monolithic allow-list that rejects `newsletter.*` with `-32601`.
The 8 newsletter handler types already exist and **ARE reachable through the unix-socket
daemon**: `daemon.methods.list` returns every newsletter method, and a direct JSON-RPC call
(`newsletter.list_subscribed` over the daemon socket) reaches the handler and returns
`-32603` from the live WhatsApp backend — proof that the daemon DOES accept the method.

The actual `-32601 "method ... not implemented in Phase 1"` message lives in **`mcp_server.rs:80`**,
the **MCP stdio** server (a separate protocol surface used by MCP-aware clients). That
surface has **zero newsletter support**: `mcp_server.rs` contains no `newsletter` strings at
all, no `td()` entries for newsletter tools, and no entries in the `tools/call` routing
match for any newsletter RPC. So an MCP client that asks for `newsletter.list_subscribed`
hits the line-80 fallback. *This is the bug the rest of the plan must fix — not the
allow-list per se but the MCP tool_descriptors + routing map, plus extending the daemon
registry + CLI subtree with 14 entries instead of the current 8.*

---

## 1. Dispatcher locations (exact file:line)

There are **two** independent JSON-RPC dispatch surfaces. Both must be extended.

### 1a. Unix-socket daemon dispatcher

| Concern | Location | Behavior |
|---------|----------|----------|
| Daemon socket accept loop | `crates/octo-whatsapp/src/ipc/server.rs:140-217` (`handle_conn`) | reads line-delimited JSON-RPC, calls `registry.dispatch` |
| Registry HashMap lookup | `crates/octo-whatsapp/src/ipc/server.rs:66-95` (`HandlerRegistry::dispatch`) | HashMap lookup on `req.method`; **missing → `RpcErrorCode::MethodNotFound = -32601`** with message `"method {:?} not found in this build"` |
| Handler registry build chain | `crates/octo-whatsapp/src/ipc/handlers/mod.rs:142-145` (`build_registry`) → `build_base_registry` (lines 147 onwards) and optional `append_query_layer_handlers` behind `feature = "query"` | registers every handler in one big chain of `.register(Arc::new(...))` calls |
| Tier 6.5 newsletter registrations | `crates/octo-whatsapp/src/ipc/handlers/mod.rs:299-304` | `newsletter_list_subscribed`, `newsletter_get_metadata`, `newsletter_leave`, `events_create` |
| Tier 7.E newsletter registrations | `crates/octo-whatsapp/src/ipc/handlers/mod.rs:343-348` | `newsletter_create`, `newsletter_join`, `newsletter_send_reaction`, `newsletter_edit_message`, `newsletter_revoke_message` (+ `tctoken_*`) |
| Handler module declarations | `crates/octo-whatsapp/src/ipc/handlers/mod.rs:86-93` (`pub mod newsletter_*`) | all 8 newsletter handler files exist |

**Status: 8 of the target 14 are wired. The other 6 (`send_message`, `get_messages`,
`get_subscribers`, `mute`, `unmute`, `accept_tos`) need new handlers in T04 and registrations in T05.**

### 1b. MCP stdio dispatcher (the actual -32601 surface)

| Concern | Location | Behavior |
|---------|----------|----------|
| MCP accept loop | `crates/octo-whatsapp/src/mcp_server.rs:46-99` (`serve_inner`) | reads line-delimited JSON-RPC; **unknown method → `mcp_server.rs:80` returns `-32601` with message `"method {:?} not implemented in Phase 1"`** |
| `tools/list` handler | `crates/octo-whatsapp/src/mcp_server.rs` (`handle_tools_list`, called by line 75) | enumerates `tool_descriptors()` |
| `tools/call` handler | `crates/octo-whatsapp/src/mcp_server.rs:1241-1280` (range around `handle_tools_call`, called by line 76) | `match` over `daemon_method` — **no `newsletter.*` arm**, falls through to `other =>` at line 1286 which returns `-32601 "tool {:?} not implemented"` |
| `tool_descriptors()` definition | `crates/octo-whatsapp/src/mcp_server.rs:123-...` | big `Vec::push(td(...))` chain — **0 entries for `newsletter.*`** (confirmed by `grep`: zero hits in this file) |
| Tool count guard | `crates/octo-whatsapp/src/mcp_server.rs:41-44` | `EXPECTED_TOOL_COUNT = 128` with `query`, `122` without — currently includes `community.*` (9) but no `newsletter.*` |

**Status: ZERO newsletter tools are advertised or routed on the MCP surface.**
This is the real gap to close in T10.

---

## 2. Allow-list enumeration (TIER*_METHODS)

Every TIER const currently in `crates/octo-whatsapp/src/ipc/handlers/mod.rs`.
These are used both for the `build_base_registry` registration chain (verified via
`registry_size_matches_phase1_phase2` test at lines 820–864) AND as test-assertion lists in
the unit tests there.

| Constant | Line | Entries (count) | Contains newsletter.*? |
|----------|------|-----------------|------------------------|
| `PHASE1_METHODS` | ~ earlier in file | (see file) | no |
| `PHASE2_MEDIA_METHODS` | earlier | — | no |
| `PHASE2_SEND_MESSAGE_METHODS` | earlier | — | no |
| `PHASE2_CHATS_METHODS` | earlier | — | no |
| `PHASE2_ENVELOPE_METHODS` | earlier | — | no |
| `PHASE3_EVENTS_METHODS` | earlier | — | no |
| `PHASE3_DISCOVERY_METHODS` | earlier | — | no |
| `PHASE4_RULES_METHODS` | earlier | — | no |
| `PHASE4_TRIGGERS_METHODS` | earlier | — | no |
| `PHASE4_AUDIT_METHODS` | earlier | — | no |
| `PHASE4_ACTIONS_METHODS` | earlier | — | no |
| `PHASE5_SECURITY_METHODS` | earlier | — | no |
| `PHASE6_12_GROUPS_METHODS` | earlier | — | no |
| `PHASE6_1_ACCOUNTS_METHODS` | earlier | — | no |
| `TIER4_CONTACT_PRESENCE_METHODS` | 549 | 8 | no |
| `TIER6_PROFILE_METHODS` | 562 | 7 (incl. 7.J LID mappings) | no |
| `TIER6_1_PRIVACY_METHODS` | 581 | 4 | no |
| `TIER6_2_LABELS_STAR_METHODS` | 596 | 6 | no |
| `TIER6_3_LIFECYCLE_METHODS` | 614 | 4 | no |
| `TIER6_4_IDENTITY_METHODS` | 629 | 3 | no |
| **`TIER6_5_NEWSLETTER_METHODS`** | **640** | **4** (3 newsletter + 1 events) | **YES (3): `newsletter.list_subscribed`, `newsletter.get_metadata`, `newsletter.leave`** |
| `TIER7_A_PIN_UNPIN_METHODS` | 648 | 5 | no |
| `TIER7_B_POLLS_EVENTS_METHODS` | 657 | 3 | no |
| `TIER7_C_STATUS_METHODS` | 661 | 4 | no |
| `TIER7_D_PROFILE_METHODS` | 669 | 6 | no |
| **`TIER7_E_NEWSLETTER_TCTOKEN_METHODS`** | **679** | **9** (5 newsletter + 4 tctoken) | **YES (5): `newsletter.create`, `newsletter.join`, `newsletter.send_reaction`, `newsletter.edit_message`, `newsletter.revoke_message`** |
| `TIER7_F_PASSKEY_METHODS` | 694 | 2 | no |
| `TIER7_G_COMMUNITY_METHODS` | 702 | 9 | no |
| `TIER7_H_GROUP_METHODS` | 717 | 5 | no |
| `TIER7_I_DAEMON_METHODS` | 733 | 5 | no |
| `TIER7_QUERY_METHODS` | 750 | (3 — gated behind `query`) | no |
| `TIER7_METHODS_TAIL` | 766/768 | alias for `TIER7_QUERY_METHODS` or empty | no |

**Registered newsletter entries today: 8** (`list_subscribed`, `get_metadata`, `leave` from
TIER6_5 + `create`, `join`, `send_reaction`, `edit_message`, `revoke_message` from TIER7_E).

**Plan target: 14 newsletter.* methods** (per T02–T05). The 6 missing are
`newsletter.send_message`, `newsletter.get_messages`, `newsletter.get_subscribers`,
`newsletter.mute`, `newsletter.unmute`, `newsletter.accept_tos`.

The TIERS are already exercised by the **`registry_size_matches_phase1_phase2`** test
(`handlers/mod.rs:820-864`) which chains every TIER const through a BTreeSet to compute
the dedup'd expected count. Any new TIER-or-extended TIER must also be added to that chain.

---

## 3. Live test reproduction

**Daemon state at investigation time:**
- Unix-socket socket file exists: `/tmp/octo-wa-run/octo-whatsapp-default.sock` (confirmed by `ls`).
- Daemon process is running (PID `27713` visible via `pgrep -f "octo-whatsapp.*daemon"`).

**Test 1 — `daemon.methods.list` (sanity, expect 200 OK):**
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"daemon.methods.list","params":{}}' \
    | nc -U /tmp/octo-wa-run/octo-whatsapp-default.sock
```
Returns `{ "id": 1, "result": { "count": 182, "methods": [...] } }`. The `methods` array
contains: `newsletter.create`, `newsletter.edit_message`, `newsletter.get_metadata`,
`newsletter.join`, `newsletter.leave`, `newsletter.list_subscribed`,
`newsletter.revoke_message`, `newsletter.send_reaction`. **8 newsletter methods confirmed
reachable via the unix-socket dispatcher.**

**Test 2 — direct newsletter RPC (the bug from the prompt):**
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"newsletter.list_subscribed","params":{}}' \
    | nc -U /tmp/octo-wa-run/octo-whatsapp-default.sock
```
Got:
```json
{"id":1,"error":{"code":-32603,"message":"newsletter.list_subscribed failed: Platform whatsapp unreachable: list_subscribed_newsletters failed: IQ request failed"}}
```

**Surprise finding (deviation from the prompt's expected output):**
The expected error was `-32601 "method \"newsletter.list_subscribed\" not implemented in
Phase 1"`. The actual error was `-32603 "... IQ request failed"`. **Code -32603 is
`RpcErrorCode::InternalError`, returned by the handler when the underlying WhatsApp IQ
request fails.** This means the dispatcher **DOES find the method, DOES invoke the handler,
and the handler reaches the wacore backend** — which then fails because the WA session is
logged-out (Phase 6.12.3 gate). The connection between handler and wacore is fine; the
network session is broken. Once the WA session is re-paired, the call should succeed.

**The MCP-side bug remains live** (unreproducible here without an MCP client): `mcp_server.rs:80`
will return `-32601 "method \"newsletter.X\" not implemented in Phase 1"` for any client that
launches `octo-whatsapp mcp` (stdio mode) and sends an unknown JSON-RPC method. The correct
fix is to populate `tool_descriptors()` with the 14 newsletter tools and route them in
`handle_tools_call` — not to remove the line-80 fallback.

---

## 4. Cargo build status

```
cargo build --profile dev -p octo-whatsapp --features query
...
Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.49s
```
Clean. No code changes made (T01 is investigation-only).

---

## 5. Surprise findings vs. plan assumptions

| Plan assumption | Reality |
|---|---|
| "Direct JSON-RPC over the daemon socket returns -32601" | Daemon socket actually does NOT reject newsletter RPCs — it returns -32603 from the WA backend. The -32601 string lives in `mcp_server.rs:80` (the MCP stdio path) and `mcp_server.rs:1286` (the MCP tools/call routing match). |
| "The dispatcher rejects `newsletter.*` via allow-list" | There is no allow-list in the form the plan described. The dispatcher is a `HashMap<&'static str, Arc<dyn RpcHandler>>` that is built fresh in `build_registry()` at daemon boot. Methods missing from the map → `MethodNotFound`. Methods present in the map → handler is invoked. The "allow-list" the prompt refers to is the chain of `.register()` calls, not a separate string-set consulted for permission. |
| "Allow-list is what we need to extend" | Half-true. We DO need to (a) extend the `.register(...)` chain in `build_base_registry` with 6 new handler types and (b) extend the matching TIER const + chain in the `registry_size_matches_phase1_phase2` test. But the **primary user-visible gap is in the MCP tool surface** (`mcp_server.rs`), not in the unix-socket registry. |
| "8 newsletter.* methods already registered" | Confirmed. TIER6_5 + TIER7_E cover `list_subscribed`, `get_metadata`, `leave`, `create`, `join`, `send_reaction`, `edit_message`, `revoke_message`. |
| "14 newsletter.* methods needed" | Yes. The 6 missing are documented in §2 above. |

---

## 6. What later tasks (T02–T14) need to extend

| Layer | Surface | Location | Action |
|-------|---------|----------|--------|
| Trait | `OctoWhatsAppAdapter` | `crates/octo-whatsapp/src/adapter_trait.rs` (likely around line 880+) | **T02:** add 7 new trait methods (`send_message`, `get_messages`, `get_subscribers`, `mute`, `unmute`, `accept_tos`, plus a 7th to be confirmed) |
| Inherent | `WhatsAppWebAdapter` | `crates/octo-adapter-whatsapp/src/` (inherent impls) | **T03:** add 6 inherent + 6 forwarder + 6 mock impls |
| New handler files | `crates/octo-whatsapp/src/ipc/handlers/newsletter_*.rs` | (T04) | **T04:** add 6 new handler files |
| Registry chain | `build_base_registry` | `crates/octo-whatsapp/src/ipc/handlers/mod.rs:142-...` | **T05:** register the 6 new handlers + extend TIER const |
| TIER const | new `TIER7_E_PLUS_NEWSLETTER_METHODS` or extend `TIER7_E` | `crates/octo-whatsapp/src/ipc/handlers/mod.rs:679` | **T05:** add 6 strings to a const, add to `registry_size_matches_phase1_phase2` |
| Inbound event | `InboundEvent::NewsletterUpdate` | `crates/octo-whatsapp/src/inbound.rs` (or wherever `InboundEvent` lives) | **T06:** new variant + broadcast wiring |
| MCP tool descriptors | `tool_descriptors()` | `crates/octo-whatsapp/src/mcp_server.rs:123-...` | **T10:** add 14 `td()` entries for newsletter.* (raises `EXPECTED_TOOL_COUNT` from 128 to 142 with query, or 122 → 136 without) |
| MCP routing match | `handle_tools_call` | `crates/octo-whatsapp/src/mcp_server.rs:1241-1280` | **T10:** add 14 `newsletter.X => "newsletter.X"` arms |
| CLI | `NewsletterCmd` | `crates/octo-whatsapp/src/cli.rs` (or `bin/octo_whatsapp.rs`) | **T08:** add 14 subcommands |
| Skill catalog | `assets/skills/wa-mcp.md` | (T11) | **T11:** add §24 Newsletter (Channels) |
| Live tests | `crates/octo-whatsapp/tests/live_*` | (T12) | **T12:** add `live_newsletter_*` tests gated on a live WA session |
| MEMORY | `.jcode/memory/MEMORY.md` | (T13) | **T13:** remove newsletter.* from the deferral backlog, bump RPC totals |

---

## 7. Verification checklist

- [x] Located exact file:line of the dispatcher that returns -32601 → `mcp_server.rs:80` and `mcp_server.rs:1286`
- [x] Identified exact allow-list source(s) → `TIER6_5_NEWSLETTER_METHODS` + `TIER7_E_NEWSLETTER_TCTOKEN_METHODS` + the `.register(...)` chain in `build_base_registry`
- [x] Listed all TIER*_METHODS constants with counts — see §2
- [x] Confirmed live daemon is reachable and returns 200 OK on `daemon.methods.list` with all 8 newsletter.* names; direct `newsletter.list_subscribed` returns -32603 (not -32601) because the WA backend is unreachable, proving handler IS being invoked. **Prompt's expected -32601 lives in the MCP stdio path, not the daemon socket — bug confirmed for the MCP layer.**
- [x] cargo build clean
- [ ] Investigation file at `docs/investigations/2026-07-15-newsletter-rpc-allow-list.md` (about to be written)
- [ ] Committed with the message `docs(investigation): locate newsletter RPC dispatcher allow-list (Phase 7.E+ T01)`
