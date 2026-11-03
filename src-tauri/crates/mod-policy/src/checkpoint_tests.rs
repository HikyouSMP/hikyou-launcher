use super::*;
use crate::decisions::{Action, Basis, Decision, Observation, Phase};
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const PROFILE: &str = "smart:latest-plus";
const FACTS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const IMPACT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const NONCE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn fixture() -> Result<(Policy, ModKey, Decision), PolicyError> {
    let key = ModKey::modrinth("example")?;
    let policy = Policy::new(
        PROFILE,
        vec![Recommendation {
            key: key.clone(),
            enabled: true,
            version: VersionRule::Compatible,
        }],
    )?;
    let decision = Decision::new(
        &policy,
        Basis {
            target: "26.3",
            facts_revision: FACTS,
            impact_digest: IMPACT,
            baseline_activation: Some(IMPACT),
            nonce: NONCE,
        },
        std::slice::from_ref(&key),
    )?;
    Ok((policy, key, decision))
}

fn observation() -> Observation<'static> {
    Observation {
        facts_revision: FACTS,
        active_activation: Some(IMPACT),
    }
}

#[test]
fn policy_roundtrip_preserves_hidden_overrides_and_revision_history() -> TestResult {
    let (initial, key, _) = fixture()?;
    let local = ModKey::local(FACTS)?;
    assert_eq!(key.project_id(), Some("example"));
    assert_eq!(key.object_id(), None);
    assert_eq!(local.project_id(), None);
    assert_eq!(local.object_id(), Some(FACTS));
    let pinned = initial.apply(
        &initial.revision()?,
        Change::SetVersion {
            key: key.clone(),
            version: VersionRule::PinnedRelease("fixed".into()),
        },
    )?;
    let excluded = pinned.apply(
        &pinned.revision()?,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Exclude,
        },
    )?;
    let policy = excluded.apply(
        &excluded.revision()?,
        Change::SetInclusion {
            key: local.clone(),
            inclusion: Inclusion::Include,
        },
    )?;
    let restored = Policy::restore_for_profile(&policy.checkpoint()?, PROFILE)?;
    assert_eq!(restored, policy);
    assert_eq!(restored.checkpoint()?, policy.checkpoint()?);
    assert_eq!(restored.revision()?.sequence(), 3);
    assert_eq!(restored.revision()?.profile_id(), PROFILE);
    assert!(valid_digest(restored.revision()?.digest()));
    assert_eq!(
        restored.apply(
            &initial.revision()?,
            Change::FollowRecommendations { key: key.clone() }
        ),
        Err(PolicyError::StaleRevision)
    );
    let enabled = restored.apply(
        &restored.revision()?,
        Change::SetInclusion {
            key: key.clone(),
            inclusion: Inclusion::Include,
        },
    )?;
    assert_eq!(
        enabled.choice(&key).desired,
        Desired::Required(VersionRule::PinnedRelease("fixed".into()))
    );
    assert_eq!(
        enabled.choice(&local).desired,
        Desired::Required(VersionRule::ExactObject)
    );
    assert_eq!(
        Policy::restore_for_profile(&policy.checkpoint()?, "other"),
        Err(PolicyError::InvalidProfile)
    );
    Ok(())
}

#[test]
fn decision_roundtrips_every_reachable_phase_without_broadening_permission() -> TestResult {
    let (policy, key, pending) = fixture()?;
    let deferred = pending.answer(&pending.revision()?, &policy, observation(), Action::Defer)?;
    for source in [&pending, &deferred] {
        let accepted =
            source.answer(&source.revision()?, &policy, observation(), Action::Accept)?;
        let revoked = accepted.answer(
            &accepted.revision()?,
            &policy,
            observation(),
            Action::Revoke,
        )?;
        for decision in [source, &accepted, &revoked] {
            let bytes = decision.checkpoint()?;
            let restored = Decision::restore_for_profile(&bytes, PROFILE)?;
            assert_eq!(&restored, decision);
            assert_eq!(restored.revision()?, decision.revision()?);
            assert_eq!(restored.checkpoint()?, bytes);
            restored.validate_against(&policy)?;
            assert_eq!(restored.target(), "26.3");
            assert_eq!(
                restored.omitted(),
                &[(key.clone(), VersionRule::Compatible)]
            );
            assert_eq!(restored.policy_revision(), &policy.revision()?);
            assert!(valid_digest(restored.revision()?.digest()));
            assert!(valid_digest(restored.revision()?.proposal_id()));
            assert_eq!(
                restored.covers_omissions(&policy, "26.3", IMPACT, std::slice::from_ref(&key)),
                decision.phase() == Phase::Accepted
            );
            assert!(!restored.covers_omissions(
                &policy,
                "different",
                IMPACT,
                std::slice::from_ref(&key)
            ));
            assert_eq!(
                Decision::restore_for_profile(&bytes, "other"),
                Err(PolicyError::InvalidProfile)
            );
        }
    }
    let changed = policy.apply(
        &policy.revision()?,
        Change::SetInclusion {
            key,
            inclusion: Inclusion::Exclude,
        },
    )?;
    assert_eq!(
        Decision::restore(&pending.checkpoint()?)?.validate_against(&changed),
        Err(PolicyError::StaleRevision)
    );
    // Retaining nonce is private: ordinary public serialization stays unchanged.
    let public = serde_json::to_value(&pending)?;
    assert!(public.get("nonce").is_none());
    Ok(())
}

