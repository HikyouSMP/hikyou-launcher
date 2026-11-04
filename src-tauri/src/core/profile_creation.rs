//! Durable creation goal: wishes commit before the profile becomes discoverable.
use super::{managed_fs, mod_intent, paths::LauncherPaths, profile};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Goal {
    profile: profile::Profile,
    intent: mod_intent::Intent,
    complete: bool,
}

fn directory(paths: &LauncherPaths) -> Result<cap_std::fs::Dir, String> {
    managed_fs::directory(
        &managed_fs::root(&paths.root())?,
        Path::new("state/profile-creation"),
        true,
    )
}
fn write(dir: &cap_std::fs::Dir, goal: &Goal) -> Result<(), String> {
    validate(goal)?;
    let bytes = serde_json::to_vec(goal).map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("creation_goal_limit".into());
    }
    managed_fs::write(
        dir,
        Path::new(&format!("{}.json", goal.profile.id)),
        &bytes,
        true,
    )
}
fn validate(goal: &Goal) -> Result<(), String> {
    profile::validate_profile_id(&goal.profile.id)?;
    goal.intent.validate(&goal.profile.id)?;
    if goal.profile.kind != "normal"
        || goal.profile.name.trim().is_empty()
        || goal.profile.name.len() > 256
        || !super::paths::is_safe_component(&goal.profile.mc_version)
        || !["vanilla", "fabric", "quilt", "forge", "neoforge"]
            .contains(&goal.profile.loader.as_str())
        || (goal.profile.loader != "vanilla" && goal.profile.loader_version.is_none())
        || goal
            .profile
            .loader_version
            .as_ref()
            .is_some_and(|v| !super::paths::is_safe_component(v))
        || !goal.intent.independent
    {
        return Err("creation_goal_invalid".into());
    }
    Ok(())
}
async fn finish(
    paths: &LauncherPaths,
    dir: &cap_std::fs::Dir,
    mut goal: Goal,
) -> Result<profile::Profile, String> {
    validate(&goal)?;
    match mod_intent::load(&paths.root(), &goal.profile.id)? {
        None => mod_intent::save(&paths.root(), &goal.intent)?,
        Some(existing) if existing == goal.intent => {}
        Some(existing) if existing.choice_sequence > goal.intent.choice_sequence => {}
        Some(_) => return Err("creation_goal_wishes_changed".into()),
    }
    let profile_path = Path::new("profiles")
        .join(&goal.profile.id)
        .join("profile.json");
    let root = managed_fs::root(&paths.root())?;
    if managed_fs::inspect(&root, &profile_path)?.is_none() {
        let game = paths.checked_profile_game_dir(&goal.profile.id)?;
        managed_fs::directory(&managed_fs::root_or_create(&game)?, Path::new("mods"), true)?;
        mod_intent::set_preparation(
            &paths.root(),
            &goal.profile.id,
            &goal.intent.fingerprint()?,
            "queued",
            None,
        )?;
        profile::save_profile(&paths.root(), &goal.profile).await?;
    }
    goal.complete = true;
    write(dir, &goal)?;
    super::mod_runtime::wake_preparation();
    Ok(goal.profile)
}
pub async fn create(
    paths: &LauncherPaths,
    profile: profile::Profile,
    intent: mod_intent::Intent,
) -> Result<profile::Profile, String> {
    let dir = directory(paths)?;
    if managed_fs::inspect(&dir, Path::new(&format!("{}.json", profile.id)))?.is_some() {
        return Err("creation_id_already_exists".into());
    }
    let goal = Goal {
        profile,
        intent,
        complete: false,
    };
    write(&dir, &goal)?;
    finish(paths, &dir, goal).await
}
pub async fn recover(paths: &LauncherPaths) -> Result<(), String> {
    let dir = directory(paths)?;
    let mut goals = Vec::new();
    for (count, entry) in dir.entries().map_err(|e| e.to_string())?.enumerate() {
        if count >= 4096 {
            return Err("creation_goal_limit".into());
        }
        let name = entry.map_err(|e| e.to_string())?.file_name();
        let name = name.to_str().ok_or("invalid_creation_goal_name")?;
        if name.starts_with(".hikyou-") && name.ends_with(".tmp") {
            continue;
        }
        let id = name
            .strip_suffix(".json")
            .ok_or("invalid_creation_goal_name")?;
        profile::validate_profile_id(id)?;
        if goals.len() >= 4096 {
            return Err("creation_goal_limit".into());
        }
        let mut bytes = Vec::new();
        managed_fs::read_file(&dir, Path::new(name))?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("creation_goal_limit".into());
        }
        let goal: Goal = serde_json::from_slice(&bytes).map_err(|_| "creation_goal_invalid")?;
        if goal.profile.id != id {
            return Err("creation_goal_identity_mismatch".into());
        }
        if !goal.complete {
            goals.push(goal);
        }
    }
    for goal in goals {
        finish(paths, &dir, goal).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn interrupted_goal_resumes_without_live_template() -> Result<(), String> {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let p = profile::Profile::new(
            "Fixture".into(),
            "1.21.1".into(),
            "fabric".into(),
            Some("0.16.10".into()),
        );
        let mut intent = mod_intent::Intent::empty(&p.id);
        intent.independent = true;
        let dir = directory(&paths)?;
        write(
            &dir,
            &Goal {
                profile: p.clone(),
                intent: intent.clone(),
                complete: false,
            },
        )?;
        let template = paths.root().join("state/mod-template");
        std::fs::create_dir_all(&template).map_err(|e| e.to_string())?;
        std::fs::write(template.join("template.json"), b"{").map_err(|e| e.to_string())?;
        recover(&paths).await?;
        recover(&paths).await?;
        assert_eq!(mod_intent::load(&paths.root(), &p.id)?, Some(intent));
        assert_eq!(
            profile::list_profiles(&paths.root())
                .await
                .iter()
                .filter(|v| v.id == p.id)
                .count(),
            1
        );
        assert!(!paths.auto_mods_file().exists());
        Ok(())
    }
}
