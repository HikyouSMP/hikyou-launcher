use std::sync::Arc;

use crate::{LauncherPaths, auth, core};

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

#[tauri::command]
pub async fn list_screenshots(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<core::screenshots::ScreenshotItem>, String> {
    core::screenshots::list_all(&paths.root()).await
}

#[tauri::command]
pub async fn get_screenshot_thumbnail(
    profile_id: String,
    filename: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<String, String> {
    core::screenshots::thumbnail(
        &paths.root(),
        &profile_id,
        &filename,
        max_width.unwrap_or(720),
        max_height.unwrap_or(480),
    )
    .await
}

#[tauri::command]
pub async fn trash_screenshot(
    profile_id: String,
    filename: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    core::screenshots::trash(&paths.root(), &profile_id, &filename).await
}

#[tauri::command]
pub async fn get_account_appearance(
    app: tauri::AppHandle,
) -> Result<auth::common::MinecraftProfile, String> {
    let session = auth::ensure_fresh_auth(&app).await?;
    auth::common::get_minecraft_profile(&session.access_token).await
}

#[tauri::command]
pub async fn upload_account_skin(
    app: tauri::AppHandle,
    filename: String,
    variant: String,
    bytes: Vec<u8>,
) -> Result<auth::common::MinecraftProfile, String> {
    if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err("skin image is empty or exceeds 4 MiB".to_string());
    }
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|_| "skin must be a valid PNG image".to_string())?;
    if !matches!((image.width(), image.height()), (64, 64) | (64, 32)) {
        return Err("skin dimensions must be 64x64 or legacy 64x32".to_string());
    }
    let session = auth::ensure_fresh_auth(&app).await?;
    auth::common::upload_minecraft_skin(&session.access_token, &variant, &filename, bytes).await
}

#[tauri::command]
pub async fn set_account_skin_variant(
    app: tauri::AppHandle,
    variant: String,
) -> Result<auth::common::MinecraftProfile, String> {
    let session = auth::ensure_fresh_auth(&app).await?;
    auth::common::set_minecraft_skin_variant(&session.access_token, &variant).await
}

#[tauri::command]
pub async fn set_account_cape(
    app: tauri::AppHandle,
    cape_id: Option<String>,
) -> Result<auth::common::MinecraftProfile, String> {
    let session = auth::ensure_fresh_auth(&app).await?;
    auth::common::set_minecraft_cape(&session.access_token, cape_id.as_deref()).await
}