#[test]
fn policy_rejects_unknown_missing_duplicate_and_corrupt_fields() -> TestResult {
    let (policy, key, _) = fixture()?;
    let policy = policy.apply(
        &policy.revision()?,
        Change::SetInclusion {
            key,
            inclusion: Inclusion::Exclude,
        },
    )?;
    let original: Value = serde_json::from_slice(&policy.checkpoint()?)?;
    for (pointer, replacement) in [
        ("/schema", json!("future")),
        ("/revision/profileId", json!("../bad")),
        ("/revision/sequence", json!(-1)),
        ("/revision/sequence", json!(0)),
        ("/revision/digest", json!(FACTS)),
        ("/recommendations/0/key/id", json!("../bad")),
        ("/recommendations/0/version", json!({"kind":"exactObject"})),
        (
            "/recommendations/0/version",
            json!({"kind":"compatible","releaseId":null}),
        ),
        (
            "/overrides/0/1",
            json!({"inclusion":"inherit","version":null}),
        ),
    ] {
        let mut value = original.clone();
        let slot = value.pointer_mut(pointer).ok_or("missing fixture field")?;
        *slot = replacement;
        assert!(
            Policy::restore(&serde_json::to_vec(&value)?).is_err(),
            "{pointer}"
        );
    }
    for pointer in [
        "",
        "/revision",
        "/recommendations/0",
        "/recommendations/0/key",
        "/recommendations/0/version",
        "/overrides/0/0",
        "/overrides/0/1",
    ] {
        let mut value = original.clone();
        value
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .ok_or("missing object")?
            .insert("unknown".into(), json!(true));
        assert!(
            Policy::restore(&serde_json::to_vec(&value)?).is_err(),
            "{pointer}"
        );
    }
    let mut value = original.clone();
    value["overrides"][0][1]
        .as_object_mut()
        .ok_or("missing override")?
        .remove("version");
    assert!(Policy::restore(&serde_json::to_vec(&value)?).is_err());
    for field in ["recommendations", "overrides"] {
        let mut value = original.clone();
        let entries = value[field].as_array_mut().ok_or("missing entries")?;
        entries.push(entries.first().ok_or("empty entries")?.clone());
        assert_eq!(
            Policy::restore(&serde_json::to_vec(&value)?),
            Err(PolicyError::DuplicateIdentity)
        );
    }
    let duplicate = String::from_utf8(policy.checkpoint()?)?.replacen(
        "\"schema\":",
        "\"schema\":\"mod-policy-checkpoint-v1\",\"schema\":",
        1,
    );
    assert!(Policy::restore(duplicate.as_bytes()).is_err());
    Ok(())
}

#[test]
fn decision_rejects_corruption_at_every_integrity_boundary() -> TestResult {
    let (_, _, decision) = fixture()?;
    let original: Value = serde_json::from_slice(&decision.checkpoint()?)?;
    for (pointer, replacement) in [
        ("/schema", json!("future")),
        ("/nonce", json!(FACTS)),
        ("/revision/proposalId", json!(FACTS)),
        ("/revision/sequence", json!(1)),
        ("/revision/digest", json!(FACTS)),
        ("/decision/proposalId", json!(FACTS)),
        ("/decision/policyRevision/digest", json!("bad")),
        ("/decision/policyRevision/profileId", json!("../bad")),
        ("/decision/target", json!("..")),
        ("/decision/factsRevision", json!("bad")),
        ("/decision/impactDigest", json!("bad")),
        ("/decision/baselineActivation", json!("bad")),
        ("/decision/omitted", json!([])),
        ("/decision/omitted/0/0/id", json!("../bad")),
        (
            "/decision/omitted/0/1",
            json!({"kind":"pinnedRelease","releaseId":"../bad"}),
        ),
        ("/decision/phase", json!("unknown")),
        ("/decision/sequence", json!(1)),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).ok_or("missing field")? = replacement;
        assert!(
            Decision::restore(&serde_json::to_vec(&value)?).is_err(),
            "{pointer}"
        );
    }
    for pointer in [
        "",
        "/revision",
        "/decision",
        "/decision/policyRevision",
        "/decision/omitted/0/0",
        "/decision/omitted/0/1",
    ] {
        let mut value = original.clone();
        value
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .ok_or("missing object")?
            .insert("unknown".into(), json!(true));
        assert!(
            Decision::restore(&serde_json::to_vec(&value)?).is_err(),
            "{pointer}"
        );
    }
    let mut value = original;
    value["decision"]
        .as_object_mut()
        .ok_or("missing decision")?
        .remove("baselineActivation");
    assert!(Decision::restore(&serde_json::to_vec(&value)?).is_err());
    Ok(())
}

