use super::decisions::{Action, Basis, Decision, Observation, Phase};
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const PROFILE: &str = "smart:latest-plus";
const FACTS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const IMPACT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const NONCE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn observation(facts_revision: &str) -> Observation<'_> {
    Observation {
        facts_revision,
        active_activation: None,
    }
}

fn recommendation(key: &ModKey, enabled: bool) -> Recommendation {
    Recommendation {
        key: key.clone(),
        enabled,
        version: key.default_version(),
    }
}

fn proposal(policy: &Policy, keys: &[ModKey]) -> Result<Decision, PolicyError> {
    Decision::new(
        policy,
        Basis {
            target: "26.3",
            facts_revision: FACTS,
            impact_digest: IMPACT,
            baseline_activation: None,
            nonce: NONCE,
        },
        keys,
    )
}

#[test]
fn inherited_default_off_is_not_an_exclusion() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, false)])?;
    assert_eq!(policy.choice(&key).desired, Desired::NotRequested);
    assert_eq!(policy.choice(&key).source, ChoiceSource::Recommendation);
    assert_eq!(
        policy.choice(&ModKey::modrinth("unknown")?).source,
        ChoiceSource::NotRequested
    );
    let off = policy.apply(
        &policy.revision()?,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Exclude,
        },
    )?;
    assert_eq!(off.choice(&key).desired, Desired::Excluded);
    assert_eq!(off.choice(&key).source, ChoiceSource::ExplicitExclude);
    Ok(())
}

#[test]
fn recommendation_changes_cannot_overwrite_a_local_choice() -> TestResult {
    let key = ModKey::modrinth("example")?;
    for preset_enabled in [false, true] {
        for choice in [Inclusion::Include, Inclusion::Exclude] {
            let policy = Policy::new(PROFILE, vec![recommendation(&key, preset_enabled)])?;
            let selected = policy.apply(
                &policy.revision()?,
                Change::SetInclusion {
                    key: key.clone(),
                    inclusion: choice,
                },
            )?;
            let updated = selected.apply(
                &selected.revision()?,
                Change::ReplaceRecommendations(vec![recommendation(&key, !preset_enabled)]),
            )?;
            assert_eq!(updated.choice(&key), selected.choice(&key));
            let inherited = updated.apply(
                &updated.revision()?,
                Change::FollowRecommendations { key: key.clone() },
            )?;
            assert_eq!(
                matches!(inherited.choice(&key).desired, Desired::Required(_)),
                !preset_enabled
            );
        }
    }
    Ok(())
}

#[test]
fn exclusion_can_be_reversed_without_an_installed_file() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let excluded = policy.apply(
        &policy.revision()?,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Exclude,
        },
    )?;
    let reenabled = excluded.apply(
        &excluded.revision()?,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Include,
        },
    )?;
    assert_eq!(
        reenabled.choice(&key).desired,
        Desired::Required(VersionRule::Compatible)
    );
    assert_eq!(reenabled.choice(&key).source, ChoiceSource::ExplicitInclude);
    assert_eq!(policy.sequence, 0);
    Ok(())
}

#[test]
fn toggling_inclusion_preserves_a_pin_and_follow_explicitly_clears_it() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let mut policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let pin = VersionRule::PinnedRelease("opaque-release".into());
    policy = policy.apply(
        &policy.revision()?,
        Change::SetVersion {
            key: key.clone(),
            version: pin.clone(),
        },
    )?;
    for inclusion in [Inclusion::Exclude, Inclusion::Include, Inclusion::Inherit] {
        policy = policy.apply(
            &policy.revision()?,
            Change::SetInclusion {
                key: key.clone(),
                inclusion,
            },
        )?;
        assert!(policy.choice(&key).version_overridden);
    }
    assert_eq!(policy.choice(&key).desired, Desired::Required(pin));
    policy = policy.apply(
        &policy.revision()?,
        Change::FollowRecommendations { key: key.clone() },
    )?;
    assert!(!policy.choice(&key).version_overridden);
    assert_eq!(
        policy.choice(&key).desired,
        Desired::Required(VersionRule::Compatible)
    );
    Ok(())
}

