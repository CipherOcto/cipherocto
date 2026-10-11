// `ProductionCapabilityVerifier` — the real `CapabilityVerifier` behind the
// RFC-0969 gateway capability pipeline (mission `0969-c`).
//
// This module owns one thing: turning a capability wire string into a verified
// `CapabilityVerification`. It does not decide routing, does not price, and does
// not settle. Everything it needs already existed in the workspace and was
// separately built and tested; the only reason there was no production verifier
// is that nobody had wired the pieces together.
//
// ## What it composes
//
// | Step                                    | Substrate                                        |
// | --------------------------------------- | ------------------------------------------------ |
// | parse wire + resolve holder + verify sig | `octo_wallet::capability::verify_with_resolve`  |
// | read the ask off the macaroon            | `CapabilityToken::ask_binding()`                 |
//
// `verify_with_resolve` performs, in order: the wire parse, the holder-registry
// lookup, the active (not revoked, not expired) check, the holder-signature
// verify against the *registry's* public key, and the RFC-0973 attestation
// dispatch. This module adds nothing to that and does not reorder it — the
// attestation gate is inside `verify_with_resolve`, after the macaroon path has
// cleared, which is the ordering RFC-0973 requires.
//
// ## What it does not depend on
//
// It does not touch `BearerCapsule`. That was recorded as the blocker holding
// the capability verifier back, and the claim is wrong: `BearerCapsule` is the
// RFC-0903 *bearer* path and the `mint_dual` stub, neither of which this
// verifier calls. Every input above is minted and verified by the macaroon
// substrate today.
//
// ## The ask, and why an unbound token is refused
//
// `CapabilityVerification.ask_id` is a bare `[u8; 32]`, and it flows onward into
// `AuthenticatedRequest.ask_id` and settlement. A capability with no
// `Caveat::AskBinding` names no ask, and there is no way to write "names no ask"
// into that field without inventing a sentinel. A zero array is the obvious
// sentinel and it is a security bug: `evaluate_linkage` compares `ask_id` bytes
// directly, so two *different* unbound capabilities held by the same subject
// would compare equal and read as linked to a shared ask that neither of them
// asserted.
//
// So this verifier refuses instead. That is the fail-closed direction, it is
// reversible (accepting unbound capabilities later is a change to this module,
// not a migration of every consumer of `ask_id`), and it cannot make a
// previously-refused request succeed by accident.
//
// The cost is real and is not hidden: **every capability the system can mint
// today is ask-unbound**, because no production mint path attaches an
// `AskBinding` caveat. Until RFC-0959 §Phase 3 lands one, this verifier
// refuses every real token. That is the correct behaviour for a verifier and
// the wrong state for a deployment, and the second is tracked separately.

use std::sync::Arc;

use octo_cap_macaroon::clock::Clock;
use octo_cap_macaroon::holder_registry::HolderRegistry;
use octo_wallet::capability::dispatch::{CapError, CapabilityVerification};
use octo_wallet::capability::verify::{verify_with_resolve, VerifyContext, VerifyError};

use super::authenticator::CapabilityVerifier;

/// The production `CapabilityVerifier` (RFC-0969 §Phase 1).
///
/// Injected into [`super::authenticator::GatewayAuthenticator`] the ordinary
/// way — as an `Arc<dyn CapabilityVerifier>` — so nothing about the
/// authenticator has to know this type exists.
pub struct ProductionCapabilityVerifier {
    ctx: VerifyContext,
}

impl ProductionCapabilityVerifier {
    /// Build from a clock and a holder registry.
    ///
    /// The registry is authoritative on holder identity: `verify_with_resolve`
    /// takes `holder_did` and `holder_pub` from it rather than from the wire,
    /// so a token cannot assert its own holder.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, registry: Arc<dyn HolderRegistry>) -> Self {
        Self {
            ctx: VerifyContext::with_registry(clock, registry),
        }
    }

    /// Build from a fully configured context.
    ///
    /// Use this to carry the RFC-0973 attestation subsystem, which
    /// [`Self::new`] leaves unset. An unset attestation registry is
    /// fail-closed for an attested token and a no-op for an unattested one.
    #[must_use]
    pub fn with_context(ctx: VerifyContext) -> Self {
        Self { ctx }
    }
}

impl std::fmt::Debug for ProductionCapabilityVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProductionCapabilityVerifier")
            .field("ctx", &self.ctx)
            .finish()
    }
}

