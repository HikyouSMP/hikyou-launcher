//! Pure selection policy. No storage, provider access, file paths or activation.
//! This model is not yet the writer for legacy Mod intents.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![forbid(unsafe_code)]

mod checkpoint;
pub mod decisions;
#[cfg(test)]
mod tests;

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_ITEMS: usize = 4096;
/// Maximum encoded size accepted or emitted by a checkpoint operation.
pub const MAX_CHECKPOINT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyError {
    InvalidProfile,
    InvalidIdentity,
    InvalidVersion,
    DuplicateIdentity,
    TooManyItems,
    StaleRevision,
    RevisionExhausted,
    InvalidDecision,
    InvalidCheckpoint,
    CheckpointTooLarge,
    Encoding,
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProfile => "Invalid Mod policy profile",
            Self::InvalidIdentity => "Invalid Mod identity",
            Self::InvalidVersion => "Invalid Mod version policy",
            Self::DuplicateIdentity => "Duplicate Mod identity",
            Self::TooManyItems => "Mod policy exceeds its item limit",
            Self::StaleRevision => "Mod choices changed; refresh before applying this action",
            Self::RevisionExhausted => "Mod policy revision limit reached",
            Self::InvalidDecision => "Invalid or inapplicable Mod decision",
            Self::InvalidCheckpoint => "Invalid or corrupt Mod checkpoint",
            Self::CheckpointTooLarge => "Mod checkpoint exceeds its size limit",
            Self::Encoding => "Could not encode Mod policy",
        })
    }
}