#[test]
fn stale_cross_profile_and_aba_actions_are_rejected_without_mutation() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let initial = policy.revision()?;
    let excluded = policy.apply(
        &initial,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Exclude,
        },
    )?;
    let restored = excluded.apply(
        &excluded.revision()?,
        Change::FollowRecommendations { key: key.clone() },
    )?;
    assert_eq!(restored.choices(), policy.choices());
    assert_eq!(
        restored.apply(
            &initial,
            Change::SetInclusion {
                key: key.clone(),
                inclusion: Inclusion::Exclude
            }
        ),
        Err(PolicyError::StaleRevision)
    );
    let other = Policy::new("smart:snapshot-plus", vec![recommendation(&key, true)])?;
    assert_eq!(
        policy.apply(&other.revision()?, Change::FollowRecommendations { key }),
        Err(PolicyError::StaleRevision)
    );
    assert_eq!(policy.revision()?, initial);
    Ok(())
}

#[test]
fn identity_and_version_rules_reject_ambiguous_or_unsafe_input() -> TestResult {
    assert!(ModKey::modrinth("../outside").is_err());
    assert!(ModKey::local("not-a-content-id").is_err());
    let key = ModKey::local(FACTS)?;
    let policy = Policy::new(PROFILE, Vec::new())?;
    assert_eq!(
        policy.apply(
            &policy.revision()?,
            Change::SetVersion {
                key: key.clone(),
                version: VersionRule::Compatible
            }
        ),
        Err(PolicyError::InvalidVersion)
    );
    let imported = policy.apply(
        &policy.revision()?,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Include,
        },
    )?;
    assert_eq!(
        imported.choice(&key).desired,
        Desired::Required(VersionRule::ExactObject)
    );
    let duplicate = vec![recommendation(&key, true), recommendation(&key, false)];
    assert_eq!(
        Policy::new(PROFILE, duplicate),
        Err(PolicyError::DuplicateIdentity)
    );
    Ok(())
}

#[test]
fn ordering_is_canonical_and_noop_edits_do_not_create_revisions() -> TestResult {
    let first = ModKey::modrinth("first")?;
    let second = ModKey::modrinth("second")?;
    let a = Policy::new(
        PROFILE,
        vec![recommendation(&first, true), recommendation(&second, false)],
    )?;
    let b = Policy::new(
        PROFILE,
        vec![recommendation(&second, false), recommendation(&first, true)],
    )?;
    assert_eq!(a.revision()?, b.revision()?);
    let noop = a.apply(&a.revision()?, Change::FollowRecommendations { key: first })?;
    assert_eq!(a.revision()?, noop.revision()?);
    Ok(())
}

#[test]
fn permission_is_exactly_scoped_and_never_rewrites_policy() -> TestResult {
    let first = ModKey::modrinth("first")?;
    let second = ModKey::modrinth("second")?;
    let policy = Policy::new(
        PROFILE,
        vec![recommendation(&first, true), recommendation(&second, true)],
    )?;
    let original = policy.revision()?;
    let pending = proposal(&policy, std::slice::from_ref(&first))?;
    assert!(!pending.covers_omissions(&policy, "26.3", IMPACT, std::slice::from_ref(&first)));
    let accepted = pending.answer(
        &pending.revision()?,
        &policy,
        observation(FACTS),
        Action::Accept,
    )?;
    assert!(accepted.covers_omissions(&policy, "26.3", IMPACT, std::slice::from_ref(&first)));
    assert!(!accepted.covers_omissions(&policy, "26.4", IMPACT, std::slice::from_ref(&first)));
    assert!(!accepted.covers_omissions(&policy, "26.3", FACTS, std::slice::from_ref(&first)));
    assert!(!accepted.covers_omissions(&policy, "26.3", IMPACT, &[first.clone(), second]));
    let other = Policy::new("smart:snapshot-plus", vec![recommendation(&first, true)])?;
    assert!(!accepted.covers_omissions(&other, "26.3", IMPACT, std::slice::from_ref(&first)));
    assert_eq!(policy.revision()?, original);
    Ok(())
}

