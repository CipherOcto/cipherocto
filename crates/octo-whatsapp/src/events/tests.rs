use super::*;

#[test]
fn parse_returns_unknown() {
    let env = EventEnvelope {
        raw: "any string".to_string(),
        ts_unix_ms: 1_700_000_000_000,
        ts_mono_ns: 123_456_789,
    };
    let ev = InboundEvent::parse(env);
    assert!(matches!(ev, InboundEvent::Unknown { .. }));
}
