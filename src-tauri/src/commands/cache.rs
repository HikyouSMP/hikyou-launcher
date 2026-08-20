use std::sync::Arc;

use crate::{LauncherPaths, core};

#[tauri::command]
pub async fn fetch_cached_icon(
    url: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<String, String> {
    core::icon_cache::fetch_icon_data_url(&url, &paths).await
}

#[tauri::command]
pub async fn clear_api_cache() -> Result<(), String> {
    if let Some(cache) = core::cache::get() {
        cache.clear_all().await
    } else {
        Err("cache is not initialized".to_string())
    }
}

#[tauri::command]
pub async fn get_cache_stats() -> core::cache::CacheStats {
    if let Some(cache) = core::cache::get() {
        cache.stats().await
    } else {
        core::cache::CacheStats {
            total_entries: 0,
            valid_entries: 0,
        }
    }
}
