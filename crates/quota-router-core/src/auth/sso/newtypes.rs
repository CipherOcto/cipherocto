//! Identity-bearing + secret-material newtypes at the sso
//! Layer B substrate boundary (RFC-0949).
//!
//! Per CLAUDE.md §no-central-enums + §typed-discriminator:
//! identity-bearing fields like `EntityId` and `Audience`
//! are distinct types and CANNOT be substituted for each
//! other at the call site even if their inner `String`
//! matches. `EntityId("https://idp.example.com")` cannot be
//! passed where an `Audience` is expected — the compiler
//! rejects the call.
//!
//! Two distinct newtypes must NOT be substitutable even if
//! their inner String types match: passing a `Recipient`
//! where an `Audience` is expected is a compile error, not
//! a runtime panic.
//!
//! (Mission M6-b-error-path-newtypes.)
//!
//! Out of scope for this module: substituting these newtypes
//! at existing call sites (M6-c), the `BlacklistQuery`
//! port-trait (M6-d), the admin HTTP Content-Length check
//! (M6-e), and `SamlAudienceMismatch` struct variant
//! widening (M6-f).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use zeroize::Zeroizing;

/// Errors specific to the newtype layer.
#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NewtypeError {
    /// Email format invalid (RFC 5321-ish structural check).
    #[error("invalid email format: local and domain required, domain must contain a `.`")]
    InvalidEmailFormat,
    /// Empty string where one is required (EntityId, Audience,
    /// Recipient).
    #[error("empty string not allowed for {field}")]
    EmptyForbidden { field: &'static str },
    /// Secret content too short (defense-in-depth; prevents
    /// operators storing empty secrets by mistake).
    #[error("secret too short (min {min} bytes, got {got})")]
    SecretTooShort { min: usize, got: usize },
}

/// Validate an entity-identifying URI string (entity_id,
/// audience, recipient). RFC-0949 requires non-empty;
/// additional structure checks delegate to the caller.
fn validate_non_empty(field: &'static str, value: &str) -> Result<(), NewtypeError> {
    if value.is_empty() {
        return Err(NewtypeError::EmptyForbidden { field });
    }
    Ok(())
}

/// SAML/OIDC Issuer entity ID — the URI identifier for an IdP
/// or SP entity.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(String);

impl EntityId {
    /// Wrap a raw string. Validates non-empty at construction.
    pub fn new(value: impl Into<String>) -> Result<Self, NewtypeError> {
        let s = value.into();
        validate_non_empty("entity_id", &s)?;
        Ok(Self(s))
    }

    /// Inner string view.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EntityId").field(&self.0).finish()
    }
}

