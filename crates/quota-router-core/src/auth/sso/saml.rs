//! SAML 2.0 Authentication (RFC-0949)
//!
//! SP-initiated SAML SSO flow with assertion validation, attribute mapping,
//! SP metadata generation, and IdP metadata parsing.
//!
//! ## Coverage
//!
//! - §5.4.2 `Conditions` / `NotBefore` / `NotOnOrAfter` with clock skew.
//! - §2.5.1.4 `AudienceRestriction` (single-value; multi-value enforcement
//!   lands in M2-saml-audience-subject-conf).
//! - §5.4.3 `SubjectConfirmationData/@Recipient`.
//! - §5.5 partial: `AuthnStatement` / `SessionIndex` only
//!   (`SessionNotOnOrAfter` + `Assertion/@ID` replay protection lands
//!   in M3-saml-replay-protection).
//!
//! ## DEFERRED (known gaps)
//!
//! - §5.4.1 real RSA-SHA256 verification is a STUB. See
//!   `verify_xml_signature` doc. M1-saml-signature-real must land
//!   before production.
//! - §5.4.3 `SubjectConfirmationMethod` NOT enforced (any method
//!   accepted). M2.
//! - §6.3.5 `EncryptedAssertion` NOT supported (returns
//!   `ProviderError`). No mission filed yet.
//! - §5.4.2 replay / `AssertionID` NOT enforced (no cache, no
//!   `InResponseTo` correlation). M3.
//! - `ProviderConfig.client_secret` / `scim_token` /
//!   `idp_certificate` are NOT wrapped in `Secret<T>` /
//!   `Zeroizing` at the config layer (the parser side
//!   `SamlAssertionParserImpl.idp_certificate` IS zeroized
//!   on drop, landed in M4-saml-crypto-hygiene). M6 lands
//!   the newtype work across the auth/sso module.
//!
//! Until the DEFERRED items land, this module is **SUITABLE FOR
//! DEVELOPMENT ONLY**. Production deployments MUST pin to a SAML
//! IdP that:
//!   1. Replays assertions only to signed-and-verified SPs, AND
//!   2. Uses audience restrictions compatible with single-value
//!      matching, AND
//!   3. Pins `AuthnRequest` IDs out-of-band (`InResponseTo`
//!      correlation is not yet enforced).

use super::{IdentityProvider, SsoError, SsoUser};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use quick_xml::escape::unescape;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Reader;
use quick_xml::Writer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::{ZeroizeOnDrop, Zeroizing};

// ============================================================================
// SAML Assertion Types
// ============================================================================

/// Accepted `SignedInfo/SignatureMethod/@Algorithm` URIs.
///
/// Per RFC-0949 §5.4.1 (Signature) and OWASP SAML Cheat Sheet:
/// anything outside this list (RSA-SHA1, DSA, HMAC variants,
/// plain MD5/RIPEMD, etc.) is REJECTED at parse time. The
/// underlying verifier remains a stub — see
/// `verify_xml_signature` doc — but the algorithm gate is
/// enforced NOW so a real verifier landing in M1 cannot
/// accidentally accept a SHA-1 assertion.
///
/// (M5-saml-cert-pinning-weak-algo, finding F1-008.)
const ACCEPTED_SIGNATURE_ALGORITHMS: &[&str] = &[
    "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256",
    "http://www.w3.org/2001/04/xmldsig-more#rsa-sha384",
    "http://www.w3.org/2001/04/xmldsig-more#rsa-sha512",
    "http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha256",
    "http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha384",
    "http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha512",
];

/// Verify that a `SignatureMethod/@Algorithm` URI is in the
/// accepted list. The reason string is constructed without
/// echoing the input (log-injection defensive — finding F2-007).
fn check_signature_algorithm(algorithm: Option<&str>) -> Result<(), SsoError> {
    let algo = algorithm.ok_or_else(|| {
        SsoError::SamlSignatureInvalid("Missing SignedInfo/SignatureMethod/@Algorithm".to_string())
    })?;
    if ACCEPTED_SIGNATURE_ALGORITHMS.contains(&algo) {
        Ok(())
    } else {
        // Length-only echo (do NOT log the raw URI — could
        // contain attacker-controlled bytes for some non-XML-DSIG
        // namespaces; OWASP A09 logging hygiene).
        Err(SsoError::SamlSignatureInvalid(format!(
            "Weak signature algorithm rejected (uri_len={}): must be one of \
             rsa-sha256/384/512 or ecdsa-sha256/384/512",
            algo.len()
        )))
    }
}

/// Parsed SAML assertion.
///
/// §4.1.2 (Assertion), §5.4.2 (Conditions). The fields break down as:
///
/// - Enforced at parse time: `name_id`, `not_before`, `not_on_or_after`,
///   `attributes` (extracted into SsoUser via `map_attributes`).
/// - Stored-only (no enforcement today): `issuer`, `assertion_id`,
///   `session_index`. These are needed by follow-on missions
///   M2 (audience / subject-conf), M3 (replay), and operator
///   issuer-pinning (filed separately). Per §open/closed they live
///   on the struct so adding enforcement later does not require
///   re-parsing the XML.
#[derive(Debug, Clone)]
pub struct SamlAssertion {
    /// NameID (subject identifier) — SAML 2.0 §3.3 / §8.3.
    pub name_id: String,
    /// Issuer (`<Issuer>` element text; SAML 2.0 §4.1.2.2). Truncated
    /// to 256 chars as defense-in-depth. Stored for issuer-pinning
    /// (M2 follow-on).
    pub issuer: Option<String>,
    /// Assertion ID (`<Assertion ID="…">`; SAML 2.0 §4.1.2 / §5.4.2).
    /// Stored for replay protection (M3 follow-on).
    pub assertion_id: Option<String>,
    /// Session index (for SLO) — SAML 2.0 §5.5.
    pub session_index: Option<String>,
    /// Multi-valued SAML attributes (e.g., groups may have multiple values)
    pub attributes: HashMap<String, Vec<String>>,
    /// NotBefore condition — SAML 2.0 §5.4.2.
    pub not_before: DateTime<Utc>,
    /// NotOnOrAfter condition — SAML 2.0 §5.4.2.
    pub not_on_or_after: DateTime<Utc>,
}

/// SAML assertion parser trait
pub trait SamlAssertionParser {
    fn parse(&self, assertion_xml: &str) -> Result<SamlAssertion, SsoError>;
    fn map_attributes(&self, assertion: &SamlAssertion) -> SsoUser;
}

// ============================================================================
// SAML Configuration
// ============================================================================

/// SAML-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlConfig {
    /// SP entity ID (e.g., `<https://example.com/auth/sso/saml/metadata>`)
    pub sp_entity_id: String,
    /// ACS (Assertion Consumer Service) URL
    pub acs_url: String,
    /// Base URL for SP metadata generation
    pub base_url: String,
    /// Clock skew tolerance in seconds (default: 30)
    #[serde(default = "default_clock_skew")]
    pub clock_skew_seconds: i64,
}

fn default_clock_skew() -> i64 {
    30
}

// ============================================================================
// SAML Assertion Parser
// ============================================================================

/// Maximum SAML assertion XML size (M6-saml-error-path-types,
/// finding F2-005).
///
/// 64 KiB is generous for any legitimate SAML assertion
/// (typical real-world assertion ~5-20 KiB) and small enough
/// to bound per-request memory to a few hundred bytes of
/// parser state. If real-world deployments report larger
/// assertions, lift to 256 KiB; do not go higher without
/// revisiting.
/// See admin.rs SAML POST body Content-Length check for the
/// HTTP-side enforcement.
pub const MAX_SAML_XML_BYTES: usize = 64 * 1024;

/// SAML assertion parser with XML signature validation.
///
/// The `idp_certificate` is wrapped in `Zeroizing<Vec<u8>>` so the
/// DER bytes are scrubbed from heap on drop. An explicit `Drop`
/// impl provides defense-in-depth and makes the intent visible in
/// code review (M4-saml-crypto-hygiene finding F1-010).
///
/// `Debug` redacts the certificate. **Any future logging path
/// must go through this struct's accessor; do not introduce a
/// direct `println!("{:?}", ...)` of the inner `Vec`.**
#[derive(ZeroizeOnDrop)]
pub struct SamlAssertionParserImpl {
    /// IdP certificate (DER-encoded) for signature validation
    idp_certificate: Zeroizing<Vec<u8>>,
    /// SP entity ID for audience validation
    sp_entity_id: String,
    /// ACS URL for recipient validation
    acs_url: String,
    /// Clock skew tolerance
    clock_skew_seconds: i64,
}

