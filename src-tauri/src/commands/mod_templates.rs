use crate::{LauncherPaths, core::mod_template};
use std::sync::Arc;

#[tauri::command]
pub fn get_mod_template(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<mod_template::Snapshot, String> {
    mod_template::snapshot(&paths)
}
#[tauri::command]
pub fn save_mod_template(
    expected_revision: String,
    entries: Vec<mod_template::Entry>,
    folders: Vec<mod_template::Folder>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<mod_template::Snapshot, String> {
    mod_template::save(&paths, &expected_revision, entries, folders)
}
#[tauri::command]
pub async fn preview_profile_mod_template(
    mc_version: String,
    loader: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<mod_template::Preview, String> {
    mod_template::preview(&paths, &mc_version, &loader).await
}
