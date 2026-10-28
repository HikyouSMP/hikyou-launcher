use std::sync::Arc;

use crate::{LauncherPaths, core};

fn ensure_profile_idle(profile_id: &str) -> Result<(), String> {
    if core::running_processes::is_active(profile_id) {
        Err("profile is currently launching or running".to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn list_profile_content(
    profile_id: String,
    kind: core::profile_content::ContentKind,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::profile_content::ContentItem>, String> {
    core::profile_content::list(&paths.root(), &profile_id, kind).await
}

#[tauri::command]
pub async fn import_profile_content(
    profile_id: String,
    kind: core::profile_content::ContentKind,
    filename: String,
    bytes: Vec<u8>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    ensure_profile_idle(&profile_id)?;
    core::profile_content::import(&paths.root(), &profile_id, kind, &filename, bytes).await
}

#[tauri::command]
pub async fn remove_profile_content(
    profile_id: String,
    kind: core::profile_content::ContentKind,
    name: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    ensure_profile_idle(&profile_id)?;
    core::profile_content::remove(&paths.root(), &profile_id, kind, &name).await
}

#[tauri::command]
pub async fn list_worlds(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::worlds::WorldSummary>, String> {
    core::worlds::list(&paths.root(), &profile_id).await
}

#[tauri::command]
pub async fn import_world(
    profile_id: String,
    filename: String,
    bytes: Vec<u8>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<String, String> {
    ensure_profile_idle(&profile_id)?;
    core::worlds::import_zip(&paths.root(), &profile_id, &filename, bytes).await
}

#[tauri::command]
pub async fn list_datapacks(
    profile_id: String,
    world_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::datapacks::DatapackItem>, String> {
    core::datapacks::list(&paths.root(), &profile_id, &world_id).await
}

#[tauri::command]
pub async fn import_datapack(
    profile_id: String,
    world_id: String,
    filename: String,
    bytes: Vec<u8>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    ensure_profile_idle(&profile_id)?;
    core::datapacks::import(&paths.root(), &profile_id, &world_id, &filename, bytes).await
}

#[tauri::command]
pub async fn remove_datapack(
    profile_id: String,
    world_id: String,
    name: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    ensure_profile_idle(&profile_id)?;
    core::datapacks::remove(&paths.root(), &profile_id, &world_id, &name).await
}

#[tauri::command]
pub async fn list_servers(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::servers::ServerSummary>, String> {
    core::servers::list(&paths.root(), &profile_id).await
}

#[tauri::command]
pub async fn save_server(
    profile_id: String,
    key: Option<String>,
    name: String,
    address: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    ensure_profile_idle(&profile_id)?;
    core::servers::save(&paths.root(), &profile_id, key.as_deref(), &name, &address).await
}

#[tauri::command]
pub async fn remove_server(
    profile_id: String,
    key: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    ensure_profile_idle(&profile_id)?;
    core::servers::remove(&paths.root(), &profile_id, &key).await
}
