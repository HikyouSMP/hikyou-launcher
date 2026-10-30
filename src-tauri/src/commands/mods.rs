use std::sync::Arc;
use tauri::Emitter;

use crate::{
    LauncherPaths, core,
    core::mod_sources::{ModSearchQuery, ModSource, ModpackInstallRequest},
};

async fn active_mods(
    paths: &Arc<LauncherPaths>,
    id: &str,
) -> Result<Vec<core::mods::ModFile>, String> {
    let root = paths.root();
    let profile_id = id.to_string();
    let snapshot = tokio::task::spawn_blocking(move || {
        let repo = core::mod_repository::ModRepository::new(&root);
        let Some(revision) = repo.head(&profile_id)? else {
            return Ok(None);
        };
        repo.quick_check(&revision)?;
        Ok::<_, String>(Some(
            revision
                .mods
                .into_iter()
                .map(|entry| core::mods::ModFile {
                    project_id: entry.project_id,
                    filename: entry.filename,
                    size_bytes: entry.size_bytes,
                    display_name: entry.display_name,
                    icon_url: entry.icon_url,
                    version_id: entry.provider_version,
                    version_number: entry.provider_version_number.or(entry.version),
                    hashes: (!entry.hashes.is_empty()).then(|| entry.hashes.into_iter().collect()),
                })
                .collect(),
        ))
    })
    .await
    .map_err(|e| e.to_string())??;
    match snapshot {
        Some(mods) => Ok(mods),
        None => Ok(core::mods::list_mods(&paths.checked_profile_game_dir(id)?).await),
    }
}