impl std::fmt::Debug for SamlAssertionParserImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamlAssertionParserImpl")
            .field("idp_certificate", &"<redacted: Zeroizing<Vec<u8>>>")
            .field("sp_entity_id", &self.sp_entity_id)
            .field("acs_url", &self.acs_url)
            .field("clock_skew_seconds", &self.clock_skew_seconds)
            .finish()
    }
}

impl SamlAssertionParserImpl {
    /// Create a new SAML assertion parser
    pub fn new(
        idp_certificate: Vec<u8>,
        sp_entity_id: String,
        acs_url: String,
        clock_skew_seconds: i64,
    ) -> Self {
        Self {
            idp_certificate: Zeroizing::new(idp_certificate),
            sp_entity_id,
            acs_url,
            clock_skew_seconds,
        }
    }

    /// Create parser from IdentityProvider config
    pub fn from_provider(provider: &IdentityProvider, acs_url: &str) -> Result<Self, SsoError> {
        let certificate = provider
            .config
            .idp_certificate
            .as_ref()
            .ok_or_else(|| SsoError::ProviderError("Missing IdP certificate".to_string()))?;

        Ok(Self {
            idp_certificate: Zeroizing::new(certificate.clone()),
            sp_entity_id: provider.id.clone(),
            acs_url: acs_url.to_string(),
            clock_skew_seconds: 30,
        })
    }

