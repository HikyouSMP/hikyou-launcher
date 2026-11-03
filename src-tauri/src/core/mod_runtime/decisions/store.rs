//! Bounded proposal/checkpoint storage and revision permission linkage.

use super::*;
use serde::de::DeserializeOwned;
use std::io::Read;

const LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Proposal {
    pub(super) schema: u32,
    pub(super) profile_id: String,
    pub(super) base_intent: Intent,
    pub(super) source_intent_id: String,
    pub(super) baseline_id: Option<String>,
    pub(super) candidate_id: String,
    pub(super) recipe: String,
    pub(super) workspace: String,
    pub(super) facts: String,
    pub(super) impact: String,
    pub(super) nonce: String,
    pub(super) omitted: Vec<String>,
    pub(super) model: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Locator {
    pub(super) proposal_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) profile_id: String,
    pub(super) revision_id: String,
    pub(super) proposal_id: String,
    pub(super) impact: String,
    pub(super) omitted: Vec<String>,
}

pub(super) fn hash(value: &impl Serialize) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)
    ))
}

pub(super) fn digest(id: &str) -> Result<(), String> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid Mod decision identity".into());
    }
    Ok(())
}

pub(super) fn directory(
    paths: &LauncherPaths,
    id: &str,
    create: bool,
) -> Result<Option<cap_std::fs::Dir>, String> {
    profile::validate_profile_ref(id)?;
    let root = managed_fs::root(&paths.root())?;
    let name = format!("state/mod-decisions/{}", key(id));
    if !create && managed_fs::inspect(&root, Path::new(&name))?.is_none() {
        return Ok(None);
    }
    managed_fs::directory(&root, Path::new(&name), create).map(Some)
}