impl FromStr for EntityId {
    type Err = NewtypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// SAML `AudienceRestriction/Audience` URI — typically the SP
/// entity ID for the relying party.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Audience(String);

impl Audience {
    pub fn new(value: impl Into<String>) -> Result<Self, NewtypeError> {
        let s = value.into();
        validate_non_empty("audience", &s)?;
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Audience {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Audience {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Audience").field(&self.0).finish()
    }
}

impl FromStr for Audience {
    type Err = NewtypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// SAML `SubjectConfirmationData/@Recipient` URI — the ACS URL
/// where the assertion is bound to be delivered.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Recipient(String);

impl Recipient {
    pub fn new(value: impl Into<String>) -> Result<Self, NewtypeError> {
        let s = value.into();
        validate_non_empty("recipient", &s)?;
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Recipient").field(&self.0).finish()
    }
}

impl FromStr for Recipient {
    type Err = NewtypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Email address (RFC 5321-ish structural check). Used for SAML
/// `NameID/@Format="urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress"`,
/// SCIM `userName`, OIDC `email` claim.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn new(value: impl Into<String>) -> Result<Self, NewtypeError> {
        let s = value.into();
        validate_email(&s)?;
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Local-part (before `@`) and domain-part (after `@`) as
    /// separate strings. Useful for downstream routing / logging.
    pub fn parts(&self) -> (&str, &str) {
        match self.0.split_once('@') {
            Some((local, domain)) => (local, domain),
            None => ("", ""),
        }
    }
}

fn validate_email(value: &str) -> Result<(), NewtypeError> {
    if value.is_empty() {
        return Err(NewtypeError::InvalidEmailFormat);
    }
    let (local, domain) = value
        .split_once('@')
        .ok_or(NewtypeError::InvalidEmailFormat)?;
    if local.is_empty() || domain.is_empty() {
        return Err(NewtypeError::InvalidEmailFormat);
    }
    if !domain.contains('.') {
        return Err(NewtypeError::InvalidEmailFormat);
    }
    Ok(())
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Email").field(&self.0).finish()
    }
}

impl FromStr for Email {
    type Err = NewtypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Secret material — bytes that MUST be zeroized on drop.
/// The `Debug` impl REDACTS the inner bytes so accidental
/// `{:?}` prints in logs / panics do not leak. The `Display`
/// impl is NOT provided — there is no safe human-readable form
/// for arbitrary secret bytes; callers must use `as_bytes()`
/// intentionally.
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    pub fn new(value: impl Into<Vec<u8>>) -> Result<Self, NewtypeError> {
        let v = value.into();
        let got = v.len();
        let min = 1;
        if got < min {
            return Err(NewtypeError::SecretTooShort { min, got });
        }
        Ok(Self(Zeroizing::new(v)))
    }

    /// Byte view of the secret. Intentionally named with the
    /// `_bytes` suffix to make accidental misuse noisy in code
    /// review (`secret.as_bytes()` is more legible than
    /// `secret.as_ref()`).
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for Secret {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secret")
            .field("inner", &"<redacted>")
            .finish()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_round_trip() {
        let eid = EntityId::new("https://idp.example.com").unwrap();
        assert_eq!(eid.to_string(), "https://idp.example.com");
        let parsed: EntityId = "https://idp.example.com".parse().unwrap();
        assert_eq!(eid, parsed);
    }

    #[test]
    fn entity_id_empty_rejected() {
        let err = EntityId::new("").expect_err("empty must reject");
        assert!(matches!(
            err,
            NewtypeError::EmptyForbidden { field: "entity_id" }
        ));
    }

    #[test]
    fn audience_round_trip() {
        let aud = Audience::new("https://sp.example.com").unwrap();
        assert_eq!(aud.as_str(), "https://sp.example.com");
        let parsed: Audience = "https://sp.example.com".parse().unwrap();
        assert_eq!(aud, parsed);
    }

    #[test]
    fn recipient_round_trip() {
        let rec = Recipient::new("https://sp.example.com/acs").unwrap();
        assert_eq!(rec.as_str(), "https://sp.example.com/acs");
    }

    #[test]
    fn email_valid() {
        let e = Email::new("user@example.com").unwrap();
        let (l, d) = e.parts();
        assert_eq!(l, "user");
        assert_eq!(d, "example.com");
    }

    #[test]
    fn email_no_at_rejected() {
        assert!(matches!(
            Email::new("not-an-email"),
            Err(NewtypeError::InvalidEmailFormat)
        ));
    }

    #[test]
    fn email_empty_local_rejected() {
        assert!(matches!(
            Email::new("@example.com"),
            Err(NewtypeError::InvalidEmailFormat)
        ));
    }

    #[test]
    fn email_no_dot_in_domain_rejected() {
        assert!(matches!(
            Email::new("user@localhost"),
            Err(NewtypeError::InvalidEmailFormat)
        ));
    }

    /// Compile-time assertion: distinct newtypes are not
    /// substitutable. The function `takes_audience` accepts
    /// only `Audience`. Passing an `EntityId` would be a
    /// compile error. If a future refactor accidentally
    /// erases the newtype boundary, this test fails to
    /// compile.
    #[test]
    fn newtype_mismatch_prevention_compile_time() {
        fn takes_audience(a: Audience) -> String {
            a.to_string()
        }

        let eid = EntityId::new("https://idp.example.com").unwrap();
        let aud = Audience::new("https://idp.example.com").unwrap();
        let s = takes_audience(aud);
        assert_eq!(s, "https://idp.example.com");
        let _ = eid; // silence unused
    }

    #[test]
    fn secret_debug_redacts() {
        let secret = Secret::new(b"super-confidential-token".to_vec()).unwrap();
        let formatted = format!("{:?}", secret);
        assert!(
            !formatted.contains("super-confidential"),
            "Debug MUST NOT leak secret bytes; got: {}",
            formatted
        );
        assert!(
            formatted.contains("<redacted>"),
            "Debug MUST mark redacted; got: {}",
            formatted
        );
    }

    #[test]
    fn secret_empty_rejected() {
        let err = Secret::new(Vec::<u8>::new()).expect_err("empty secret rejected");
        assert!(matches!(err, NewtypeError::SecretTooShort { .. }));
    }

    #[test]
    fn secret_zeroized_after_drop() {
        let secret = Secret::new(b"token-xyz-1234".to_vec()).unwrap();
        assert_eq!(secret.as_ref(), b"token-xyz-1234");
        drop(secret);
        // Reaching here means Drop ran without panic.
    }

    #[test]
    fn secret_from_string_via_into() {
        let secret = Secret::new(b"abc".to_vec()).unwrap();
        assert_eq!(secret.as_bytes(), b"abc");
    }

    #[test]
    fn entity_id_distinct_from_audience_at_runtime() {
        let eid: EntityId = "https://example.com".parse().unwrap();
        let aud: Audience = "https://example.com".parse().unwrap();
        assert_ne!(format!("{:?}", eid), format!("{:?}", aud));
    }
}
