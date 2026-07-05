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
fn peer_to_jid_rejects_group_jid() {
    assert!(matches!(
        peer_to_jid("120363@g.us"),
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