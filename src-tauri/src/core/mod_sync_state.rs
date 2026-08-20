use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::core::mod_files::mods_dir;
use crate::core::mod_recommendations::AutoMod;

const AUTO_MOD_SYNC_TTL_SECONDS: i64 = 6 * 60 * 60;

#[derive(Serialize, Deserialize)]
struct AutoModSyncState {
    mc_version: String,
    loader: String,
    signature: String,
    #[serde(default)]
    mod_dir_signature: Option<String>,
    synced_at: chrono::DateTime<chrono::Utc>,
}

pub fn signature(mods: &[AutoMod], resolver_version: u8) -> String {
    let mut entries: Vec<String> = mods
        .iter()
        .map(|mod_def| {
            format!(
                "{}:{}:{}:{}:{}:{}:{}",
                mod_def.project_id,
                mod_def.name,
                mod_def.loaders.join(","),
                mod_def.install_rank,
                mod_def.keep_priority,
                mod_def.min_mc_version.as_deref().unwrap_or_default(),
                mod_def.max_mc_version.as_deref().unwrap_or_default()
            )
        })
        .collect();
    entries.push(format!("resolver:{}", resolver_version));
    entries.sort();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub async fn is_fresh(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
    signature: &str,
    mod_dir_signature: &str,
) -> bool {
    let path = state_path(game_dir);
    let Ok(text) = fs::read_to_string(path).await else {
        return false;
    };
    let Ok(state) = serde_json::from_str::<AutoModSyncState>(&text) else {
        return false;
    };
    if state.mc_version != mc_version || state.loader != loader || state.signature != signature {
        return false;
    }
    if state.mod_dir_signature.as_deref() != Some(mod_dir_signature) {
        log::info!("[mods] Auto mod sync cache invalidated because the mod folder changed");
        return false;
    }
    chrono::Utc::now()
        .signed_duration_since(state.synced_at)
        .num_seconds()
        < AUTO_MOD_SYNC_TTL_SECONDS
}

pub async fn save(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
    signature: &str,
    mod_dir_signature: &str,
) {
    let state = AutoModSyncState {
        mc_version: mc_version.to_string(),
        loader: loader.to_string(),
        signature: signature.to_string(),
        mod_dir_signature: Some(mod_dir_signature.to_string()),
        synced_at: chrono::Utc::now(),
    };
    if let Ok(text) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(state_path(game_dir), text).await;
    }
}

pub async fn dir_signature(game_dir: &Path) -> String {
    let dir = mods_dir(game_dir);
    let mut entries = Vec::new();
    let Ok(mut rd) = fs::read_dir(&dir).await else {
        return String::new();
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.starts_with('.')
            || (!filename.ends_with(".jar") && !filename.ends_with(".disabled"))
        {
            continue;
        }
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        entries.push(format!("{}:{}:{}", filename, metadata.len(), modified));
    }
    entries.sort();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub async fn read_status(game_dir: &Path) -> Option<serde_json::Value> {
    let path = state_path(game_dir);
    let text = fs::read_to_string(path).await.ok()?;
    let state = serde_json::from_str::<AutoModSyncState>(&text).ok()?;
    let current_mod_dir_signature = dir_signature(game_dir).await;
    let folder_changed = state.mod_dir_signature.as_deref() != Some(&current_mod_dir_signature);
    let age_seconds = chrono::Utc::now()
        .signed_duration_since(state.synced_at)
        .num_seconds()
        .max(0);
    Some(serde_json::json!({
        "mc_version": state.mc_version,
        "loader": state.loader,
        "signature": state.signature,
        "synced_at": state.synced_at.to_rfc3339(),
        "age_seconds": age_seconds,
        "folder_changed": folder_changed,
        "fresh": !folder_changed && age_seconds < AUTO_MOD_SYNC_TTL_SECONDS,
    }))
}

fn state_path(game_dir: &Path) -> PathBuf {
    game_dir.join(".hikyou_auto_mod_sync.json")
}