impl std::error::Error for PolicyError {}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
enum Identity {
    Modrinth(String),
    Local(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ModKey(Identity);

impl ModKey {
    pub fn project_id(&self) -> Option<&str> {
        match &self.0 {
            Identity::Modrinth(id) => Some(id),
            Identity::Local(_) => None,
        }
    }

    pub fn object_id(&self) -> Option<&str> {
        match &self.0 {
            Identity::Local(id) => Some(id),
            Identity::Modrinth(_) => None,
        }
    }

    // Syntax validation is not proof that a provider slug has been canonicalized.
    // Provider adapters must establish canonical identity before creating policy.
    pub fn modrinth(id: &str) -> Result<Self, PolicyError> {
        if !valid_provider_id(id) {
            return Err(PolicyError::InvalidIdentity);
        }
        Ok(Self(Identity::Modrinth(id.into())))
    }

    pub fn local(object_id: &str) -> Result<Self, PolicyError> {
        if !valid_digest(object_id) {
            return Err(PolicyError::InvalidIdentity);
        }
        Ok(Self(Identity::Local(object_id.into())))
    }

    fn default_version(&self) -> VersionRule {
        match self.0 {
            Identity::Modrinth(_) => VersionRule::Compatible,
            Identity::Local(_) => VersionRule::ExactObject,
        }
    }

    fn validate_version(&self, version: &VersionRule) -> Result<(), PolicyError> {
        match (&self.0, version) {
            (Identity::Modrinth(_), VersionRule::Compatible)
            | (Identity::Local(_), VersionRule::ExactObject) => Ok(()),
            (Identity::Modrinth(_), VersionRule::PinnedRelease(id)) if valid_provider_id(id) => {
                Ok(())
            }
            _ => Err(PolicyError::InvalidVersion),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "releaseId", rename_all = "camelCase")]
pub enum VersionRule {
    Compatible,
    PinnedRelease(String),
    ExactObject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Recommendation {
    pub key: ModKey,
    pub enabled: bool,
    pub version: VersionRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Inclusion {
    Inherit,
    Include,
    Exclude,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Override {
    inclusion: Inclusion,
    version: Option<VersionRule>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceSource {
    Recommendation,
    ExplicitInclude,
    ExplicitExclude,
    NotRequested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "version", rename_all = "camelCase")]
pub enum Desired {
    Required(VersionRule),
    NotRequested,
    Excluded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub key: ModKey,
    pub desired: Desired,
    pub source: ChoiceSource,
    pub version_overridden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    profile_id: String,
    sequence: u64,
    digest: String,
}

impl Revision {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    profile_id: String,
    sequence: u64,
    recommendations: BTreeMap<ModKey, Recommendation>,
    overrides: BTreeMap<ModKey, Override>,
}

#[derive(Clone, Debug)]
pub enum Change {
    SetInclusion { key: ModKey, inclusion: Inclusion },
    SetVersion { key: ModKey, version: VersionRule },
    FollowRecommendations { key: ModKey },
    ReplaceRecommendations(Vec<Recommendation>),
}

impl Policy {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn new(profile_id: &str, entries: Vec<Recommendation>) -> Result<Self, PolicyError> {
        // The application adapter establishes existence/canonical identity.
        // This library treats profile IDs as bounded opaque scope identifiers.
        if profile_id.is_empty()
            || profile_id.len() > 128
            || !profile_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.')
            })
        {
            return Err(PolicyError::InvalidProfile);
        }
        Ok(Self {
            profile_id: profile_id.into(),
            sequence: 0,
            recommendations: recommendations(entries)?,
            overrides: BTreeMap::new(),
        })
    }

    pub fn revision(&self) -> Result<Revision, PolicyError> {
        let digest = digest(&(
            "mod-policy-v1",
            &self.profile_id,
            self.sequence,
            self.recommendations.iter().collect::<Vec<_>>(),
            self.overrides.iter().collect::<Vec<_>>(),
        ))?;
        Ok(Revision {
            profile_id: self.profile_id.clone(),
            sequence: self.sequence,
            digest,
        })
    }

    pub fn choices(&self) -> Vec<Choice> {
        let keys: BTreeSet<_> = self
            .recommendations
            .keys()
            .chain(self.overrides.keys())
            .collect();
        keys.into_iter().map(|key| self.choice(key)).collect()
    }

    pub fn choice(&self, key: &ModKey) -> Choice {
        let preset = self.recommendations.get(key);
        let selected = self.overrides.get(key);
        let version_override = selected.and_then(|entry| entry.version.as_ref());
        let version = version_override
            .cloned()
            .or_else(|| preset.map(|entry| entry.version.clone()))
            .unwrap_or_else(|| key.default_version());
        let (desired, source) = match selected.map(|entry| entry.inclusion) {
            Some(Inclusion::Include) => (Desired::Required(version), ChoiceSource::ExplicitInclude),
            Some(Inclusion::Exclude) => (Desired::Excluded, ChoiceSource::ExplicitExclude),
            _ if preset.is_some_and(|entry| entry.enabled) => {
                (Desired::Required(version), ChoiceSource::Recommendation)
            }
            _ if preset.is_some() => (Desired::NotRequested, ChoiceSource::Recommendation),
            _ => (Desired::NotRequested, ChoiceSource::NotRequested),
        };
        Choice {
            key: key.clone(),
            desired,
            source,
            version_overridden: version_override.is_some(),
        }
    }

    /// Returns a successor; rejecting an action never partially changes the input.
    pub fn apply(&self, expected: &Revision, change: Change) -> Result<Self, PolicyError> {
        if &self.revision()? != expected {
            return Err(PolicyError::StaleRevision);
        }
        let mut next = self.clone();
        match change {
            Change::SetInclusion { key, inclusion } => {
                let entry = next.overrides.entry(key.clone()).or_insert(Override {
                    inclusion: Inclusion::Inherit,
                    version: None,
                });
                entry.inclusion = inclusion;
                if inclusion == Inclusion::Inherit && entry.version.is_none() {
                    next.overrides.remove(&key);
                }
            }
            Change::SetVersion { key, version } => {
                key.validate_version(&version)?;
                next.overrides
                    .entry(key)
                    .or_insert(Override {
                        inclusion: Inclusion::Inherit,
                        version: None,
                    })
                    .version = Some(version);
            }
            Change::FollowRecommendations { key } => {
                next.overrides.remove(&key);
            }
            Change::ReplaceRecommendations(entries) => {
                next.recommendations = recommendations(entries)?;
            }
        }
        if next.overrides.len() > MAX_ITEMS
            || next
                .recommendations
                .keys()
                .chain(next.overrides.keys())
                .collect::<BTreeSet<_>>()
                .len()
                > MAX_ITEMS
        {
            return Err(PolicyError::TooManyItems);
        }
        if next != *self {
            next.sequence = self
                .sequence
                .checked_add(1)
                .ok_or(PolicyError::RevisionExhausted)?;
        }
        Ok(next)
    }
}

fn recommendations(
    entries: Vec<Recommendation>,
) -> Result<BTreeMap<ModKey, Recommendation>, PolicyError> {
    if entries.len() > MAX_ITEMS {
        return Err(PolicyError::TooManyItems);
    }
    let mut result = BTreeMap::new();
    for entry in entries {
        entry.key.validate_version(&entry.version)?;
        if result.insert(entry.key.clone(), entry).is_some() {
            return Err(PolicyError::DuplicateIdentity);
        }
    }
    Ok(result)
}

fn valid_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest(value: &impl Serialize) -> Result<String, PolicyError> {
    let bytes = serde_json::to_vec(value).map_err(|_| PolicyError::Encoding)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
