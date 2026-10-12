---
name: 0949-d-admin-auth-routes-serve-unlabelled-stubs-and-never-call-the-rfc-0949-implementation
description: 'The admin.rs HTTP router serves /auth/token, /auth/token/refresh, /auth/token/revoke, /auth/token/introspect and /auth/jwks from four hand-written stub functions that are not marked as stubs. POST /auth/token returns access_token = "at_" + the client-supplied code with no PKCE check, no client authentication and no code verification. The client_credentials grant returns the literal constant at_cc_placeholder. POST /auth/token/introspect reports active:true for any non-empty token that does not start with revoked_. The RFC-0949 OAuth2/OIDC implementation in auth/sso/oauth2.rs exists, is covered by 113 tests, and has zero production callers.'
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0949
    - 0949-b-sso-oauth2-oidc
---

# Mission `0949-d-admin-auth-routes-serve-unlabelled-stubs-and-never-call-the-rfc-0949-implementation`

## Status

Open. Nothing has been changed on the auth path. The findings below are
measured, not inferred from reading.

## Severity

**Local-only, not remote.** The reachability question that gated this
mission is now answered — see §Reachability, resolved. Both servers the
`quota-router` binary starts bind `127.0.0.1`, so these routes are not
reachable from off-host. That lowers the urgency but does not make the
behaviour correct, and it is one reverse proxy away from being remote
again.

## Reachability, resolved

Traced from the route handlers outward:

| Question                                     | Answer            | Evidence                                                                 |
| -------------------------------------------- | ----------------- | ------------------------------------------------------------------------ |
| Is `handle_request` reachable from a server? | Yes               | `AdminServer::run` serves it over HTTP/1.1 via `http1::Builder`          |
| Is that server bound publicly?               | No, loopback only | `SocketAddr::from(([127, 0, 0, 1], self.port))`                          |
| Does anything start it?                      | Yes               | the `proxy` subcommand spawns `AdminServer::run` alongside `ProxyServer` |
| Is that a shipped binary?                    | Yes               | `[[bin]] name = "quota-router"`                                          |
| Is the sibling proxy public?                 | No, also loopback | `SocketAddr::from(([127, 0, 0, 1], self.port))` in `proxy.rs`            |
| Are the routes documented as real?           | No                | `auth/token` appears in no doc under `docs/quota-router-cli/`            |

So: any process on the host can mint a bearer token from this listener and
have `/auth/token/introspect` confirm it active. Nothing off-host can, today.

Two things make that worth closing rather than deferring indefinitely.
`/auth/token/introspect` answering `active: true` for an arbitrary string is
wrong under any deployment, and loopback-only is a deployment property, not
a code property — the usual way to expose a loopback service is a reverse
proxy or a port forward, and either one turns this into a remote
authentication bypass without touching this crate.

The routes are undocumented rather than falsely documented. There is no
false claim in the CLI API reference to correct; the problem is confined to
the code.

## The finding

`handle_request` in `crates/quota-router-core/src/admin.rs` is a flat
`match (method, path)` router. Five auth paths are dispatched from it. None
of the five handler functions is marked `stub`, `placeholder`, `TODO`, or
`unimplemented!`. They read like finished implementations, and three of them
return HTTP 200 with a JSON body.

The RFC-0949 OAuth2/OIDC implementation lives in
`crates/quota-router-core/src/auth/sso/oauth2.rs`. It implements the
authorization-code-with-PKCE flow, client credentials, a session store and a
token blacklist. It is not stubbed and it is not broken.

**The router never calls it.**

## Evidence

### 1. Measured responses

Captured by driving the real router through `handle_request` and printing the
status and the first 160 bytes of each body:

| Request                                                                                    | Status | Body                                                                                                     |
| ------------------------------------------------------------------------------------------ | ------ | -------------------------------------------------------------------------------------------------------- |
| `POST /auth/token` `{"grant_type":"authorization_code","code":"test-code"}`                | 200    | `{"access_token":"at_test-code","expires_in":3600,"refresh_token":"rt_test-code","token_type":"Bearer"}` |
| `POST /auth/token/refresh` `{"grant_type":"refresh_token","refresh_token":"test-refresh"}` | 200    | `{"access_token":"at_rotated_test-refresh",...}`                                                         |
| `POST /auth/token/revoke` `{"token":"test-token"}`                                         | 200    | `{"revoked":true}`                                                                                       |
| `POST /auth/token/introspect` `{"token":"test-token"}`                                     | 200    | `{"active":true,"exp":...,"sub":"user-id","token_type":"Bearer"}`                                        |
| `GET /auth/userinfo` (no auth header)                                                      | 401    | `{"error":"missing_token"}`                                                                              |
| `POST /auth/logout`                                                                        | 200    | `{"logged_out":true}`                                                                                    |

