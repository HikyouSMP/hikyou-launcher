//! Untrusted checkpoint bytes become private state only after complete validation.
//! Digests detect corruption, not malicious replacement or rollback by the owner
//! of the storage. The application owns atomic writes and freshness authority.

use super::*;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};

const POLICY_SCHEMA: &str = "mod-policy-checkpoint-v1";

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyOutput<'a> {
    schema: &'static str,
    revision: Revision,
    recommendations: Vec<&'a Recommendation>,
    overrides: Vec<(&'a ModKey, &'a Override)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PolicyInput {
    schema: String,
    revision: StoredRevision,
    recommendations: Vec<StoredRecommendation>,
    overrides: Vec<(StoredKey, StoredOverride)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRecommendation {
    key: StoredKey,
    enabled: bool,
    version: StoredRule,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOverride {
    inclusion: StoredInclusion,
    #[serde(deserialize_with = "required_option")]
    version: Option<StoredRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum StoredInclusion {
    Inherit,
    Include,
    Exclude,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum StoredKey {
    Modrinth(String),
    Local(String),
}

impl StoredKey {
    pub(crate) fn validate(self) -> Result<ModKey, PolicyError> {
        match self {
            Self::Modrinth(id) => ModKey::modrinth(&id),
            Self::Local(id) => ModKey::local(&id),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum StoredRule {
    Compatible {},
    PinnedRelease {
        #[serde(rename = "releaseId")]
        release_id: String,
    },
    ExactObject {},
}

impl StoredRule {
    pub(crate) fn validate(self, key: &ModKey) -> Result<VersionRule, PolicyError> {
        let rule = match self {
            Self::Compatible {} => VersionRule::Compatible,
            Self::PinnedRelease { release_id } => VersionRule::PinnedRelease(release_id),
            Self::ExactObject {} => VersionRule::ExactObject,
        };
        key.validate_version(&rule)?;
        Ok(rule)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredRevision {
    profile_id: String,
    sequence: u64,
    digest: String,
}

impl StoredRevision {
    pub(crate) fn validate(self) -> Result<Revision, PolicyError> {
        Policy::new(&self.profile_id, Vec::new())?;
        if !valid_digest(&self.digest) {
            return Err(PolicyError::InvalidCheckpoint);
        }
        Ok(Revision {
            profile_id: self.profile_id,
            sequence: self.sequence,
            digest: self.digest,
        })
    }
}

impl Policy {
    /// Encodes private policy state, including explicit choices hidden by defaults.
    pub fn checkpoint(&self) -> Result<Vec<u8>, PolicyError> {
        let bytes = encode(&PolicyOutput {
            schema: POLICY_SCHEMA,
            revision: self.revision()?,
            recommendations: self.recommendations.values().collect(),
            overrides: self.overrides.iter().collect(),
        })?;
        // Keep the writer and untrusted reader on the same invariant boundary.
        Self::restore(&bytes)?;
        Ok(bytes)
    }

    pub fn restore(bytes: &[u8]) -> Result<Self, PolicyError> {
        let input: PolicyInput = decode(bytes)?;
        if input.schema != POLICY_SCHEMA {
            return Err(PolicyError::InvalidCheckpoint);
        }
        let revision = input.revision.validate()?;
        if input.recommendations.len() > MAX_ITEMS || input.overrides.len() > MAX_ITEMS {
            return Err(PolicyError::TooManyItems);
        }
        let entries = input
            .recommendations
            .into_iter()
            .map(|entry| {
                let key = entry.key.validate()?;
                let version = entry.version.validate(&key)?;
                Ok(Recommendation {
                    key,
                    enabled: entry.enabled,
                    version,
                })
            })
            .collect::<Result<Vec<_>, PolicyError>>()?;
        let mut policy = Self::new(&revision.profile_id, entries)?;
        for (key, entry) in input.overrides {
            let key = key.validate()?;
            let inclusion = match entry.inclusion {
                StoredInclusion::Inherit => Inclusion::Inherit,
                StoredInclusion::Include => Inclusion::Include,
                StoredInclusion::Exclude => Inclusion::Exclude,
            };
            let version = entry.version.map(|rule| rule.validate(&key)).transpose()?;
            if inclusion == Inclusion::Inherit && version.is_none() {
                return Err(PolicyError::InvalidCheckpoint);
            }
            if policy
                .overrides
                .insert(key, Override { inclusion, version })
                .is_some()
            {
                return Err(PolicyError::DuplicateIdentity);
            }
        }
        if policy
            .recommendations
            .keys()
            .chain(policy.overrides.keys())
            .collect::<BTreeSet<_>>()
            .len()
            > MAX_ITEMS
        {
            return Err(PolicyError::TooManyItems);
        }
        let minimum_edits = policy
            .overrides
            .values()
            .map(|entry| {
                u64::from(entry.inclusion != Inclusion::Inherit)
                    + u64::from(entry.version.is_some())
            })
            .sum::<u64>();
        if revision.sequence < minimum_edits {
            return Err(PolicyError::InvalidCheckpoint);
        }
        policy.sequence = revision.sequence;
        if policy.revision()? != revision {
            return Err(PolicyError::InvalidCheckpoint);
        }
        Ok(policy)
    }

    /// Storage adapters should bind a record to the profile selected by its owner.
    pub fn restore_for_profile(bytes: &[u8], profile_id: &str) -> Result<Self, PolicyError> {
        let policy = Self::restore(bytes)?;
        if policy.profile_id != profile_id {
            return Err(PolicyError::InvalidProfile);
        }
        Ok(policy)
    }
}

pub(crate) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, PolicyError> {
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(PolicyError::CheckpointTooLarge);
    }
    serde_json::from_slice(bytes).map_err(|_| PolicyError::InvalidCheckpoint)
}

pub(crate) fn encode(value: &impl Serialize) -> Result<Vec<u8>, PolicyError> {
    let bytes = serde_json::to_vec(value).map_err(|_| PolicyError::Encoding)?;
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(PolicyError::CheckpointTooLarge);
    }
    Ok(bytes)
}
