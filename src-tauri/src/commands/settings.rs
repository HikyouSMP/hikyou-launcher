use std::sync::Arc;

use crate::core;
use crate::core::paths::LauncherPaths;
use core::settings_store::{read as read_settings, write as write_settings};
use std::sync::Mutex;

static SETTINGS_WRITE: Mutex<()> = Mutex::new(());

#[tauri::command]
pub async fn get_settings(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<serde_json::Value, String> {
    let _lock = SETTINGS_WRITE
        .lock()
        .map_err(|e| format!("Settings are unavailable: {e}"))?;
    let mut settings = read_settings(&paths.root())?;
    let mut changed = false;

    changed |= sanitize_settings(&mut settings);
    if settings
        .pointer("/game/memoryMb")
        .and_then(|v| v.as_u64())
        .is_none()
    {
        let recommended = recommended_memory_mb();
        if !settings.is_object() {
            settings = serde_json::json!({});
            changed = true;
        }
        let Some(obj) = settings.as_object_mut() else {
            return Err("settings must be an object".to_string());
        };
        let game = obj.entry("game").or_insert_with(|| serde_json::json!({}));
        if !game.is_object() {
            *game = serde_json::json!({});
        }
        if let Some(game_obj) = game.as_object_mut() {
            game_obj.insert("memoryMb".to_string(), serde_json::json!(recommended));
            changed = true;
        }
    }

    if changed {
        write_settings(&paths.root(), &settings)?;
    }

    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(
    mut settings: serde_json::Value,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    let _lock = SETTINGS_WRITE
        .lock()
        .map_err(|e| format!("Settings are unavailable: {e}"))?;
    // Never replace a damaged or unreadable record with a frontend default.
    read_settings(&paths.root())?;
    sanitize_settings(&mut settings);
    write_settings(&paths.root(), &settings)
}

fn sanitize_settings(settings: &mut serde_json::Value) -> bool {
    let mut changed = false;
    if let Some(accounts) = settings.get_mut("accounts").and_then(|a| a.as_array_mut()) {
        for account in accounts.iter_mut() {
            if let Some(obj) = account.as_object_mut() {
                changed |= obj.remove("access_token").is_some();
                changed |= obj.remove("refresh_token").is_some();
                changed |= obj.remove("token").is_some();
            }
        }
    }
    changed
}

fn recommended_memory_mb() -> u32 {
    let total_mb = core::java::get_max_memory_mb() as u32;
    let recommended = match total_mb {
        0 => 4096,
        1..=4096 => 1536,
        4097..=8192 => 3072,
        8193..=12288 => 4096,
        12289..=16384 => 6144,
        _ => 8192,
    };
    if total_mb == 0 {
        return recommended;
    }
    let safe_ceiling = ((total_mb as f32) * 0.45).round() as u32;
    recommended.min(safe_ceiling).clamp(1024, 8192)
}
