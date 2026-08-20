//! Profile storage and path boundaries.
//!
//! Normal user-created profiles live under `profiles/<uuid>`.
//! Launcher-managed smart profiles live under `smart-profiles/<key>` and are
//! addressed by fixed refs such as `smart:latest-plus`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

const LATEST_PLUS_KEY: &str = "latest-plus";
const SNAPSHOT_PLUS_KEY: &str = "snapshot-plus";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    #[serde(default = "normal_kind")]
    pub kind: String,
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    #[serde(default)]
    pub smart_key: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub loader_policy: Option<String>,
    #[serde(default)]
    pub resolved: Option<ResolvedTarget>,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub window_w: Option<u32>,
    #[serde(default)]
    pub window_h: Option<u32>,
    pub last_launched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTarget {
    pub mc_version: String,
    pub loader: String,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub resolved_at: DateTime<Utc>,
}

fn normal_kind() -> String {
    "normal".to_string()
}

impl Profile {
    pub fn new(
        name: String,
        mc_version: String,
        loader: String,
        loader_version: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            kind: normal_kind(),
            name,
            mc_version,
            loader,
            smart_key: None,
            channel: None,
            loader_policy: None,
            resolved: None,
            loader_version,
            memory_mb: None,
            window_w: None,
            window_h: None,
            last_launched_at: None,
            created_at: Utc::now(),
        }
    }

    fn latest_plus() -> Self {
        Self::smart(
            LATEST_PLUS_KEY,
            "Latest+",
            "latest-release",
            "fabric-then-vanilla",
        )
    }

    fn snapshot_plus() -> Self {
        Self::smart(
            SNAPSHOT_PLUS_KEY,
            "Snapshot+",
            "latest-snapshot",
            "fabric-then-vanilla",
        )
    }

    fn smart(key: &str, name: &str, channel: &str, loader_policy: &str) -> Self {
        Self {
            id: smart_profile_ref(key),
            kind: "smart".to_string(),
            name: name.to_string(),
            mc_version: "latest".to_string(),
            loader: "auto".to_string(),
            smart_key: Some(key.to_string()),
            channel: Some(channel.to_string()),
            loader_policy: Some(loader_policy.to_string()),
            resolved: None,
            loader_version: None,
            memory_mb: None,
            window_w: None,
            window_h: None,
            last_launched_at: None,
            created_at: Utc::now(),
        }
    }
}

fn profiles_dir(root: &Path) -> PathBuf {
    root.join("profiles")
}

pub fn smart_profiles_dir(root: &Path) -> PathBuf {
    root.join("smart-profiles")
}

fn profile_dir(root: &Path, id: &str) -> PathBuf {
    profiles_dir(root).join(id)
}

pub fn smart_profile_dir(root: &Path, key: &str) -> PathBuf {
    smart_profiles_dir(root).join(key)
}

fn profile_json_path(root: &Path, id: &str) -> PathBuf {
    profile_dir(root, id).join("profile.json")
}

fn smart_profile_json_path(root: &Path, key: &str) -> PathBuf {
    smart_profile_dir(root, key).join("profile.json")
}

pub fn validate_profile_id(id: &str) -> Result<(), String> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "invalid profile id".to_string())
}

pub fn smart_profile_ref(key: &str) -> String {
    format!("smart:{key}")
}

pub fn smart_key_from_ref(profile_ref: &str) -> Option<&'static str> {
    match profile_ref {
        "smart:latest-plus" => Some(LATEST_PLUS_KEY),
        "smart:snapshot-plus" => Some(SNAPSHOT_PLUS_KEY),
        _ => None,
    }
}

pub fn is_smart_profile_ref(profile_ref: &str) -> bool {
    smart_key_from_ref(profile_ref).is_some()
}

pub fn validate_profile_ref(profile_ref: &str) -> Result<(), String> {
    if is_smart_profile_ref(profile_ref) {
        return Ok(());
    }
    validate_profile_id(profile_ref)
}

