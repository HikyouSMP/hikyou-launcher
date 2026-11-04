//! Explicit product policy, not a fabricated user answer. Only complete graphs publish.
use super::*;
use std::collections::BTreeSet;
use std::io::Read;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Authorization {
    policy: String,
    profile_id: String,
    revision_id: String,
    source: Intent,
    reduced: String,
    omitted: Vec<String>,
}
fn dir(paths: &LauncherPaths) -> Result<cap_std::fs::Dir, String> {
    managed_fs::directory(
        &managed_fs::root(&paths.root())?,
        Path::new("state/snapshot-policy"),
        true,
    )
}
fn omissions(source: &Intent, reduced: &Intent) -> Result<Vec<String>, String> {
    source.validate(&source.profile_id)?;
    reduced.validate(&source.profile_id)?;
    let mut result = Vec::new();
    for r in &source.requirements {
        let other = reduced
            .requirements
            .iter()
            .find(|v| v.key() == r.key())
            .ok_or("snapshot_policy_missing_root")?;
        if r.enabled && !other.enabled {
            result.push(r.key());
        } else if r != other {
            return Err("snapshot_policy_changed_root".into());
        }
    }
    if source.requirements.len() != reduced.requirements.len()
        || source.excluded_projects != reduced.excluded_projects
    {
        return Err("snapshot_policy_changed_roots".into());
    }
    result.sort();
    Ok(result)
}
pub(super) fn can_advance(paths: &LauncherPaths, id: &str) -> Result<bool, String> {
    if profile::smart_key_from_ref(id) != Some("snapshot-plus") {
        return Ok(false);
    }
    let game = paths.checked_profile_game_dir(id)?;
    if !game.try_exists().map_err(|e| e.to_string())? {
        return Ok(true);
    }
    let root = managed_fs::root(&game)?;
    if managed_fs::inspect(&root, Path::new("saves"))?.is_none() {
        return Ok(true);
    }
    let saves = managed_fs::directory(&root, Path::new("saves"), false)?;
    Ok(saves.entries().map_err(|e| e.to_string())?.next().is_none())
}
pub(super) fn authorize(
    paths: &LauncherPaths,
    source: &Intent,
    revision: &ModRevision,
) -> Result<bool, String> {
    if source.profile_id != revision.profile_id {
        return Err("snapshot_policy_scope_mismatch".into());
    }
    if !source.independent || !can_advance(paths, &source.profile_id)? {
        return Ok(false);
    }
    let resolved = mod_intent::for_revision(&paths.root(), revision)?
        .ok_or("snapshot_policy_resolution_missing")?;
    let omitted = omissions(source, &resolved.intent)?;
    if omitted.is_empty() || !revision.requires_decision || revision.target.loader != "fabric" {
        return Err("snapshot_policy_invalid_candidate".into());
    }
    let record = Authorization {
        policy: "snapshot-freshness-v1".into(),
        profile_id: source.profile_id.clone(),
        revision_id: revision.id.clone(),
        source: source.clone(),
        reduced: resolved.intent.fingerprint()?,
        omitted,
    };
    let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("snapshot_policy_too_large".into());
    }
    managed_fs::write(
        &dir(paths)?,
        Path::new(&format!("{}.json", revision.id)),
        &bytes,
        true,
    )?;
    Ok(true)
}
fn read(paths: &LauncherPaths, revision: &ModRevision) -> Result<Option<Authorization>, String> {
    let directory = dir(paths)?;
    let name = format!("{}.json", revision.id);
    if managed_fs::inspect(&directory, Path::new(&name))?.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(&directory, Path::new(&name))?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("snapshot_policy_too_large".into());
    }
    let record: Authorization =
        serde_json::from_slice(&bytes).map_err(|_| "snapshot_policy_invalid")?;
    let resolved = mod_intent::for_revision(&paths.root(), revision)?
        .ok_or("snapshot_policy_resolution_missing")?;
    if record.policy != "snapshot-freshness-v1"
        || record.profile_id != revision.profile_id
        || record.revision_id != revision.id
        || profile::smart_key_from_ref(&record.profile_id) != Some("snapshot-plus")
        || !record.source.independent
        || !revision.requires_decision
        || revision.target.loader != "fabric"
        || record.reduced != resolved.intent.fingerprint()?
        || record.omitted.is_empty()
        || omissions(&record.source, &resolved.intent)? != record.omitted
    {
        return Err("snapshot_policy_scope_mismatch".into());
    }
    Ok(Some(record))
}
pub(super) fn check(paths: &LauncherPaths, revision: &ModRevision) -> Result<bool, String> {
    let Some(record) = read(paths, revision)? else {
        return Ok(false);
    };
    let current = mod_intent::load(&paths.root(), &revision.profile_id)?
        .ok_or("snapshot_policy_wishes_missing")?;
    if current.fingerprint()? != record.source.fingerprint()? {
        return Err("snapshot_policy_wishes_changed".into());
    }
    Ok(true)
}
pub(super) fn original(
    paths: &LauncherPaths,
    revision: &ModRevision,
) -> Result<Option<Intent>, String> {
    Ok(read(paths, revision)?.map(|v| v.source))
}
pub(super) fn pending(
    paths: &LauncherPaths,
    revision: &ModRevision,
) -> Result<BTreeSet<String>, String> {
    Ok(read(paths, revision)?
        .map(|v| v.omitted.into_iter().collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::mod_repository::CapturePolicy;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    #[test]
    fn automatic_permission_is_bound_to_snapshot_wishes_and_exact_revision() -> TestResult {
        let temp = tempfile::tempdir()?;
        let paths = LauncherPaths::new(temp.path().canonicalize()?);
        paths.ensure_dirs()?;
        let id = "smart:snapshot-plus";
        let game = paths.checked_profile_game_dir(id)?;
        std::fs::create_dir_all(game.join("mods"))?;
        let mut source = Intent::empty(id);
        source.independent = true;
        source
            .requirements
            .push(crate::core::mod_intent::Requirement::project(
                "render",
                true,
                Some("Renderer".into()),
                None,
                "user",
            ));
        mod_intent::save(&paths.root(), &source)?;
        let effective = preparation::without(&source, &["modrinth:render".into()])?;
        let repo = ModRepository::new(&paths.root());
        let revision = repo.capture_config(
            id,
            &game,
            ModTarget {
                mc_version: "1.21.1".into(),
                loader: "fabric".into(),
                loader_version: None,
            },
            None,
            CapturePolicy {
                intent_id: Some(&effective.fingerprint()?),
                requires_decision: true,
            },
        )?;
        requests::record_resolution(&paths, &revision, effective, Vec::new())?;
        assert!(!check(&paths, &revision)?);
        assert!(authorize(&paths, &source, &revision)?);
        assert!(check(&paths, &revision)?);
        assert_eq!(
            pending(&paths, &revision)?,
            BTreeSet::from(["modrinth:render".into()])
        );
        let mut other = revision.clone();
        other.profile_id = "smart:latest-plus".into();
        assert!(check(&paths, &other).is_err());
        let mut changed = source.clone();
        changed.advance_choice_sequence()?;
        mod_intent::save(&paths.root(), &changed)?;
        assert!(check(&paths, &revision).is_err());
        mod_intent::save(&paths.root(), &source)?;
        managed_fs::write(
            &dir(&paths)?,
            Path::new(&format!("{}.json", revision.id)),
            b"{",
            true,
        )?;
        assert!(check(&paths, &revision).is_err());
        Ok(())
    }
    #[test]
    fn existing_worlds_and_other_environments_never_receive_automatic_permission() -> TestResult {
        let temp = tempfile::tempdir()?;
        let paths = LauncherPaths::new(temp.path().canonicalize()?);
        paths.ensure_dirs()?;
        assert!(can_advance(&paths, "smart:snapshot-plus")?);
        assert!(!can_advance(&paths, "smart:latest-plus")?);
        let saves = paths
            .checked_profile_game_dir("smart:snapshot-plus")?
            .join("saves");
        std::fs::create_dir_all(&saves)?;
        std::fs::write(saves.join("world"), b"user-asset")?;
        assert!(!can_advance(&paths, "smart:snapshot-plus")?);
        assert_eq!(std::fs::read(saves.join("world"))?, b"user-asset");
        Ok(())
    }
}