#[tauri::command]
pub async fn get_profile_mod_requests(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::mod_runtime::RequestedState, String> {
    let paths = paths.inner().clone();
    tokio::task::spawn_blocking(move || core::mod_runtime::requested_state(&paths, &profile_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_requested_mod(
    profile_id: String,
    requested_key: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    core::mod_runtime::remove_request(&paths, &profile_id, &requested_key).await?;
    let _ = app.emit(
        "mods://requests-changed",
        serde_json::json!({ "profileId": profile_id }),
    );
    Ok(())
}

#[tauri::command]
pub async fn get_profile_mods(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::ModFile>, String> {
    active_mods(&paths, &profile_id).await
}

#[tauri::command]
pub async fn remove_profile_mod(
    profile_id: String,
    filename: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    core::mod_runtime::change_installed_request(&paths, &profile_id, &filename, true).await?;
    let _ = app.emit(
        "mods://requests-changed",
        serde_json::json!({ "profileId": profile_id }),
    );
    Ok(())
}

#[tauri::command]
pub async fn search_modrinth(
    query: String,
    loader: String,
    mc_version: String,
) -> Result<Vec<core::mods::ModSearchResult>, String> {
    core::mod_sources::search_mods(
        ModSource::Modrinth,
        ModSearchQuery {
            query: &query,
            loader: &loader,
            mc_version: &mc_version,
        },
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn install_modrinth_mod(
    profile_id: String,
    project_id: String,
    mc_version: String,
    loader: String,
    display_name: Option<String>,
    icon_url: Option<String>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<Vec<core::mods::ModFile>, String> {
    // Target selection is backend-owned; old clients may still send display hints.
    let _ = (mc_version, loader);
    core::mod_runtime::request_project(&paths, &profile_id, &project_id, display_name, icon_url)
        .await?;
    let _ = app.emit(
        "mods://requests-changed",
        serde_json::json!({ "profileId": profile_id }),
    );
    active_mods(&paths, &profile_id).await
}

#[tauri::command]
pub async fn mark_profile_auto_mod_sync_current(
    profile_id: String,
    mc_version: String,
    loader: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    core::mods::mark_auto_mod_sync_current(&game_dir, paths.auto_mods_file(), &mc_version, &loader)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_profile_mod(
    profile_id: String,
    filename: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<core::mods::ModFile, String> {
    core::mod_runtime::change_installed_request(&paths, &profile_id, &filename, false).await?;
    let _ = app.emit(
        "mods://requests-changed",
        serde_json::json!({ "profileId": profile_id }),
    );
    let base = filename.strip_suffix(".disabled").unwrap_or(&filename);
    active_mods(&paths, &profile_id)
        .await?
        .into_iter()
        .find(|entry| {
            entry
                .filename
                .strip_suffix(".disabled")
                .unwrap_or(&entry.filename)
                == base
        })
        .ok_or_else(|| {
            "The requested Mod state is pending; the active configuration is unchanged".into()
        })
}

#[tauri::command]
pub async fn backfill_mod_metadata(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::ModFile>, String> {
    let _guard = core::mod_runtime::edit_guard(&paths, &profile_id).await?;
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    let _ = core::mods::backfill_metadata(&game_dir).await;
    if let Err(error) = core::mod_runtime::record(&paths, &profile_id).await {
        log::info!(
            "[mods] Metadata is available but the working configuration is not ready: {error}"
        );
    }
    active_mods(&paths, &profile_id).await
}

#[tauri::command]
pub async fn get_recommended_mods(loader: String) -> Vec<core::mods::RecommendedMod> {
    core::mods::get_recommended_mods(&loader).await
}

#[tauri::command]
pub async fn get_all_recommended_mods() -> Vec<core::mods::RecommendedMod> {
    core::mods::get_all_recommended_mods().await
}

#[tauri::command]
pub fn get_auto_mods(
    paths: tauri::State<Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::AutoMod>, String> {
    core::mods::load_auto_mods(&paths.auto_mods_file())
}

#[tauri::command]
pub async fn save_auto_mods(
    mods: Vec<core::mods::AutoMod>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    for profile_id in core::mod_runtime::recommendation_changes(&paths, Some(&mods)).await? {
        let _ = app.emit(
            "mods://requests-changed",
            serde_json::json!({ "profileId": profile_id }),
        );
    }
    core::mod_runtime::wake_preparation();
    Ok(())
}

#[tauri::command]
pub async fn init_auto_mods(
    gpu_vendor: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::AutoMod>, String> {
    let mods = core::mods::init_auto_mods(&paths.auto_mods_file(), &gpu_vendor).await?;
    core::mod_runtime::wake_preparation();
    Ok(mods)
}

#[tauri::command]
pub async fn search_modrinth_modpacks(
    query: String,
    mc_version: String,
) -> Result<Vec<core::mods::ModSearchResult>, String> {
    core::mod_sources::search_modpacks(ModSource::Modrinth, &query, &mc_version).await
}

#[tauri::command]
pub async fn get_modpack_versions(
    project_id: String,
) -> Result<Vec<core::mods::ModpackVersionInfo>, String> {
    core::mod_sources::get_modpack_versions(ModSource::Modrinth, &project_id).await
}

#[tauri::command]
pub async fn install_modpack_as_profile(
    project_id: String,
    mc_version: String,
    version_id: Option<String>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<serde_json::Value, String> {
    let profile_id = uuid::Uuid::new_v4().to_string();
    let game_dir = paths.profile_game_dir(&profile_id);

    let result = core::mod_sources::install_modpack(
        ModSource::Modrinth,
        ModpackInstallRequest {
            game_dir: &game_dir,
            project_id: &project_id,
            mc_version: &mc_version,
            version_id: version_id.as_deref(),
        },
    )
    .await?;

    let mut profile = core::profile::Profile::new(
        result.profile_name.clone(),
        result.mc_version.clone(),
        result.loader.clone(),
        result.loader_version.clone(),
    );
    profile.id = profile_id.clone();
    core::profile::save_profile(&paths.root(), &profile).await?;
    core::mod_runtime::record(&paths, &profile_id).await?;

    Ok(serde_json::json!({
        "profile_id": profile_id,
        "name": result.profile_name,
        "mc_version": result.mc_version,
        "loader": result.loader,
        "mod_count": result.mods.len(),
    }))
}
