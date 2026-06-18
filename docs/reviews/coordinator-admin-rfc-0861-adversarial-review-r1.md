# CoordinatorAdmin RFC-0861 + Mission: adversarial review round 1 (R24a)

**Date:** 2026-06-18
**Branch:** `next`
**Scope:** Review the just-created `RFC-0861` (`CoordinatorAdmin
Adapter Contract Refinements`) and the matching mission
(`missions/open/0861-coordinator-admin-trait-refinements.md`) for
internal consistency, technical correctness, and cross-reference
fidelity against the current state of the code.
**Method:** Read both files end-to-end; cross-reference every line
number, every method name, and every claim about the existing code
against the current `next` branch; verify the spec against the actual
SDK APIs (`wacore::groups::join_with_invite_code`); check test
counts.

## Severity legend

- **CRITICAL** — the spec is wrong about the code; the proposed fix
  would silently fail or introduce a regression.
- **HIGH** — the spec is internally inconsistent or based on a
  premise that's no longer true; the fix path is blocked or wrong.
- **MEDIUM** — stale or incorrect line numbers, missing details, or
  unclear acceptance criteria that the implementer would have to
  re-derive.
- **LOW** — nits, style, doc consistency.

## CRITICAL findings

### N22 — M8 spec is wrong: the IRC listener does NOT parse RPL_WELCOME (001)

**File:** `missions/open/0861-coordinator-admin-trait-refinements.md:101-103`

The mission's "Implementation Notes" says:

> *"For M8's RPL_WELCOME parsing: the listener already parses 001 for
> its existing 'ready' logic (R23d H5 fix); add a
> `*self.is_authenticated.lock().await = true;` there."*

This is **false**. Cross-checked against the current code:

| File | Line | Snippet | What it does |
|---|---|---|---|
| `crates/octo-adapter-irc/src/lib.rs` | 829-836 | `if let Some(server) = trimmed.strip_prefix("PING ")` | PING/PONG keepalive |
| `crates/octo-adapter-irc/src/lib.rs` | 838-849 | `if !joined && (trimmed.contains(" 376 ") || trimmed.contains(" 422 "))` | JOIN trigger on RPL_ENDOFMOTD (376) or ERR_NOMOTD (422) |
| `crates/octo-adapter-irc/src/lib.rs` | 851-901 | `if let Some(msg) = parse_privmsg(trimmed)` | DOT message parsing |
| `crates/octo-adapter-irc/src/lib.rs` | 1915-1919 | `test_parse_privmsg_not_privmsg` | **Only here** is 001 mentioned — and only to test that `parse_privmsg` returns `None` for it |

There is **no** RPL_WELCOME (001) parsing in the listener. The only
listener signals are 376/422 (for JOIN), PING (for PONG), and PRIVMSG
(for DOT messages).