#[test]
fn stale_facts_policy_and_duplicate_replies_do_not_authorize_new_work() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let pending = proposal(&policy, std::slice::from_ref(&key))?;
    assert_eq!(
        pending.answer(
            &pending.revision()?,
            &policy,
            observation(NONCE),
            Action::Accept
        ),
        Err(PolicyError::StaleRevision)
    );
    let changed = policy.apply(
        &policy.revision()?,
        Change::SetVersion {
            key,
            version: VersionRule::PinnedRelease("fixed".into()),
        },
    )?;
    assert_eq!(
        pending.answer(
            &pending.revision()?,
            &changed,
            observation(FACTS),
            Action::Accept
        ),
        Err(PolicyError::StaleRevision)
    );
    let accepted = pending.answer(
        &pending.revision()?,
        &policy,
        observation(FACTS),
        Action::Accept,
    )?;
    assert_eq!(
        accepted.answer(
            &pending.revision()?,
            &policy,
            observation(FACTS),
            Action::Accept
        ),
        Err(PolicyError::StaleRevision)
    );
    Ok(())
}

#[test]
fn deferred_questions_stay_quiet_and_revoked_answers_cannot_be_reaccepted() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let pending = proposal(&policy, std::slice::from_ref(&key))?;
    assert!(pending.needs_attention());
    let deferred = pending.answer(
        &pending.revision()?,
        &policy,
        observation(FACTS),
        Action::Defer,
    )?;
    assert!(!deferred.needs_attention());
    assert_eq!(
        deferred,
        deferred.answer(
            &deferred.revision()?,
            &policy,
            observation(FACTS),
            Action::Defer
        )?
    );
    let accepted = deferred.answer(
        &deferred.revision()?,
        &policy,
        observation(FACTS),
        Action::Accept,
    )?;
    let revoked = accepted.answer(
        &accepted.revision()?,
        &policy,
        observation(NONCE),
        Action::Revoke,
    )?;
    assert_eq!(revoked.phase(), Phase::Revoked);
    assert!(!revoked.covers_omissions(&policy, "26.3", IMPACT, std::slice::from_ref(&key)));
    assert_eq!(
        revoked.answer(
            &accepted.revision()?,
            &policy,
            observation(FACTS),
            Action::Accept
        ),
        Err(PolicyError::StaleRevision)
    );
    assert_eq!(
        revoked.answer(
            &revoked.revision()?,
            &policy,
            observation(FACTS),
            Action::Accept
        ),
        Err(PolicyError::InvalidDecision)
    );
    Ok(())
}

#[test]
fn changed_pins_and_exclusions_invalidate_an_existing_permission() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let pending = proposal(&policy, std::slice::from_ref(&key))?;
    let accepted = pending.answer(
        &pending.revision()?,
        &policy,
        observation(FACTS),
        Action::Accept,
    )?;
    let changed = policy.apply(
        &policy.revision()?,
        Change::SetVersion {
            key: key.clone(),
            version: VersionRule::PinnedRelease("new-pin".into()),
        },
    )?;
    assert!(!accepted.covers_omissions(&changed, "26.3", IMPACT, std::slice::from_ref(&key)));
    let excluded = policy.apply(
        &policy.revision()?,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Exclude,
        },
    )?;
    assert!(!accepted.covers_omissions(&excluded, "26.3", IMPACT, std::slice::from_ref(&key)));
    Ok(())
}

#[test]
fn invalid_proposals_and_revision_overflow_fail_closed() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let mut policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    assert!(proposal(&policy, &[]).is_err());
    assert!(proposal(&policy, &[key.clone(), key.clone()]).is_err());
    assert!(proposal(&policy, &[ModKey::modrinth("unknown")?]).is_err());
    policy.sequence = u64::MAX;
    assert_eq!(
        policy.apply(
            &policy.revision()?,
            Change::SetInclusion {
                key,
                inclusion: Inclusion::Exclude
            }
        ),
        Err(PolicyError::RevisionExhausted)
    );
    Ok(())
}

