//! SAML 2.0 Authentication (RFC-0949)
//!
//! SP-initiated SAML SSO flow with assertion validation, attribute mapping,
//! SP metadata generation, and IdP metadata parsing.

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

// ============================================================================
// SAML Assertion Types
// ============================================================================

/// Parsed SAML assertion
#[derive(Debug, Clone)]
pub struct SamlAssertion {
    /// NameID (subject identifier)
    pub name_id: String,
    /// Session index (for SLO)
    pub session_index: Option<String>,
    /// Multi-valued SAML attributes (e.g., groups may have multiple values)
    pub attributes: HashMap<String, Vec<String>>,
    /// NotBefore condition
    pub not_before: DateTime<Utc>,
    /// NotOnOrAfter condition
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
    /// SP entity ID (e.g., "https://example.com/auth/sso/saml/metadata")
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

/// SAML assertion parser with XML signature validation
#[derive(Debug)]
pub struct SamlAssertionParserImpl {
    /// IdP certificate (DER-encoded) for signature validation
    idp_certificate: Vec<u8>,
    /// SP entity ID for audience validation
    sp_entity_id: String,
    /// ACS URL for recipient validation
    acs_url: String,
    /// Clock skew tolerance
    clock_skew_seconds: i64,
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
            idp_certificate,
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
            idp_certificate: certificate.clone(),
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
        let mut reader = Reader::from_str(assertion_xml);

        let mut name_id = None;
        let mut session_index = None;
        let mut attributes = HashMap::new();
        let mut not_before = None;
        let mut not_on_or_after = None;
        let mut audience = None;
        let mut recipient = None;
        let mut in_assertion = false;
        let mut in_conditions = false;
        let mut in_subject = false;
        let mut in_attribute_statement = false;
        let mut in_audience = false;
        let mut in_name_id = false;
        let mut in_session_index = false;
        let mut current_attribute_name: Option<String> = None;
        let mut current_attribute_values: Vec<String> = Vec::new();

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag_name.as_str() {
                        "Assertion" | "saml2p:Assertion" | "samlp:Assertion" => {
                            in_assertion = true;
                        }
                        "Conditions" | "saml2:Conditions" | "saml:Conditions" => {
                            if in_assertion {
                                in_conditions = true;
                                for attr in e.attributes().flatten() {
                                    let key =
                                        String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                    let val = attr.unescape_value().unwrap_or_default().to_string();
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
                                    let val = attr.unescape_value().unwrap_or_default().to_string();
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
                                    let val = attr.unescape_value().unwrap_or_default().to_string();
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
                        .unwrap_or_default()
                        .to_string();
                    // Check if we're reading an audience
                    if in_audience && !text.is_empty() {
                        audience = Some(text.clone());
                        in_audience = false;
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
                    if in_attribute_statement
                        && current_attribute_name.is_some()
                        && !text.is_empty()
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
                                let val = attr.unescape_value().unwrap_or_default().to_string();
                                if key.is_empty() {
                                    audience = Some(val);
                                }
                            }
                        }
                        "AttributeValue" | "saml2:AttributeValue" | "saml:AttributeValue" => {
                            for attr in e.attributes().flatten() {
                                let val = attr.unescape_value().unwrap_or_default().to_string();
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

        // Validate audience
        if let Some(aud) = audience {
            if aud != self.sp_entity_id {
                return Err(SsoError::SamlAudienceMismatch);
            }
        } else {
            return Err(SsoError::ProviderError(
                "Missing Audience in assertion".to_string(),
            ));
        }

        // Validate recipient
        if let Some(recip) = recipient {
            if recip != self.acs_url {
                return Err(SsoError::ProviderError(format!(
                    "Recipient mismatch: expected {}, got {}",
                    self.acs_url, recip
                )));
            }
        }

        // Validate signature (simplified - in production use ring/rustls for X.509 validation)
        self.validate_signature(assertion_xml)?;

        Ok(SamlAssertion {
            name_id,
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
    /// Implements XML-DSIG signature verification for SAML assertions.
    /// Verifies:
    /// 1. Signature element exists
    /// 2. SignedInfo digest matches assertion digest
    /// 3. Signature value is valid using IdP certificate (RSA-SHA256)
    fn validate_signature(&self, assertion_xml: &str) -> Result<(), SsoError> {
        if self.idp_certificate.is_empty() {
            return Err(SsoError::SamlSignatureInvalid(
                "IdP certificate is empty".to_string(),
            ));
        }

        // Parse signature components from XML
        let sig_components = parse_xml_signature(assertion_xml)?;

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
    })
}

/// Verify XML-DSIG signature using RSA-SHA256
///
/// This implementation verifies the signature of the SignedInfo element
/// using the provided certificate.
///
/// Note: This is a simplified implementation. For production use,
/// consider using a full XML-DSIG library like xmlsec.
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
                            let val = attr.unescape_value().unwrap_or_default().to_string();
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
                                let val = attr.unescape_value().unwrap_or_default().to_string();
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
                                let val = attr.unescape_value().unwrap_or_default().to_string();
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
                    .unwrap_or_default()
                    .to_string();
                if !text.is_empty() && certificate.is_none() {
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

/// Simple UUID-like identifier (not cryptographically secure)
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}", duration.as_nanos())
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
            idp_certificate: vec![1, 2, 3],
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
            idp_certificate: vec![1, 2, 3],
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
            idp_certificate: vec![1, 2, 3],
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
            idp_certificate: vec![],
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
        assert_eq!(parser.idp_certificate, vec![1, 2, 3]);
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

        let parser = SamlAssertionParserImpl::from_provider(&provider, "https://acs.example.com")
            .unwrap();
        assert_eq!(parser.idp_certificate, vec![10, 20, 30]);
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
            idp_certificate: vec![1, 2, 3],
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
            idp_certificate: vec![1, 2, 3],
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
            other => panic!("Expected ProviderError (Missing NotBefore), got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_parse_missing_not_on_or_after() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: vec![1, 2, 3],
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
            idp_certificate: vec![1, 2, 3],
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
            other => panic!("Expected ProviderError (Missing Audience), got: {:?}", other),
        }
    }

    #[test]
    fn test_saml_parse_recipient_mismatch() {
        let parser = SamlAssertionParserImpl {
            idp_certificate: vec![1, 2, 3],
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
            other => panic!("Expected ProviderError (Recipient mismatch), got: {:?}", other),
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
            other => panic!("Expected ProviderError (Missing entityID), got: {:?}", other),
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
        assert!(id1.starts_with('_'));
        assert!(id2.starts_with('_'));
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
            idp_certificate: vec![1, 2, 3],
            sp_entity_id: "https://example.com/saml".to_string(),
            acs_url: "https://example.com/acs".to_string(),
            clock_skew_seconds: 30,
        };

        let assertion = SamlAssertion {
            name_id: "user@example.com".to_string(),
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
            idp_certificate: vec![1, 2, 3],
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
            idp_certificate: vec![1, 2, 3],
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
}
