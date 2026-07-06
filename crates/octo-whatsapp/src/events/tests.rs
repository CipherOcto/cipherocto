use super::*;

fn env(raw: &str) -> EventEnvelope {
    EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1_700_000_000_000,
        ts_mono_ns: 123_456_789,
    }
}

#[test]
fn unknown_fallback_for_unrecognised_input() {
    let ev = InboundEvent::parse(env("completely unrelated text"));
    assert!(matches!(ev, InboundEvent::Unknown { .. }));
}

#[test]
fn unknown_fallback_for_empty_input() {
    let ev = InboundEvent::parse(env(""));
    assert!(matches!(ev, InboundEvent::Unknown { .. }));
}

#[test]
fn message_text_parses() {
    let raw = r#"Message(id: "ABC123", peer: "1234@s.whatsapp.net", sender: "5678@s.whatsapp.net", text: "hello", kind: Text, is_group: false)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Message {
            id,
            text,
            kind,
            is_group,
            ..
        } => {
            assert_eq!(id, "ABC123");
            assert_eq!(text, "hello");
            assert_eq!(kind, MessageKind::Text);
            assert!(!is_group);
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn message_image_with_caption() {
    let raw = r#"Message(id: "M1", peer: "X", sender: "Y", text: "caption here", kind: Image, media_token: "tok-abc", is_group: true)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Message {
            kind,
            media_token,
            is_group,
            text,
            ..
        } => {
            assert_eq!(kind, MessageKind::Image);
            assert_eq!(media_token.as_deref(), Some("tok-abc"));
            assert!(is_group);
            assert_eq!(text, "caption here");
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn message_truncates_text_at_max() {
    let big = "x".repeat(MAX_INLINE_TEXT_BYTES + 100);
    let raw = format!(
        r#"Message(id: "X", peer: "P", sender: "S", text: "{big}", kind: Text, is_group: false)"#
    );
    let ev = InboundEvent::parse(env(&raw));
    match ev {
        InboundEvent::Message { text, .. } => {
            assert_eq!(text.len(), MAX_INLINE_TEXT_BYTES);
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn bound_mentions_truncates_with_flag() {
    let mut v: Vec<String> = (0..MAX_INLINE_MENTIONS + 5)
        .map(|i| format!("m{i}"))
        .collect();
    let (kept, truncated) = InboundEvent::bound_mentions(v.clone());
    assert_eq!(kept.len(), MAX_INLINE_MENTIONS);
    assert!(truncated);
    v.truncate(MAX_INLINE_MENTIONS);
    let (kept, truncated) = InboundEvent::bound_mentions(v);
    assert_eq!(kept.len(), MAX_INLINE_MENTIONS);
    assert!(!truncated);
}

#[test]
fn reaction_parses() {
    let raw = r#"Reaction(id: "R1", target_msg_id: "M0", emoji: "👍", from: "X", peer: "Y")"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Reaction {
            id,
            target_msg_id,
            emoji,
            ..
        } => {
            assert_eq!(id, "R1");
            assert_eq!(target_msg_id, "M0");
            assert_eq!(emoji, "👍");
        }
        other => panic!("expected Reaction, got {other:?}"),
    }
}

#[test]
fn group_change_subject_parses() {
    let raw = r#"GroupChange(group_jid: "123@g.us", kind: Subject, actor: "A", after: "new name")"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::GroupChange {
            group_jid,
            kind,
            actor,
            after,
            ..
        } => {
            assert_eq!(group_jid, "123@g.us");
            assert_eq!(kind, GroupChangeKind::Subject);
            assert_eq!(actor.as_deref(), Some("A"));
            assert_eq!(after.as_deref(), Some("new name"));
        }
        other => panic!("expected GroupChange, got {other:?}"),
    }
}

#[test]
fn presence_with_last_seen() {
    let raw = r#"Presence(jid: "X@s.whatsapp.net", kind: Available, last_seen: 1700000000)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Presence {
            jid,
            kind,
            last_seen,
        } => {
            assert_eq!(jid, "X@s.whatsapp.net");
            assert_eq!(kind, PresenceKind::Available);
            assert_eq!(last_seen, Some(1700000000));
        }
        other => panic!("expected Presence, got {other:?}"),
    }
}

#[test]
fn presence_typing() {
    let raw = r#"Presence(jid: "X", kind: Typing)"#;
    let ev = InboundEvent::parse(env(raw));
    assert!(matches!(
        ev,
        InboundEvent::Presence {
            kind: PresenceKind::Typing,
            ..
        }
    ));
}

#[test]
fn connection_connected() {
    let raw = "Connection(kind: Connected)";
    let ev = InboundEvent::parse(env(raw));
    assert!(matches!(
        ev,
        InboundEvent::Connection {
            kind: ConnectionKind::Connected,
            ..
        }
    ));
}