pub fn profile_dir_for_ref(root: &Path, profile_ref: &str) -> Result<PathBuf, String> {
    if let Some(key) = smart_key_from_ref(profile_ref) {
        return Ok(smart_profile_dir(root, key));
    }
    validate_profile_id(profile_ref)?;
    Ok(profile_dir(root, profile_ref))
}

pub fn profile_game_dir_for_ref(root: &Path, profile_ref: &str) -> Result<PathBuf, String> {
    Ok(profile_dir_for_ref(root, profile_ref)?.join(".minecraft"))
}

pub async fn list_profiles(root: &Path) -> Vec<Profile> {
    let mut normal_profiles = list_normal_profiles(root).await;
    normal_profiles.sort_by(|a, b| {
        let a_time = a.last_launched_at.unwrap_or(a.created_at);
        let b_time = b.last_launched_at.unwrap_or(b.created_at);
        b_time.cmp(&a_time)
    });

    let mut smart_profiles = list_smart_profiles(root).await;
    smart_profiles.extend(normal_profiles);
    smart_profiles
}

async fn list_normal_profiles(root: &Path) -> Vec<Profile> {
    let dir = profiles_dir(root);
    let mut profiles = Vec::new();

    let mut rd = match fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(_) => return profiles,
    };

    while let Ok(Some(entry)) = rd.next_entry().await {
        let json_path = entry.path().join("profile.json");
        if let Ok(json) = fs::read_to_string(&json_path).await
            && let Ok(mut profile) = serde_json::from_str::<Profile>(&json)
        {
            profile.kind = normal_kind();
            profile.smart_key = None;
            profile.channel = None;
            profile.loader_policy = None;
            profile.resolved = None;
            profiles.push(profile);
        }
    }

    profiles
}

async fn list_smart_profiles(root: &Path) -> Vec<Profile> {
    let defaults = [Profile::latest_plus(), Profile::snapshot_plus()];
    let mut profiles = Vec::with_capacity(defaults.len());

    for default_profile in defaults {
        let key = default_profile.smart_key.as_deref().unwrap_or_default();
        let profile = match fs::read_to_string(smart_profile_json_path(root, key)).await {
            Ok(json) => serde_json::from_str::<Profile>(&json).unwrap_or(default_profile),
            Err(_) => default_profile,
        };
        profiles.push(normalize_smart_profile(profile));
    }

    profiles
}

fn normalize_smart_profile(mut profile: Profile) -> Profile {
    let key = profile
        .smart_key
        .clone()
        .or_else(|| smart_key_from_ref(&profile.id).map(str::to_string))
        .unwrap_or_else(|| LATEST_PLUS_KEY.to_string());

    profile.id = smart_profile_ref(&key);
    profile.kind = "smart".to_string();
    profile.smart_key = Some(key.clone());
    profile.loader = "auto".to_string();
    profile.loader_version = None;

    match key.as_str() {
        LATEST_PLUS_KEY => {
            if profile.name.trim().is_empty() {
                profile.name = "Latest+".to_string();
            }
            profile
                .channel
                .get_or_insert_with(|| "latest-release".to_string());
            profile
                .loader_policy
                .get_or_insert_with(|| "fabric-then-vanilla".to_string());
        }
        SNAPSHOT_PLUS_KEY => {
            if profile.name.trim().is_empty() {
                profile.name = "Snapshot+".to_string();
            }
            profile
                .channel
                .get_or_insert_with(|| "latest-snapshot".to_string());
            profile
                .loader_policy
                .get_or_insert_with(|| "fabric-then-vanilla".to_string());
        }
        _ => {}
    }

    profile
}

pub async fn save_profile(root: &Path, profile: &Profile) -> Result<(), String> {
    if profile.kind == "smart" || is_smart_profile_ref(&profile.id) {
        return save_smart_profile(root, profile).await;
    }

    validate_profile_id(&profile.id)?;
    let dir = profile_dir(root, &profile.id);
    fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed to create profile directory: {}", e))?;

    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("failed to serialize: {}", e))?;

    fs::write(profile_json_path(root, &profile.id), json)
        .await
        .map_err(|e| format!("profile.json failed to write: {}", e))?;

    log::info!("[profile] Saved: {} ({})", profile.name, profile.id);
    Ok(())
}

