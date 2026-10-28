use std::{collections::BTreeMap, sync::Arc};

use serde::Serialize;

use crate::{
    LauncherPaths,
    core::{
        self,
        mod_repository::{ModEntry, ModRepository, ModRevision, ModTarget},
    },
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRevisionChange {
    before: ModEntry,
    after: ModEntry,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRevisionDiff {
    from_id: String,
    to_id: String,
    from_target: ModTarget,
    to_target: ModTarget,
    added: Vec<ModEntry>,
    removed: Vec<ModEntry>,
    changed: Vec<ModRevisionChange>,
}

fn check_owner(profile_id: &str, revision: &ModRevision) -> Result<(), String> {
    if revision.profile_id != profile_id {
        return Err("revision does not belong to this profile".to_string());
    }
    Ok(())
}

fn entry_key(entry: &ModEntry) -> String {
    if entry.filename.to_ascii_lowercase().ends_with(".disabled") {
        entry.filename[..entry.filename.len() - ".disabled".len()].to_string()
    } else {
        entry.filename.clone()
    }
}

fn identity(entry: &ModEntry) -> String {
    entry
        .project_id
        .as_ref()
        .map(|id| format!("project:{id}"))
        .or_else(|| entry.mod_id.as_ref().map(|id| format!("mod:{id}")))
        .unwrap_or_else(|| format!("filename:{}", entry_key(entry)))
}

fn revision_diff(from: ModRevision, to: ModRevision) -> Result<ModRevisionDiff, String> {
    let mut before: BTreeMap<_, _> = from
        .mods
        .into_iter()
        .map(|entry| (entry.filename.clone(), entry))
        .collect();
    let mut after: BTreeMap<_, _> = to
        .mods
        .into_iter()
        .map(|entry| (entry.filename.clone(), entry))
        .collect();
    let mut pairs = Vec::new();
    for key in before.keys().cloned().collect::<Vec<_>>() {
        if let Some(next) = after.remove(&key) {
            pairs.push((before.remove(&key).unwrap(), next));
        }
    }
    // A version update can rename the jar. Pair only unambiguous stable identities.
    for key in before.keys().cloned().collect::<Vec<_>>() {
        let id = identity(&before[&key]);
        if before
            .values()
            .filter(|entry| identity(entry) == id)
            .count()
            != 1
        {
            continue;
        }
        let matches: Vec<_> = after
            .iter()
            .filter(|(_, entry)| identity(entry) == id)
            .map(|(key, _)| key.clone())
            .collect();
        if matches.len() == 1 {
            pairs.push((
                before.remove(&key).unwrap(),
                after.remove(&matches[0]).unwrap(),
            ));
        }
    }
    let mut changed = Vec::new();
    for (before, after) in pairs {
        if before != after {
            changed.push(ModRevisionChange { before, after });
        }
    }
    changed.sort_by(|a, b| a.after.filename.cmp(&b.after.filename));
    Ok(ModRevisionDiff {
        from_id: from.id,
        to_id: to.id,
        from_target: from.target,
        to_target: to.target,
        added: after.into_values().collect(),
        removed: before.into_values().collect(),
        changed,
    })
}

#[tauri::command]
pub async fn get_profile_mod_history(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<ModRevision>, String> {
    core::profile::validate_profile_ref(&profile_id)?;
    let root = paths.root();
    tauri::async_runtime::spawn_blocking(move || {
        let history = ModRepository::new(&root).history(&profile_id)?;
        for revision in &history {
            check_owner(&profile_id, revision)?;
        }
        Ok(history)
    })
    .await
    .map_err(|_| "mod history task failed".to_string())?
}

#[tauri::command]
pub async fn get_profile_mod_revision_diff(
    profile_id: String,
    from_id: String,
    to_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<ModRevisionDiff, String> {
    core::profile::validate_profile_ref(&profile_id)?;
    let root = paths.root();
    tauri::async_runtime::spawn_blocking(move || {
        let repository = ModRepository::new(&root);
        let from = repository.load(&from_id)?;
        let to = repository.load(&to_id)?;
        check_owner(&profile_id, &from)?;
        check_owner(&profile_id, &to)?;
        revision_diff(from, to)
    })
    .await
    .map_err(|_| "mod diff task failed".to_string())?
}

#[tauri::command]
pub async fn restore_profile_mod_revision(
    profile_id: String,
    revision_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<ModRevision, String> {
    core::profile::validate_profile_ref(&profile_id)?;
    core::mod_runtime::restore(paths.inner().as_ref(), &profile_id, &revision_id).await
}

#[tauri::command]
pub async fn resume_profile_mod_updates(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<ModRevision, String> {
    core::profile::validate_profile_ref(&profile_id)?;
    if !core::profile::is_smart_profile_ref(&profile_id) {
        return Err("automatic mod updates require a smart profile".to_string());
    }
    core::mod_runtime::resume(paths.inner().as_ref(), &profile_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(id: &str, mods: serde_json::Value) -> ModRevision {
        serde_json::from_value(serde_json::json!({
            "id": id, "profileId": "smart:latest-plus", "parent": null,
            "target": { "mcVersion": "1.21.1", "loader": "fabric", "loaderVersion": "0.16.10" },
            "createdAt": "2026-10-26T00:00:00Z", "mods": mods,
        }))
        .unwrap()
    }

    fn entry(name: &str, object: &str, enabled: bool) -> serde_json::Value {
        serde_json::json!({ "filename": name, "objectId": object, "sizeBytes": 42, "enabled": enabled })
    }

    #[test]
    fn diff_is_sorted_and_tracks_hash_version_and_enabled_state() {
        let mut old = entry("sodium-1.jar", "a".repeat(64).as_str(), true);
        old["projectId"] = "sodium".into();
        old["version"] = "1.0.0".into();
        let mut new = entry("sodium-2.jar.disabled", "b".repeat(64).as_str(), false);
        new["projectId"] = "sodium".into();
        new["version"] = "2.0.0".into();
        let from = revision(
            "from",
            serde_json::json!([entry("removed.jar", "c", true), old]),
        );
        let to = revision(
            "to",
            serde_json::json!([entry("z.jar", "z", true), new, entry("a.jar", "a", true)]),
        );
        let diff = revision_diff(from, to).unwrap();
        assert_eq!(
            diff.added
                .iter()
                .map(|entry| entry.filename.as_str())
                .collect::<Vec<_>>(),
            ["a.jar", "z.jar"]
        );
        assert_eq!(diff.removed[0].filename, "removed.jar");
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].before.version.as_deref(), Some("1.0.0"));
        assert_eq!(diff.changed[0].after.version.as_deref(), Some("2.0.0"));
        assert!(!diff.changed[0].after.enabled);
        assert_ne!(
            diff.changed[0].before.object_id,
            diff.changed[0].after.object_id
        );
    }

    #[test]
    fn rejects_cross_profile_revision() {
        let rev = revision("id", serde_json::json!([]));
        assert!(check_owner("smart:snapshot-plus", &rev).is_err());
        assert!(check_owner("smart:latest-plus", &rev).is_ok());
    }

    #[test]
    fn enabled_disabled_pair_uses_same_object_and_no_entry_is_lost() {
        let from = revision("from", serde_json::json!([entry("same.jar", "a", true)]));
        let to = revision(
            "to",
            serde_json::json!([entry("same.jar.DISABLED", "a", false)]),
        );
        let diff = revision_diff(from, to).unwrap();
        assert_eq!(diff.changed.len(), 1);
        assert!(diff.added.is_empty() && diff.removed.is_empty());

        let from = revision(
            "from",
            serde_json::json!([
                entry("same.jar", "a", true),
                entry("same.jar.disabled", "b", false)
            ]),
        );
        let to = revision("to", serde_json::json!([]));
        assert_eq!(revision_diff(from, to).unwrap().removed.len(), 2);
    }

    #[test]
    fn ambiguous_identity_is_not_guessed_and_order_is_stable() {
        let mut a = entry("a.jar", "a", true);
        let mut b = entry("b.jar", "b", true);
        let mut c = entry("c.jar", "c", true);
        for entry in [&mut a, &mut b, &mut c] {
            entry["projectId"] = "shared".into();
        }
        let first = revision_diff(
            revision("from", serde_json::json!([b.clone(), a.clone()])),
            revision("to", serde_json::json!([c.clone()])),
        )
        .unwrap();
        let second = revision_diff(
            revision("from", serde_json::json!([a, b])),
            revision("to", serde_json::json!([c])),
        )
        .unwrap();
        assert_eq!(first.removed.len(), 2);
        assert_eq!(first.added.len(), 1);
        assert!(first.changed.is_empty());
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
    }

    #[test]
    fn provider_version_and_upstream_hash_changes_are_not_hidden() {
        let mut old = entry("same.jar", "a", true);
        old["version"] = "1.0.0".into();
        old["providerVersion"] = "release-old".into();
        let mut new = old.clone();
        new["providerVersion"] = "release-new".into();
        new["hashes"] = serde_json::json!({ "sha512": "upstream-digest" });
        let diff = revision_diff(
            revision("from", serde_json::json!([old])),
            revision("to", serde_json::json!([new])),
        )
        .unwrap();
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(
            diff.changed[0].after.provider_version.as_deref(),
            Some("release-new")
        );
    }
}
