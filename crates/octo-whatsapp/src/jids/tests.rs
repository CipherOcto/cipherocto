use super::*;

#[test]
fn peer_to_jid_accepts_e164_us() {
    let jid = peer_to_jid("+15551234567").unwrap();
    assert_eq!(jid, "15551234567@s.whatsapp.net");
}

#[test]
fn peer_to_jid_accepts_e164_br() {
    let jid = peer_to_jid("+5511987654321").unwrap();
    assert_eq!(jid, "5511987654321@s.whatsapp.net");
}

#[test]
fn peer_to_jid_accepts_s_whatsapp_net_explicit() {
    let jid = peer_to_jid("15551234567@s.whatsapp.net").unwrap();
    assert_eq!(jid, "15551234567@s.whatsapp.net");
}

#[test]
fn peer_to_jid_accepts_lid() {
    let jid = peer_to_jid("1234567890@lid").unwrap();
    assert_eq!(jid, "1234567890@lid");
}

#[test]
fn peer_to_jid_strips_leading_plus() {
    let jid = peer_to_jid("+447911123456").unwrap();
    assert!(jid.ends_with("@s.whatsapp.net"));
    assert!(!jid.starts_with("+"));
}

#[test]
fn peer_to_jid_rejects_empty() {
    assert_eq!(
        peer_to_jid(""),
        Err(JidError::InvalidPeerFormat(String::new())),
    );
}

#[test]
fn peer_to_jid_accepts_group_jid() {
    let jid = peer_to_jid("120363123456789@g.us").unwrap();
    assert_eq!(jid, "120363123456789@g.us");
}

#[test]
fn peer_to_jid_rejects_short_group_jid() {
    // Group local part must be >= 10 digits per `group_to_jid` rules.
    assert!(matches!(
        peer_to_jid("120363@g.us"),
        Err(JidError::InvalidPeerFormat(_))
    ));
}

#[test]
fn peer_to_jid_rejects_non_digit_group_local() {
    assert!(matches!(
        peer_to_jid("not-a-number@g.us"),
        Err(JidError::InvalidPeerFormat(_))
    ));
}

#[test]
fn peer_to_jid_rejects_arbitrary_at_sign() {
    assert!(matches!(
        peer_to_jid("foo@bar"),
        Err(JidError::InvalidPeerFormat(_))
    ));
}

#[test]
fn peer_to_jid_rejects_short_e164() {
    // 5 digits is below the 7-digit minimum
    assert!(matches!(
        peer_to_jid("+12345"),
        Err(JidError::InvalidPhone(_))
    ));
}

#[test]
fn peer_to_jid_rejects_long_e164() {
    // 17 digits is above the 15-digit E.164 maximum
    assert!(matches!(
        peer_to_jid("+12345678901234567"),
        Err(JidError::InvalidPhone(_))
    ));
}

#[test]
fn peer_to_jid_trims_whitespace() {
    let jid = peer_to_jid("  +15551234567  ").unwrap();
    assert_eq!(jid, "15551234567@s.whatsapp.net");
}

#[test]
fn group_to_jid_trims_whitespace() {
    let jid = group_to_jid("  120363123456789@g.us  ").unwrap();
    assert_eq!(jid, "120363123456789@g.us");
}

#[test]
fn group_to_jid_accepts_canonical() {
    let jid = group_to_jid("120363123456789@g.us").unwrap();
    assert_eq!(jid, "120363123456789@g.us");
}

#[test]
fn group_to_jid_rejects_dm_jid() {
    assert!(matches!(
        group_to_jid("15551234567@s.whatsapp.net"),
        Err(JidError::InvalidGroupFormat(_))
    ));
}

#[test]
fn group_to_jid_rejects_lid() {
    assert!(matches!(
        group_to_jid("1234@lid"),
        Err(JidError::InvalidGroupFormat(_))
    ));
}

#[test]
fn group_to_jid_rejects_bare_digits() {
    assert!(matches!(
        group_to_jid("120363123456789"),
        Err(JidError::InvalidGroupFormat(_))
    ));
}

// ===========================================================================
// apply_self_routing
// ===========================================================================

#[test]
fn apply_self_routing_none_session_returns_peer() {
    let routed = apply_self_routing("552199554474325@s.whatsapp.net", None);
    assert_eq!(routed, "552199554474325@s.whatsapp.net");
}

#[test]
fn apply_self_routing_exact_match_swaps() {
    let routed = apply_self_routing(
        "552199554474325@s.whatsapp.net",
        Some("552199554474325:25@s.whatsapp.net"),
    );
    assert_eq!(routed, "552199554474325:25@s.whatsapp.net");
}

/// Operator's current phone (+552199554474325, 15 digits) vs session
/// pn (5521995544743:25, 13 digits + :25 suffix). Old pn cache, new
/// number. Prefix-match must fire; the swap routes to the
/// device-suffixed session JID. This is the live case from the
/// 2026-07-11 self-image diagnostic.
#[test]
fn apply_self_routing_prefix_match_swaps() {
    let routed = apply_self_routing(
        "552199554474325@s.whatsapp.net",
        Some("5521995544743:25@s.whatsapp.net"),
    );
    assert_eq!(routed, "5521995544743:25@s.whatsapp.net");
}

#[test]
fn apply_self_routing_prefix_match_e164_input() {
    // User types +E164 form (digits only after peer_to_jid strips +).
    let routed = apply_self_routing(
        "552199554474325@s.whatsapp.net",
        Some("552199554474325:7@s.whatsapp.net"),
    );
    assert_eq!(routed, "552199554474325:7@s.whatsapp.net");
}

#[test]
fn apply_self_routing_no_match_for_other_phone() {
    // Different operator entirely — must NOT swap.
    let routed = apply_self_routing(
        "15551234567@s.whatsapp.net",
        Some("5521995544743:25@s.whatsapp.net"),
    );
    assert_eq!(routed, "15551234567@s.whatsapp.net");
}

#[test]
fn apply_self_routing_rejects_overlong_suffix() {
    // Adversarial: peer=55219955447439999 (13 + 5 trailing), self=5521995544743.
    // 5 trailing > 3 limit, must NOT swap.
    let routed = apply_self_routing(
        "55219955447439999@s.whatsapp.net",
        Some("5521995544743:25@s.whatsapp.net"),
    );
    assert_eq!(routed, "55219955447439999@s.whatsapp.net");
}

#[test]
fn apply_self_routing_domain_mismatch_no_swap() {
    // Cross-domain: peer on s.whatsapp.net, self on lid. Don't swap —
    // lid is the long-form identifier and may not be a valid dispatch
    // target for a user-jid peer.
    let routed = apply_self_routing(
        "1234567890@s.whatsapp.net",
        Some("1234567890@lid"),
    );
    assert_eq!(routed, "1234567890@s.whatsapp.net");
}

#[test]
fn apply_self_routing_rejects_empty_self_digits() {
    // Session pn is "@s.whatsapp.net" (no digits) — must NOT swap.
    let routed = apply_self_routing(
        "15551234567@s.whatsapp.net",
        Some("@s.whatsapp.net"),
    );
    assert_eq!(routed, "15551234567@s.whatsapp.net");
}