The authorization-code grant returns the caller-supplied `code` echoed into the
access token. There is no code verification, no client authentication and no
PKCE check, because `handle_token_exchange` never calls the module that
implements any of them.

The `client_credentials` grant is worse: it returns the fixed string
`at_cc_placeholder` to any caller. That constant is in the source.

### 2. Introspection reports arbitrary tokens as active

```rust
let token = json.get("token").and_then(|v| v.as_str()).unwrap_or("");
let active = !token.is_empty() & !token.starts_with("revoked_");
```

Any non-empty string introspects as an active bearer token with
`sub: "user-id"`. RFC-7662 introspection must report `active: false` for a
token it does not recognise. A resource server that trusts this endpoint
accepts any string as a credential.

### 3. The JWKS is empty

`handle_jwks` returns `{"keys": []}`. Any resource server validating tokens
against this key set finds no keys.

### 4. The real implementation has no production caller

`admin.rs` references `auth::sso` in exactly three places, none of them token
handling:

- `generate_sp_metadata` (SAML SP metadata)
- `OAuth2State::new(provider_id)` (state at the callback)
- `OidcDiscovery::from_provider(...)` (the discovery document)

A tree-wide search for the module's token-exchange entry points
(`exchange_authorization_code`, `ClientCredentialsFlow`, `OAuth2Provider::`)
returns no hits outside `oauth2.rs` itself. The RFC-0949 implementation is
reachable only from its own tests.

### 5. The mission that closed the gap described the module, not the surface

`missions/claimed/0949-b-sso-oauth2-oidc.md` is marked LANDED with 30/31 ACs
passing, and its ACs include "Implement Authorization Code + PKCE flow" and
"Implement Client Credentials flow". Both statements are true of
`oauth2.rs`. Neither says the routes serve it, and they do not.

### 6. The tests over this surface assert nothing

Nine tests covering these routes issued the request and discarded the result:

```rust
let resp = do_request("POST", "/auth/token", Some(body.to_string())).await;
let _status = resp.status();
```

One of them says so outright: "Token exchange may fail without proper OAuth2
setup, but shouldn't panic". `let _status = resp.status()` binds a value to
keep the compiler quiet without checking it. This is why the divergence was
never caught: the tests could not fail, and they were in the file where a
reviewer would look first.

## Why this is filed rather than fixed

Three questions are not the agent's to answer:

1. ~~**Is this router mounted in a production binary?**~~ Answered: yes, and
   loopback-only. See §Reachability, resolved. The remaining two questions
   are still open.
2. **Should the routes be deleted, or wired to `oauth2.rs`?** Wiring them
   requires provider configuration, a session store and a blacklist that the
   router does not currently receive as parameters. `handle_request` takes
   `storage` and `registry` only. That is a signature and architecture change.
3. **What happens to tokens already issued by the stub?** `at_cc_placeholder`
   is a constant. If this was ever reachable, every client holds the same
   bearer token.

All three need a maintainer.

## What is NOT claimed

- No claim that these routes are reachable off-host. Established: both
  listeners bind `127.0.0.1`. See §Reachability, resolved.
- No claim that a vulnerability has been exploited. None is evidenced.
- No claim that `oauth2.rs` is defective. It was not modified and is not the
  subject of this mission.
- No claim that HTTP 200 on token exchange is wrong per se. RFC-6749 uses 200
  for a successful exchange. The defect is that no exchange is performed.

## Acceptance Criteria

- [x] Reachability of the `admin.rs` router in a production binary is
      established and written down — loopback only, started by the `proxy`
      subcommand of the shipped `quota-router` binary
- [ ] A decision is recorded: wire the routes to `auth::sso::oauth2`, or
      remove them and return 404
- [ ] `/auth/token/introspect` reports `active: false` for an unrecognised
      token
- [ ] `/auth/jwks` serves the real key set, or the route is removed
- [x] Every handler reached from this router is labelled if it is a stub —
      the five stub handlers now carry a `STUB` doc comment naming this
      mission
- [x] The nine tests over these routes assert real responses instead of
      discarding them

## Related work in this arc

Found by the same mechanical sweep that found this:

- `crates/octo-network/src/dot/adapters/registry.rs` had a test named
  `test_mark_unhealthy_and_healthy` with an empty body and a comment conceding
  "the API is verified by the existence of both methods". Fixed.
- `crates/quota-router-core/src/admin.rs` had `test_route_get_team_list` with an
  empty body documenting a live panic. Fixed, along with the panic itself.