#[test]
fn a_changed_activation_and_a_different_proposal_reject_the_old_reply() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let pending = proposal(&policy, std::slice::from_ref(&key))?;
    let changed = Observation {
        facts_revision: FACTS,
        active_activation: Some(IMPACT),
    };
    assert_eq!(
        pending.answer(&pending.revision()?, &policy, changed, Action::Accept),
        Err(PolicyError::StaleRevision)
    );
    let replacement = Decision::new(
        &policy,
        Basis {
            target: "26.3",
            facts_revision: FACTS,
            impact_digest: IMPACT,
            baseline_activation: None,
            nonce: FACTS,
        },
        std::slice::from_ref(&key),
    )?;
    assert_ne!(pending.revision()?, replacement.revision()?);
    assert_eq!(
        replacement.answer(
            &pending.revision()?,
            &policy,
            observation(FACTS),
            Action::Accept
        ),
        Err(PolicyError::StaleRevision)
    );
    Ok(())
}

#[test]
fn partial_recovery_can_shrink_but_never_expand_the_omission_scope() -> TestResult {
    let first = ModKey::modrinth("first")?;
    let second = ModKey::modrinth("second")?;
    let extra = ModKey::modrinth("extra")?;
    let policy = Policy::new(
        PROFILE,
        vec![recommendation(&first, true), recommendation(&second, true)],
    )?;
    let pending = proposal(&policy, &[first.clone(), second.clone()])?;
    let accepted = pending.answer(
        &pending.revision()?,
        &policy,
        observation(FACTS),
        Action::Accept,
    )?;
    let next = policy.apply(
        &policy.revision()?,
        Change::SetInclusion {
            key: extra.clone(),
            inclusion: Inclusion::Include,
        },
    )?;
    assert!(accepted.covers_omissions(&next, "26.3", IMPACT, std::slice::from_ref(&second)));
    assert!(!accepted.covers_omissions(&next, "26.3", IMPACT, &[first, second, extra]));
    Ok(())
}

#[test]
fn item_limits_are_checked_for_the_union_and_rejected_actions_preserve_state() -> TestResult {
    let entries: Result<Vec<_>, PolicyError> = (0..MAX_ITEMS)
        .map(|index| {
            Ok(recommendation(
                &ModKey::modrinth(&format!("project-{index}"))?,
                true,
            ))
        })
        .collect();
    let policy = Policy::new(PROFILE, entries?)?;
    let original = policy.revision()?;
    assert_eq!(
        policy.apply(
            &original,
            Change::SetInclusion {
                key: ModKey::modrinth("extra")?,
                inclusion: Inclusion::Include,
            }
        ),
        Err(PolicyError::TooManyItems)
    );
    assert_eq!(policy.revision()?, original);
    Ok(())
}

fn explore_answers(
    current: &Decision,
    initial: &Decision,
    policy: &Policy,
    key: &ModKey,
    depth: usize,
) -> Result<usize, PolicyError> {
    let permits = current.covers_omissions(policy, "26.3", IMPACT, std::slice::from_ref(key));
    assert_eq!(permits, current.phase() == Phase::Accepted);
    assert!(!current.covers_omissions(policy, "26.4", IMPACT, std::slice::from_ref(key)));
    if current.revision()? != initial.revision()? {
        assert_eq!(
            current.answer(
                &initial.revision()?,
                policy,
                observation(FACTS),
                Action::Accept
            ),
            Err(PolicyError::StaleRevision)
        );
    }
    let mut prefixes = 1;
    if depth > 0 {
        for action in [Action::Accept, Action::Defer, Action::Revoke] {
            let next =
                match current.answer(&current.revision()?, policy, observation(FACTS), action) {
                    Ok(next) => next,
                    Err(PolicyError::InvalidDecision) => current.clone(),
                    Err(error) => return Err(error),
                };
            prefixes += explore_answers(&next, initial, policy, key, depth - 1)?;
        }
    }
    Ok(prefixes)
}

#[test]
fn every_answer_sequence_through_depth_five_preserves_permission_boundaries() -> TestResult {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(PROFILE, vec![recommendation(&key, true)])?;
    let original = policy.revision()?;
    let pending = proposal(&policy, std::slice::from_ref(&key))?;
    assert_eq!(explore_answers(&pending, &pending, &policy, &key, 5)?, 364);
    assert_eq!(policy.revision()?, original);
    Ok(())
}
