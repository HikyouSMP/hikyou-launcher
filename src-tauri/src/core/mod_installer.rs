use crate::core::{
    managed_fs,
    mod_files::{
        ModFile, ModMeta, ModMetaEntry, inspect_mod_path, is_safe_filename, load_meta,
        load_meta_checked_in, managed_mods, open_mods, read_mod_file, save_meta,
    },
    mod_recommendations::{ModrinthProject, fetch_projects},
    modrinth_provider::verify_mod_bytes,
};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone)]
pub(super) struct PendingModInstall {
    pub(super) project_id: String,
    pub(super) filename: String,
    pub(super) bytes: Option<Vec<u8>>,
    pub(super) size_bytes: u64,
    pub(super) version_id: Option<String>,
    pub(super) version_number: Option<String>,
    pub(super) hashes: HashMap<String, String>,
    pub(super) advertised_size: Option<u64>,
    pub(super) game_versions: Vec<String>,
    pub(super) loaders: Vec<String>,
}

#[derive(Clone)]
pub(super) struct ModInstallPlan {
    pub(super) installs: Vec<PendingModInstall>,
}

pub(super) async fn commit_mod_install_plan(
    game_dir: &Path,
    project_id: &str,
    display_name: Option<String>,
    icon_url: Option<String>,
    plan: ModInstallPlan,
) -> Result<Vec<ModFile>, String> {
    let dir = open_mods(game_dir, true)?;
    let client = crate::http::builder()?
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let dep_ids: Vec<String> = plan
        .installs
        .iter()
        .filter(|pending| pending.project_id.as_str() != project_id)
        .map(|pending| pending.project_id.clone())
        .collect();

    let dep_projects = fetch_projects(&client, &dep_ids).await;
    let dep_map: HashMap<String, ModrinthProject> = dep_projects
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();

    let mut meta = load_meta_checked_in(&dir)?;

    for pending in &plan.installs {
        let previous = meta.get(&pending.filename);
        let project = dep_map.get(&pending.project_id);
        let name = if pending.project_id == project_id {
            display_name.clone()
        } else {
            None
        }
        .or_else(|| project.map(|p| p.title.clone()))
        .or_else(|| previous.map(|p| p.display_name.clone()))
        .unwrap_or_else(|| pending.project_id.clone());
        let icon = if pending.project_id == project_id {
            icon_url.clone()
        } else {
            None
        }
        .or_else(|| project.and_then(|p| p.icon_url.clone()))
        .or_else(|| previous.and_then(|p| p.icon_url.clone()));
        meta.insert(
            pending.filename.clone(),
            ModMetaEntry {
                display_name: name,
                icon_url: icon,
                project_id: pending.project_id.clone(),
                version_id: pending.version_id.clone(),
                version_number: pending.version_number.clone(),
                hashes: (!pending.hashes.is_empty()).then(|| pending.hashes.clone()),
                game_versions: pending.game_versions.clone(),
                loaders: pending.loaders.clone(),
            },
        );
    }

    commit_plan_in(&dir, &plan, meta)
}

fn commit_plan_in(
    dir: &cap_std::fs::Dir,
    plan: &ModInstallPlan,
    meta: ModMeta,
) -> Result<Vec<ModFile>, String> {
    commit_plan_in_with_hook(dir, plan, meta, |_| Ok(()))
}

