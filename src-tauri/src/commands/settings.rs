use std::sync::Arc;

use crate::core;
use crate::core::paths::LauncherPaths;

#[tauri::command]
pub async fn get_settings(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<serde_json::Value, String> {
    let path = paths.root().join("settings.json");
    let mut settings = match tokio::fs::read_to_string(&path).await {
        Ok(json) => serde_json::from_str(&json).map_err(|e| e.to_string())?,
        Err(_) => serde_json::json!({}),
    };
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
        let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        tokio::fs::write(&path, json)
            .await
            .map_err(|e| format!("failed to write settings.json: {}", e))?;
    }

    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(
    mut settings: serde_json::Value,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    sanitize_settings(&mut settings);
    let path = paths.root().join("settings.json");
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| format!("failed to write settings.json: {}", e))
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
