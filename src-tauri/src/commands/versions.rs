use std::sync::Arc;

use crate::{LauncherPaths, core};

#[tauri::command]
pub async fn get_version_manifest(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::manifest::VersionManifest, String> {
    core::manifest::fetch_manifest(&paths).await
}

#[tauri::command]
pub async fn refresh_version_manifest(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<core::manifest::VersionManifest, String> {
    core::manifest::refresh_manifest(&paths).await
}

#[tauri::command]
pub async fn get_fabric_versions(
    mc_version: String,
) -> Result<Vec<core::fabric::FabricLoaderVersion>, String> {
    core::fabric::fetch_loader_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_quilt_versions(
    mc_version: String,
) -> Result<Vec<core::quilt::QuiltLoaderVersion>, String> {
    core::quilt::fetch_loader_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_neoforge_versions(
    mc_version: String,
) -> Result<Vec<core::neoforge::NeoForgeVersion>, String> {
    core::neoforge::fetch_loader_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_forge_versions(
    mc_version: String,
) -> Result<Vec<core::forge::ForgeVersion>, String> {
    core::forge::fetch_loader_versions(&mc_version).await
}
