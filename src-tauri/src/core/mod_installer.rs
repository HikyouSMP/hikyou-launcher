use crate::core::{
    mod_files::{ModFile, ModMetaEntry, is_safe_filename, load_meta, mods_dir, save_meta},
    mod_recommendations::{ModrinthProject, fetch_projects},
};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

#[derive(Clone)]
pub(super) struct PendingModInstall {
    pub(super) project_id: String,
    pub(super) filename: String,
    pub(super) bytes: Option<Vec<u8>>,
    pub(super) size_bytes: u64,
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
    let dir = mods_dir(game_dir);
    let client = reqwest::Client::builder()
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let mut installed: Vec<ModFile> = Vec::new();
    let mut written_files: Vec<std::path::PathBuf> = Vec::new();

    for pending in &plan.installs {
        let dest = dir.join(&pending.filename);
        remove_previous_project_files(game_dir, &pending.project_id, &pending.filename).await;

        if let Some(bytes) = &pending.bytes {
            if let Err(error) = fs::write(&dest, bytes).await {
                for written in written_files {
                    let _ = fs::remove_file(written).await;
                }
                return Err(format!("failed to save: {}", error));
            }
            written_files.push(dest.clone());
            log::info!(
                "[mods] Install complete: {} -> {:?}",
                pending.filename,
                dest
            );
        }

        if dest.exists() {
            installed.push(ModFile {
                filename: pending.filename.clone(),
                size_bytes: pending.size_bytes,
                display_name: None,
                icon_url: None,
            });
        }
    }

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

    let mut meta = load_meta(game_dir).await;

    for pending in &plan.installs {
        if pending.project_id.as_str() == project_id {
            if let Some(ref name) = display_name {
                meta.insert(
                    pending.filename.clone(),
                    ModMetaEntry {
                        display_name: name.clone(),
                        icon_url: icon_url.clone(),
                        project_id: pending.project_id.clone(),
                    },
                );
            }
        } else if let Some(proj) = dep_map.get(&pending.project_id) {
            meta.insert(
                pending.filename.clone(),
                ModMetaEntry {
                    display_name: proj.title.clone(),
                    icon_url: proj.icon_url.clone(),
                    project_id: pending.project_id.clone(),
                },
            );
        }
    }

    save_meta(game_dir, &meta).await;

    for file in &mut installed {
        if let Some(entry) = meta.get(&file.filename) {
            file.display_name = Some(entry.display_name.clone());
            file.icon_url = entry.icon_url.clone();
        }
    }

    Ok(installed)
}

async fn remove_previous_project_files(game_dir: &Path, project_id: &str, keep_filename: &str) {
    let mut meta = load_meta(game_dir).await;
    let stale_files: Vec<String> = meta
        .iter()
        .filter_map(|(filename, entry)| {
            if entry.project_id == project_id && filename != keep_filename {
                Some(filename.clone())
            } else {
                None
            }
        })
        .collect();

    if stale_files.is_empty() {
        return;
    }

    let dir = mods_dir(game_dir);
    for filename in stale_files {
        if !is_safe_filename(&filename) {
            meta.remove(&filename);
            continue;
        }
        let path = dir.join(&filename);
        match fs::remove_file(&path).await {
            Ok(_) => {
                log::info!(
                    "[mods] Removed stale project file for {}: {}",
                    project_id,
                    filename
                );
                meta.remove(&filename);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                meta.remove(&filename);
            }
            Err(e) => {
                log::warn!(
                    "[mods] Failed to remove stale project file {}: {}",
                    filename,
                    e
                );
            }
        }
    }

    save_meta(game_dir, &meta).await;
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
    let dir = mods_dir(game_dir);

    for filename in files {
        let path = dir.join(&filename);
        match fs::remove_file(&path).await {
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

    let path = dir.join(filename);
    let disabled_name = format!("{}.disabled", filename);
    let disabled_path = dir.join(&disabled_name);
    match fs::rename(&path, &disabled_path).await {
        Ok(_) => {
            if let Some(entry) = meta.remove(filename) {
                meta.insert(disabled_name, entry);
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&path).await;
            meta.remove(filename);
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
