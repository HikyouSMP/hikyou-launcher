//! User requirements are independent of a particular resolved set of jar files.

use super::{managed_fs, mod_repository::ModRevision, profile};
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_REQUIREMENTS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Requirement {
    pub project_id: Option<String>,
    pub object_id: Option<String>,
    pub filename: Option<String>,
    pub enabled: bool,
    pub display_name: Option<String>,
    pub icon_url: Option<String>,
    pub origin: String,
}

impl Requirement {
    pub fn key(&self) -> String {
        match &self.project_id {
            Some(project) => format!("modrinth:{project}"),
            None => format!("local:{}", self.object_id.as_deref().unwrap_or_default()),
        }
    }

    pub fn project(
        project_id: &str,
        enabled: bool,
        display_name: Option<String>,
        icon_url: Option<String>,
        origin: &str,
    ) -> Self {
        Self {
            project_id: Some(project_id.into()),
            object_id: None,
            filename: None,
            enabled,
            display_name,
            icon_url,
            origin: origin.into(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        match (&self.project_id, &self.object_id, &self.filename) {
            (Some(id), None, None) if valid_project(id) => {}
            (None, Some(id), Some(name))
                if valid_hash(id)
                    && super::mod_files::is_safe_filename(name)
                    && (name.to_ascii_lowercase().ends_with(".jar")
                        || name.to_ascii_lowercase().ends_with(".jar.disabled")) => {}
            _ => return Err("invalid requested Mod identity".into()),
        }
        if !matches!(self.origin.as_str(), "user" | "imported" | "recommended")
            || self
                .display_name
                .as_ref()
                .is_some_and(|value| value.len() > 4096)
            || self
                .icon_url
                .as_ref()
                .is_some_and(|value| value.len() > 8192)
        {
            return Err("invalid requested Mod metadata".into());
        }
        Ok(())
    }
}

pub fn valid_project(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn valid_hash(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Intent {
    pub schema: u32,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recommendations_initialized: bool,
    pub requirements: Vec<Requirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_projects: Vec<String>,
}

impl Intent {
    pub fn from_revision(revision: &ModRevision) -> Self {
        let mut requests = BTreeMap::new();
        for entry in &revision.mods {
            let requirement =
                if let Some(project) = entry.project_id.as_deref().filter(|id| valid_project(id)) {
                    Requirement::project(
                        project,
                        entry.enabled,
                        entry.display_name.clone(),
                        entry.icon_url.clone(),
                        "imported",
                    )
                } else {
                    Requirement {
                        project_id: None,
                        object_id: Some(entry.object_id.clone()),
                        filename: Some(entry.filename.clone()),
                        enabled: entry.enabled,
                        display_name: entry.display_name.clone(),
                        icon_url: entry.icon_url.clone(),
                        origin: "imported".into(),
                    }
                };
            requests
                .entry(requirement.key())
                .and_modify(|previous: &mut Requirement| previous.enabled |= requirement.enabled)
                .or_insert(requirement);
        }
        // Legacy files do not prove which roots were chosen by the user. Import
        // conservatively instead of deleting a presumed transitive dependency.
        Self {
            schema: 1,
            profile_id: revision.profile_id.clone(),
            recommendations_initialized: false,
            requirements: requests.into_values().collect(),
            excluded_projects: Vec::new(),
        }
    }

    pub fn validate(&self, id: &str) -> Result<(), String> {
        profile::validate_profile_ref(id)?;
        if self.schema != 1
            || self.profile_id != id
            || self.requirements.len() > MAX_REQUIREMENTS
            || self.excluded_projects.len() > MAX_REQUIREMENTS
        {
            return Err("invalid Mod intent record".into());
        }
        let mut keys = std::collections::BTreeSet::new();
        for requirement in &self.requirements {
            requirement.validate()?;
            if !keys.insert(requirement.key()) {
                return Err("duplicate requested Mod identity".into());
            }
        }
        let mut excluded = std::collections::BTreeSet::new();
        for project in &self.excluded_projects {
            if !valid_project(project)
                || !excluded.insert(project)
                || keys.contains(&format!("modrinth:{project}"))
            {
                return Err("invalid or contradictory excluded Mod identity".into());
            }
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<String, String> {
        let mut canonical = self.clone();
        canonical.requirements.sort_by_key(Requirement::key);
        canonical.excluded_projects.sort();
        canonical.validate(&self.profile_id)?;
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical).map_err(|e| e.to_string())?)
        ))
    }

    pub fn upsert(&mut self, requirement: Requirement) -> Result<(), String> {
        requirement.validate()?;
        let key = requirement.key();
        if let Some(project) = &requirement.project_id {
            self.excluded_projects
                .retain(|excluded| excluded != project);
        }
        self.requirements.retain(|previous| previous.key() != key);
        self.requirements.push(requirement);
        self.requirements.sort_by_key(Requirement::key);
        self.validate(&self.profile_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DependencyEdge {
    pub requester: String,
    pub dependency: String,
    pub predicate: Option<String>,
    pub exact_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resolution {
    pub revision_id: String,
    pub intent: Intent,
    pub edges: Vec<DependencyEdge>,
}

fn key(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}

fn directory(root: &Path, create: bool) -> Result<Option<Dir>, String> {
    let root = managed_fs::root(root)?;
    if !create && managed_fs::inspect(&root, Path::new("state/mod-intents"))?.is_none() {
        return Ok(None);
    }
    managed_fs::directory(&root, Path::new("state/mod-intents"), create).map(Some)
}

fn read<T: DeserializeOwned>(dir: &Dir, name: &str) -> Result<Option<T>, String> {
    if managed_fs::inspect(dir, Path::new(name))?.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(dir, Path::new(name))?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Mod intent record is too large".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid Mod intent data: {e}"))
}

fn write<T: Serialize>(dir: &Dir, name: &str, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Mod intent record is too large".into());
    }
    managed_fs::write(dir, Path::new(name), &bytes, true)
}

pub fn load(root: &Path, id: &str) -> Result<Option<Intent>, String> {
    profile::validate_profile_ref(id)?;
    let Some(dir) = directory(root, false)? else {
        return Ok(None);
    };
    let value: Option<Intent> = read(&dir, &format!("{}.json", key(id)))?;
    if let Some(value) = &value {
        value.validate(id)?;
    }
    Ok(value)
}

/// The runtime edit lease serializes desired-state mutations with preparation.
pub fn save(root: &Path, intent: &Intent) -> Result<(), String> {
    intent.validate(&intent.profile_id)?;
    write(
        &directory(root, true)?.ok_or("Mod intent directory unavailable")?,
        &format!("{}.json", key(&intent.profile_id)),
        intent,
    )
}

pub fn resolution(root: &Path, id: &str, revision: &str) -> Result<Option<Resolution>, String> {
    profile::validate_profile_ref(id)?;
    if !valid_hash(revision) {
        return Err("invalid Mod revision identity".into());
    }
    let Some(dir) = directory(root, false)? else {
        return Ok(None);
    };
    let value: Option<Resolution> = read(&dir, &format!("resolutions/{revision}.json"))?;
    if let Some(value) = &value {
        value.intent.validate(id)?;
        if value.revision_id != revision {
            return Err("Mod resolution identity mismatch".into());
        }
        validate_edges(&value.edges)?;
    }
    Ok(value)
}

fn validate_edges(edges: &[DependencyEdge]) -> Result<(), String> {
    if edges.len() > MAX_REQUIREMENTS * 8
        || edges.iter().any(|edge| {
            !valid_project(&edge.requester)
                || !valid_project(&edge.dependency)
                || edge.predicate.as_ref().is_some_and(|s| s.len() > 4096)
                || edge
                    .exact_version
                    .as_ref()
                    .is_some_and(|s| !valid_project(s))
        })
    {
        return Err("invalid Mod dependency graph".into());
    }
    Ok(())
}

pub fn save_resolution(root: &Path, value: &Resolution) -> Result<(), String> {
    value.intent.validate(&value.intent.profile_id)?;
    if !valid_hash(&value.revision_id) {
        return Err("invalid Mod revision identity".into());
    }
    validate_edges(&value.edges)?;
    let dir = directory(root, true)?.ok_or("Mod intent directory unavailable")?;
    let name = format!("resolutions/{}.json", value.revision_id);
    if let Some(existing) = read::<Resolution>(&dir, &name)? {
        if existing.intent.fingerprint()? != value.intent.fingerprint()?
            || existing.edges != value.edges
        {
            return Err("a completed Mod resolution cannot be overwritten".into());
        }
        return Ok(());
    }
    write(&dir, &name, value)
}

pub fn for_revision(root: &Path, revision: &ModRevision) -> Result<Option<Resolution>, String> {
    let value = resolution(root, &revision.profile_id, &revision.id)?;
    if let Some(value) = &value {
        if revision
            .intent_id
            .as_ref()
            .is_some_and(|id| value.intent.fingerprint().ok().as_ref() != Some(id))
        {
            return Err("Mod revision intent digest mismatch".into());
        }
    } else if revision.intent_id.is_some() {
        return Err("Mod revision is missing its resolved intent".into());
    }
    Ok(value)
}

pub fn verify_selected(intent: &Intent, revision: &ModRevision) -> Result<(), String> {
    intent.validate(&revision.profile_id)?;
    for request in &intent.requirements {
        let matching: Vec<_> = revision
            .mods
            .iter()
            .filter(|entry| match &request.project_id {
                Some(project) => entry.project_id.as_ref() == Some(project),
                None => request.object_id.as_ref() == Some(&entry.object_id),
            })
            .collect();
        if request.enabled && !matching.iter().any(|entry| entry.enabled) {
            return Err(format!(
                "requested Mod is missing from resolved configuration: {}",
                request.key()
            ));
        }
        if !request.enabled && matching.iter().any(|entry| entry.enabled) {
            return Err(format!(
                "disabled Mod was enabled by resolution: {}",
                request.key()
            ));
        }
    }
    if revision.mods.iter().any(|entry| {
        entry.enabled
            && entry
                .project_id
                .as_ref()
                .is_some_and(|project| intent.excluded_projects.contains(project))
    }) {
        return Err("an explicitly removed Mod was enabled by resolution".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preparation {
    pub intent_id: String,
    pub phase: String,
    pub reason: Option<String>,
    pub updated_at: i64,
}

pub fn set_preparation(
    root: &Path,
    id: &str,
    intent_id: &str,
    phase: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    profile::validate_profile_ref(id)?;
    if !valid_hash(intent_id)
        || !matches!(phase, "queued" | "preparing" | "ready" | "blocked")
        || reason.is_some_and(|r| {
            r.len() > 64 || !r.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
        })
    {
        return Err("invalid Mod preparation status".into());
    }
    write(
        &directory(root, true)?.ok_or("Mod intent directory unavailable")?,
        &format!("{}.preparation.json", key(id)),
        &Preparation {
            intent_id: intent_id.into(),
            phase: phase.into(),
            reason: reason.map(str::to_owned),
            updated_at: chrono::Utc::now().timestamp(),
        },
    )
}

pub fn preparation(root: &Path, id: &str) -> Result<Option<Preparation>, String> {
    profile::validate_profile_ref(id)?;
    let Some(dir) = directory(root, false)? else {
        return Ok(None);
    };
    read(&dir, &format!("{}.preparation.json", key(id)))
}

pub fn materialize_local(root: &Path, game_dir: &Path, intent: &Intent) -> Result<(), String> {
    if !intent
        .requirements
        .iter()
        .any(|request| request.object_id.is_some())
    {
        return Ok(());
    }
    let objects = managed_fs::directory(
        &managed_fs::root(root)?,
        Path::new("meta/mod-objects"),
        false,
    )?;
    let destination = managed_fs::directory(&managed_fs::root(game_dir)?, Path::new("mods"), true)?;
    let mut metadata = super::mod_files::load_meta_checked_in(&destination)?;
    let mut metadata_changed = false;
    for request in &intent.requirements {
        let (Some(hash), Some(filename)) = (&request.object_id, &request.filename) else {
            continue;
        };
        request.validate()?;
        let base = if filename.to_ascii_lowercase().ends_with(".disabled") {
            &filename[..filename.len() - ".disabled".len()]
        } else {
            filename
        };
        let name = if request.enabled {
            base.to_string()
        } else {
            format!("{base}.disabled")
        };
        let bytes = super::mod_files::read_bounded(
            managed_fs::read_file(&objects, Path::new(hash))?,
            super::mod_files::MAX_MOD_BYTES,
        )?;
        if format!("{:x}", Sha256::digest(&bytes)) != *hash {
            return Err("local Mod object changed".into());
        }
        let other = if request.enabled {
            format!("{base}.disabled")
        } else {
            base.to_string()
        };
        metadata_changed |= metadata.remove(&name).is_some();
        metadata_changed |= metadata.remove(&other).is_some();
        if let Some(metadata) = managed_fs::inspect(&destination, Path::new(&other))? {
            if !metadata.is_file() {
                return Err("local Mod target is not a regular file".into());
            }
            destination.remove_file(&other).map_err(|e| e.to_string())?;
        }
        managed_fs::write(&destination, Path::new(&name), &bytes, true)?;
    }
    if metadata_changed {
        managed_fs::write(
            &destination,
            Path::new(".mod_meta.json"),
            &serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
            true,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "smart:latest-plus";
    fn intent() -> Intent {
        Intent {
            schema: 1,
            profile_id: ID.into(),
            recommendations_initialized: false,
            requirements: vec![Requirement::project("sodium", true, None, None, "user")],
            excluded_projects: Vec::new(),
        }
    }
    #[test]
    fn pending_requirement_survives_without_changing_any_ready_revision() {
        let temp = tempfile::tempdir().unwrap();
        let mut wish = intent();
        save(temp.path(), &wish).unwrap();
        wish.upsert(Requirement::project("iris", true, None, None, "user"))
            .unwrap();
        save(temp.path(), &wish).unwrap();
        assert_eq!(load(temp.path(), ID).unwrap(), Some(wish));
        assert!(!temp.path().join("state/mod-revisions").exists());
    }
    #[test]
    fn fingerprints_are_order_independent_and_resolution_records_are_immutable() {
        let temp = tempfile::tempdir().unwrap();
        let mut wish = intent();
        wish.upsert(Requirement::project("iris", false, None, None, "user"))
            .unwrap();
        let fingerprint = wish.fingerprint().unwrap();
        wish.requirements.reverse();
        assert_eq!(fingerprint, wish.fingerprint().unwrap());
        let mut record = Resolution {
            revision_id: "a".repeat(64),
            intent: wish,
            edges: vec![],
        };
        save_resolution(temp.path(), &record).unwrap();
        record.intent.requirements[0].enabled = !record.intent.requirements[0].enabled;
        assert!(save_resolution(temp.path(), &record).is_err());
        assert!(resolution(temp.path(), "smart:snapshot-plus", &record.revision_id).is_err());
    }
    #[test]
    fn invalid_or_ambiguous_sources_never_reach_storage() {
        let mut wish = intent();
        wish.requirements[0].project_id = Some("../escape".into());
        assert!(wish.validate(ID).is_err());
        wish = intent();
        wish.requirements[0].object_id = Some("a".repeat(64));
        assert!(wish.validate(ID).is_err());
        wish = intent();
        wish.requirements.push(wish.requirements[0].clone());
        assert!(wish.validate(ID).is_err());
    }
}