fn commit_plan_in_with_hook(
    dir: &cap_std::fs::Dir,
    plan: &ModInstallPlan,
    mut meta: ModMeta,
    mut after_install: impl FnMut(usize) -> Result<(), String>,
) -> Result<Vec<ModFile>, String> {
    let old_meta = load_meta_checked_in(dir)?;
    let mut affected = std::collections::BTreeSet::new();
    let mut names = std::collections::HashSet::new();
    let mut projects = std::collections::HashSet::new();
    for pending in &plan.installs {
        if !is_safe_filename(&pending.filename)
            || !pending.filename.ends_with(".jar")
            || !names.insert(pending.filename.clone())
            || !projects.insert(pending.project_id.clone())
        {
            return Err("invalid or duplicate mod install filename".to_string());
        }
        affected.insert(pending.filename.clone());
        if old_meta
            .get(&pending.filename)
            .is_some_and(|entry| entry.project_id != pending.project_id)
        {
            return Err("mod destination belongs to a different project".to_string());
        }
        for (name, entry) in &old_meta {
            if entry.project_id == pending.project_id && name != &pending.filename {
                if !is_safe_filename(name)
                    || !(name.ends_with(".jar") || name.ends_with(".jar.disabled"))
                {
                    return Err("invalid stale mod filename".to_string());
                }
                affected.insert(name.clone());
                meta.remove(name);
            }
        }
    }
    affected.insert(".mod_meta.json".to_string());
    // Inspect every destination before moving anything, including stale files.
    for name in &affected {
        if inspect_mod_path(dir, Path::new(name))?.is_some() {
            read_mod_file(dir, Path::new(name))?;
        }
    }
    let transaction = format!(".mod-install-{}", uuid::Uuid::new_v4());
    dir.create_dir(&transaction).map_err(|e| e.to_string())?;
    inspect_mod_path(dir, Path::new(&transaction))?;
    let stage = managed_fs::directory(dir, Path::new(&transaction), false)?;
    let mut backups = Vec::<(String, String)>::new();
    let mut written = Vec::<String>::new();
    let result = (|| {
        for (index, pending) in plan.installs.iter().enumerate() {
            let cached;
            let bytes = match &pending.bytes {
                Some(bytes) => bytes.as_slice(),
                None => {
                    cached = super::mod_files::read_bounded(
                        read_mod_file(dir, Path::new(&pending.filename))?,
                        super::mod_files::MAX_MOD_BYTES,
                    )?;
                    &cached
                }
            };
            if bytes.len() as u64 > super::mod_files::MAX_MOD_BYTES {
                return Err("prepared mod exceeds supported size limit".to_string());
            }
            verify_mod_bytes(bytes, &pending.hashes, pending.advertised_size)?;
            if bytes.len() as u64 != pending.size_bytes {
                return Err("prepared mod size changed".to_string());
            }
            managed_fs::write(&stage, Path::new(&format!("new-{index}")), bytes, false)?;
        }
        let metadata = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;
        managed_fs::write(&stage, Path::new("new-meta"), &metadata, false)?;
        for (index, name) in affected.iter().enumerate() {
            if inspect_mod_path(dir, Path::new(name))?.is_some() {
                read_mod_file(dir, Path::new(name))?;
                let backup = format!("old-{index}");
                dir.rename(name, &stage, &backup)
                    .map_err(|e| e.to_string())?;
                backups.push((name.clone(), backup));
            }
        }
        for (index, pending) in plan.installs.iter().enumerate() {
            let bytes = super::mod_files::read_bounded(
                managed_fs::read_file(&stage, Path::new(&format!("new-{index}")))?,
                super::mod_files::MAX_MOD_BYTES,
            )?;
            managed_fs::write(dir, Path::new(&pending.filename), &bytes, false)?;
            written.push(pending.filename.clone());
            after_install(index)?;
        }
        managed_fs::write(dir, Path::new(".mod_meta.json"), &metadata, false)?;
        written.push(".mod_meta.json".to_string());
        Ok(())
    })();
    if let Err(error) = result {
        let mut rollback_errors = Vec::new();
        for name in written.iter().rev() {
            if let Err(e) = dir.remove_file(name) {
                rollback_errors.push(e.to_string());
            }
        }
        for (name, backup) in backups.iter().rev() {
            // Never overwrite a concurrent destination while restoring a backup.
            match inspect_mod_path(dir, Path::new(name)) {
                Ok(None) => {
                    if let Err(e) = stage.rename(backup, dir, name) {
                        rollback_errors.push(e.to_string());
                    }
                }
                _ => rollback_errors.push(format!("rollback destination unavailable: {name}")),
            }
        }
        if !rollback_errors.is_empty() {
            return Err(format!(
                "{error}; rollback failed (backups retained in {transaction}): {}",
                rollback_errors.join("; ")
            ));
        }
        drop(stage);
        let _ = dir.remove_dir_all(&transaction);
        return Err(error);
    }
    drop(stage);
    if let Err(error) = dir.remove_dir_all(&transaction) {
        log::warn!("Mod transaction cleanup failed: {error}");
    }
    Ok(plan
        .installs
        .iter()
        .map(|pending| {
            let entry = meta.get(&pending.filename);
            ModFile {
                project_id: Some(pending.project_id.clone()),
                filename: pending.filename.clone(),
                size_bytes: pending.size_bytes,
                display_name: entry.map(|e| e.display_name.clone()),
                icon_url: entry.and_then(|e| e.icon_url.clone()),
                version_id: pending.version_id.clone(),
                version_number: pending.version_number.clone(),
                hashes: (!pending.hashes.is_empty()).then(|| pending.hashes.clone()),
            }
        })
        .collect())
}

