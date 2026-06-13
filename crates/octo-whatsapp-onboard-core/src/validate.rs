//! Shared validation helpers.
//!
//! R1-M2: phone validation was duplicated between the adapter's
//! `is_e164` (in the `validate()` impl) and the core lib's
//! `pair_link::validate_phone`. Move it here so both call sites
//! use the same function. Future bug fixes need only be applied
//! once.

/// E.164 phone validation: `+` followed by 7-15 ASCII digits, no
/// leading 0 after `+`.
///
/// Returns the validation result as a `Result<(), String>` for
/// direct use in `validate()` impls.
pub fn validate_phone_e164(phone: &str) -> Result<(), String> {
    if !phone.starts_with('+') {
        return Err(format!("{phone:?}: missing leading +"));
    }
    let digits = &phone[1..];
    if digits.is_empty() {
        return Err(format!("{phone:?}: no digits after +"));
    }
    if digits.len() < 7 || digits.len() > 15 {
        return Err(format!(
            "{phone:?}: expected 7-15 digits, got {}",
            digits.len()
        ));
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("{phone:?}: non-digit character"));
    }
    if digits.starts_with('0') {
        return Err(format!("{phone:?}: leading 0 after +"));
    }
    Ok(())
}

/// Boolean form of `validate_phone_e164` (used by the adapter's
/// `validate()` which just wants a yes/no).
pub fn is_e164(phone: &str) -> bool {
    validate_phone_e164(phone).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_valid_e164() {
        for good in ["+15551234567", "+1234567", "+123456789012345"] {
            assert!(is_e164(good), "{good:?} should be accepted");
        }
    }

    #[test]
    fn reject_malformed() {
        for bad in [
            "5551234",       // no +
            "+0123456789",   // leading 0
            "+1-555-1234567", // non-digit
            "+",             // no digits
            "+abcdefg",      // non-digit
            "+123456",       // too short (6 digits)
            "+1234567890123456", // too long (16 digits)
        ] {
            assert!(!is_e164(bad), "{bad:?} should be rejected");
        }
    }
}