pub(super) fn read<T: DeserializeOwned>(
    dir: &cap_std::fs::Dir,
    name: &str,
) -> Result<Option<T>, String> {
    if managed_fs::inspect(dir, Path::new(name))?.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(dir, Path::new(name))?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read Mod decision: {e}"))?;
    if bytes.len() as u64 > LIMIT {
        return Err("Mod decision exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("Invalid Mod decision; changes are blocked: {e}"))
}

pub(super) fn write(
    dir: &cap_std::fs::Dir,
    name: &str,
    value: &impl Serialize,
    replace: bool,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Mod decision exceeds its size limit".into());
    }
    managed_fs::write(dir, Path::new(name), &bytes, replace)
}

pub(super) fn model(record: &Proposal) -> Result<Decision, String> {
    if record.schema != 1 {
        return Err("Unsupported Mod decision schema".into());
    }
    record.base_intent.validate(&record.profile_id)?;
    for value in [
        &record.candidate_id,
        &record.source_intent_id,
        &record.recipe,
        &record.workspace,
        &record.facts,
        &record.impact,
        &record.nonce,
    ] {
        digest(value)?;
    }
    if let Some(id) = &record.baseline_id {
        digest(id)?;
    }
    let policy = selection::policy(&record.base_intent)?;
    if hash(&(
        &record.source_intent_id,
        record.base_intent.fingerprint()?,
        &record.recipe,
        &record.workspace,
        &record.impact,
    ))? != record.facts
    {
        return Err("Mod decision observation digest mismatch".into());
    }
    let candidate_target = target_from_model(&record.model)?;
    let omitted = mod_keys(&record.omitted)?;
    let expected = Decision::new(
        &policy,
        Basis {
            target: &candidate_target,
            facts_revision: &record.facts,
            impact_digest: &record.impact,
            baseline_activation: record.baseline_id.as_deref(),
            nonce: &record.nonce,
        },
        &omitted,
    )
    .map_err(|e| e.to_string())?;
    let decision =
        Decision::restore(&serde_json::to_vec(&record.model).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if decision.id() != expected.id() {
        return Err("Mod decision input binding mismatch".into());
    }
    Ok(decision)
}

pub(super) fn target_from_model(value: &serde_json::Value) -> Result<String, String> {
    // Checkpoint shape is owned by the library; do not infer private fields here.
    let decision = Decision::restore(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(decision.target().to_owned())
}

pub(super) fn checkpoint(decision: &Decision) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&decision.checkpoint().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub(super) fn mod_keys(keys: &[String]) -> Result<Vec<ModKey>, String> {
    let mut unique = BTreeSet::new();
    if keys.is_empty() || keys.len() > 128 {
        return Err("Invalid omitted Mod set".into());
    }
    keys.iter()
        .map(|key| {
            if !unique.insert(key) {
                return Err("Duplicate omitted Mod identity".into());
            }
            if let Some(id) = key.strip_prefix("modrinth:") {
                ModKey::modrinth(id).map_err(|e| e.to_string())
            } else if let Some(id) = key.strip_prefix("local:") {
                ModKey::local(id).map_err(|e| e.to_string())
            } else {
                Err("Invalid omitted Mod identity".into())
            }
        })
        .collect()
}

pub(super) fn load(paths: &LauncherPaths, id: &str, proposal: &str) -> Result<Proposal, String> {
    digest(proposal)?;
    let dir = directory(paths, id, false)?.ok_or("Mod decision not found")?;
    let record: Proposal =
        read(&dir, &format!("{proposal}.json"))?.ok_or("Mod decision not found")?;
    if record.profile_id != id || model(&record)?.id() != proposal {
        return Err("Mod decision profile or identity mismatch".into());
    }
    Ok(record)
}

pub(super) fn current(paths: &LauncherPaths, id: &str) -> Result<Option<Proposal>, String> {
    let Some(dir) = directory(paths, id, false)? else {
        return Ok(None);
    };
    let Some(locator) = read::<Locator>(&dir, "current.json")? else {
        return Ok(None);
    };
    load(paths, id, &locator.proposal_id).map(Some)
}

pub(super) fn receipt(
    paths: &LauncherPaths,
    revision: &ModRevision,
) -> Result<Option<Receipt>, String> {
    let Some(dir) = directory(paths, &revision.profile_id, false)? else {
        return Ok(None);
    };
    let value = read::<Receipt>(&dir, &format!("revision-{}.json", revision.id))?;
    if let Some(value) = &value {
        if value.profile_id != revision.profile_id || value.revision_id != revision.id {
            return Err("Mod decision receipt ownership mismatch".into());
        }
        digest(&value.proposal_id)?;
        digest(&value.impact)?;
        mod_keys(&value.omitted)?;
    }
    Ok(value)
}

pub(super) fn plan_digest(revision: &ModRevision, omitted: &[String]) -> Result<String, String> {
    let mut objects: Vec<_> = revision
        .mods
        .iter()
        .map(|entry| (&entry.object_id, entry.enabled, &entry.project_id))
        .collect();
    objects.sort();
    hash(&(1u32, &revision.target, objects, omitted))
}

pub(super) fn bind_revision(
    paths: &LauncherPaths,
    record: &Proposal,
    decision: &Decision,
    revision: &ModRevision,
) -> Result<(), String> {
    if decision.phase() != Phase::Accepted
        || plan_digest(revision, &record.omitted)? != record.impact
    {
        return Err("Mod alternative changed".into());
    }
    let value = Receipt {
        profile_id: record.profile_id.clone(),
        revision_id: revision.id.clone(),
        proposal_id: decision.id().into(),
        impact: record.impact.clone(),
        omitted: record.omitted.clone(),
    };
    let dir =
        directory(paths, &record.profile_id, true)?.ok_or("Mod decision directory unavailable")?;
    let name = format!("revision-{}.json", revision.id);
    if let Some(previous) = read::<Receipt>(&dir, &name)? {
        if hash(&previous)? != hash(&value)? {
            // A fresh explicit acceptance may replace a withdrawn/older answer
            // for identical immutable bytes. The old proposal remains history.
            let old = load(paths, &revision.profile_id, &previous.proposal_id)?;
            if previous.profile_id != revision.profile_id
                || previous.revision_id != revision.id
                || previous.impact != plan_digest(revision, &previous.omitted)?
                || old.impact != previous.impact
            {
                return Err("Mod alternative permission locator is damaged".into());
            }
            return write(&dir, &name, &value, true);
        }
        return Ok(());
    }
    write(&dir, &name, &value, false)
}