#[test]
fn connection_logged_out_with_cause() {
    let raw = "Connection(kind: LoggedOut, cause: UserInitiated)";
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Connection { kind, cause, .. } => {
            assert_eq!(kind, ConnectionKind::LoggedOut);
            assert_eq!(cause, Some(LoggedOutCause::UserInitiated));
        }
        other => panic!("expected Connection, got {other:?}"),
    }
}

#[test]
fn receipt_read() {
    let raw = r#"Receipt(msg_id: "M1", peer: "P", kind: Read)"#;
    let ev = InboundEvent::parse(env(raw));
    assert!(matches!(
        ev,
        InboundEvent::Receipt {
            kind: ReceiptKind::Read,
            ..
        }
    ));
}

#[test]
fn call_voice_offered() {
    let raw = r#"Call(id: "C1", peer: "P", kind: Voice, state: Offered)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Call { kind, state, .. } => {
            assert_eq!(kind, CallKind::Voice);
            assert_eq!(state, CallState::Offered);
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn story_posted() {
    let raw = r#"Story(id: "S1", peer: "P", kind: Posted)"#;
    let ev = InboundEvent::parse(env(raw));
    assert!(matches!(
        ev,
        InboundEvent::Story {
            kind: StoryKind::Posted,
            ..
        }
    ));
}

#[test]
fn ts_unix_ms_accessible_for_all_variants() {
    // Presence has last_seen instead of ts_unix_ms, so we verify
    // the explicit ts_unix_ms() == last_seen mapping.
    let unknown = InboundEvent::Unknown {
        raw: "x".into(),
        ts_unix_ms: 42,
        ts_mono_ns: 1,
        untrusted: false,
    };
    assert_eq!(unknown.ts_unix_ms(), 42);
    let presence = InboundEvent::Presence {
        jid: "x".into(),
        kind: PresenceKind::Available,
        last_seen: Some(42),
    };
    assert_eq!(presence.ts_unix_ms(), 42);
    let presence_no_seen = InboundEvent::Presence {
        jid: "x".into(),
        kind: PresenceKind::Available,
        last_seen: None,
    };
    assert_eq!(presence_no_seen.ts_unix_ms(), 0);
}

#[test]
fn ts_mono_ns_none_for_presence() {
    let ev = InboundEvent::Presence {
        jid: "x".into(),
        kind: PresenceKind::Available,
        last_seen: None,
    };
    assert_eq!(ev.ts_mono_ns(), None);
}

#[test]
fn ts_mono_ns_some_for_typed_variants() {
    let ev = InboundEvent::Unknown {
        raw: "x".into(),
        ts_unix_ms: 0,
        ts_mono_ns: 999,
        untrusted: false,
    };
    assert_eq!(ev.ts_mono_ns(), Some(999));
}

#[test]
fn is_untrusted_far_future_timestamp() {
    let ev = InboundEvent::parse_with_now(
        EventEnvelope { raw: "Message(id: \"X\", peer: \"P\", sender: \"S\", text: \"\", kind: Text, is_group: false)".into(), ts_unix_ms: 1_000_000_000_000, ts_mono_ns: 0 },
        1_000_000_000_000 - SKEW_TOLERANCE_MS - 1,
    );
    assert!(ev.is_untrusted(1_000_000_000_000 - SKEW_TOLERANCE_MS - 1));
}

#[test]
fn is_untrusted_false_within_tolerance() {
    let ev = InboundEvent::parse_with_now(
        EventEnvelope {
            raw: "x".into(),
            ts_unix_ms: 1000,
            ts_mono_ns: 0,
        },
        1000 - SKEW_TOLERANCE_MS + 1000,
    );
    assert!(!ev.is_untrusted(1000 - SKEW_TOLERANCE_MS + 1000));
}

#[test]
fn parse_with_now_marks_unknown_untrusted() {
    let ev = InboundEvent::parse_with_now(
        EventEnvelope {
            raw: "garbage".into(),
            ts_unix_ms: 1_000_000_000_000,
            ts_mono_ns: 0,
        },
        1_000_000_000_000 - SKEW_TOLERANCE_MS - 1,
    );
    match ev {
        InboundEvent::Unknown { untrusted, .. } => assert!(untrusted),
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[test]
fn serde_json_round_trip_message() {
    let raw =
        r#"Message(id: "M1", peer: "P", sender: "S", text: "hi", kind: Text, is_group: false)"#;
    let ev = InboundEvent::parse(env(raw));
    let json = serde_json::to_string(&ev).unwrap();
    let back: InboundEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(ev, back);
}

#[test]
fn serde_json_round_trip_receipt() {
    let raw = r#"Receipt(msg_id: "M1", peer: "P", kind: Delivered)"#;
    let ev = InboundEvent::parse(env(raw));
    let json = serde_json::to_string(&ev).unwrap();
    let back: InboundEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(ev, back);
}