async fn save_smart_profile(root: &Path, profile: &Profile) -> Result<(), String> {
    let key = profile
        .smart_key
        .as_deref()
        .or_else(|| smart_key_from_ref(&profile.id))
        .ok_or_else(|| "invalid smart profile".to_string())?;
    if !matches!(key, LATEST_PLUS_KEY | SNAPSHOT_PLUS_KEY) {
        return Err("invalid smart profile".to_string());
    }

    let profile = normalize_smart_profile(profile.clone());
    let dir = smart_profile_dir(root, key);
    fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed to create smart profile directory: {}", e))?;

    let json = serde_json::to_string_pretty(&profile)
        .map_err(|e| format!("failed to serialize: {}", e))?;
    fs::write(smart_profile_json_path(root, key), json)
        .await
        .map_err(|e| format!("smart profile failed to write: {}", e))?;

    log::info!(
        "[profile] Saved smart profile: {} ({})",
        profile.name,
        profile.id
    );
    Ok(())
}

pub async fn delete_profile(root: &Path, id: &str) -> Result<(), String> {
    if is_smart_profile_ref(id) {
        return Err("smart profiles cannot be deleted".to_string());
    }
    validate_profile_id(id)?;
    let dir = profile_dir(root, id);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .await
            .map_err(|e| format!("failed to delete profile: {}", e))?;
        log::info!("[profile] Deleted: {}", id);
    }
    Ok(())
}

pub async fn update_profile(
    root: &Path,
    id: &str,
    name: Option<String>,
    memory_mb: Option<u32>,
    window_w: Option<u32>,
    window_h: Option<u32>,
) -> Result<Profile, String> {
    validate_profile_ref(id)?;
    let all = list_profiles(root).await;
    let mut profile = all
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("profile {} was not found", id))?;
    if let Some(n) = name {
        let trimmed = n.trim().to_string();
        if !trimmed.is_empty() {
            profile.name = trimmed;
        }
    }
    if let Some(m) = memory_mb {
        profile.memory_mb = if m == 0 { None } else { Some(m) };
    }
    if let Some(w) = window_w {
        profile.window_w = if w == 0 { None } else { Some(w) };
    }
    if let Some(h) = window_h {
        profile.window_h = if h == 0 { None } else { Some(h) };
    }
    save_profile(root, &profile).await?;
    Ok(profile)
}

pub async fn touch_launched_with_resolved(
    root: &Path,
    id: &str,
    resolved: Option<ResolvedTarget>,
) -> Result<(), String> {
    validate_profile_ref(id)?;
    let all = list_profiles(root).await;
    let profile = all
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("profile {} was not found", id))?;
    let mut updated = profile.clone();
    updated.last_launched_at = Some(Utc::now());
    if let Some(resolved) = resolved {
        updated.resolved = Some(resolved);
    }
    save_profile(root, &updated).await
}

#[cfg(test)]
mod tests {
    use super::{profile_game_dir_for_ref, validate_profile_id, validate_profile_ref};

    #[test]
    fn accepts_uuid_profile_id() {
        assert!(validate_profile_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn accepts_known_smart_refs() {
        assert!(validate_profile_ref("smart:latest-plus").is_ok());
        assert!(validate_profile_ref("smart:snapshot-plus").is_ok());
    }

    #[test]
    fn rejects_path_like_profile_ref() {
        assert!(validate_profile_ref("../meta").is_err());
        assert!(validate_profile_ref("smart:../meta").is_err());
        assert!(validate_profile_ref("profiles\\..\\meta").is_err());
        assert!(validate_profile_ref("").is_err());
    }

    #[test]
    fn resolves_smart_profile_to_dedicated_space() {
        let root = std::path::Path::new("/launcher");
        let dir = profile_game_dir_for_ref(root, "smart:latest-plus").unwrap();
        assert!(
            dir.ends_with(
                std::path::Path::new("smart-profiles")
                    .join("latest-plus")
                    .join(".minecraft")
            )
        );
    }
}