pub(super) async fn remove_project_files(game_dir: &Path, project_id: &str) {
    let mut meta = load_meta(game_dir).await;
    let files: Vec<String> = meta
        .iter()
        .filter_map(|(filename, entry)| {
            if entry.project_id == project_id && is_safe_filename(filename) {
                Some(filename.clone())
            } else {
                None
            }
        })
        .collect();
    let Ok(dir) = managed_mods(game_dir) else {
        return;
    };

    for filename in files {
        if inspect_mod_path(&dir, Path::new(&filename)).is_err() {
            continue;
        }
        match dir.remove_file(&filename) {
            Ok(_) => {
                meta.remove(&filename);
                log::info!(
                    "[mods] Removed auto mod file for dependency re-resolution: {}",
                    filename
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                meta.remove(&filename);
            }
            Err(e) => {
                log::warn!(
                    "[mods] Failed to remove auto mod file {} for re-resolution: {}",
                    filename,
                    e
                );
            }
        }
    }

    save_meta(game_dir, &meta).await;
}

pub(super) async fn disable_mod_file(
    dir: &Path,
    meta: &mut HashMap<String, ModMetaEntry>,
    filename: &str,
) -> bool {
    if !is_safe_filename(filename) {
        meta.remove(filename);
        return false;
    }

    let Some(game_dir) = dir.parent() else {
        return false;
    };
    let Ok(dir) = managed_mods(game_dir) else {
        return false;
    };
    let disabled_name = format!("{}.disabled", filename);
    if !matches!(inspect_mod_path(&dir, Path::new(filename)), Ok(Some(m)) if m.is_file())
        || !matches!(inspect_mod_path(&dir, Path::new(&disabled_name)), Ok(None))
    {
        return false;
    }
    match dir.rename(filename, &dir, &disabled_name) {
        Ok(_) => {
            if let Some(entry) = meta.remove(filename) {
                meta.insert(disabled_name, entry);
            }
            true
        }
        Err(e) => {
            log::warn!(
                "[mods] Failed to disable incompatible mod {}: {}",
                filename,
                e
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(filename: &str) -> PendingModInstall {
        PendingModInstall {
            project_id: "project".to_string(),
            filename: filename.to_string(),
            bytes: Some(b"replacement".to_vec()),
            size_bytes: 11,
            version_id: Some("provider-version-id".to_string()),
            version_number: Some("2.3.4".to_string()),
            hashes: HashMap::from([(
                "sha1".to_string(),
                sha1_smol::Sha1::from(b"replacement".as_slice())
                    .digest()
                    .to_string(),
            )]),
            advertised_size: Some(11),
            game_versions: vec!["1.21.6".to_string()],
            loaders: vec!["fabric".to_string()],
        }
    }

    fn entry() -> ModMetaEntry {
        ModMetaEntry {
            project_id: "project".to_string(),
            display_name: "Name".to_string(),
            icon_url: None,
            version_id: Some("old-id".to_string()),
            version_number: Some("1.0.0".to_string()),
            hashes: None,
            game_versions: Vec::new(),
            loaders: Vec::new(),
        }
    }

    #[test]
    fn hash_failure_does_not_remove_old_install() {
        let temp = tempfile::tempdir().unwrap();
        let dir = managed_fs::root(temp.path()).unwrap();
        managed_fs::write(&dir, Path::new("old.jar"), b"old", false).unwrap();
        let meta = HashMap::from([("old.jar".to_string(), entry())]);
        let metadata = serde_json::to_vec(&meta).unwrap();
        managed_fs::write(&dir, Path::new(".mod_meta.json"), &metadata, false).unwrap();
        let mut install = pending("new.jar");
        install.hashes.insert("sha1".to_string(), "00".repeat(20));
        assert!(
            commit_plan_in(
                &dir,
                &ModInstallPlan {
                    installs: vec![install]
                },
                meta
            )
            .is_err()
        );
        assert_eq!(std::fs::read(temp.path().join("old.jar")).unwrap(), b"old");
        assert_eq!(
            std::fs::read(temp.path().join(".mod_meta.json")).unwrap(),
            metadata
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
    }

    #[test]
    fn commit_error_rolls_back_replaced_and_new_files_and_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let dir = managed_fs::root(temp.path()).unwrap();
        managed_fs::write(&dir, Path::new("same.jar"), b"old", false).unwrap();
        managed_fs::write(&dir, Path::new("stale.jar"), b"stale", false).unwrap();
        let meta = HashMap::from([
            ("same.jar".to_string(), entry()),
            ("stale.jar".to_string(), entry()),
        ]);
        let metadata = serde_json::to_vec(&meta).unwrap();
        managed_fs::write(&dir, Path::new(".mod_meta.json"), &metadata, false).unwrap();
        let mut second = pending("new.jar");
        second.project_id = "dependency".to_string();
        let plan = ModInstallPlan {
            installs: vec![pending("same.jar"), second],
        };
        let result = commit_plan_in_with_hook(&dir, &plan, meta, |index| {
            if index == 1 {
                Err("injected commit failure".to_string())
            } else {
                Ok(())
            }
        });
        assert!(result.unwrap_err().contains("injected commit failure"));
        assert_eq!(std::fs::read(temp.path().join("same.jar")).unwrap(), b"old");
        assert_eq!(
            std::fs::read(temp.path().join("stale.jar")).unwrap(),
            b"stale"
        );
        assert_eq!(
            std::fs::read(temp.path().join(".mod_meta.json")).unwrap(),
            metadata
        );
        assert!(!temp.path().join("new.jar").exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 3);
    }

    #[test]
    fn display_preserves_remote_version_not_filename() {
        let temp = tempfile::tempdir().unwrap();
        let dir = managed_fs::root(temp.path()).unwrap();
        let install = pending("unrelated-filename.jar");
        let mut metadata = entry();
        metadata.version_id = install.version_id.clone();
        metadata.version_number = install.version_number.clone();
        metadata.hashes = Some(install.hashes.clone());
        let meta = HashMap::from([(install.filename.clone(), metadata)]);
        let files = commit_plan_in(
            &dir,
            &ModInstallPlan {
                installs: vec![install],
            },
            meta,
        )
        .unwrap();
        assert_eq!(files[0].version_id.as_deref(), Some("provider-version-id"));
        assert_eq!(files[0].version_number.as_deref(), Some("2.3.4"));
        let cached = load_meta_checked_in(&dir).unwrap();
        assert_eq!(
            cached["unrelated-filename.jar"].version_number.as_deref(),
            Some("2.3.4")
        );
        assert_eq!(files[0].hashes, cached["unrelated-filename.jar"].hashes);
    }

    #[test]
    fn installer_rejects_linked_mod_directory() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&game).unwrap();
        std::fs::create_dir(&outside).unwrap();
        crate::core::test_support::directory_link(&outside, &game.join("mods"));
        assert!(open_mods(&game, true).is_err());
        assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
    }
}
