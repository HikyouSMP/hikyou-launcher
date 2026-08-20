use std::sync::Arc;

use crate::{
    LauncherPaths, core,
    core::mod_sources::{ModInstallRequest, ModSearchQuery, ModSource, ModpackInstallRequest},
};

#[tauri::command]
pub async fn get_profile_mods(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::ModFile>, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    Ok(core::mods::list_mods(&game_dir).await)
}

#[tauri::command]
pub async fn remove_profile_mod(
    profile_id: String,
    filename: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    core::mods::remove_mod(&game_dir, &filename).await
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
pub async fn install_modrinth_mod(
    profile_id: String,
    project_id: String,
    mc_version: String,
    loader: String,
    display_name: Option<String>,
    icon_url: Option<String>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::ModFile>, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    core::mod_sources::install_mod(
        ModSource::Modrinth,
        ModInstallRequest {
            game_dir: &game_dir,
            project_id: &project_id,
            mc_version: &mc_version,
            loader: &loader,
            display_name,
            icon_url,
        },
    )
    .await
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
        .await;
    Ok(())
}

#[tauri::command]
pub async fn toggle_profile_mod(
    profile_id: String,
    filename: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::mods::ModFile, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    core::mods::toggle_mod(&game_dir, &filename).await
}

#[tauri::command]
pub async fn backfill_mod_metadata(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::ModFile>, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    Ok(core::mods::backfill_metadata(&game_dir).await)
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
pub fn get_auto_mods(paths: tauri::State<Arc<LauncherPaths>>) -> Vec<core::mods::AutoMod> {
    core::mods::load_auto_mods(&paths.auto_mods_file())
}

#[tauri::command]
pub fn save_auto_mods(
    mods: Vec<core::mods::AutoMod>,
    paths: tauri::State<Arc<LauncherPaths>>,
) -> Result<(), String> {
    core::mods::save_auto_mods_to_file(&paths.auto_mods_file(), &mods)
}

#[tauri::command]
pub async fn init_auto_mods(
    gpu_vendor: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::mods::AutoMod>, String> {
    Ok(core::mods::init_auto_mods(&paths.auto_mods_file(), &gpu_vendor).await)
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
pub async fn install_modrinth_modpack(
    profile_id: String,
    project_id: String,
    mc_version: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::mods::ModpackInstallResult, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    core::mod_sources::install_modpack(
        ModSource::Modrinth,
        ModpackInstallRequest {
            game_dir: &game_dir,
            project_id: &project_id,
            mc_version: &mc_version,
            version_id: None,
        },
    )
    .await
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

    Ok(serde_json::json!({
        "profile_id": profile_id,
        "name": result.profile_name,
        "mc_version": result.mc_version,
        "loader": result.loader,
        "mod_count": result.mods.len(),
    }))
}
