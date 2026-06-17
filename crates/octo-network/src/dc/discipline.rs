//! Slash vs UNBIND discipline for small groups
//! (mission 0855p-c-slash-small-groups).
//!
//! For groups with < 4 members, slash (demote+cooldown) the
//! misbehaving member instead of UNBIND (which would lose the
//! entire group). UNBIND on a 3-member group is overly
//! aggressive (loses 33% of membership on a single slash).
//!
//! ## Threshold
//!
//! - `MIN_GROUP_SIZE_FOR_UNBIND = 4`. Groups with
//!   `member_count < 4` use slash instead of UNBIND.
//!
//! ## Re-strike escalation
//!
//! - 1st slash: `Suspect` + cool-down
//! - 2nd slash: `Demoting` + 2× cool-down
//! - 3rd slash: UNBIND (forced)

use serde::{Deserialize, Serialize};

use crate::mon::bind_envelope::BindEnvelope;

/// Group size below which we slash (not UNBIND) on misbehavior.
pub const MIN_GROUP_SIZE_FOR_UNBIND: u16 = 4;

/// The discipline action to take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisciplineAction {
    /// Slash with cool-down (used for small groups).
    Slash {
        /// Cool-down in epochs: `2^slash_count`.
        cooldown_epochs: u64,
        state: SuspectState,
    },
    /// UNBIND (forced for large groups or 3rd strike).
    Unbind,
}

/// The suspect state escalation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuspectState {
    /// First-time offender.
    Suspect,
    /// Second-time offender.
    Demoting,
}

/// Context for the discipline decision.
#[derive(Clone, Debug)]
pub struct DisciplineContext {
    /// The BIND envelope (provides `member_count_at_bind`).
    pub bind: BindEnvelope,
    /// The current slash count for the member.
    pub current_slash_count: u32,
    /// The current epoch (for cool-down calculation).
    pub current_epoch: u64,
}

/// Decide the discipline action for a member.
pub fn discipline_for(ctx: &DisciplineContext) -> DisciplineAction {
    let member_count = ctx.bind.member_count_at_bind;
    let slash_count = ctx.current_slash_count;

    // Large groups (>= 4): straight UNBIND.
    if member_count >= MIN_GROUP_SIZE_FOR_UNBIND {
        return DisciplineAction::Unbind;
    }

    // Small groups: escalation.
    // 0 prior slashes → 1st slash (Suspect, cool-down = 2^1 = 2)
    // 1 prior slash → 2nd slash (Demoting, cool-down = 2^2 = 4)
    // 2 prior slashes → forced UNBIND (the small-group grace
    // period is exhausted; losing this member is now preferable
    // to retaining an untrustworthy one).
    // 3+ prior slashes → forced UNBIND (3rd strike semantics
    // extend; the cool-down for a 3rd strike is undefined so we
    // escalate to UNBIND to avoid applying a stale cool-down).
    let next_slash_count = slash_count + 1;
    match next_slash_count {
        1 => DisciplineAction::Slash {
            cooldown_epochs: 1u64 << 1, // 2
            state: SuspectState::Suspect,
        },
        2 => DisciplineAction::Slash {
            cooldown_epochs: 1u64 << 2, // 4
            state: SuspectState::Demoting,
        },
        _ => DisciplineAction::Unbind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bind(member_count: u16) -> BindEnvelope {
        let mut b = BindEnvelope::new("d1", "whatsapp", "g1");
        b.member_count_at_bind = member_count;
        b
    }

    #[test]
    fn large_group_first_offender_unbind() {
        let bind = make_bind(4);
        let ctx = DisciplineContext {
            bind,
            current_slash_count: 0,
            current_epoch: 100,
        };
        // Large group: always UNBIND.
        assert_eq!(discipline_for(&ctx), DisciplineAction::Unbind);
    }

    #[test]
    fn large_group_with_priors_unbind() {
        let bind = make_bind(10);
        let ctx = DisciplineContext {
            bind,
            current_slash_count: 5,
            current_epoch: 100,
        };
        assert_eq!(discipline_for(&ctx), DisciplineAction::Unbind);
    }

    #[test]
    fn small_group_first_offender_suspect() {
        let bind = make_bind(3);
        let ctx = DisciplineContext {
            bind,
            current_slash_count: 0,
            current_epoch: 100,
        };
        let action = discipline_for(&ctx);
        assert!(matches!(
            action,
            DisciplineAction::Slash {
                cooldown_epochs: 2,
                state: SuspectState::Suspect
            }
        ));
    }

    #[test]
    fn small_group_second_offender_demoting() {
        let bind = make_bind(2);
        let ctx = DisciplineContext {
            bind,
            current_slash_count: 1,
            current_epoch: 100,
        };
        let action = discipline_for(&ctx);
        assert!(matches!(
            action,
            DisciplineAction::Slash {
                cooldown_epochs: 4,
                state: SuspectState::Demoting
            }
        ));
    }

    #[test]
    fn small_group_third_offender_forced_unbind() {
        let bind = make_bind(3);
        let ctx = DisciplineContext {
            bind,
            current_slash_count: 2,
            current_epoch: 100,
        };
        // 3rd strike (slash_count=2) in a small group escalates
        // to UNBIND, not a slash with a cool-down.
        assert_eq!(discipline_for(&ctx), DisciplineAction::Unbind);
    }

    #[test]
    fn small_group_fourth_offender_also_unbind() {
        // 4th strike (slash_count=3) — there's no defined 4th
        // strike cool-down, so the function must still escalate
        // to UNBIND.
        let bind = make_bind(2);
        let ctx = DisciplineContext {
            bind,
            current_slash_count: 3,
            current_epoch: 100,
        };
        assert_eq!(discipline_for(&ctx), DisciplineAction::Unbind);
    }

    #[test]
    fn threshold_constant() {
        assert_eq!(MIN_GROUP_SIZE_FOR_UNBIND, 4);
    }
}