#[test]
fn malformed_and_oversized_inputs_never_become_empty_state() -> TestResult {
    for bytes in [
        b"".as_slice(),
        b"null",
        b"{}",
        b"[]",
        b"{",
        b"\xff",
        b"{}{}",
    ] {
        assert!(Policy::restore(bytes).is_err());
        assert!(Decision::restore(bytes).is_err());
    }
    let maximum = vec![b' '; MAX_CHECKPOINT_BYTES];
    assert_eq!(
        Policy::restore(&maximum),
        Err(PolicyError::InvalidCheckpoint)
    );
    let oversized = vec![b' '; MAX_CHECKPOINT_BYTES + 1];
    assert_eq!(
        Policy::restore(&oversized),
        Err(PolicyError::CheckpointTooLarge)
    );
    assert_eq!(
        Decision::restore(&oversized),
        Err(PolicyError::CheckpointTooLarge)
    );
    let mut nested = vec![b'['; 256];
    nested.extend(vec![b']'; 256]);
    assert!(Policy::restore(&nested).is_err());
    Ok(())
}

#[test]
fn internally_inconsistent_policy_is_not_checkpointable_even_with_fresh_digest() -> TestResult {
    let (mut policy, key, _) = fixture()?;
    policy.overrides.insert(
        key,
        Override {
            inclusion: Inclusion::Exclude,
            version: Some(VersionRule::PinnedRelease("fixed".into())),
        },
    );
    for sequence in [0, 1] {
        policy.sequence = sequence;
        assert_eq!(policy.checkpoint(), Err(PolicyError::InvalidCheckpoint));
    }
    policy.sequence = 2;
    assert_eq!(Policy::restore(&policy.checkpoint()?)?, policy);
    Ok(())
}

#[test]
fn checkpoint_item_bounds_cover_union_and_each_collection() -> TestResult {
    let (policy, _, decision) = fixture()?;
    let mut value: Value = serde_json::from_slice(&policy.checkpoint()?)?;
    value["recommendations"] = json!(vec![value["recommendations"][0].clone(); MAX_ITEMS + 1]);
    assert_eq!(
        Policy::restore(&serde_json::to_vec(&value)?),
        Err(PolicyError::TooManyItems)
    );
    let entries = (0..MAX_ITEMS)
        .map(|index| {
            Ok(Recommendation {
                key: ModKey::modrinth(&format!("project-{index}"))?,
                enabled: true,
                version: VersionRule::Compatible,
            })
        })
        .collect::<Result<Vec<_>, PolicyError>>()?;
    let mut full = Policy::new(PROFILE, entries)?;
    assert_eq!(Policy::restore(&full.checkpoint()?)?, full);
    full.sequence = 1;
    full.overrides.insert(
        ModKey::modrinth("extra")?,
        Override {
            inclusion: Inclusion::Include,
            version: None,
        },
    );
    assert_eq!(full.checkpoint(), Err(PolicyError::TooManyItems));
    let mut value: Value = serde_json::from_slice(&decision.checkpoint()?)?;
    value["decision"]["omitted"] =
        json!(vec![value["decision"]["omitted"][0].clone(); MAX_ITEMS + 1]);
    assert_eq!(
        Decision::restore(&serde_json::to_vec(&value)?),
        Err(PolicyError::TooManyItems)
    );
    Ok(())
}

#[test]
fn restored_proposal_identity_binds_all_basis_inputs_without_binding_answer_phase() -> TestResult {
    let (policy, key, decision) = fixture()?;
    let accepted = decision.answer(
        &decision.revision()?,
        &policy,
        observation(),
        Action::Accept,
    )?;
    let restored = Decision::restore(&accepted.checkpoint()?)?;
    assert_eq!(restored.id(), decision.id());
    assert_eq!(restored.id(), restored.revision()?.proposal_id());
    let basis = Basis {
        target: "26.3",
        facts_revision: FACTS,
        impact_digest: IMPACT,
        baseline_activation: Some(IMPACT),
        nonce: NONCE,
    };
    assert!(restored.matches_basis(&policy, &basis, std::slice::from_ref(&key))?);
    for changed in [
        Basis {
            target: "26.4",
            ..basis
        },
        Basis {
            facts_revision: NONCE,
            ..basis
        },
        Basis {
            impact_digest: NONCE,
            ..basis
        },
        Basis {
            baseline_activation: None,
            ..basis
        },
        Basis {
            nonce: FACTS,
            ..basis
        },
    ] {
        assert!(!restored.matches_basis(&policy, &changed, std::slice::from_ref(&key))?);
    }
    assert_eq!(
        restored.matches_basis(&policy, &basis, &[]),
        Err(PolicyError::InvalidDecision)
    );
    let changed = policy.apply(
        &policy.revision()?,
        Change::SetVersion {
            key: key.clone(),
            version: VersionRule::PinnedRelease("fixed".into()),
        },
    )?;
    assert!(!restored.matches_basis(&changed, &basis, &[key])?);
    Ok(())
}
