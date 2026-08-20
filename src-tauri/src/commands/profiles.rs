use std::sync::Arc;

use crate::{LauncherPaths, auth, core};

#[tauri::command]
pub async fn list_profiles(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::profile::Profile>, String> {
    Ok(core::profile::list_profiles(&paths.root()).await)
}

#[tauri::command]
pub async fn create_profile(
    name: String,
    mc_version: String,
    loader: String,
    loader_version: Option<String>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::profile::Profile, String> {
    let profile = core::profile::Profile::new(name, mc_version, loader, loader_version);
    core::profile::save_profile(&paths.root(), &profile).await?;
    Ok(profile)
}

#[tauri::command]
pub async fn update_profile(
    id: String,
    name: Option<String>,
    memory_mb: Option<u32>,
    window_w: Option<u32>,
    window_h: Option<u32>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::profile::Profile, String> {
    core::profile::update_profile(&paths.root(), &id, name, memory_mb, window_w, window_h).await
}

#[tauri::command]
pub async fn delete_profile(
    id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    core::profile::delete_profile(&paths.root(), &id).await
}

#[tauri::command]
pub async fn copy_profile_options(
    source_profile_id: String,
    target_profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    if source_profile_id == target_profile_id {
        return Err("source and target profiles must be different".to_string());
    }

    let root = paths.root();
    core::profile::validate_profile_ref(&source_profile_id)?;
    core::profile::validate_profile_ref(&target_profile_id)?;

    let source_options =
        core::profile::profile_game_dir_for_ref(&root, &source_profile_id)?.join("options.txt");
    let target_game_dir = core::profile::profile_game_dir_for_ref(&root, &target_profile_id)?;
    let target_options = target_game_dir.join("options.txt");

    if !source_options.exists() {
        return Err("source profile does not have options.txt yet".to_string());
    }

    tokio::fs::create_dir_all(&target_game_dir)
        .await
        .map_err(|e| format!("failed to create target game directory: {}", e))?;
    tokio::fs::copy(&source_options, &target_options)
        .await
        .map_err(|e| format!("failed to copy options.txt: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn switch_account(uuid: String) -> Result<(), String> {
    let auth = auth::load_account_auth(uuid).await?;
    auth::save_auth(&auth).await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_account_auth_cmd(uuid: String) -> Result<(), String> {
    auth::delete_account_auth(uuid).await
}
