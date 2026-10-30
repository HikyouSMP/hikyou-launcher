//! Removing a launch entry retires the complete asset directory; it never purges it.

use super::{managed_fs, mod_runtime, paths::LauncherPaths, profile, running_processes};
use std::{io::Read, path::Path};

async fn idle(paths: &LauncherPaths, id: &str) -> Result<(), String> {
    if running_processes::is_active(id) || mod_runtime::surviving_game(paths, id, false).await? {
        return Err("Profile is in use; stop its game before removing or restoring it".into());
    }
    Ok(())
}

pub async fn retire(paths: &LauncherPaths, id: &str) -> Result<(), String> {
    profile::validate_profile_id(id)?;
    let _guard = mod_runtime::edit_guard(paths, id).await?;
    let _play = mod_runtime::play_lease(paths, id)?;
    idle(paths, id).await?;
    let root = managed_fs::root(&paths.root())?;
    let profiles = managed_fs::directory(&root, Path::new("profiles"), false)?;
    let retired = managed_fs::directory(&root, Path::new("retired-profiles"), true)?;
    let path = Path::new(id);
    match (
        managed_fs::inspect(&profiles, path)?,
        managed_fs::inspect(&retired, path)?,
    ) {
        (None, Some(metadata)) if metadata.is_dir() => return Ok(()),
        (None, _) => return Err("Profile not found".into()),
        (Some(metadata), None) if metadata.is_dir() => {}
        _ => return Err("Profile retirement destination exists or source is invalid".into()),
    }
    managed_fs::rename_new(&profiles, id.as_ref(), &retired, id.as_ref())
        .map_err(|e| format!("Could not retire profile; existing data was not purged: {e}"))?;
    managed_fs::sync_dir(&profiles)?;
    managed_fs::sync_dir(&retired)?;
    log::info!("[profile] Retired profile {id}; user assets retained");
    Ok(())
}

pub async fn restore(paths: &LauncherPaths, id: &str) -> Result<profile::Profile, String> {
    profile::validate_profile_id(id)?;
    let _guard = mod_runtime::edit_guard(paths, id).await?;
    let _play = mod_runtime::play_lease(paths, id)?;
    idle(paths, id).await?;
    let root = managed_fs::root(&paths.root())?;
    let profiles = managed_fs::directory(&root, Path::new("profiles"), true)?;
    let retired = managed_fs::directory(&root, Path::new("retired-profiles"), false)?;
    let data = managed_fs::directory(&retired, Path::new(id), false)?;
    let mut bytes = Vec::new();
    managed_fs::read_file(&data, Path::new("profile.json"))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Retired profile metadata is too large".into());
    }
    let profile: profile::Profile = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Retired profile metadata is invalid: {e}"))?;
    if profile.id != id || profile.kind == "smart" {
        return Err("Retired profile identity mismatch".into());
    }
    drop(data);
    managed_fs::rename_new(&retired, id.as_ref(), &profiles, id.as_ref()).map_err(|e| {
        format!("Could not restore profile without replacing an existing entry: {e}")
    })?;
    managed_fs::sync_dir(&retired)?;
    managed_fs::sync_dir(&profiles)?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retirement_preserves_assets_retries_and_restores_without_overwrite()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let paths = LauncherPaths::new(temp.path().canonicalize()?);
        paths.ensure_dirs()?;
        let profile =
            profile::Profile::new("fixture".into(), "1.21.1".into(), "vanilla".into(), None);
        profile::save_profile(&paths.root(), &profile).await?;
        let game = paths.checked_profile_game_dir(&profile.id)?;
        std::fs::create_dir_all(game.join("saves/world"))?;
        std::fs::write(game.join("saves/world/level.dat"), b"world-marker")?;
        std::fs::write(game.join("options.txt"), b"config-marker")?;
        let use_lease = mod_runtime::play_lease(&paths, &profile.id)?;
        assert!(retire(&paths, &profile.id).await.is_err());
        assert_eq!(
            std::fs::read(game.join("saves/world/level.dat"))?,
            b"world-marker"
        );
        drop(use_lease);
        let launch = running_processes::LaunchGuard::begin(profile.id.clone())?;
        assert!(retire(&paths, &profile.id).await.is_err());
        drop(launch);
        retire(&paths, &profile.id).await?;
        retire(&paths, &profile.id).await?;
        assert!(!game.exists());
        let retired_game = paths
            .root()
            .join("retired-profiles")
            .join(&profile.id)
            .join(".minecraft");
        assert_eq!(
            std::fs::read(retired_game.join("saves/world/level.dat"))?,
            b"world-marker"
        );
        std::fs::create_dir_all(&game)?;
        assert!(restore(&paths, &profile.id).await.is_err());
        std::fs::remove_dir(&game)?;
        std::fs::remove_dir(game.parent().ok_or("profile parent missing")?)?;
        assert_eq!(restore(&paths, &profile.id).await?.id, profile.id);
        assert_eq!(
            std::fs::read(game.join("saves/world/level.dat"))?,
            b"world-marker"
        );
        assert_eq!(std::fs::read(game.join("options.txt"))?, b"config-marker");
        Ok(())
    }
}
