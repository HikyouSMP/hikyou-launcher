//! Backend-owned scoped answers. A caller must independently validate the offered
//! alternative and final runtime graph; this module never proves compatibility.

use super::{
    Desired, MAX_ITEMS, ModKey, Policy, PolicyError, Revision, VersionRule, digest, valid_digest,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

mod checkpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Pending,
    Deferred,
    Accepted,
    Revoked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Defer,
    Accept,
    Revoke,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRevision {
    proposal_id: String,
    sequence: u64,
    digest: String,
}

impl DecisionRevision {
    pub fn proposal_id(&self) -> &str {
        &self.proposal_id
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    // Retained for checkpoint integrity without changing the existing JSON/hash.
    #[serde(skip_serializing)]
    nonce: String,
    proposal_id: String,
    policy_revision: Revision,
    target: String,
    facts_revision: String,
    impact_digest: String,
    baseline_activation: Option<String>,
    // Sorted pairs serialize as records, never as non-string JSON object keys.
    omitted: Vec<(ModKey, VersionRule)>,
    phase: Phase,
    sequence: u64,
}

/// Identity of backend-observed inputs, not frontend evidence of compatibility.
pub struct Basis<'a> {
    pub target: &'a str,
    pub facts_revision: &'a str,
    pub impact_digest: &'a str,
    pub baseline_activation: Option<&'a str>,
    pub nonce: &'a str,
}

pub struct Observation<'a> {
    pub facts_revision: &'a str,
    pub active_activation: Option<&'a str>,
}

impl Decision {
    /// Only a backend coordinator may construct a proposal. These checks validate
    /// its scope, not solver evidence, world safety, or artifact completeness.
    pub fn new(policy: &Policy, basis: Basis<'_>, omitted: &[ModKey]) -> Result<Self, PolicyError> {
        validate_basis(&basis)?;
        if omitted.is_empty() {
            return Err(PolicyError::InvalidDecision);
        }
        if omitted.len() > MAX_ITEMS {
            return Err(PolicyError::TooManyItems);
        }
        let mut requirements = BTreeMap::new();
        for key in omitted {
            let Desired::Required(version) = policy.choice(key).desired else {
                return Err(PolicyError::InvalidDecision);
            };
            if requirements.insert(key.clone(), version).is_some() {
                return Err(PolicyError::DuplicateIdentity);
            }
        }
        let policy_revision = policy.revision()?;
        let omitted: Vec<_> = requirements.into_iter().collect();
        let proposal_id = digest(&(
            "mod-advance-proposal-v1",
            &policy_revision,
            basis.target,
            basis.facts_revision,
            basis.impact_digest,
            basis.baseline_activation,
            basis.nonce,
            &omitted,
        ))?;
        Ok(Self {
            nonce: basis.nonce.into(),
            proposal_id,
            policy_revision,
            target: basis.target.into(),
            facts_revision: basis.facts_revision.into(),
            impact_digest: basis.impact_digest.into(),
            baseline_activation: basis.baseline_activation.map(str::to_owned),
            omitted,
            phase: Phase::Pending,
            sequence: 0,
        })
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn id(&self) -> &str {
        &self.proposal_id
    }

    /// Reconstructs the exact proposal identity, independently of its answer.
    /// Invalid basis/omission inputs fail rather than becoming a matching scope.
    pub fn matches_basis(
        &self,
        policy: &Policy,
        basis: &Basis<'_>,
        omitted: &[ModKey],
    ) -> Result<bool, PolicyError> {
        let proposal = Self::new(
            policy,
            Basis {
                target: basis.target,
                facts_revision: basis.facts_revision,
                impact_digest: basis.impact_digest,
                baseline_activation: basis.baseline_activation,
                nonce: basis.nonce,
            },
            omitted,
        )?;
        Ok(self.proposal_id == proposal.proposal_id)
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn profile_id(&self) -> &str {
        self.policy_revision.profile_id()
    }

    pub fn omitted(&self) -> &[(ModKey, VersionRule)] {
        &self.omitted
    }

    pub fn policy_revision(&self) -> &Revision {
        &self.policy_revision
    }

    pub fn needs_attention(&self) -> bool {
        self.phase == Phase::Pending
    }

    pub fn revision(&self) -> Result<DecisionRevision, PolicyError> {
        Ok(DecisionRevision {
            proposal_id: self.proposal_id.clone(),
            sequence: self.sequence,
            digest: digest(&("mod-decision-v1", self))?,
        })
    }

    /// Accept/defer must still describe the shown inputs. Revocation remains
    /// possible after those inputs change, but only for this exact decision.
    pub fn answer(
        &self,
        expected: &DecisionRevision,
        policy: &Policy,
        observation: Observation<'_>,
        action: Action,
    ) -> Result<Self, PolicyError> {
        if &self.revision()? != expected {
            return Err(PolicyError::StaleRevision);
        }
        if action != Action::Revoke
            && (policy.revision()? != self.policy_revision
                || observation.facts_revision != self.facts_revision
                || observation.active_activation != self.baseline_activation.as_deref())
        {
            return Err(PolicyError::StaleRevision);
        }
        if policy.profile_id != self.policy_revision.profile_id {
            return Err(PolicyError::InvalidDecision);
        }
        let next_phase = match (self.phase, action) {
            (Phase::Pending | Phase::Deferred, Action::Accept) => Phase::Accepted,
            (Phase::Pending | Phase::Deferred, Action::Defer) => Phase::Deferred,
            (Phase::Accepted, Action::Revoke) => Phase::Revoked,
            _ => return Err(PolicyError::InvalidDecision),
        };
        let mut next = self.clone();
        if next_phase != self.phase {
            next.sequence = self
                .sequence
                .checked_add(1)
                .ok_or(PolicyError::RevisionExhausted)?;
            next.phase = next_phase;
        }
        Ok(next)
    }

    /// Checks omission permission only. Final-plan validation, generation leases,
    /// current job ownership and user-data protection remain separate gates.
    pub fn covers_omissions(
        &self,
        policy: &Policy,
        target: &str,
        impact_digest: &str,
        omitted: &[ModKey],
    ) -> bool {
        if self.phase != Phase::Accepted
            || policy.profile_id != self.policy_revision.profile_id
            || target != self.target
            || impact_digest != self.impact_digest
            || omitted.len() > MAX_ITEMS
        {
            return false;
        }
        let actual: BTreeSet<_> = omitted.iter().collect();
        if actual.len() != omitted.len() {
            return false;
        }
        // Reusing an answer never authorizes a changed pin or an excluded root.
        if self
            .omitted
            .iter()
            .any(|(key, rule)| policy.choice(key).desired != Desired::Required(rule.clone()))
        {
            return false;
        }
        actual
            .into_iter()
            .all(|key| self.omitted.iter().any(|(allowed, _)| allowed == key))
    }
}

fn validate_basis(basis: &Basis<'_>) -> Result<(), PolicyError> {
    if basis.target.len() > 128
        || basis.target.is_empty()
        || !basis
            .target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+'))
        || matches!(basis.target, "." | "..")
        || !valid_digest(basis.facts_revision)
        || !valid_digest(basis.impact_digest)
        || !valid_digest(basis.nonce)
        || basis
            .baseline_activation
            .is_some_and(|id| !valid_digest(id))
    {
        return Err(PolicyError::InvalidDecision);
    }
    Ok(())
}
