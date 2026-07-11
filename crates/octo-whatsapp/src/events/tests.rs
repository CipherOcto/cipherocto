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

// ===========================================================================
// ServerAck → Receipt parser
//
// Tier 3 (receipts) relied on wacore's ServerAck events being routed to a
// typed Receipt, but the original parser only matched the literal
// `Receipt(` prefix and dropped everything else into Unknown. These tests
// pin the parser behaviour so a future refactor doesn't silently break
// the chain again.
// ===========================================================================

#[test]
fn parser_routes_server_ack_to_receipt_kind_delivered() {
    let raw = r#"ServerAck(ServerAck { id: "3EB0123", class: Some("message"), from: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), timestamp: Some(2026-07-11T12:00:00Z), error: None })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt {
            msg_id,
            peer,
            kind,
            ..
        } => {
            assert_eq!(msg_id, "3EB0123");
            assert_eq!(peer, "15551234567@s.whatsapp.net");
            assert!(matches!(kind, ReceiptKind::Delivered));
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_routes_server_ack_with_device_suffix() {
    let raw = r#"ServerAck(ServerAck { id: "3EB0999", class: Some("message"), from: Some(Jid { user: "5521995544743", server: Pn, agent: 0, device: 25, integrator: 0 }), timestamp: Some(2026-07-11T12:00:00Z), error: None })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt { peer, .. } => {
            assert_eq!(peer, "5521995544743:25@s.whatsapp.net");
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_drops_non_message_server_ack_to_unknown() {
    let raw = r#"ServerAck(ServerAck { id: "3EB0AAA", class: Some("receipt"), from: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), timestamp: Some(2026-07-11T12:00:00Z), error: None })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    assert!(
        matches!(ev, InboundEvent::Unknown { .. }),
        "non-message class must stay Unknown, got {ev:?}"
    );
}

#[test]
fn parser_routes_lid_server_to_lid_canonical() {
    let raw = r#"ServerAck(ServerAck { id: "3EB0BBB", class: Some("message"), from: Some(Jid { user: "999", server: Lid, agent: 0, device: 0, integrator: 0 }), timestamp: Some(2026-07-11T12:00:00Z), error: None })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt { peer, .. } => {
            assert_eq!(peer, "999@lid");
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

// ===========================================================================
// Receipt parser — wacore `r#type` raw-identifier field
//
// The prior parser matched a non-existent `kind` field and silently
// routed every wacore Receipt to ReceiptKind::Delivered via the
// wildcard arm. wacore emits `r#type:` (Rust raw-identifier escape
// for the field literally named `type`), so Read / Played receipts
// were masquerading as Delivered. These tests pin the corrected
// mapping so the bug class can't regress.
// ===========================================================================

#[test]
fn parser_routes_wacore_receipt_read_kind() {
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), sender: Some(Jid { user: "9988776655", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0READ"], timestamp: 2026-07-11T12:00:00Z, r#type: Read, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt {
            msg_id,
            peer,
            kind,
            ..
        } => {
            assert_eq!(msg_id, "3EB0READ");
            assert!(matches!(kind, ReceiptKind::Read));
            // `peer` is the chat Jid from MessageSource.
            assert_eq!(peer, "15551234567@s.whatsapp.net");
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_routes_wacore_receipt_played_kind() {
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), sender: Some(Jid { user: "9988776655", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0PLAY"], timestamp: 2026-07-11T12:00:00Z, r#type: Played, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt { kind, .. } => {
            assert!(matches!(kind, ReceiptKind::Played));
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_routes_wacore_receipt_delivered_kind() {
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), sender: Some(Jid { user: "9988776655", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0DLV"], timestamp: 2026-07-11T12:00:00Z, r#type: Delivered, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt {
            msg_id, kind, ..
        } => {
            assert_eq!(msg_id, "3EB0DLV");
            assert!(matches!(kind, ReceiptKind::Delivered));
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_routes_wacore_receipt_read_self() {
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 25, integrator: 0 }), sender: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: true, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0SELF"], timestamp: 2026-07-11T12:00:00Z, r#type: ReadSelf, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt { kind, .. } => {
            assert!(matches!(kind, ReceiptKind::Read));
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

// ===========================================================================
// Receipt Debug-format pin tests
//
// The actual Receipt event body in our buffer is
// `Receipt(Receipt { source: ..., message_ids: ["3EB0..."], timestamp: ...,
// r#type: Delivered/Read/Played, offline: false })` — wacore's struct
// Debug format. There is NO `id:` field name; the message ids live
// inside `message_ids:` (Vec<MessageId>, with MessageId = String).
//
// The prior parser searched for `id:` (which would also match the
// `message_ids:` substring if naive) and the
// Jid Debug format `Jid { user, server, agent, device, integrator }`
// has no `id:` either. These tests pin the precise format we accept.
// ===========================================================================

#[test]
fn parser_extracts_msg_id_from_wacore_receipt_message_ids_field() {
    // Format that wacore actually emits for `Event::Receipt(receipt)`
    // Debug. Reproduced from wacore's Receipt struct definition.
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0BC6BF3DF275DC4D29A"], timestamp: 2026-07-11T20:14:00Z, r#type: Delivered, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt { msg_id, peer, kind, .. } => {
            assert_eq!(msg_id, "3EB0BC6BF3DF275DC4D29A", "msg_id must match the message_ids[0]");
            assert_eq!(peer, "5521998201100@s.whatsapp.net", "peer must be the source.chat Jid");
            assert!(matches!(kind, ReceiptKind::Delivered));
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_extracts_msg_id_from_wacore_receipt_read() {
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0READ0001"], timestamp: 2026-07-11T20:14:00Z, r#type: Read, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt { msg_id, kind, .. } => {
            assert_eq!(msg_id, "3EB0READ0001");
            assert!(matches!(kind, ReceiptKind::Read));
        }
        other => panic!("expected Receipt Read, got {other:?}"),
    }
}