**Impact:** A naive implementer following the mission's instruction
would grep for `001` in the listener, find nothing, and either:
1. Add 001 parsing from scratch (which the spec doesn't say how to do),
   or
2. Conclude the spec is wrong and stop.

The spec needs to be reworked. Two options:

**Option A (preferred): use 376/422 as the "authenticated" trigger.**
The server only sends RPL_ENDOFMOTD (376) or ERR_NOMOTD (422) AFTER
the NICK/USER handshake succeeds. So by the time the listener sees
376/422, the bot is authenticated. Set `is_authenticated = true` in
the 376/422 branch (which already exists at line 838-849). This
requires zero new parsing.

**Option B: add a new 001 handler.** This is a wider change and
arguably less correct (001 is the "welcome" message, but 376/422
indicates the post-MOTD state which is more semantically "ready").

**Fix:** rewrite the M8 implementation note to use Option A.

## HIGH findings

### N23 — Phase 4 ("M3 TLS health check") is bogus: R23d C1 is already fixed

**File:** `missions/open/0861-coordinator-admin-trait-refinements.md:65-68`,
`rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:238-243, 274-276`

The RFC and mission claim Phase 4 is blocked on R23d C1:

> *"M3 marked as a sub-task; only implementable once C1's
> `connect_tls` is real (R23d C1 fix path)"*

But R23d (commit `4b0f5e0`, "R23d: fix all R23c CRITICAL/HIGH
findings") already fixed C1. The current `connect_tls` at
`crates/octo-adapter-irc/src/lib.rs:713-723`:

```rust
async fn connect_tls(server: &str, port: u16, sni: &str) -> Result<IrcStream, String> {
    let tcp = connect_plain(server, port).await?;
    let connector = TlsConnector::from(tls_client_config());
    let name = ServerName::try_from(sni.to_string())
        .map_err(|e| format!("invalid server name for SNI {sni:?}: {e}"))?;
    connector
        .connect(name, tcp)
        .await
        .map(IrcStream::Tls)
        .map_err(|e| format!("TLS handshake: {e}"))
}
```

This uses real `tokio-rustls` TLS. C1 is fixed.

**Impact:** Phase 4 is a no-op acceptance criterion; an implementer
would correctly wonder what they're supposed to do. M3 (the TLS
health check) is unblocked and should fold into Phase 3.

**Fix:** delete Phase 4; add M3 acceptance criteria to Phase 3. Update
the RFC §3 ("Phase 4: Update M3 once C1 is fixed") and the
Implementation Phases table.

## MEDIUM findings

### N24 — Multiple stale line numbers

**File:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md`

| Quote | Stated location | Actual location |
|---|---|---|
| H2: "leave_group_str" precedent at `adapter.rs:1767-1796` | 1767-1796 | inherent method at line 1769; comment block at 1763-1764; trait impl at 1467-1479 |
| §6 M14: "IRC `join_by_invite` (line 1036) and `list_own_groups` (line 976)" | 1036, 976 | `join_by_invite` at line 1518; `list_own_groups` at line 1443 (with `is_admin: false` at line 1469) |
| RFC R1 review inheritance: "IRC `add_member` doesn't require op status. File: `crates/octo-adapter-irc/src/lib.rs:784-796`" | 784-796 | `add_member` is at line 1261-1273 |
| Key Files to Modify: "M7: `add_member` (INVITE)" | (no line) | line 1261-1273 |

**Impact:** A reviewer or implementer cross-referencing these lines
will be confused; the R1 review's "line 1036" and "line 976" are
particularly off (~500 lines).

**Fix:** update all line references to the actual current locations.

### N25 — H1 spec doesn't specify how to map `JoinGroupResult` into `GroupHandle`

**File:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:102, 259`

The H1 "implement" alternative says:
> *"Implement `join_by_invite` via `client.groups().join_with_invite_code(...)`"*

Cross-checked the SDK at
`/home/mmacedoeu/.cargo/git/checkouts/whatsapp-rust-6e26428647c827f3/9734fb2/src/features/groups.rs:476-483`:

```rust
pub async fn join_with_invite_code(
    &self,
    code: &str,
) -> Result<JoinGroupResult, anyhow::Error> { ... }
```

And `JoinGroupResult` is an enum
(`/home/mmacedoeu/.cargo/git/checkouts/whatsapp-rust-6e26428647c827f3/9734fb2/wacore/src/iq/groups.rs:2318-2322`):

```rust
pub enum JoinGroupResult {
    Joined(Jid),
    PendingApproval(Jid),
}
```

So the WhatsApp impl needs to:
1. Parse the invite URL/code (the SDK does this internally via `extract_invite_code`)
2. Call `client.groups().join_with_invite_code(invite.0.as_str())`
3. Map the `anyhow::Error` to `PlatformAdapterError::ApiError`
4. Map the two `JoinGroupResult` variants to `GroupHandle`:
   - `Joined(jid)` → `GroupHandle { id: GroupId::new(jid.to_string()), is_admin: false, ... }`
   - `PendingApproval(jid)` → ??? — return `Ok(GroupHandle { id, is_admin: false, member_count: 0, ... })` (caller can check), or return a structured `ApiError { code: 202, message: "join pending approval" }`?

**Impact:** Without the spec, the implementer has to invent the
`PendingApproval` semantics, which risks a breaking decision.

**Fix:** add a paragraph to H1 specifying the two-variant mapping.
Recommendation: return `Ok(GroupHandle)` in both cases with
`is_admin: false`; document in `GroupHandle` that the caller can
detect "pending" by `subject: None` (since the bot isn't fully in the
group yet, subject may be hidden).

### N26 — `futures::future::join_all` is NOT a workspace dep of `octo-adapter-whatsapp`

**File:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:183`

The M13 spec says:
> *"using `futures::future::join_all` (already a workspace dep)"*

`grep` of `Cargo.toml` files:
- `crates/octo-adapter-nostr/Cargo.toml`: `futures-util = "0.3"` ✅
- `crates/octo-adapter-p2p/Cargo.toml`: `futures = "0.3"` ✅
- `crates/octo-whatsapp-onboard/Cargo.toml`: `futures = "0.3"` ✅
- `crates/octo-adapter-whatsapp/Cargo.toml`: **no `futures` dep** ❌

**Impact:** Implementer would need to add a new dep, which is fine
but should be in the mission's acceptance criteria.

**Fix:** change M13 to specify that the implementer MUST add
`futures = "0.3"` to `octo-adapter-whatsapp/Cargo.toml`. Or use
`tokio::task::JoinSet` (already a dep via `tokio`).

## LOW findings

### N27 — Test count undersold for WhatsApp

**File:** `missions/open/0861-coordinator-admin-trait-refinements.md:53`

> *"All Phase 2 changes pass `cargo check` and `cargo test` for
> `octo-adapter-whatsapp` (existing 50+ tests still pass; new tests
> for each finding)"*

Current count: `cargo test -p octo-adapter-whatsapp --lib` reports
**63 tests passing**. The mission's "50+" is correct in direction but
undersells the baseline. (R5 review said 50 for IRC, which is correct.)

**Fix:** use exact numbers (IRC: 50, WhatsApp: 63) or just "all existing tests".

### N28 — Phase 1 `cargo test` command needs `--lib` flag

**File:** `missions/open/0861-coordinator-admin-trait-refinements.md:41`

> *"All Phase 1 changes pass `cargo check` and `cargo test` for
> `octo-network` (the trait crate)"*

The trait is in `octo-network/src/dot/adapters/coordinator_admin.rs`
which is a lib. The `cargo test -p octo-network` command would also
run integration tests, which are out of scope for Phase 1. Use
`cargo test -p octo-network --lib`.

**Fix:** add `--lib` flag for precision.

### N29 — M4 `#[serde(default)]` rationale could be clearer

**File:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:166-170`

The spec says the field is "defaulted via `#[serde(default)]` for
backward compatibility". This is technically correct, but the
existing fields (`subject`, `invite_url`, etc.) are `Option<T>` which
already serializes as a `None` default. A `bool` with `#[serde(default)]`
explicitly defaults to `false` on missing fields. The implementer
should understand this is a wire-compatibility consideration.

**Fix:** add one sentence: "this is an additive change; pre-RFC-0861
serialized handles deserialize with `initial_admins_promoted: false`."

### N30 — N22's downstream: M8 doc must agree with the chosen trigger

**File:** `rfcs/draft/networking/0861-coordinator-admin-trait-refinements.md:201-206`

RFC §4 M8 says: *"set it on `RPL_WELCOME` (001)"* — same false claim
as N22. Once N22 is fixed (use 376/422 instead), the doc must be
updated to match.

**Fix:** bundle with N22.

## Verification of N22 / N23 hypotheses

To confirm R23d C1 is fixed, I ran `grep` on the current `next`
branch:

```
$ grep -n "fn connect_tls\|connect_tls" crates/octo-adapter-irc/src/lib.rs
713:async fn connect_tls(server: &str, port: u16, sni: &str) -> Result<IrcStream, String> {
```

And the body uses `TlsConnector`, `ServerName`, `connector.connect(...)` —
real `tokio-rustls` calls, not the `connect_plain` placeholder from
R1. R23d C1 is fixed; Phase 4 is bogus.

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 1 (N22) |
| HIGH     | 1 (N23) |
| MEDIUM   | 3 (N24, N25, N26) |
| LOW      | 4 (N27, N28, N29, N30) |
| **Total**| **9** |

Net: 1 critical correctness bug in the mission (M8 spec is wrong
about the listener), 1 stale blocker (Phase 4 / R23d C1), 3 stale or
missing details, 4 nits. All fixable in a single follow-up commit.
