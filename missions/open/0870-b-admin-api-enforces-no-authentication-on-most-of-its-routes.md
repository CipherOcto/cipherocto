---
name: 0870-b-admin-api-enforces-no-authentication-on-most-of-its-routes
description: "The quota-router admin router has 57 route arms. Four handlers receive the request headers; none of them authorizes the caller. One looks the presented key up in storage but only to describe the caller's own key, two accept any string prefixed with Bearer and return a hardcoded identity, and one reads the Authorization header, discards it with a discard binding, and returns success unconditionally. POST /key/generate mints a spending API key with no credential of any kind. RFC-0904 specifies exactly the missing model — Bearer, out-of-band provisioning, no mint endpoint — but for a different admin surface that this router is not. The listener binds loopback only."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0870
    - RFC-0904
---

# Mission `0870-b-admin-api-enforces-no-authentication-on-most-of-its-routes`

## Status

Open. Nothing has been changed on the admin authentication path.

## Severity

**Bounded by loopback, unbounded by design.** The listener binds `127.0.0.1`,
so this is not remotely reachable as shipped. But loopback is a property of the
current deployment, not a property of the code, and on a host with a shared
network namespace — a container, a sidecar, a multi-tenant box — it is not a
boundary at all. The specific thing reachable is credential minting.

## The finding

`handle_request` in `crates/quota-router-core/src/admin.rs` is a flat
`match (method_str, path)` router with **57 route arms** — 35 matching an exact
literal, 22 matching a prefix or shape — plus a catch-all that falls through to
a `404`. Authentication is not middleware; there is no wrapper. Four handlers
receive the request headers, and **not one of them authorizes the caller.**

| Handler                  | Receives headers | What it actually does with the header                                                               |
| ------------------------ | ---------------- | --------------------------------------------------------------------------------------------------- |
| `handle_get_key_info`    | yes              | Looks the presented key up in storage and describes _that_ key — self-inspection, not authorization |
| `handle_userinfo`        | yes              | Checks only that the value starts with `Bearer `; returns a hardcoded identity                      |
| `handle_userinfo_claims` | yes              | Same shape check; same hardcoded claims                                                             |
| `handle_logout`          | yes              | **Reads the credential, discards it, returns success — see below**                                  |
| the other 53             | no               | nothing                                                                                             |

### Two of the four look authenticated and are not

```rust
fn handle_userinfo(headers: &HeaderMap) -> Response<String> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth.is_empty() || !auth.starts_with("Bearer ") {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(serde_json::json!({"error": "missing_token"}).to_string())
            .unwrap();
    }
    // ...
```

The 401 is real, but nothing after it looks the token up. There is no
signature check, no storage lookup, no expiry check, no scope check. The string
`Bearer x` is sufficient. What comes back is a constant:

```json
{ "sub": "user-id", "email": "user@example.com", "name": "SSO User" }
```

`handle_userinfo_claims` is the same shape with the same hardcoded values plus
`"roles": []`, `"groups": []`, `"iss": "https://localhost:8080"`. Every caller
sees the same claims, so the endpoint discloses nothing and authenticates no
one. A shape check that gates a constant response is the most misleading kind of
check to leave in place: it reads as authentication in the router, in the tests,
and in review.

### The one that throws the credential away

```rust
fn handle_logout(headers: &HeaderMap) -> Response<String> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let _ = auth;
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(serde_json::json!({"logged_out": true}).to_string())
        .unwrap()
}
```

It reads the `Authorization` header, binds it, discards the binding, and
returns `200 {"logged_out": true}` for every request including one with no
credentials at all. The shape of the function implies a check that the
behaviour does not contain.

### What is reachable without a credential

The most consequential is `POST /key/generate`, whose handler signature carries
no headers parameter at all:

```rust
fn handle_create_key(storage: &StoolapKeyStorage, req: &GenerateKeyRequest) -> Response<String>
```

It validates the team key limit and then mints. **Any local process can mint a
quota-router API key** — the credential that authorizes spend against the proxy.

Also unauthenticated: `POST /auth/token` and `POST /auth/token/introspect` (the
stub surface filed separately as mission 0949-d), `POST /team`,
`DELETE /key/:id`, `PUT /key/:id`, `POST /key/:id/regenerate`, and the rest of
the route table.

### The model that is missing is written down — just not here

RFC-0904 §Admin API Endpoints for Budget Status specifies, for its own admin
surface, precisely the discipline this router lacks: `Authorization: Bearer
<token>`, hex-encoded, minimum 32 bytes, validated by constant-time comparison
against a SHA-256-hashed configured token; provisioned out of band, with the
spec stating that _no API endpoint exists to create or rotate admin tokens_;
`401` on auth failure and `403` on insufficient privilege.

That surface is not this router — RFC-0904's budget endpoints are not
implemented in `admin.rs`, which is itself worth a decision. The point for this
mission is narrower and more useful: the pattern is already written down in the
RFC set, so this is not an open question of taste. This router is an
unauthenticated admin surface that mints the credentials the proxy trusts, and
the project has already specified what the alternative looks like.

### Path drift in the one place that names the admin plane

`CIPHEROCTO_INTERNAL_KEY_PREFIXES` in `crates/quota-router-core/src/egress/key_swap.rs`
documents its prefixes as identifying keys "minted by the CipherOcto admin plane
(`/admin/keys`)". The route that actually mints them is `POST /key/generate`.
The only written description of this admin surface names a path that does not
exist on it.

## What is NOT claimed

- No claim of remote exposure. The listener binds loopback, and that was
  measured.
- No claim that a key cannot legitimately be minted without a session. That is
  the design question, and it is a maintainer's call.
- No claim about the key _content_ — only that nothing gates its creation.
- No claim that RFC-0904's budget admin endpoints are meant to be this router.
  They are not, and the mission does not claim they are.

## Acceptance Criteria

- [ ] A trust model for this router is written down: loopback-trusted,
      authenticated, or both, and the RFC or doc that carries it
- [ ] `handle_userinfo` and `handle_userinfo_claims` either validate the
      credential or are documented as shape-only; a `Bearer ` prefix check that
      gates a constant response is the one state that must not survive
- [ ] `handle_logout` either validates the credential it reads, or stops
      reading it — the current shape implies a check that is not there
- [ ] `POST /key/generate` is behind whatever the written trust model
      requires, or the exemption is stated explicitly
- [ ] The auth-check discipline is expressed once — a wrapper or middleware —
      rather than restated in whichever handler remembers
- [ ] The `/admin/keys` path in the `key_swap` prefix documentation points at
      the route that exists

## Why this is filed rather than fixed

Whether an unauthenticated loopback admin API is acceptable is a security
posture decision with no obviously correct answer, and it depends on the
deployment model — a developer laptop, a single-tenant node, a shared host.
Those are the maintainers' to weigh.

Three items are _not_ judgement calls and are listed first in the acceptance
criteria because they are cheap and unambiguous:

- `handle_logout` reads a credential and throws it away.
- `handle_userinfo` and `handle_userinfo_claims` gate a constant response behind
  a string prefix.
- The `key_swap` documentation names a route that does not exist.

Each of those is wrong on its own terms, independent of where the trust line is
drawn.

## Related

Mission 0949-d covers the `/auth/*` routes on this same router, where the
RFC-0949 implementation exists but is never called. The two findings are
adjacent and may want one decision between them: this router is an
unauthenticated admin surface, and it also happens to expose token minting on
it.

The route table in the module documentation at the top of `admin.rs` lists each
route with its handler, which is what makes the handler-level check discipline
legible. It is also part of why the missing wrapper is easy to miss: the table
reads as if each handler were responsible.