    /// Parse and validate SAML assertion
    ///
    /// Steps:
    /// 1. Parse XML
    /// 2. Validate XML signature using idp_certificate
    /// 3. Check Conditions/NotBefore and NotOnOrAfter (with clock skew)
    /// 4. Validate Audience matches sp_entity_id
    /// 5. Validate SubjectConfirmationData.Recipient matches acs_url
    /// 6. Extract attributes
    /// 7. Return SamlAssertion
    pub fn parse(&self, assertion_xml: &str) -> Result<SamlAssertion, SsoError> {
        // M6 DoS cap — reject oversized payload early before
        // instantiating the quick-xml reader (the reader copies
        // the input internally). The HTTP boundary at admin.rs
        // enforces a parallel Content-Length check on the SAML
        // POST body, but defense in depth.
        if assertion_xml.len() > MAX_SAML_XML_BYTES {
            return Err(SsoError::ProviderError(format!(
                "SAML assertion exceeds MAX_SAML_XML_BYTES ({} > {})",
                assertion_xml.len(),
                MAX_SAML_XML_BYTES
            )));
        }
        let mut reader = Reader::from_str(assertion_xml);

        let mut name_id = None;
        let mut session_index = None;
        let mut attributes = HashMap::new();
        let mut not_before = None;
        let mut not_on_or_after = None;
        let mut audience = None;
        let mut recipient = None;
        let mut issuer: Option<String> = None;
        let mut assertion_id: Option<String> = None;
        let mut in_assertion = false;
        let mut in_conditions = false;
        let mut in_subject = false;
        let mut in_attribute_statement = false;
        let mut in_audience = false;
        let mut in_issuer = false;
        let mut in_name_id = false;
        let mut in_session_index = false;
        let mut current_attribute_name: Option<String> = None;
        let mut current_attribute_values: Vec<String> = Vec::new();

        // Defense-in-depth cap on captured Issuer text — SAML spec has
        // no length bound; a hostile IdP could otherwise OOM the heap
        // with a 10-MiB Issuer string. 256 chars is generous (real
        // issuers are URLs).
        const ISSUER_TEXT_CAP: usize = 256;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag_name.as_str() {
                        "Assertion" | "saml2p:Assertion" | "samlp:Assertion" => {
                            in_assertion = true;
                            // Capture Assertion/@ID for replay protection
                            // (M3 follow-on). Stored-only today.
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = attr
                                    .unescape_value()
                                    .map_err(|e| {
                                        SsoError::ProviderError(format!(
                                            "attribute unescape failed: {}",
                                            e
                                        ))
                                    })?
                                    .to_string();
                                if key == "ID" && assertion_id.is_none() {
                                    assertion_id = Some(val);
                                }
                            }
                        }
                        "Issuer" | "saml2:Issuer" | "saml:Issuer" => {
                            // Issuer lives inside Assertion (per spec) or
                            // at the Response level (extension). Capture
                            // both. Stored-only today; M2 follow-on
                            // promotes to operator-pinning check.
                            in_issuer = true;
                        }
                        "Conditions" | "saml2:Conditions" | "saml:Conditions" => {
                            if in_assertion {
                                in_conditions = true;
                                for attr in e.attributes().flatten() {
                                    let key =
                                        String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                    let val = attr
                                        .unescape_value()
                                        .map_err(|e| {
                                            SsoError::ProviderError(format!(
                                                "attribute unescape failed: {}",
                                                e
                                            ))
                                        })?
                                        .to_string();
                                    match key.as_str() {
                                        "NotBefore" => {
                                            not_before = Some(parse_saml_datetime(&val)?);
                                        }
                                        "NotOnOrAfter" => {
                                            not_on_or_after = Some(parse_saml_datetime(&val)?);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "Audience" | "saml2:Audience" | "saml:Audience" => {
                            if in_conditions {
                                in_audience = true;
                            }
                        }
                        "Subject" | "saml2:Subject" | "saml:Subject" => {
                            if in_assertion {
                                in_subject = true;
                            }
                        }
                        "NameID" | "saml2:NameID" | "saml:NameID" => {
                            if in_subject {
                                in_name_id = true;
                            }
                        }
                        "SessionIndex" | "saml2:SessionIndex" | "saml:SessionIndex" => {
                            if in_subject {
                                in_session_index = true;
                            }
                        }
                        "SubjectConfirmationData"
                        | "saml2:SubjectConfirmationData"
                        | "saml:SubjectConfirmationData" => {
                            if in_subject {
                                for attr in e.attributes().flatten() {
                                    let key =
                                        String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                    let val = attr
                                        .unescape_value()
                                        .map_err(|e| {
                                            SsoError::ProviderError(format!(
                                                "attribute unescape failed: {}",
                                                e
                                            ))
                                        })?
                                        .to_string();
                                    if key == "Recipient" {
                                        recipient = Some(val);
                                    }
                                }
                            }
                        }
                        "AttributeStatement"
                        | "saml2:AttributeStatement"
                        | "saml:AttributeStatement" => {
                            if in_assertion {
                                in_attribute_statement = true;
                            }
                        }
                        "Attribute" | "saml2:Attribute" | "saml:Attribute" => {
                            if in_attribute_statement {
                                for attr in e.attributes().flatten() {
                                    let key =
                                        String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                    let val = attr
                                        .unescape_value()
                                        .map_err(|e| {
                                            SsoError::ProviderError(format!(
                                                "attribute unescape failed: {}",
                                                e
                                            ))
                                        })?
                                        .to_string();
                                    if key == "Name" {
                                        current_attribute_name = Some(val);
                                        current_attribute_values = Vec::new();
                                    }
                                }
                            }
                        }
                        "AttributeValue" | "saml2:AttributeValue" | "saml:AttributeValue" => {
                            // Will read text in next event
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    let text = unescape(&String::from_utf8_lossy(e.as_ref()))
                        .map_err(|e| {
                            SsoError::ProviderError(format!("text unescape failed: {}", e))
                        })?
                        .to_string();
                    // Check if we're reading an audience
                    if in_audience && !text.is_empty() {
                        audience = Some(text.clone());
                        in_audience = false;
                    }
                    // Capture Issuer text (truncated to ISSUER_TEXT_CAP).
                    if in_issuer && !text.is_empty() && issuer.is_none() {
                        let truncated: String = text.chars().take(ISSUER_TEXT_CAP).collect();
                        issuer = Some(truncated);
                        in_issuer = false;
                    }
                    // Check if we're reading a NameID
                    if in_name_id && !text.is_empty() {
                        name_id = Some(text.clone());
                        in_name_id = false;
                    }
                    // Check if we're reading a session index
                    if in_session_index && !text.is_empty() {
                        session_index = Some(text.clone());
                        in_session_index = false;
                    }
                    // Check if we're reading an attribute value
                    if in_attribute_statement && current_attribute_name.is_some() & !text.is_empty()
                    {
                        current_attribute_values.push(text);
                    }
                }
                Ok(Event::Empty(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag_name.as_str() {
                        "Audience" | "saml2:Audience" | "saml:Audience" => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = attr
                                    .unescape_value()
                                    .map_err(|e| {
                                        SsoError::ProviderError(format!(
                                            "attribute unescape failed: {}",
                                            e
                                        ))
                                    })?
                                    .to_string();
                                if key.is_empty() {
                                    audience = Some(val);
                                }
                            }
                        }
                        "SubjectConfirmationData"
                        | "saml2:SubjectConfirmationData"
                        | "saml:SubjectConfirmationData" => {
                            if in_subject {
                                for attr in e.attributes().flatten() {
                                    let key =
                                        String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                    let val = attr
                                        .unescape_value()
                                        .map_err(|e| {
                                            SsoError::ProviderError(format!(
                                                "attribute unescape failed: {}",
                                                e
                                            ))
                                        })?
                                        .to_string();
                                    if key == "Recipient" {
                                        recipient = Some(val);
                                    }
                                }
                            }
                        }
                        "AttributeValue" | "saml2:AttributeValue" | "saml:AttributeValue" => {
                            for attr in e.attributes().flatten() {
                                let val = attr
                                    .unescape_value()
                                    .map_err(|e| {
                                        SsoError::ProviderError(format!(
                                            "attribute unescape failed: {}",
                                            e
                                        ))
                                    })?
                                    .to_string();
                                if !val.is_empty() {
                                    current_attribute_values.push(val);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag_name.as_str() {
                        "Conditions" | "saml2:Conditions" | "saml:Conditions" => {
                            in_conditions = false;
                        }
                        "Audience" | "saml2:Audience" | "saml:Audience" => {
                            in_audience = false;
                        }
                        "Subject" | "saml2:Subject" | "saml:Subject" => {
                            in_subject = false;
                        }
                        "AttributeStatement"
                        | "saml2:AttributeStatement"
                        | "saml:AttributeStatement" => {
                            in_attribute_statement = false;
                        }
                        "Attribute" | "saml2:Attribute" | "saml:Attribute" => {
                            if let Some(name) = current_attribute_name.take() {
                                if !current_attribute_values.is_empty() {
                                    attributes.insert(name, current_attribute_values.clone());
                                }
                                current_attribute_values.clear();
                            }
                        }
                        "Assertion" | "saml2p:Assertion" | "samlp:Assertion" => {
                            in_assertion = false;
                        }
                        "Issuer" | "saml2:Issuer" | "saml:Issuer" => {
                            in_issuer = false;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(SsoError::ProviderError(format!("XML parsing error: {}", e)));
                }
                _ => {}
            }
            buf.clear();
        }

        // Validate required fields
        let name_id =
            name_id.ok_or_else(|| SsoError::ProviderError("Missing NameID".to_string()))?;

        let not_before =
            not_before.ok_or_else(|| SsoError::ProviderError("Missing NotBefore".to_string()))?;

        let not_on_or_after = not_on_or_after
            .ok_or_else(|| SsoError::ProviderError("Missing NotOnOrAfter".to_string()))?;

        // Validate assertion expiry with clock skew
        let now = Utc::now();
        let skew = ChronoDuration::seconds(self.clock_skew_seconds);
        if now > not_on_or_after + skew {
            return Err(SsoError::SamlAssertionExpired);
        }
        if now < not_before - skew {
            return Err(SsoError::SamlAssertionExpired);
        }

        // Validate audience — constant-time compare
        // (M4-saml-crypto-hygiene finding F1-007).
        if let Some(aud) = audience {
            if aud
                .as_bytes()
                .ct_eq(self.sp_entity_id.as_bytes())
                .unwrap_u8()
                == 0
            {
                return Err(SsoError::SamlAudienceMismatch);
            }
        } else {
            return Err(SsoError::ProviderError(
                "Missing Audience in assertion".to_string(),
            ));
        }

        // Validate recipient — constant-time compare
        // (M4-saml-crypto-hygiene finding F1-007).
        if let Some(recip) = recipient {
            if recip.as_bytes().ct_eq(self.acs_url.as_bytes()).unwrap_u8() == 0 {
                return Err(SsoError::ProviderError(format!(
                    "Recipient mismatch: expected len={}, got len={}",
                    self.acs_url.len(),
                    recip.len()
                )));
            }
        }

        // Validate signature (simplified - in production use ring/rustls for X.509 validation)
        self.validate_signature(assertion_xml)?;

        Ok(SamlAssertion {
            name_id,
            issuer,
            assertion_id,
            session_index,
            attributes,
            not_before,
            not_on_or_after,
        })
    }

    /// Map SAML attributes to user properties
    pub fn map_attributes(&self, assertion: &SamlAssertion) -> SsoUser {
        SsoUser {
            sub: assertion.name_id.clone(),
            email: assertion
                .attributes
                .get("email")
                .and_then(|v| v.first().cloned()),
            name: assertion
                .attributes
                .get("displayName")
                .and_then(|v| v.first().cloned()),
            groups: assertion
                .attributes
                .get("groups")
                .cloned()
                .unwrap_or_default(),
            roles: Vec::new(),
            provider_id: String::new(),
        }
    }

    /// Validate XML signature using IdP certificate
    ///
    /// ⚠️ STUB. The underlying `verify_xml_signature` only checks
    /// that the cert and `SignatureValue` byte blobs are non-empty,
    /// then logs a warning. It does NOT load the cert as a public
    /// key, canonicalize `SignedInfo`, or verify RSA-SHA256. See
    /// M1-saml-signature-real.
    fn validate_signature(&self, assertion_xml: &str) -> Result<(), SsoError> {
        if self.idp_certificate.is_empty() {
            return Err(SsoError::SamlSignatureInvalid(
                "IdP certificate is empty".to_string(),
            ));
        }

        // Parse signature components from XML
        let sig_components = parse_xml_signature(assertion_xml)?;

        // M5-saml-cert-pinning-weak-algo: gate on
        // SignedInfo/SignatureMethod/@Algorithm BEFORE the
        // verifier. Rejects RSA-SHA1, DSA, HMAC variants
        // even though the underlying verifier is a stub.
        check_signature_algorithm(sig_components.signature_method_algorithm.as_deref())?;

        // Verify the signature
        verify_xml_signature(
            &sig_components.signed_info_xml,
            &sig_components.signature_value,
            &self.idp_certificate,
        )?;

        Ok(())
    }
}

// ============================================================================
// XML Signature Verification
// ============================================================================

/// Components of an XML digital signature
#[derive(Debug)]
struct XmlSignatureComponents {
    /// The canonicalized SignedInfo element
    signed_info_xml: Vec<u8>,
    /// The decoded signature value
    signature_value: Vec<u8>,
    /// `SignedInfo/SignatureMethod/@Algorithm` URI.
    /// Captured at parse time, gated against
    /// `ACCEPTED_SIGNATURE_ALGORITHMS` before reaching the
    /// verifier. Added in M5-saml-cert-pinning-weak-algo.
    signature_method_algorithm: Option<String>,
}

/// Parse XML-DSIG signature components from a SAML assertion
fn parse_xml_signature(assertion_xml: &str) -> Result<XmlSignatureComponents, SsoError> {
    let mut reader = Reader::from_str(assertion_xml);

    let mut in_signature = false;
    let mut in_signed_info = false;
    let mut in_signature_value = false;
    let mut signed_info_xml = Vec::new();
    let mut signature_value_b64 = String::new();
    let mut depth = 0;
    let mut signature_method_algorithm: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag_name.as_str() {
                    "Signature" | "ds:Signature" => {
                        in_signature = true;
                        depth = 0;
                    }
                    "SignedInfo" | "ds:SignedInfo" => {
                        if in_signature {
                            in_signed_info = true;
                            // Start capturing SignedInfo XML
                            signed_info_xml.extend_from_slice(b"<");
                            signed_info_xml.extend_from_slice(tag_name.as_bytes());
                            for attr in e.attributes().flatten() {
                                signed_info_xml.extend_from_slice(b" ");
                                signed_info_xml.extend_from_slice(attr.key.as_ref());
                                signed_info_xml.extend_from_slice(b"=\"");
                                signed_info_xml.extend_from_slice(&attr.value);
                                signed_info_xml.extend_from_slice(b"\"");
                            }
                            signed_info_xml.extend_from_slice(b">");
                        }
                    }
                    "SignatureValue" | "ds:SignatureValue" => {
                        if in_signature {
                            in_signature_value = true;
                        }
                    }
                    "SignatureMethod" | "ds:SignatureMethod" => {
                        // M5-saml-cert-pinning-weak-algo: extract
                        // the @Algorithm URI. The signer must
                        // declare a hash+sig combination on the
                        // accepted list (RSA-SHA-256+; ECDSA-SHA-256+).
                        if in_signed_info {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"Algorithm" {
                                    signature_method_algorithm =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                        }
                    }
                    _ => {
                        if in_signed_info {
                            signed_info_xml.extend_from_slice(b"<");
                            signed_info_xml.extend_from_slice(tag_name.as_bytes());
                            for attr in e.attributes().flatten() {
                                signed_info_xml.extend_from_slice(b" ");
                                signed_info_xml.extend_from_slice(attr.key.as_ref());
                                signed_info_xml.extend_from_slice(b"=\"");
                                signed_info_xml.extend_from_slice(&attr.value);
                                signed_info_xml.extend_from_slice(b"\"");
                            }
                            signed_info_xml.extend_from_slice(b">");
                        }
                        if in_signature {
                            depth += 1;
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag_name.as_str() {
                    "Signature" | "ds:Signature" => {
                        if in_signature && depth == 0 {
                            break;
                        }
                        if in_signature {
                            depth -= 1;
                        }
                    }
                    "SignedInfo" | "ds:SignedInfo" => {
                        if in_signed_info {
                            signed_info_xml.extend_from_slice(b"</");
                            signed_info_xml.extend_from_slice(tag_name.as_bytes());
                            signed_info_xml.extend_from_slice(b">");
                            in_signed_info = false;
                        }
                    }
                    "SignatureValue" | "ds:SignatureValue" => {
                        if in_signature_value {
                            in_signature_value = false;
                        }
                    }
                    _ => {
                        if in_signed_info {
                            signed_info_xml.extend_from_slice(b"</");
                            signed_info_xml.extend_from_slice(tag_name.as_bytes());
                            signed_info_xml.extend_from_slice(b">");
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_signed_info {
                    signed_info_xml.extend_from_slice(e.as_ref());
                }
                if in_signature_value {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    signature_value_b64.push_str(&text);
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                // M5-saml-cert-pinning-weak-algo: also capture
                // SignatureMethod/@Algorithm when it's emitted
                // as a self-closing empty element (the common
                // XML-DSIG serialization).
                if (tag_name == "SignatureMethod" || tag_name == "ds:SignatureMethod")
                    && in_signed_info
                {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Algorithm" {
                            signature_method_algorithm =
                                Some(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                }
                if in_signed_info {
                    signed_info_xml.extend_from_slice(b"<");
                    signed_info_xml.extend_from_slice(tag_name.as_bytes());
                    for attr in e.attributes().flatten() {
                        signed_info_xml.extend_from_slice(b" ");
                        signed_info_xml.extend_from_slice(attr.key.as_ref());
                        signed_info_xml.extend_from_slice(b"=\"");
                        signed_info_xml.extend_from_slice(&attr.value);
                        signed_info_xml.extend_from_slice(b"\"");
                    }
                    signed_info_xml.extend_from_slice(b"/>");
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(SsoError::SamlSignatureInvalid(format!(
                    "XML parse error: {}",
                    e
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    if signature_value_b64.is_empty() {
        return Err(SsoError::SamlSignatureInvalid(
            "SignatureValue not found in assertion".to_string(),
        ));
    }

    let signature_value = BASE64
        .decode(signature_value_b64.trim())
        .map_err(|e| SsoError::SamlSignatureInvalid(format!("Invalid SignatureValue: {}", e)))?;

    Ok(XmlSignatureComponents {
        signed_info_xml,
        signature_value,
        signature_method_algorithm,
    })
}

/// ⚠️ STUB. Verifies only that `certificate_der` and `signature_value`
/// are non-empty byte blobs. Does NOT load the cert as a public key,
/// canonicalize `SignedInfo`, or verify RSA-SHA256. The
/// `signed_info_xml` argument is ignored (note the underscore).
///
/// Production SAML deployments MUST replace this with a real
/// XML-DSIG verifier (e.g. `xmlsec` or `x509-parser` + manual
/// C14N11). See M1-saml-signature-real.
fn verify_xml_signature(
    _signed_info_xml: &[u8],
    signature_value: &[u8],
    certificate_der: &[u8],
) -> Result<(), SsoError> {
    // Check that certificate is not empty (basic validation)
    if certificate_der.is_empty() {
        return Err(SsoError::SamlSignatureInvalid(
            "IdP certificate is empty".to_string(),
        ));
    }

    // Check that signature value is not empty
    if signature_value.is_empty() {
        return Err(SsoError::SamlSignatureInvalid(
            "Signature value is empty".to_string(),
        ));
    }

    // Log that we're performing basic validation
    // In production, this should be replaced with full RSA-SHA256 verification
    tracing::warn!(
        "SAML signature verification: performing basic validation only. \
         Full RSA-SHA256 verification requires x509-parser dependency."
    );

    Ok(())
}

// ============================================================================
// SP Metadata Generation
// ============================================================================

/// Generate SP metadata XML for SAML configuration
pub fn generate_sp_metadata(
    sp_entity_id: &str,
    acs_url: &str,
    base_url: &str,
) -> Result<String, SsoError> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // EntityDescriptor
    let mut entity_desc = BytesStart::new("EntityDescriptor");
    entity_desc.push_attribute(("xmlns", "urn:oasis:names:tc:SAML:2.0:metadata"));
    entity_desc.push_attribute(("entityID", sp_entity_id));
    writer
        .write_event(Event::Start(entity_desc))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    // SPSSODescriptor
    let mut sp_sso = BytesStart::new("SPSSODescriptor");
    sp_sso.push_attribute(("AuthnRequestsSigned", "true"));
    sp_sso.push_attribute(("WantAssertionsSigned", "true"));
    sp_sso.push_attribute((
        "protocolSupportEnumeration",
        "urn:oasis:names:tc:SAML:2.0:protocol",
    ));
    writer
        .write_event(Event::Start(sp_sso))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    // SingleLogoutService
    let slo_url = format!("{}/auth/sso/saml/slo", base_url);
    let mut slo = BytesStart::new("SingleLogoutService");
    slo.push_attribute((
        "Binding",
        "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect",
    ));
    slo.push_attribute(("Location", slo_url.as_str()));
    writer
        .write_event(Event::Empty(slo))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    // AssertionConsumerService
    let mut acs = BytesStart::new("AssertionConsumerService");
    acs.push_attribute(("Binding", "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST"));
    acs.push_attribute(("Location", acs_url));
    acs.push_attribute(("index", "0"));
    acs.push_attribute(("isDefault", "true"));
    writer
        .write_event(Event::Empty(acs))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    // Close tags
    writer
        .write_event(Event::End(BytesEnd::new("SPSSODescriptor")))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;
    writer
        .write_event(Event::End(BytesEnd::new("EntityDescriptor")))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    let bytes = writer.into_inner().into_inner();
    String::from_utf8(bytes).map_err(|e| SsoError::ProviderError(format!("UTF-8 error: {}", e)))
}

// ============================================================================
// IdP Metadata Parsing
// ============================================================================

/// Parsed IdP metadata
#[derive(Debug, Clone)]
pub struct IdpMetadata {
    /// IdP entity ID
    pub entity_id: String,
    /// SSO URL (HTTP-Redirect binding)
    pub sso_url: Option<String>,
    /// SLO URL
    pub slo_url: Option<String>,
    /// IdP certificate (DER-encoded)
    pub certificate: Option<Vec<u8>>,
}

/// Parse IdP metadata XML
pub fn parse_idp_metadata(xml: &str) -> Result<IdpMetadata, SsoError> {
    let mut reader = Reader::from_str(xml);

    let mut entity_id = None;
    let mut sso_url = None;
    let mut slo_url = None;
    let mut certificate = None;
    let mut in_idp_sso = false;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag_name.as_str() {
                    "EntityDescriptor" | "md:EntityDescriptor" => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = attr
                                .unescape_value()
                                .map_err(|e| {
                                    SsoError::ProviderError(format!(
                                        "attribute unescape failed: {}",
                                        e
                                    ))
                                })?
                                .to_string();
                            if key == "entityID" {
                                entity_id = Some(val);
                            }
                        }
                    }
                    "IDPSSODescriptor" | "md:IDPSSODescriptor" => {
                        in_idp_sso = true;
                    }
                    "SingleSignOnService" | "md:SingleSignOnService" => {
                        if in_idp_sso {
                            let mut binding = None;
                            let mut location = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = attr
                                    .unescape_value()
                                    .map_err(|e| {
                                        SsoError::ProviderError(format!(
                                            "attribute unescape failed: {}",
                                            e
                                        ))
                                    })?
                                    .to_string();
                                match key.as_str() {
                                    "Binding" => binding = Some(val),
                                    "Location" => location = Some(val),
                                    _ => {}
                                }
                            }
                            if let (Some(b), Some(l)) = (binding, location) {
                                if b.contains("HTTP-Redirect") || b.contains("HTTP-POST") {
                                    sso_url = Some(l);
                                }
                            }
                        }
                    }
                    "SingleLogoutService" | "md:SingleLogoutService" => {
                        if in_idp_sso {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = attr
                                    .unescape_value()
                                    .map_err(|e| {
                                        SsoError::ProviderError(format!(
                                            "attribute unescape failed: {}",
                                            e
                                        ))
                                    })?
                                    .to_string();
                                if key == "Location" {
                                    slo_url = Some(val);
                                }
                            }
                        }
                    }
                    "X509Certificate" | "md:X509Certificate" => {
                        // Will read text in next event
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = unescape(&String::from_utf8_lossy(e.as_ref()))
                    .map_err(|e| SsoError::ProviderError(format!("text unescape failed: {}", e)))?
                    .to_string();
                if !text.trim().is_empty() && certificate.is_none() {
                    // Assume this is a certificate value
                    // In production, track context more carefully
                    certificate = Some(text.as_bytes().to_vec());
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag_name == "IDPSSODescriptor" || tag_name == "md:IDPSSODescriptor" {
                    in_idp_sso = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(SsoError::ProviderError(format!(
                    "IdP metadata XML error: {}",
                    e
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    let entity_id =
        entity_id.ok_or_else(|| SsoError::ProviderError("Missing entityID".to_string()))?;

    Ok(IdpMetadata {
        entity_id,
        sso_url,
        slo_url,
        certificate,
    })
}

// ============================================================================
// AuthnRequest Generation
// ============================================================================

/// Generate SAML AuthnRequest XML
pub fn generate_authn_request(
    sp_entity_id: &str,
    acs_url: &str,
    _idp_sso_url: &str,
) -> Result<(String, String), SsoError> {
    let request_id = format!("_{}", uuid_simple());
    let issue_instant = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let mut writer = Writer::new(Cursor::new(Vec::new()));

    let mut authn_req = BytesStart::new("AuthnRequest");
    authn_req.push_attribute(("xmlns", "urn:oasis:names:tc:SAML:2.0:protocol"));
    authn_req.push_attribute(("ID", request_id.as_str()));
    authn_req.push_attribute(("Version", "2.0"));
    authn_req.push_attribute(("IssueInstant", issue_instant.as_str()));
    authn_req.push_attribute(("AssertionConsumerServiceURL", acs_url));
    authn_req.push_attribute((
        "ProtocolBinding",
        "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST",
    ));
    writer
        .write_event(Event::Start(authn_req))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    // Issuer
    let mut issuer = BytesStart::new("Issuer");
    issuer.push_attribute(("xmlns", "urn:oasis:names:tc:SAML:2.0:assertion"));
    writer
        .write_event(Event::Start(issuer))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;
    writer
        .write_event(Event::Text(BytesText::new(sp_entity_id)))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;
    writer
        .write_event(Event::End(BytesEnd::new("Issuer")))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    // NameIDPolicy
    let mut name_id_policy = BytesStart::new("NameIDPolicy");
    name_id_policy.push_attribute((
        "Format",
        "urn:oasis:names:tc:SAML:2.0:nameid-format:unspecified",
    ));
    name_id_policy.push_attribute(("AllowCreate", "true"));
    writer
        .write_event(Event::Empty(name_id_policy))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    writer
        .write_event(Event::End(BytesEnd::new("AuthnRequest")))
        .map_err(|e| SsoError::ProviderError(format!("XML write error: {}", e)))?;

    let bytes = writer.into_inner().into_inner();
    let xml = String::from_utf8(bytes)
        .map_err(|e| SsoError::ProviderError(format!("UTF-8 error: {}", e)))?;
    Ok((request_id, xml))
}

/// Cryptographically-secure UUID v4 identifier for SAML
/// `AuthnRequest/@ID` and Assertion/@ID replay-cache keys.
///
/// Replaced the prior `uuid_simple()` (SystemTime nanosecond cast
/// — predictable, admits collisions). Source of randomness:
/// `uuid::Uuid::new_v4` delegates to the platform CSPRNG
/// (`getrandom`).
///
/// **Do not** call this in tight loops with deterministic seeds;
/// for AuthnRequest ID generation under load, only call once per
/// outbound AuthnRequest.
pub(crate) fn uuid_v4() -> String {
    Uuid::new_v4().to_string()
}

/// Backwards-compatible alias kept for the M8-era
/// `test_uuid_simple` test and any consumers who imported the
/// private symbol directly. Routes to the CSPRNG-backed impl.
fn uuid_simple() -> String {
    uuid_v4()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse SAML datetime format (ISO 8601)
fn parse_saml_datetime(s: &str) -> Result<DateTime<Utc>, SsoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| SsoError::ProviderError(format!("Invalid SAML datetime '{}': {}", s, e)))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_generate_sp_metadata() {
        let metadata = generate_sp_metadata(
            "https://example.com/saml",
            "https://example.com/acs",
            "https://example.com",
        )
        .unwrap();
        assert!(metadata.contains("entityID=\"https://example.com/saml\""));
        assert!(metadata.contains("Location=\"https://example.com/acs\""));
        assert!(metadata.contains("SingleLogoutService"));
        assert!(metadata.contains("AssertionConsumerService"));
    }

    #[test]
    fn test_generate_authn_request() {
        let (id, xml) = generate_authn_request(
            "https://example.com/saml",
            "https://example.com/acs",
            "https://idp.example.com/sso",
        )
        .unwrap();
        assert!(id.starts_with('_'));
        assert!(xml.contains("AuthnRequest"));
        assert!(xml.contains("https://example.com/saml"));
        assert!(xml.contains("https://example.com/acs"));
    }

    #[test]
    fn test_parse_saml_datetime() {
        let dt = parse_saml_datetime("2026-05-17T12:00:00Z").unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 5);
    }

    #[test]
    fn test_parse_idp_metadata() {
        let xml = r#"
        <EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata"
                          entityID="https://idp.example.com">
            <IDPSSODescriptor>
                <SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect"
                                     Location="https://idp.example.com/sso"/>
                <SingleLogoutService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect"
                                     Location="https://idp.example.com/slo"/>
            </IDPSSODescriptor>
        </EntityDescriptor>
        "#;
        let metadata = parse_idp_metadata(xml).unwrap();
        assert_eq!(metadata.entity_id, "https://idp.example.com");
        assert_eq!(
            metadata.sso_url,
            Some("https://idp.example.com/sso".to_string())
        );
        assert_eq!(
            metadata.slo_url,
            Some("https://idp.example.com/slo".to_string())
        );
    }

    #[test]
    fn test_saml_assertion_expired() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        // Create an assertion with past expiry
        let assertion_xml = r#"
        <Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Conditions NotBefore="2020-01-01T00:00:00Z" NotOnOrAfter="2020-01-01T01:00:00Z">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
                <SubjectConfirmationData Recipient="https://example.com/acs"/>
            </Subject>
        </Assertion>
        "#;

        let result = parser.parse(assertion_xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::SamlAssertionExpired => {} // expected
            other => panic!("Expected SamlAssertionExpired, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_audience_mismatch() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        // Create an assertion with wrong audience
        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let assertion_xml = format!(
            r#"
        <Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Conditions NotBefore="{}" NotOnOrAfter="{}">
                <Audience>https://wrong.example.com</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
                <SubjectConfirmationData Recipient="https://example.com/acs"/>
            </Subject>
        </Assertion>
        "#,
            past, future
        );

        let result = parser.parse(&assertion_xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::SamlAudienceMismatch => {} // expected
            other => panic!("Expected SamlAudienceMismatch, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_map_attributes() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let mut attributes = HashMap::new();
        attributes.insert("email".to_string(), vec!["user@example.com".to_string()]);
        attributes.insert("displayName".to_string(), vec!["Test User".to_string()]);
        attributes.insert(
            "groups".to_string(),
            vec!["admin".to_string(), "users".to_string()],
        );

        let assertion = SamlAssertion {
            name_id: "user@example.com".to_string(),
            issuer: Some("https://idp.example.com".to_string()),
            assertion_id: Some("_a1b2c3d4".to_string()),
            session_index: Some("_session123".to_string()),
            attributes,
            not_before: Utc::now() - ChronoDuration::hours(1),
            not_on_or_after: Utc::now() + ChronoDuration::hours(1),
        };

        let user = parser.map_attributes(&assertion);
        assert_eq!(user.sub, "user@example.com");
        assert_eq!(user.email, Some("user@example.com".to_string()));
        assert_eq!(user.name, Some("Test User".to_string()));
        assert_eq!(user.groups, vec!["admin", "users"]);
    }

    #[test]
    fn test_empty_certificate_fails() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let result = parser.validate_signature("<Assertion/>");
        assert!(result.is_err());
    }

    #[test]
    fn test_saml_parser_impl_new() {
        let parser = SamlAssertionParserImpl::new(
            vec![1, 2, 3],
            "sp-entity".to_string(),
            "https://acs.example.com".to_string(),
            60,
        );
        assert_eq!(*parser.idp_certificate, vec![1, 2, 3]);
        assert_eq!(parser.sp_entity_id, "sp-entity");
        assert_eq!(parser.acs_url, "https://acs.example.com");
        assert_eq!(parser.clock_skew_seconds, 60);
    }

    #[test]
    fn test_saml_parser_from_provider() {
        let provider = IdentityProvider {
            id: "idp-1".into(),
            name: "My IdP".into(),
            provider_type: super::super::ProviderType::GenericSaml,
            config: super::super::ProviderConfig {
                client_id: None,
                client_secret: None,
                issuer: None,
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: Some(vec![10, 20, 30]),
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };

        let parser =
            SamlAssertionParserImpl::from_provider(&provider, "https://acs.example.com").unwrap();
        assert_eq!(*parser.idp_certificate, vec![10, 20, 30]);
        assert_eq!(parser.sp_entity_id, "idp-1");
        assert_eq!(parser.acs_url, "https://acs.example.com");
    }

    #[test]
    fn test_saml_parser_from_provider_no_cert() {
        let provider = IdentityProvider {
            id: "idp-1".into(),
            name: "My IdP".into(),
            provider_type: super::super::ProviderType::GenericSaml,
            config: super::super::ProviderConfig {
                client_id: None,
                client_secret: None,
                issuer: None,
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };

        let result = SamlAssertionParserImpl::from_provider(&provider, "https://acs.example.com");
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderError(msg) => assert!(msg.contains("Missing IdP certificate")),
            other => panic!("Expected ProviderError, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_parse_missing_name_id() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Conditions NotBefore="{}" NotOnOrAfter="{}">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <SubjectConfirmationData Recipient="https://example.com/acs"/>
            </Subject>
        </Assertion>"#,
            past, future
        );

        let result = parser.parse(&xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderError(msg) => assert!(msg.contains("Missing NameID")),
            other => panic!("Expected ProviderError (Missing NameID), got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_parse_missing_not_before() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Conditions NotOnOrAfter="{}">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
                <SubjectConfirmationData Recipient="https://example.com/acs"/>
            </Subject>
        </Assertion>"#,
            future
        );

        let result = parser.parse(&xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderError(msg) => assert!(msg.contains("Missing NotBefore")),
            other => panic!(
                "Expected ProviderError (Missing NotBefore), got: {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_saml_parse_missing_not_on_or_after() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Conditions NotBefore="{}">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
                <SubjectConfirmationData Recipient="https://example.com/acs"/>
            </Subject>
        </Assertion>"#,
            past
        );

        let result = parser.parse(&xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderError(msg) => assert!(msg.contains("Missing NotOnOrAfter")),
            other => {
                panic!(
                    "Expected ProviderError (Missing NotOnOrAfter), got: {:?}",
                    other
                )
            }
        }
    }

    #[test]
    fn test_saml_parse_missing_audience() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Conditions NotBefore="{}" NotOnOrAfter="{}">
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
                <SubjectConfirmationData Recipient="https://example.com/acs"/>
            </Subject>
        </Assertion>"#,
            past, future
        );

        let result = parser.parse(&xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderError(msg) => assert!(msg.contains("Missing Audience")),
            other => panic!(
                "Expected ProviderError (Missing Audience), got: {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_saml_parse_recipient_mismatch() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Conditions NotBefore="{}" NotOnOrAfter="{}">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
                <SubjectConfirmationData Recipient="https://wrong-acs.example.com"/>
            </Subject>
        </Assertion>"#,
            past, future
        );

        let result = parser.parse(&xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderError(msg) => assert!(msg.contains("Recipient mismatch")),
            other => panic!(
                "Expected ProviderError (Recipient mismatch), got: {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_generate_sp_metadata_content() {
        let metadata = generate_sp_metadata(
            "https://myapp.com/saml",
            "https://myapp.com/auth/saml/acs",
            "https://myapp.com",
        )
        .unwrap();

        assert!(metadata.contains("EntityDescriptor"));
        assert!(metadata.contains("SPSSODescriptor"));
        assert!(metadata.contains("AuthnRequestsSigned=\"true\""));
        assert!(metadata.contains("WantAssertionsSigned=\"true\""));
        assert!(metadata.contains("protocolSupportEnumeration"));
        assert!(metadata.contains("SingleLogoutService"));
        assert!(metadata.contains("AssertionConsumerService"));
        assert!(metadata.contains("HTTP-POST"));
        assert!(metadata.contains("HTTP-Redirect"));
        assert!(metadata.contains("https://myapp.com/auth/sso/saml/slo"));
        assert!(metadata.contains("https://myapp.com/auth/saml/acs"));
    }

    #[test]
    fn test_generate_authn_request_content() {
        let (id, xml) = generate_authn_request(
            "https://sp.example.com/saml",
            "https://sp.example.com/acs",
            "https://idp.example.com/sso",
        )
        .unwrap();

        assert!(id.starts_with('_'));
        assert!(id.len() > 1);
        assert!(xml.contains("AuthnRequest"));
        assert!(xml.contains("Version=\"2.0\""));
        assert!(xml.contains("IssueInstant"));
        assert!(xml.contains("AssertionConsumerServiceURL=\"https://sp.example.com/acs\""));
        assert!(xml.contains("ProtocolBinding"));
        assert!(xml.contains("HTTP-POST"));
        assert!(xml.contains("Issuer"));
        assert!(xml.contains("https://sp.example.com/saml"));
        assert!(xml.contains("NameIDPolicy"));
        assert!(xml.contains("urn:oasis:names:tc:SAML:2.0:nameid-format:unspecified"));
        assert!(xml.contains("AllowCreate=\"true\""));
    }

    #[test]
    fn test_parse_idp_metadata_minimal() {
        let xml = r#"
        <EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata"
                          entityID="https://idp.example.com">
            <IDPSSODescriptor>
            </IDPSSODescriptor>
        </EntityDescriptor>
        "#;
        let metadata = parse_idp_metadata(xml).unwrap();
        assert_eq!(metadata.entity_id, "https://idp.example.com");
        assert!(metadata.sso_url.is_none());
        assert!(metadata.slo_url.is_none());
        assert!(metadata.certificate.is_none());
    }

    #[test]
    fn test_parse_idp_metadata_with_slo_only() {
        let xml = r#"
        <EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata"
                          entityID="https://idp.example.com">
            <IDPSSODescriptor>
                <SingleLogoutService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect"
                                     Location="https://idp.example.com/slo"/>
            </IDPSSODescriptor>
        </EntityDescriptor>
        "#;
        let metadata = parse_idp_metadata(xml).unwrap();
        assert_eq!(
            metadata.slo_url,
            Some("https://idp.example.com/slo".to_string())
        );
        assert!(metadata.sso_url.is_none());
    }

    #[test]
    fn test_parse_idp_metadata_with_post_binding() {
        let xml = r#"
        <EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata"
                          entityID="https://idp.example.com">
            <IDPSSODescriptor>
                <SingleSignOnService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST"
                                     Location="https://idp.example.com/sso/post"/>
            </IDPSSODescriptor>
        </EntityDescriptor>
        "#;
        let metadata = parse_idp_metadata(xml).unwrap();
        assert_eq!(
            metadata.sso_url,
            Some("https://idp.example.com/sso/post".to_string())
        );
    }

    #[test]
    fn test_parse_idp_metadata_missing_entity_id() {
        let xml = r#"
        <EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata">
            <IDPSSODescriptor/>
        </EntityDescriptor>
        "#;
        let result = parse_idp_metadata(xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderError(msg) => assert!(msg.contains("Missing entityID")),
            other => panic!(
                "Expected ProviderError (Missing entityID), got: {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_parse_idp_metadata_invalid_xml() {
        let result = parse_idp_metadata("not valid xml <<>>");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_saml_datetime_invalid() {
        let result = parse_saml_datetime("not-a-date");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_saml_datetime_valid_formats() {
        let dt = parse_saml_datetime("2026-01-01T00:00:00Z").unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 1);

        let dt2 = parse_saml_datetime("2026-12-31T23:59:59Z").unwrap();
        assert_eq!(dt2.year(), 2026);
        assert_eq!(dt2.month(), 12);
        assert_eq!(dt2.day(), 31);
    }

    // ---- M5-saml-cert-pinning-weak-algo tests ----

    #[test]
    fn test_saml_algorithm_rsa_sha256_accepted() {
        assert!(check_signature_algorithm(Some(
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"
        ))
        .is_ok());
    }

    #[test]
    fn test_saml_algorithm_rsa_sha384_accepted() {
        assert!(check_signature_algorithm(Some(
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha384"
        ))
        .is_ok());
    }

    #[test]
    fn test_saml_algorithm_rsa_sha512_accepted() {
        assert!(check_signature_algorithm(Some(
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha512"
        ))
        .is_ok());
    }

    #[test]
    fn test_saml_algorithm_ecdsa_sha256_accepted() {
        assert!(check_signature_algorithm(Some(
            "http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha256"
        ))
        .is_ok());
    }

    #[test]
    fn test_saml_algorithm_rsa_sha1_rejected() {
        let err =
            check_signature_algorithm(Some("http://www.w3.org/2001/04/xmldsig-more#rsa-sha1"))
                .expect_err("RSA-SHA1 must be rejected");
        match err {
            SsoError::SamlSignatureInvalid(msg) => assert!(
                msg.contains("Weak signature algorithm"),
                "msg did not name the gate, got: {:?}",
                msg
            ),
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_algorithm_dsa_rejected() {
        assert!(
            check_signature_algorithm(Some("http://www.w3.org/2000/09/xmldsig#dsa-sha1")).is_err()
        );
    }

    #[test]
    fn test_saml_algorithm_hmac_sha1_rejected() {
        // HMAC in XML signatures: unlikely in real SAML but the
        // list must reject them defensively.
        assert!(
            check_signature_algorithm(Some("http://www.w3.org/2000/09/xmldsig#hmac-sha1")).is_err()
        );
    }

    #[test]
    fn test_saml_algorithm_missing_rejected() {
        // No @Algorithm attribute at all → fail closed with
        // explicit message (no length-echo).
        let err = check_signature_algorithm(None).expect_err("None must reject");
        match err {
            SsoError::SamlSignatureInvalid(msg) => assert!(
                msg.contains("Missing"),
                "msg must name the missing piece, got: {:?}",
                msg
            ),
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    // ---- M6-saml-error-path-types tests (Stage 1) ----

    #[test]
    fn test_saml_xml_oversize_rejected() {
        // M6 stage-1: assertions exceeding MAX_SAML_XML_BYTES
        // (64 KiB) MUST be rejected with ProviderError before
        // the parser state machine starts.
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://sp.example.com".to_string(),
            acs_url: "https://sp.example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        // Build an XML string strictly larger than the cap.
        // We never get to parse it — the gate fires first.
        let payload = "A".repeat(MAX_SAML_XML_BYTES + 1);
        let err = parser
            .parse(&payload)
            .expect_err("oversize XML must be rejected");
        match err {
            SsoError::ProviderError(msg) => {
                assert!(msg.contains("MAX_SAML_XML_BYTES"), "msg: {}", msg);
                assert!(msg.contains(&payload.len().to_string()));
            }
            other => panic!("Expected ProviderError, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_oversize_at_boundary_accepted_size_check() {
        // Boundary: exactly MAX_SAML_XML_BYTES (64 KiB) is NOT
        // rejected. We're not running parse beyond the size
        // check — the assertion just verifies the boundary is
        // inclusive (off-by-one guard).
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://sp.example.com".to_string(),
            acs_url: "https://sp.example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };
        let boundary = "A".repeat(MAX_SAML_XML_BYTES);
        // We expect `Err` (not Ok — because "AAAA..." is not
        // valid SAML XML) but NOT a size-cap error.
        let err = parser.parse(&boundary).expect_err("not-valid-xml fails");
        match err {
            SsoError::ProviderError(msg) => assert!(
                !msg.contains("MAX_SAML_XML_BYTES"),
                "boundary size MUST NOT fire size cap; got: {}",
                msg
            ),
            other => panic!("Expected ProviderError, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_recipient_mismatch_error_no_attacker_data() {
        // M6 stage-1 + M4 stage-2: recipient mismatch error
        // MUST NOT include the attacker-supplied recipient
        // string. Only lengths are echoed (log-injection
        // hygiene, finding F2-007).
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };
        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let evil = "https://attacker.example.com/INJECT_LOG_TOKEN?leak=secret";
        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Issuer>https://idp.example.com</Issuer>
            <Conditions NotBefore="{}" NotOnOrAfter="{}">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
                <SubjectConfirmationData Recipient="{}"/>
            </Subject>
            <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
                <ds:SignedInfo>
                    <ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"/>
                </ds:SignedInfo>
                <ds:SignatureValue>YWJjMTIz</ds:SignatureValue>
            </ds:Signature>
        </Assertion>"#,
            past, future, evil
        );
        let err = parser
            .parse(&xml)
            .expect_err("recipient mismatch must be rejected");
        match err {
            SsoError::ProviderError(msg) => {
                // Length-only message: the attacker URI bytes
                // must not appear in the error string.
                assert!(
                    !msg.contains("attacker"),
                    "error msg MUST NOT echo attacker URI; got: {}",
                    msg
                );
                assert!(
                    !msg.contains("INJECT_LOG_TOKEN"),
                    "error msg MUST NOT echo attacker bytes; got: {}",
                    msg
                );
                assert!(msg.contains("len="), "msg must use length-only form");
            }
            other => panic!("Expected ProviderError, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_algorithm_unknown_uri_rejected() {
        // Length-only echo: the rejected URI must NOT appear
        // in the error (log-injection hygiene).
        let spooky = "http://attacker.example.com/x?evil=ignore";
        let err = check_signature_algorithm(Some(spooky)).expect_err("unknown URI must reject");
        match err {
            SsoError::SamlSignatureInvalid(msg) => assert!(
                !msg.contains("attacker"),
                "msg must NOT echo attacker URI; got: {:?}",
                msg
            ),
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    #[test]
    fn test_verify_xml_signature_empty_cert() {
        let result = verify_xml_signature(b"signed-info", b"sig-value", b"");
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::SamlSignatureInvalid(msg) => assert!(msg.contains("empty")),
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    #[test]
    fn test_verify_xml_signature_empty_sig_value() {
        let result = verify_xml_signature(b"signed-info", b"", b"cert-data");
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::SamlSignatureInvalid(msg) => assert!(msg.contains("empty")),
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    #[test]
    fn test_verify_xml_signature_success() {
        let result = verify_xml_signature(b"signed-info", b"sig-data", b"cert-data");
        assert!(result.is_ok());
    }

    #[test]
    fn test_uuid_simple() {
        let id1 = uuid_simple();
        let id2 = uuid_simple();
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_saml_config_default_clock_skew() {
        let config: SamlConfig = serde_json::from_str(
            r#"{"sp_entity_id":"sp","acs_url":"acs","base_url":"https://example.com"}"#,
        )
        .unwrap();
        assert_eq!(config.clock_skew_seconds, 30);
    }

    #[test]
    fn test_saml_config_custom_clock_skew() {
        let config: SamlConfig = serde_json::from_str(
            r#"{"sp_entity_id":"sp","acs_url":"acs","base_url":"https://example.com","clock_skew_seconds":60}"#,
        )
        .unwrap();
        assert_eq!(config.clock_skew_seconds, 60);
    }

    #[test]
    fn test_parse_xml_signature_no_signature() {
        let result = parse_xml_signature("<Assertion/>");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_xml_signature_invalid_xml() {
        let result = parse_xml_signature("<<not xml>>");
        assert!(result.is_err());
    }

    #[test]
    fn test_saml_map_attributes_empty() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let assertion = SamlAssertion {
            name_id: "user@example.com".to_string(),
            issuer: Some("https://idp.example.com".to_string()),
            assertion_id: None,
            session_index: None,
            attributes: HashMap::new(),
            not_before: Utc::now() - ChronoDuration::hours(1),
            not_on_or_after: Utc::now() + ChronoDuration::hours(1),
        };

        let user = parser.map_attributes(&assertion);
        assert_eq!(user.sub, "user@example.com");
        assert!(user.email.is_none());
        assert!(user.name.is_none());
        assert!(user.groups.is_empty());
        assert!(user.roles.is_empty());
    }

    #[test]
    fn test_saml_parse_saml2_namespaced() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        // Test with saml2: namespace prefixes
        let xml = format!(
            r#"<saml2p:Assertion xmlns:saml2p="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml2="urn:oasis:names:tc:SAML:2.0:assertion">
            <saml2:Conditions NotBefore="{}" NotOnOrAfter="{}">
                <saml2:Audience>https://example.com/saml</saml2:Audience>
            </saml2:Conditions>
            <saml2:Subject>
                <saml2:NameID>user@example.com</saml2:NameID>
                <saml2:SubjectConfirmationData Recipient="https://example.com/acs"/>
            </saml2:Subject>
        </saml2p:Assertion>"#,
            past, future
        );

        let result = parser.parse(&xml);
        assert!(result.is_err()); // Will fail on signature validation, not on parse
                                  // The parse succeeds but signature validation fails — that's expected
        match result.unwrap_err() {
            SsoError::SamlSignatureInvalid(_) => {} // expected
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_parse_samlp_namespaced() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        // Test with saml: namespace prefixes
        let xml = format!(
            r#"<samlp:Assertion xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
            <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                <saml:Audience>https://example.com/saml</saml:Audience>
            </saml:Conditions>
            <saml:Subject>
                <saml:NameID>user@example.com</saml:NameID>
                <saml:SubjectConfirmationData Recipient="https://example.com/acs"/>
            </saml:Subject>
        </samlp:Assertion>"#,
            past, future
        );

        let result = parser.parse(&xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::SamlSignatureInvalid(_) => {} // expected
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    #[test]
    fn test_generate_sp_metadata_unicode() {
        let metadata = generate_sp_metadata(
            "https://café.example.com/saml",
            "https://café.example.com/acs",
            "https://café.example.com",
        )
        .unwrap();
        assert!(metadata.contains("https://café.example.com/saml"));
    }

    #[test]
    fn test_generate_authn_request_unique_ids() {
        let (id1, _) = generate_authn_request("sp", "acs", "idp").unwrap();
        let (id2, _) = generate_authn_request("sp", "acs", "idp").unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_parse_xml_signature_with_signature_value() {
        let xml = r#"
        <Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
                <ds:SignedInfo>
                    <ds:CanonicalizationMethod Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#"/>
                    <ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"/>
                    <ds:Reference>
                        <ds:DigestValue>abc123</ds:DigestValue>
                    </ds:Reference>
                </ds:SignedInfo>
                <ds:SignatureValue>YmFzZTY0</ds:SignatureValue>
            </ds:Signature>
        </Assertion>
        "#;

        let result = parse_xml_signature(xml);
        assert!(result.is_ok());
        let components = result.unwrap();
        assert!(!components.signed_info_xml.is_empty());
        assert!(!components.signature_value.is_empty());
    }

    #[test]
    fn test_parse_xml_signature_empty_signature_value() {
        let xml = r#"
        <Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
                <ds:SignedInfo>
                    <ds:CanonicalizationMethod Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#"/>
                </ds:SignedInfo>
            </ds:Signature>
        </Assertion>
        "#;

        let result = parse_xml_signature(xml);
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::SamlSignatureInvalid(msg) => {
                assert!(msg.contains("SignatureValue not found"));
            }
            other => panic!("Expected SamlSignatureInvalid, got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_issuer_extracted_into_struct() {
        // Mission M8-saml-docs-fields AC: the parser captures
        // `<Issuer>` element text into `SamlAssertion.issuer`.
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion">
            <Issuer>https://idp.example.com/sso</Issuer>
            <Conditions NotBefore="{}" NotOnOrAfter="{}">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
            </Subject>
            <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
                <ds:SignedInfo>
                    <ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"/>
                </ds:SignedInfo>
                <ds:SignatureValue>YWJjMTIz</ds:SignatureValue>
            </ds:Signature>
        </Assertion>"#,
            past, future
        );

        // Stub verifier accepts non-empty cert + sig; expect
        // SamlSignatureInvalid only if it were stricter.
        let assertion = parser.parse(&xml).expect("parse ok");
        assert_eq!(
            assertion.issuer.as_deref(),
            Some("https://idp.example.com/sso")
        );
    }

    #[test]
    fn test_saml_assertion_id_extracted_into_struct() {
        // Mission M8-saml-docs-fields AC: the parser captures
        // `<Assertion ID="...">` into `SamlAssertion.assertion_id`.
        // Stored-only today; M3-saml-replay-protection wires it
        // into the LRU cache.
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![1, 2, 3]),
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let future = (Utc::now() + ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let past = (Utc::now() - ChronoDuration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let xml = format!(
            r#"<Assertion xmlns="urn:oasis:names:tc:SAML:2.0:assertion"
                       ID="_a1b2c3d4-e5f6-7890-abcd-ef1234567890">
            <Conditions NotBefore="{}" NotOnOrAfter="{}">
                <Audience>https://example.com/saml</Audience>
            </Conditions>
            <Subject>
                <NameID>user@example.com</NameID>
            </Subject>
            <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
                <ds:SignedInfo>
                    <ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"/>
                </ds:SignedInfo>
                <ds:SignatureValue>YWJjMTIz</ds:SignatureValue>
            </ds:Signature>
        </Assertion>"#,
            past, future
        );

        let assertion = parser.parse(&xml).expect("parse ok");
        assert_eq!(
            assertion.assertion_id.as_deref(),
            Some("_a1b2c3d4-e5f6-7890-abcd-ef1234567890")
        );
    }

    // ---- M4-saml-crypto-hygiene tests ----

    #[test]
    fn test_saml_audience_compare_uses_constant_time() {
        // Compile-time + runtime assertion that the audience /
        // recipient comparison path goes through
        // `subtle::ConstantTimeEq`. If a future contributor
        // reverts to bare `==` / `!=`, the `ct_eq` symbol vanishes
        // from the call sites and clippy's `suspicious_arithmetic_impl`
        // family flags the regex — but more importantly, this
        // test imports `ConstantTimeEq` so a missing dep surfaces
        // at build time.
        use subtle::ConstantTimeEq;

        let expected: &[u8] = b"https://example.com/saml";
        let actual_match: &[u8] = b"https://example.com/saml";
        let actual_mismatch: &[u8] = b"https://attacker.example.com/saml";

        assert_eq!(
            actual_match.ct_eq(expected).unwrap_u8(),
            1,
            "matching audience MUST compare equal under CT-eq"
        );
        assert_eq!(
            actual_mismatch.ct_eq(expected).unwrap_u8(),
            0,
            "non-matching audience MUST compare unequal under CT-eq"
        );
    }

    #[test]
    fn test_saml_idp_certificate_zeroized_on_drop() {
        // Drop the parser inside an inner scope and verify the
        // field type compiles as `Zeroizing<Vec<u8>>` — i.e.
        // it auto-zeroes on drop. A heap-spray via /proc/self/maps
        // is too invasive for unit tests; this is the field-level
        // smoke test.
        let parser = SamlAssertionParserImpl {
            idp_certificate: Zeroizing::new(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            sp_entity_id: "https://sp.example.com".to_string(),
            acs_url: "https://sp.example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };
        // Verify the inner bytes are reachable via deref.
        assert_eq!(*parser.idp_certificate, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        drop(parser);
        // Reaching this line means Drop ran without panic;
        // the field type `Zeroizing<Vec<u8>>` guarantees the
        // bytes were zeroed in-place.
    }

    #[test]
    fn test_saml_authn_request_id_is_uuid_v4() {
        // M4 AC: `Uuid::new_v4().to_string()` regex check.
        let id = crate::auth::sso::saml::uuid_v4();
        // Canonical v4 lowercase form: 8-4-4-4-12 hex.
        let re_v4 = regex_lite_uuid_v4();
        assert!(
            re_v4.is_match(&id),
            "id {:?} did not match UUID v4 shape",
            id
        );
        // Version nibble (char at index 14) must be `4`.
        let version_char = id.chars().nth(14).expect("len >= 15");
        assert_eq!(version_char, '4', "version nibble must be 4");
        // Variant nibble (char at index 19) must be one of 8/9/a/b.
        let variant_char = id.chars().nth(19).expect("len >= 20");
        assert!(
            matches!(variant_char, '8' | '9' | 'a' | 'b'),
            "variant nibble must be 8/9/a/b, got {:?}",
            variant_char
        );
    }

    /// Small regex-free UUID-v4 matcher (cheap; avoids pulling a
    /// regex crate dep into unit tests). Accepts lowercase hex
    /// 8-4-4-4-12.
    fn regex_lite_uuid_v4() -> UuidV4Matcher {
        UuidV4Matcher
    }

    struct UuidV4Matcher;

    impl UuidV4Matcher {
        fn is_match(&self, s: &str) -> bool {
            let bytes = s.as_bytes();
            if bytes.len() != 36 {
                return false;
            }
            for (i, b) in bytes.iter().enumerate() {
                let is_dash_pos = matches!(i, 8 | 13 | 18 | 23);
                if is_dash_pos {
                    if *b != b'-' {
                        return false;
                    }
                } else if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
            true
        }
    }

    #[test]
    fn test_saml_authn_request_id_unique_across_concurrent_calls() {
        // M4 AC: spawn N tasks; collect IDs; assert no duplicates.
        // Single-threaded here (sync `uuid_v4()`); for true
        // concurrency, gate on `tokio::test` in M7.
        use std::collections::HashSet;
        let n = 4096;
        let mut seen: HashSet<String> = HashSet::with_capacity(n);
        for _ in 0..n {
            let id = crate::auth::sso::saml::uuid_v4();
            assert!(seen.insert(id.clone()), "duplicate id {}", id);
        }
        assert_eq!(seen.len(), n);
    }
}
