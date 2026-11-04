use super::*;
use crate::checkpoint::{StoredKey, StoredRevision, StoredRule, decode, encode, required_option};
use serde::Deserialize;

const DECISION_SCHEMA: &str = "mod-decision-checkpoint-v1";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionOutput<'a> {
    schema: &'static str,
    nonce: &'a str,
    revision: DecisionRevision,
    decision: &'a Decision,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DecisionInput {
    schema: String,
    nonce: String,
    revision: StoredDecisionRevision,
    decision: StoredDecision,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDecisionRevision {
    proposal_id: String,
    sequence: u64,
    digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDecision {
    proposal_id: String,
    policy_revision: StoredRevision,
    target: String,
    facts_revision: String,
    impact_digest: String,
    #[serde(deserialize_with = "required_option")]
    baseline_activation: Option<String>,
    omitted: Vec<(StoredKey, StoredRule)>,
    phase: StoredPhase,
    sequence: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum StoredPhase {
    Pending,
    Deferred,
    Accepted,
    Revoked,
}

impl Decision {
    /// The application owns storage, rollback protection and proposal provenance.
    /// This checkpoint preserves syntax, scope and content integrity, not consent
    /// against an attacker able to replace the complete record and its digests.
    pub fn checkpoint(&self) -> Result<Vec<u8>, PolicyError> {
        let bytes = encode(&DecisionOutput {
            schema: DECISION_SCHEMA,
            nonce: &self.nonce,
            revision: self.revision()?,
            decision: self,
        })?;
        Self::restore(&bytes)?;
        Ok(bytes)
    }

    /// Restores a self-consistent record, not evidence that its policy is current.
    /// Bind the profile in the storage adapter; use `validate_against` when the
    /// original policy snapshot is available, and existing runtime gates on use.
    pub fn restore(bytes: &[u8]) -> Result<Self, PolicyError> {
        let input: DecisionInput = decode(bytes)?;
        if input.schema != DECISION_SCHEMA || !valid_digest(&input.nonce) {
            return Err(PolicyError::InvalidCheckpoint);
        }
        let state = input.decision;
        let phase = match state.phase {
            StoredPhase::Pending if state.sequence == 0 => Phase::Pending,
            StoredPhase::Deferred if state.sequence == 1 => Phase::Deferred,
            StoredPhase::Accepted if matches!(state.sequence, 1 | 2) => Phase::Accepted,
            StoredPhase::Revoked if matches!(state.sequence, 2 | 3) => Phase::Revoked,
            _ => return Err(PolicyError::InvalidCheckpoint),
        };
        if state.omitted.len() > MAX_ITEMS {
            return Err(PolicyError::TooManyItems);
        }
        validate_basis(&Basis {
            target: &state.target,
            facts_revision: &state.facts_revision,
            impact_digest: &state.impact_digest,
            baseline_activation: state.baseline_activation.as_deref(),
            nonce: &input.nonce,
        })?;
        if state.omitted.is_empty() {
            return Err(PolicyError::InvalidDecision);
        }
        let omitted = state
            .omitted
            .into_iter()
            .map(|(key, rule)| {
                let key = key.validate()?;
                let rule = rule.validate(&key)?;
                Ok((key, rule))
            })
            .collect::<Result<Vec<_>, PolicyError>>()?;
        // The proposal hashes canonical ordered pairs. Duplicate or reordered
        // pairs must not be normalized into a different stored proposal.
        if omitted.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err(PolicyError::InvalidCheckpoint);
        }
        let decision = Self {
            nonce: input.nonce,
            proposal_id: state.proposal_id,
            policy_revision: state.policy_revision.validate()?,
            target: state.target,
            facts_revision: state.facts_revision,
            impact_digest: state.impact_digest,
            baseline_activation: state.baseline_activation,
            omitted,
            phase,
            sequence: state.sequence,
        };
        let proposal_id = digest(&(
            "mod-advance-proposal-v1",
            &decision.policy_revision,
            &decision.target,
            &decision.facts_revision,
            &decision.impact_digest,
            &decision.baseline_activation,
            &decision.nonce,
            &decision.omitted,
        ))?;
        let revision = decision.revision()?;
        if decision.proposal_id != proposal_id
            || revision.proposal_id != input.revision.proposal_id
            || revision.sequence != input.revision.sequence
            || revision.digest != input.revision.digest
        {
            return Err(PolicyError::InvalidCheckpoint);
        }
        Ok(decision)
    }

    pub fn restore_for_profile(bytes: &[u8], profile_id: &str) -> Result<Self, PolicyError> {
        let decision = Self::restore(bytes)?;
        if decision.profile_id() != profile_id {
            return Err(PolicyError::InvalidProfile);
        }
        Ok(decision)
    }

    pub fn restore_against(bytes: &[u8], policy: &Policy) -> Result<Self, PolicyError> {
        let decision = Self::restore_for_profile(bytes, policy.profile_id())?;
        decision.validate_against(policy)?;
        Ok(decision)
    }

    /// Strictly validates against the original policy, not a later policy whose
    /// unchanged requirements may still be covered by an accepted permission.
    pub fn validate_against(&self, policy: &Policy) -> Result<(), PolicyError> {
        if policy.revision()? != self.policy_revision {
            return Err(PolicyError::StaleRevision);
        }
        if self
            .omitted
            .iter()
            .any(|(key, rule)| policy.choice(key).desired != Desired::Required(rule.clone()))
        {
            return Err(PolicyError::InvalidDecision);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Recommendation;

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn fixture() -> Result<(Policy, Decision), PolicyError> {
        let key = ModKey::modrinth("example")?;
        let policy = Policy::new(
            "profile",
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
                facts_revision: HASH,
                impact_digest: HASH,
                baseline_activation: None,
                nonce: HASH,
            },
            &[key],
        )?;
        Ok((policy, decision))
    }

    #[test]
    fn fresh_revision_hash_cannot_legitimize_unreachable_phase_sequences() -> TestResult {
        let (_, initial) = fixture()?;
        for phase in [
            Phase::Pending,
            Phase::Deferred,
            Phase::Accepted,
            Phase::Revoked,
        ] {
            for sequence in 0..=5 {
                let mut decision = initial.clone();
                decision.phase = phase;
                decision.sequence = sequence;
                let reachable = match phase {
                    Phase::Pending => sequence == 0,
                    Phase::Deferred => sequence == 1,
                    Phase::Accepted => matches!(sequence, 1 | 2),
                    Phase::Revoked => matches!(sequence, 2 | 3),
                };
                assert_eq!(
                    decision.checkpoint().is_ok(),
                    reachable,
                    "{phase:?}/{sequence}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn proposal_content_is_checked_even_when_outer_revision_is_rehashed() -> TestResult {
        let (_, initial) = fixture()?;
        let mut changed = initial.clone();
        changed.target = "26.4".into();
        assert_eq!(changed.checkpoint(), Err(PolicyError::InvalidCheckpoint));
        changed = initial.clone();
        changed.omitted.push(changed.omitted[0].clone());
        assert_eq!(changed.checkpoint(), Err(PolicyError::InvalidCheckpoint));
        changed = initial.clone();
        changed.omitted[0].1 = VersionRule::PinnedRelease("different".into());
        assert_eq!(changed.checkpoint(), Err(PolicyError::InvalidCheckpoint));
        changed = initial.clone();
        changed.nonce = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
        assert_eq!(changed.checkpoint(), Err(PolicyError::InvalidCheckpoint));
        Ok(())
    }

    #[test]
    fn original_policy_validation_rejects_self_consistent_inapplicable_requirements() -> TestResult
    {
        let (policy, mut decision) = fixture()?;
        decision.omitted[0].1 = VersionRule::PinnedRelease("different".into());
        // A complete replacement with new digests is not corruption detection's
        // threat model. The original policy remains a separate authority gate.
        decision.proposal_id = digest(&(
            "mod-advance-proposal-v1",
            &decision.policy_revision,
            &decision.target,
            &decision.facts_revision,
            &decision.impact_digest,
            &decision.baseline_activation,
            &decision.nonce,
            &decision.omitted,
        ))?;
        let bytes = decision.checkpoint()?;
        assert_eq!(Decision::restore(&bytes)?, decision);
        assert_eq!(
            Decision::restore_against(&bytes, &policy),
            Err(PolicyError::InvalidDecision)
        );
        Ok(())
    }
}
