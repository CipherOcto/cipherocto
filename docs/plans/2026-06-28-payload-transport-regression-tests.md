# Payload Transport Regression Test Plan

## Context

RFC-0850 v1.3.0 renamed `send_envelope(domain, envelope)` → `send_message(domain, envelope, payload)`. The bridge now passes payload bytes to adapters. This plan covers regression tests to verify the payload flows correctly through all layers.

## Test Layers

### L1: Bridge payload passthrough (octo-transport)

**File:** `octo-transport/src/adapter_bridge.rs` (extend existing tests)

| Test                                      | What it verifies                                                                |
| ----------------------------------------- | ------------------------------------------------------------------------------- |
| `bridge_passes_payload_to_adapter`        | Mock adapter receives the exact payload bytes passed to `NetworkSender::send()` |
| `bridge_payload_matches_envelope_hash`    | Payload bytes match `envelope.payload_hash` (BLAKE3 integrity)                  |
| `bridge_empty_payload`                    | Empty payload `b""` is passed correctly (not None, not dropped)                 |
| `bridge_large_payload`                    | 1MB payload passes without truncation                                           |
| `bridge_payload_not_shared_between_calls` | Two sequential sends with different payloads don't leak data                    |

### L2: Adapter payload receipt (all 25 adapters)

**Files:** Each adapter's test module

For adapters that USE payload (bluetooth, discord, irc, lora, matrix, matrix-sdk, nostr, tcp, udp, whatsapp, telegram-mtproto):

| Test                             | What it verifies                                                                  |
| -------------------------------- | --------------------------------------------------------------------------------- |
| `send_message_receives_payload`  | Adapter's send_message body can access the payload parameter                      |
| `send_message_payload_not_empty` | When non-empty payload is sent, adapter processes it (mock transport captures it) |

For adapters that DON'T use payload yet (bluesky, dingtalk, lark, p2p, qq, quic, reddit, signal, slack, telegram, twitter, webhook, webrtc, wechat):

| Test                                  | What it verifies                                                         |
| ------------------------------------- | ------------------------------------------------------------------------ |
| `send_message_compiles_with_payload`  | Adapter compiles and runs with new signature (existing tests cover this) |
| `send_message_ignores_payload_safely` | Adapter can safely ignore payload without panic or error                 |

### L3: NodeTransport integration (octo-transport)

**File:** `octo-transport/src/node_transport.rs` (extend existing tests)

| Test                                        | What it verifies                                                       |
| ------------------------------------------- | ---------------------------------------------------------------------- |
| `node_transport_send_best_passes_payload`   | `send_best(payload, ctx)` → bridge → adapter receives payload          |
| `node_transport_broadcast_passes_payload`   | `broadcast(payload, ctx)` → all bridges → all adapters receive payload |
| `node_transport_failover_preserves_payload` | First adapter fails, second adapter receives same payload              |

### L4: Full chain regression (octo-network + octo-transport)

**File:** `crates/octo-network/tests/e2e_live_scenarios.rs` (extend)

| Test                             | What it verifies                                                                |
| -------------------------------- | ------------------------------------------------------------------------------- |
| `full_chain_payload_integrity`   | NodeTransport → PlatformAdapterBridge → MockAdapter → payload bytes match input |
| `payload_roundtrip_through_mock` | Send payload, adapter captures it, verify exact bytes                           |

### L5: Quota router end-to-end (quota-router-e2e-tests)

**File:** `quota-router-e2e-tests/tests/l3_tcp_basic.rs` (extend)

| Test                                     | What it verifies                                                  |
| ---------------------------------------- | ----------------------------------------------------------------- |
| `tcp_adapter_sends_payload_over_wire`    | TcpAdapter send_message sends payload bytes in TCP frame          |
| `tcp_adapter_receives_payload_from_wire` | TcpAdapter receive_messages returns payload in RawPlatformMessage |

## Implementation Order

1. L1: Bridge tests (5 tests)
2. L2: Adapter receipt tests (25 adapters × 1-2 tests each)
3. L3: NodeTransport tests (3 tests)
4. L4: Full chain tests (2 tests)
5. L5: TCP e2e tests (2 tests)

## Acceptance Criteria

- [ ] All L1-L5 tests pass
- [ ] No adapter panics when receiving payload
- [ ] Payload bytes are identical through the full chain (NodeTransport → Bridge → Adapter)
- [ ] Empty payload handled correctly
- [ ] Large payload (1MB) handled correctly
- [ ] `cargo clippy` clean
- [ ] `cargo fmt` clean