impl CapabilityVerifier for ProductionCapabilityVerifier {
    /// # Errors
    /// One [`CapError`] per distinct refusal cause. See [`classify`] for the
    /// mapping; the property this function guarantees is that a token is
    /// accepted only after the full `verify_with_resolve` pipeline has cleared
    /// *and* the token has named exactly one ask.
    fn verify(&self, token: &str) -> Result<CapabilityVerification, CapError> {
        let verified = verify_with_resolve(&self.ctx, token).map_err(classify)?;

        // `ask_binding` returns `Result<Option<AskId>, MacaroonError>`. The
        // error arm is the ambiguous case — two or more *distinct* bindings,
        // so the token has no single ask — and is refused rather than resolved
        // to whichever binding happened to come first in the caveat list.
        let ask_id = verified
            .token
            .ask_binding()
            .map_err(|_| CapError::AskBindingAmbiguous)?
            .ok_or(CapError::AskUnbound)?;

        Ok(CapabilityVerification {
            holder_did: verified.token.holder_did.clone(),
            ask_id,
        })
    }
}

/// Map a pipeline failure to its own refusal variant.
///
/// One variant per cause, never a catch-all. Collapsing these would make an
/// infrastructure fault indistinguishable from a forged credential, and
/// `CapError`'s original three variants cannot express any of the causes a real
/// pipeline produces.
///
/// The mapping deliberately carries no message across. `VerifyError`'s variants
/// embed strings that originate in parsing and signature code, and forwarding
/// them into an error the gateway logs is how token bytes reach operator-visible
/// text. The cause is named by the variant; the detail stays where it was
/// produced.
fn classify(e: VerifyError) -> CapError {
    match e {
        VerifyError::Wire(_) => CapError::Malformed,
        VerifyError::Registry(_) => CapError::RegistryUnavailable,
        VerifyError::HolderNotFound => CapError::HolderUnknown,
        VerifyError::HolderNotActive => CapError::HolderNotActive,
        VerifyError::Macaroon(_) => CapError::MacaroonInvalid,
        VerifyError::HolderSig(_) => CapError::InvalidSignature,
        VerifyError::Attestation(_) => CapError::AttestationRefused,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use octo_cap_macaroon::caveat::Caveat;
    use octo_cap_macaroon::holder_kind::HolderKind;
    use octo_cap_macaroon::holder_record::{CapabilityClass, CapabilityTokenLike, HolderRecord};
    use octo_cap_macaroon::holder_registry::RegistryError;
    use octo_cap_macaroon::CapabilityToken;
    use octo_wallet::capability::wire::{compute_cap_root_hash_from_wire, serialize_wire};
    use octo_wallet::IdentityKey;

    use super::*;

    const NOW_MS: u64 = 1_700_000_000_000;
    const ASK: [u8; 32] = [0xA5; 32];
    const OTHER_ASK: [u8; 32] = [0x5A; 32];

    fn clock() -> Arc<dyn Clock> {
        Arc::new(octo_cap_macaroon::clock::FixedClock::new(NOW_MS))
    }

    /// Hermetic registry: a map plus an explicit revoked set, so a test can
    /// reach every branch `verify_with_resolve` distinguishes.
    struct TestRegistry {
        records: Mutex<HashMap<[u8; 32], HolderRecord>>,
        revoked: Mutex<HashMap<[u8; 32], u64>>,
        /// When set, every lookup reports a storage fault. This is what
        /// separates "the credential is bad" from "we could not check".
        broken: bool,
    }

    impl TestRegistry {
        fn empty() -> Self {
            Self {
                records: Mutex::new(HashMap::new()),
                revoked: Mutex::new(HashMap::new()),
                broken: false,
            }
        }

        fn broken() -> Self {
            Self {
                broken: true,
                ..Self::empty()
            }
        }
    }

    impl HolderRegistry for TestRegistry {
        fn lookup(&self, cap_root_hash: &[u8; 32]) -> Result<Option<HolderRecord>, RegistryError> {
            if self.broken {
                return Err(RegistryError::Storage("fixture failure".to_owned()));
            }
            Ok(self.records.lock().unwrap().get(cap_root_hash).cloned())
        }

        fn lookup_by_ask(
            &self,
            ask_id: &[u8; 32],
            _kind: HolderKind,
        ) -> Result<Option<HolderRecord>, RegistryError> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .values()
                .find(|r| r.ask_id.as_ref() == Some(ask_id))
                .cloned())
        }

        fn lookup_active(
            &self,
            cap_root_hash: &[u8; 32],
            _clock: &dyn Clock,
        ) -> Result<Option<HolderRecord>, RegistryError> {
            let rec = self.lookup(cap_root_hash)?;
            if self.revoked.lock().unwrap().contains_key(cap_root_hash) {
                return Ok(None);
            }
            Ok(rec)
        }

        fn insert(&self, record: HolderRecord) -> Result<(), RegistryError> {
            self.records
                .lock()
                .unwrap()
                .insert(record.cap_root_hash, record);
            Ok(())
        }

        fn revoke(&self, cap_root_hash: &[u8; 32], clock: &dyn Clock) -> Result<(), RegistryError> {
            self.revoked
                .lock()
                .unwrap()
                .insert(*cap_root_hash, clock.unix_millis());
            Ok(())
        }

        fn sync_peers(&self) -> Result<(), RegistryError> {
            Ok(())
        }
    }

    /// A minted capability: the wire string plus the material a registry needs
    /// to vouch for it.
    struct Minted {
        wire: String,
        cap_root_hash: [u8; 32],
        holder_pub: [u8; 32],
        holder_did: String,
    }

    /// Mint a real capability with real Ed25519 holder signatures.
    ///
    /// `IdentityKey` is the one production `CapabilitySigner` in the tree, so
    /// this fixture exercises the same signing path production would, not a
    /// stub that returns a fixed signature.
    fn mint(caveats: &[Caveat]) -> Minted {
        let mut holder = IdentityKey::generate().unwrap();
        holder.activate(NOW_MS / 1000).unwrap();
        let holder_did = octo_ident::test_helpers::sample_did(7);

        let token =
            CapabilityToken::mint(&[0x42; 32], &holder, &holder_did, caveats).expect("mint");

        let wire = serialize_wire(&token).expect("serialize");
        let cap_root_hash = compute_cap_root_hash_from_wire(&wire).expect("cap_root_hash");

        Minted {
            wire,
            cap_root_hash,
            holder_pub: holder.public_key_bytes().to_vec().try_into().unwrap(),
            holder_did,
        }
    }

    /// Register a minted capability so it passes the registry checks.
    fn register(reg: &TestRegistry, m: &Minted, ask: Option<[u8; 32]>) {
        let rec = HolderRecord::from_capability(
            &CapabilityTokenLike {
                cap_root_hash: m.cap_root_hash,
                class: CapabilityClass::V1,
            },
            &m.holder_pub,
            &m.holder_did,
            ask,
            NOW_MS + 3_600_000,
        );
        reg.insert(rec).unwrap();
    }

    fn verifier(reg: &TestRegistry) -> ProductionCapabilityVerifier {
        ProductionCapabilityVerifier::new(
            clock(),
            Arc::new(TestRegistry {
                records: Mutex::new(reg.records.lock().unwrap().clone()),
                revoked: Mutex::new(reg.revoked.lock().unwrap().clone()),
                broken: reg.broken,
            }),
        )
    }

    /// The positive path: a real, registered, ask-bound capability verifies and
    /// reports the holder the *registry* named and the ask the token bound.
    #[test]
    fn a_valid_ask_bound_capability_verifies() {
        let m = mint(&[Caveat::AskBinding(ASK)]);
        let reg = TestRegistry::empty();
        register(&reg, &m, Some(ASK));
        let v = verifier(&reg);

        let out = v.verify(&m.wire).expect("valid capability must verify");
        assert_eq!(
            out.ask_id, ASK,
            "the ask the token bound, not a derived one"
        );
        assert_eq!(out.holder_did, m.holder_did);
    }

    /// `a_forged_capability_is_refused_by_the_gateway` — the named AC gate.
    ///
    /// The forgery modelled is a real credential whose holder signature was
    /// made by a key the registry does not vouch for: the record carries a
    /// different public key under the same `cap_root_hash`. This is not a
    /// mutated string — the token is genuine, it simply is not the holder the
    /// registry says it is.
    #[test]
    fn a_forged_capability_is_refused_by_the_gateway() {
        let m = mint(&[Caveat::AskBinding(ASK)]);
        let reg = TestRegistry::empty();

        let impostor = mint(&[Caveat::AskBinding(ASK)]);
        let rec = HolderRecord::from_capability(
            &CapabilityTokenLike {
                cap_root_hash: m.cap_root_hash,
                class: CapabilityClass::V1,
            },
            &impostor.holder_pub,
            &m.holder_did,
            Some(ASK),
            NOW_MS + 3_600_000,
        );
        reg.insert(rec).unwrap();

        let v = verifier(&reg);
        assert!(
            matches!(v.verify(&m.wire), Err(CapError::InvalidSignature)),
            "a signature the registry cannot vouch for must be refused as a signature failure"
        );
    }

    /// An ask-unbound capability is refused, not zero-filled.
    ///
    /// This is the test that pins the refusal. Substituting `[0u8; 32]` here
    /// would keep this test green *and* introduce the bug — so the assertion
    /// is on the specific variant, not merely on `is_err`.
    #[test]
    fn an_ask_unbound_capability_is_refused_not_zero_filled() {
        let m = mint(&[]);
        let reg = TestRegistry::empty();
        register(&reg, &m, None);
        let v = verifier(&reg);

        assert!(
            matches!(v.verify(&m.wire), Err(CapError::AskUnbound)),
            "an unbound capability names no ask; it must be refused, not given a sentinel"
        );
    }

    /// Two *different* ask bindings on one token have no single answer, so the
    /// verifier refuses rather than taking whichever caveat came first.
    #[test]
    fn conflicting_ask_bindings_are_refused() {
        let m = mint(&[Caveat::AskBinding(ASK), Caveat::AskBinding(OTHER_ASK)]);
        let reg = TestRegistry::empty();
        register(&reg, &m, Some(ASK));
        let v = verifier(&reg);

        assert!(
            matches!(v.verify(&m.wire), Err(CapError::AskBindingAmbiguous)),
            "caveat order, not the verifier, must not decide the ask"
        );
    }

    /// Repeated *identical* bindings agree, so they are not ambiguous.
    ///
    /// This is the paired negative: without it, refusing any repeat would
    /// pass the test above for the wrong reason.
    #[test]
    fn repeated_identical_ask_bindings_are_accepted() {
        let m = mint(&[Caveat::AskBinding(ASK), Caveat::AskBinding(ASK)]);
        let reg = TestRegistry::empty();
        register(&reg, &m, Some(ASK));
        let v = verifier(&reg);

        let out = v.verify(&m.wire).expect("identical bindings agree");
        assert_eq!(out.ask_id, ASK);
    }

    /// Nothing vouches for a token whose holder is not registered.
    #[test]
    fn an_unregistered_holder_is_refused() {
        let m = mint(&[Caveat::AskBinding(ASK)]);
        let v = verifier(&TestRegistry::empty());
        assert!(matches!(v.verify(&m.wire), Err(CapError::HolderUnknown)));
    }

    /// A revoked holder must not authenticate.
    #[test]
    fn a_revoked_holder_is_refused() {
        let m = mint(&[Caveat::AskBinding(ASK)]);
        let reg = TestRegistry::empty();
        register(&reg, &m, Some(ASK));
        reg.revoke(&m.cap_root_hash, clock().as_ref()).unwrap();
        let v = verifier(&reg);

        assert!(matches!(v.verify(&m.wire), Err(CapError::HolderNotActive)));
    }

    /// An unreachable registry is an infrastructure fault, reported as one.
    ///
    /// Collapsing this into `MacaroonInvalid` would report a node outage as a
    /// wave of forged credentials.
    #[test]
    fn an_unreachable_registry_is_not_reported_as_a_bad_token() {
        let m = mint(&[Caveat::AskBinding(ASK)]);
        let v = verifier(&TestRegistry::broken());
        assert!(matches!(
            v.verify(&m.wire),
            Err(CapError::RegistryUnavailable)
        ));
    }

    /// Garbage in is refused before anything is looked up.
    #[test]
    fn malformed_input_is_refused_as_malformed() {
        let v = verifier(&TestRegistry::empty());
        assert!(matches!(v.verify("not-a-wire"), Err(CapError::Malformed)));
    }

    /// The false-linkage property, stated as a test.
    ///
    /// Two unbound capabilities for the *same* holder would compare equal on
    /// `ask_id` if absence were encoded as a sentinel, because
    /// `evaluate_linkage` compares those bytes directly. Refusing is what
    /// makes that comparison unreachable: no ask_id is ever produced for a
    /// token that named no ask.
    #[test]
    fn two_unbound_capabilities_never_produce_the_same_ask_id() {
        let first = mint(&[]);
        let second = mint(&[]);
        let reg = TestRegistry::empty();
        register(&reg, &first, None);
        register(&reg, &second, None);
        let v = verifier(&reg);

        assert_eq!(first.holder_did, second.holder_did, "same holder");
        let a = v.verify(&first.wire);
        let b = v.verify(&second.wire);

        // Neither produces an ask_id at all, so no two unbound capabilities
        // can ever compare equal as though they named a shared ask.
        assert!(a.is_err() && b.is_err(), "both must be refused");
        assert!(matches!(a, Err(CapError::AskUnbound)));
        assert!(matches!(b, Err(CapError::AskUnbound)));
    }
}
