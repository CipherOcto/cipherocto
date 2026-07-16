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
fn inbound_message_with_view_once_flag_round_trips() {
    let raw = r#"Message(id: "M1", peer: "X", sender: "Y", text: "", kind: Image, media_token: "tok", view_once: true, ephemeral_expires_at_seconds: 86400, is_group: false)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Message {
            view_once,
            ephemeral_expires_at_seconds,
            kind,
            ..
        } => {
            assert!(view_once);
            assert_eq!(ephemeral_expires_at_seconds, Some(86400));
            assert_eq!(kind, MessageKind::Image);
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn inbound_message_without_flags_round_trips_with_defaults() {
    let raw =
        r#"Message(id: "M1", peer: "X", sender: "Y", text: "hi", kind: Text, is_group: false)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::Message {
            view_once,
            ephemeral_expires_at_seconds,
            ..
        } => {
            assert!(!view_once);
            assert_eq!(ephemeral_expires_at_seconds, None);
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
            msg_id, peer, kind, ..
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
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), sender: Some(Jid { user: "9988776655", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0READ"], timestamp: 2026-07-11T12:00:00Z, type: Read, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt {
            msg_id, peer, kind, ..
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
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), sender: Some(Jid { user: "9988776655", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0PLAY"], timestamp: 2026-07-11T12:00:00Z, type: Played, offline: false })"#;
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
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 0, integrator: 0 }), sender: Some(Jid { user: "9988776655", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0DLV"], timestamp: 2026-07-11T12:00:00Z, type: Delivered, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt { msg_id, kind, .. } => {
            assert_eq!(msg_id, "3EB0DLV");
            assert!(matches!(kind, ReceiptKind::Delivered));
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_routes_wacore_receipt_read_self() {
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 25, integrator: 0 }), sender: Some(Jid { user: "15551234567", server: Pn, agent: 0, device: 25, integrator: 0 }), sender_alt: None, is_from_me: true, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0SELF"], timestamp: 2026-07-11T12:00:00Z, type: ReadSelf, offline: false })"#;
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
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0BC6BF3DF275DC4D29A"], timestamp: 2026-07-11T20:14:00Z, type: Delivered, offline: false })"#;
    let env = EventEnvelope {
        raw: raw.to_string(),
        ts_unix_ms: 1,
        ts_mono_ns: 0,
    };
    let ev = InboundEvent::parse(env);
    match ev {
        InboundEvent::Receipt {
            msg_id, peer, kind, ..
        } => {
            assert_eq!(
                msg_id, "3EB0BC6BF3DF275DC4D29A",
                "msg_id must match the message_ids[0]"
            );
            assert_eq!(
                peer, "5521998201100@s.whatsapp.net",
                "peer must be the source.chat Jid"
            );
            assert!(matches!(kind, ReceiptKind::Delivered));
        }
        other => panic!("expected Receipt, got {other:?}"),
    }
}

#[test]
fn parser_extracts_msg_id_from_wacore_receipt_read() {
    let raw = r#"Receipt(Receipt { source: MessageSource { chat: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "5521998201100", server: Pn, agent: 0, device: 0, integrator: 0 }, sender_alt: None, is_from_me: false, is_group: false, is_broadcast: false, is_status_v3: false, is_newsletter: false }, message_ids: ["3EB0READ0001"], timestamp: 2026-07-11T20:14:00Z, type: Read, offline: false })"#;
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

// --- MessageBatch (group traffic) tests -------------------------------

const SAMPLE_BATCH_TEXT: &str = r#"Messages(MessageBatch { messages: [InboundMessage { message: Message { conversation: Some("hello group"), sender_key_distribution_message: MessageField::Unset, image_message: MessageField::Unset, contact_message: MessageField::Unset, location_message: MessageField::Unset, extended_text_message: MessageField::Unset, document_message: MessageField::Unset, audio_message: MessageField::Unset, video_message: MessageField::Unset, call: MessageField::Unset, chat: MessageField::Unset, protocol_message: MessageField::Unset, contacts_array_message: MessageField::Unset, highly_structured_message: MessageField::Unset, fast_ratchet_key_sender_key_distribution_message: MessageField::Unset, send_payment_message: MessageField::Unset, live_location_message: MessageField::Unset, request_payment_message: MessageField::Unset, decline_payment_request_message: MessageField::Unset, cancel_payment_request_message: MessageField::Unset, template_message: MessageField::Unset, sticker_message: MessageField::Unset, group_invite_message: MessageField::Unset, template_button_reply_message: MessageField::Unset, product_message: MessageField::Unset, device_sent_message: MessageField::Unset, message_context_info: MessageField::Unset, list_message: MessageField::Unset, view_once_message: MessageField::Unset, order_message: MessageField::Unset, list_response_message: MessageField::Unset, ephemeral_message: MessageField::Unset, invoice_message: MessageField::Unset, buttons_message: MessageField::Unset, buttons_response_message: MessageField::Unset, payment_invite_message: MessageField::Unset, interactive_message: MessageField::Unset, reaction_message: MessageField::Unset, sticker_sync_rmr_message: MessageField::Unset, interactive_response_message: MessageField::Unset, poll_creation_message: MessageField::Unset, poll_update_message: MessageField::Unset, keep_in_chat_message: MessageField::Unset, document_with_caption_message: MessageField::Unset, request_phone_number_message: MessageField::Unset, view_once_message_v2: MessageField::Unset, enc_reaction_message: MessageField::Unset, edited_message: MessageField::Unset, view_once_message_v2_extension: MessageField::Unset, poll_creation_message_v2: MessageField::Unset, scheduled_call_creation_message: MessageField::Unset, group_mentioned_message: MessageField::Unset, pin_in_chat_message: MessageField::Unset, poll_creation_message_v3: MessageField::Unset, scheduled_call_edit_message: MessageField::Unset, ptv_message: MessageField::Unset, bot_invoke_message: MessageField::Unset, call_log_messsage: MessageField::Unset, message_history_bundle: MessageField::Unset, enc_comment_message: MessageField::Unset, bcall_message: MessageField::Unset, lottie_sticker_message: MessageField::Unset, event_message: MessageField::Unset, enc_event_response_message: MessageField::Unset, comment_message: MessageField::Unset, newsletter_admin_invite_message: MessageField::Unset, placeholder_message: MessageField::Unset, secret_encrypted_message: MessageField::Unset, album_message: MessageField::Unset, event_cover_image: MessageField::Unset, sticker_pack_message: MessageField::Unset, status_mention_message: MessageField::Unset, poll_result_snapshot_message: MessageField::Unset, poll_creation_option_image_message: MessageField::Unset, associated_child_message: MessageField::Unset, group_status_mention_message: MessageField::Unset, poll_creation_message_v4: MessageField::Unset, status_add_yours: MessageField::Unset, group_status_message: MessageField::Unset, rich_response_message: MessageField::Unset, status_notification_message: MessageField::Unset, limit_sharing_message: MessageField::Unset, bot_task_message: MessageField::Unset, question_message: MessageField::Unset, message_history_notice: MessageField::Unset, group_status_message_v2: MessageField::Unset, bot_forwarded_message: MessageField::Unset, status_question_answer_message: MessageField::Unset, question_reply_message: MessageField::Unset, question_response_message: MessageField::Unset, status_quoted_message: MessageField::Unset, status_sticker_interaction_message: MessageField::Unset, poll_creation_message_v5: MessageField::Unset, newsletter_follower_invite_message_v2: MessageField::Unset, poll_result_snapshot_message_v3: MessageField::Unset, newsletter_admin_profile_message: MessageField::Unset, newsletter_admin_profile_message_v2: MessageField::Unset, spoiler_message: MessageField::Unset, poll_creation_message_v6: MessageField::Unset, conditional_reveal_message: MessageField::Unset, poll_add_option_message: MessageField::Unset, event_invite_message: MessageField::Unset, group_root_key_share: MessageField::Unset, payment_reminder_message: MessageField::Unset, split_payment_message: MessageField::Unset, newsletter_admin_profile_status_message: MessageField::Unset, root_secret_distribute_message: MessageField::Unset }, info: MessageInfo { source: MessageSource { chat: Jid { user: "120363411021224818", server: Group, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "171979281834195", server: Lid, agent: 1, device: 0, integrator: 0 }, is_from_me: false, is_group: true, addressing_mode: Some(Lid), sender_alt: Some(Jid { user: "5524992057890", server: Pn, agent: 0, device: 0, integrator: 0 }), recipient_alt: None, broadcast_list_owner: None, recipient: None }, id: "3A1B00B2FA737D1DC945", server_id: 0, type: "", push_name: "Júlio César", timestamp: 2026-07-12T20:49:20Z, category: Empty, multicast: false, media_type: "", edit: Empty, bot_info: None, meta_info: MsgMetaInfo { target_id: None, target_sender: None, target_chat: None, deprecated_lid_session: None, thread_message_id: None, thread_message_sender_jid: None, content_type: Some("add_on"), appdata: None, reporting_tag: None, reporting_token: None, reporting_token_version: None }, verified_name: None, device_sent_meta: None, ephemeral_expiration: None, is_offline: false, unavailable_request_id: None, server_timestamp_us: Some(1783889361071932), verified_level: None, verified_name_serial: None, peer_recipient_pn: None, comment_target: None, bcl_participants: [] } }], origin: Live })"#;

#[test]
fn message_batch_text_extracts_as_message() {
    let events = InboundEvent::parse_many(env(SAMPLE_BATCH_TEXT), None);
    assert_eq!(events.len(), 1);
    match &events[0] {
        InboundEvent::Message {
            id,
            peer,
            sender,
            kind,
            text,
            is_group,
            from_me,
            ..
        } => {
            assert_eq!(id, "3A1B00B2FA737D1DC945");
            assert_eq!(peer, "120363411021224818");
            assert_eq!(sender, "171979281834195");
            assert!(matches!(kind, MessageKind::Text));
            assert_eq!(text, "hello group");
            assert!(*is_group);
            assert!(!*from_me);
        }
        other => panic!("expected Message, got {other:?}"),
    }
}

#[test]
fn message_batch_video_with_caption_extracts_kind_and_text() {
    let raw = r#"Messages(MessageBatch { messages: [InboundMessage { message: Message { conversation: None, image_message: MessageField::Unset, video_message: MessageField::Set(VideoMessage { url: Some("https://x"), caption: Some("Lei seca arraial"), mimetype: Some("video/mp4"), file_sha256: None, file_length: Some(10), seconds: Some(6), media_key: Some("mk"), gif_playback: None, height: Some(1024), width: Some(576), file_enc_sha256: None, interactive_annotations: [], direct_path: None, media_key_timestamp: None, jpeg_thumbnail: None, context_info: MessageField::Unset, streaming_sidecar: None, streaming_sidecar_timestamp: None, security_token: Some("t"), first_scan_sidecar: Some("fs"), first_scan_length: None, scan_length: None, mid_quality_file_sha256: None, mid_quality_file_length: None, mid_quality_file_enc_sha256: None, video_attribution: None }), document_message: MessageField::Unset, audio_message: MessageField::Unset, contact_message: MessageField::Unset, location_message: MessageField::Unset, extended_text_message: MessageField::Unset, sticker_message: MessageField::Unset }, info: MessageInfo { source: MessageSource { chat: Jid { user: "120363411021224818", server: Group, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "9999", server: Lid, agent: 1, device: 0, integrator: 0 }, is_from_me: false, is_group: true, addressing_mode: Some(Lid), sender_alt: Some(Jid { user: "5511", server: Pn, agent: 0, device: 0, integrator: 0 }), recipient_alt: None, broadcast_list_owner: None, recipient: None }, id: "VID1", server_id: 0, type: "", push_name: "X", timestamp: 2026-07-12T20:49:20Z, category: Empty, multicast: false, media_type: "", edit: Empty, bot_info: None, meta_info: MsgMetaInfo { target_id: None, target_sender: None, target_chat: None, deprecated_lid_session: None, thread_message_id: None, thread_message_sender_jid: None, content_type: None, appdata: None, reporting_tag: None, reporting_token: None, reporting_token_version: None }, verified_name: None, device_sent_meta: None, ephemeral_expiration: None, is_offline: false, unavailable_request_id: None, server_timestamp_us: None, verified_level: None, verified_name_serial: None, peer_recipient_pn: None, comment_target: None, bcl_participants: [] } }], origin: History })"#;
    let events = InboundEvent::parse_many(env(raw), None);
    assert_eq!(events.len(), 1);
    match &events[0] {
        InboundEvent::Message {
            kind,
            text,
            media_token,
            ..
        } => {
            assert!(matches!(kind, MessageKind::Video));
            assert_eq!(text, "Lei seca arraial");
            assert_eq!(media_token.as_deref(), Some("mk"));
        }
        other => panic!("expected Message Video, got {other:?}"),
    }
}

#[test]
fn message_batch_with_multiple_messages_fans_out() {
    let raw = r#"Messages(MessageBatch { messages: [InboundMessage { message: Message { conversation: Some("first"), image_message: MessageField::Unset, video_message: MessageField::Unset, document_message: MessageField::Unset, audio_message: MessageField::Unset, contact_message: MessageField::Unset, location_message: MessageField::Unset, extended_text_message: MessageField::Unset, sticker_message: MessageField::Unset, reaction_message: MessageField::Unset, protocol_message: MessageField::Unset, chat: MessageField::Unset, call: MessageField::Unset }, info: MessageInfo { source: MessageSource { chat: Jid { user: "1", server: Group, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "2", server: Lid, agent: 1, device: 0, integrator: 0 }, is_from_me: false, is_group: true, addressing_mode: Some(Lid), sender_alt: None, recipient_alt: None, broadcast_list_owner: None, recipient: None }, id: "A", server_id: 0, type: "", push_name: "x", timestamp: 2026-07-12T20:49:20Z, category: Empty, multicast: false, media_type: "", edit: Empty, bot_info: None, meta_info: MsgMetaInfo { target_id: None, target_sender: None, target_chat: None, deprecated_lid_session: None, thread_message_id: None, thread_message_sender_jid: None, content_type: None, appdata: None, reporting_tag: None, reporting_token: None, reporting_token_version: None }, verified_name: None, device_sent_meta: None, ephemeral_expiration: None, is_offline: false, unavailable_request_id: None, server_timestamp_us: None, verified_level: None, verified_name_serial: None, peer_recipient_pn: None, comment_target: None, bcl_participants: [] } }, InboundMessage { message: Message { conversation: Some("second"), image_message: MessageField::Unset, video_message: MessageField::Unset, document_message: MessageField::Unset, audio_message: MessageField::Unset, contact_message: MessageField::Unset, location_message: MessageField::Unset, extended_text_message: MessageField::Unset, sticker_message: MessageField::Unset, reaction_message: MessageField::Unset, protocol_message: MessageField::Unset, chat: MessageField::Unset, call: MessageField::Unset }, info: MessageInfo { source: MessageSource { chat: Jid { user: "3", server: Group, agent: 0, device: 0, integrator: 0 }, sender: Jid { user: "4", server: Lid, agent: 1, device: 0, integrator: 0 }, is_from_me: true, is_group: false, addressing_mode: None, sender_alt: None, recipient_alt: None, broadcast_list_owner: None, recipient: None }, id: "B", server_id: 0, type: "", push_name: "y", timestamp: 2026-07-12T20:50:00Z, category: Empty, multicast: false, media_type: "", edit: Empty, bot_info: None, meta_info: MsgMetaInfo { target_id: None, target_sender: None, target_chat: None, deprecated_lid_session: None, thread_message_id: None, thread_message_sender_jid: None, content_type: None, appdata: None, reporting_tag: None, reporting_token: None, reporting_token_version: None }, verified_name: None, device_sent_meta: None, ephemeral_expiration: None, is_offline: false, unavailable_request_id: None, server_timestamp_us: None, verified_level: None, verified_name_serial: None, peer_recipient_pn: None, comment_target: None, bcl_participants: [] } }], origin: Live })"#;
    let events = InboundEvent::parse_many(env(raw), None);
    assert_eq!(events.len(), 2);
    match (&events[0], &events[1]) {
        (
            InboundEvent::Message {
                id: a, text: at, ..
            },
            InboundEvent::Message {
                id: b, text: bt, ..
            },
        ) => {
            assert_eq!(a, "A");
            assert_eq!(at, "first");
            assert_eq!(b, "B");
            assert_eq!(bt, "second");
        }
        _ => panic!("expected 2 Messages"),
    }
}

#[test]
fn parse_iso8601_ms_handles_common_shapes() {
    // 2026-07-12 20:49:20 UTC: 20454 days from 1970-01-01 + 192 day-of-year
    // + 20:49:20 = 1_783_889_360_000 ms.
    assert_eq!(
        parse_iso8601_ms("2026-07-12T20:49:20Z").unwrap(),
        1_783_889_360_000
    );
    assert_eq!(
        parse_iso8601_ms("\"2026-07-12T20:49:20Z\"").unwrap(),
        1_783_889_360_000
    );
    // Sanity: epoch.
    assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z").unwrap(), 0);
    // Leap-year aware.
    assert!(
        parse_iso8601_ms("2024-02-29T00:00:00Z").unwrap()
            > parse_iso8601_ms("2023-03-01T00:00:00Z").unwrap()
    );
    assert!(parse_iso8601_ms("garbage").is_none());
}

// ── Phase 7.E+ T15 — NewsletterUpdate parser ────────────────────────
//
// Adapter bridges wacore's `Event::NewsletterLiveUpdate` into our
// `InboundEvent::NewsletterUpdate` via the raw_event_tx bus with this
// exact string shape. Three hermetic cases cover the happy path, an
// unknown-kind fallback, and a missing-jid fallback.

#[test]
fn newsletter_update_parses_message_received() {
    let raw = r#"NewsletterUpdate(jid: "1234567890@newsletter", kind: MessageReceived)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::NewsletterUpdate { jid, kind, .. } => {
            assert_eq!(jid, "1234567890@newsletter");
            assert_eq!(kind, NewsletterUpdateKind::MessageReceived);
        }
        other => panic!("expected NewsletterUpdate, got {other:?}"),
    }
}

#[test]
fn newsletter_update_unknown_kind_defaults_to_subscribed() {
    // Garbage / future kind falls back to `Subscribed` (conservative).
    let raw = r#"NewsletterUpdate(jid: "X@newsletter", kind: FutureKind)"#;
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::NewsletterUpdate { jid, kind, .. } => {
            assert_eq!(jid, "X@newsletter");
            assert_eq!(kind, NewsletterUpdateKind::Subscribed);
        }
        other => panic!("expected NewsletterUpdate, got {other:?}"),
    }
}

#[test]
fn newsletter_update_missing_jid_yields_empty_string() {
    // Garbage envelope (no fields) — jid falls back to empty string;
    // kind falls back to Subscribed. The parser never errors.
    let raw = "NewsletterUpdate()";
    let ev = InboundEvent::parse(env(raw));
    match ev {
        InboundEvent::NewsletterUpdate { jid, kind, .. } => {
            assert_eq!(jid, "");
            assert_eq!(kind, NewsletterUpdateKind::Subscribed);
        }
        other => panic!("expected NewsletterUpdate, got {other:?}"),
    }
}
