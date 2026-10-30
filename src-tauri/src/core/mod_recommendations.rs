use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path};

use super::managed_fs;

#[derive(Deserialize, Clone)]
pub(super) struct ModrinthProject {
    pub id: String,
    pub title: String,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct RecommendedMod {
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub tags: Vec<String>,
    pub default_enabled: bool,
    pub install_rank: u8,
    pub keep_priority: u8,
    pub min_mc_version: Option<String>,
    pub max_mc_version: Option<String>,
}

struct ModDef {
    slug: &'static str,
    name: &'static str,
    fallback_desc: &'static str,
    loaders: &'static [&'static str],
    tags: &'static [&'static str],
    default_enabled: bool,
    install_rank: u8,
    keep_priority: u8,
    min_mc_version: Option<&'static str>,
    max_mc_version: Option<&'static str>,
}

const BUILTIN_MODS: &[ModDef] = &[
    ModDef {
        slug: "sodium",
        name: "Sodium",
        fallback_desc: "Complete rendering engine replacement. Improves FPS by 200-500% over vanilla.",
        loaders: &["fabric", "quilt", "neoforge"],
        tags: &[],
        default_enabled: true,
        install_rank: 3,
        keep_priority: 100,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "nvidium",
        name: "Nvidium",
        fallback_desc: "Sodium add-on using mesh shaders for up to 100% more FPS on Nvidia Turing or newer GPUs.",
        loaders: &["fabric", "quilt"],
        tags: &["nvidia-only"],
        default_enabled: false,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "lithium",
        name: "Lithium",
        fallback_desc: "Optimizes tick processing, chunk generation, and physics. Reduces tick time by 30-50%.",
        loaders: &["fabric", "quilt", "neoforge"],
        tags: &["server-focus"],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 100,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "moonrise-opt",
        name: "Moonrise",
        fallback_desc: "Optimizes chunk systems and entity tracking for much faster chunk generation.",
        loaders: &["fabric", "neoforge"],
        tags: &["beta", "server-focus"],
        default_enabled: false,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "ferrite-core",
        name: "FerriteCore",
        fallback_desc: "Deduplicates block states to reduce memory usage by 40-50%.",
        loaders: &["fabric", "quilt", "forge", "neoforge"],
        tags: &[],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 100,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "modernfix",
        name: "ModernFix",
        fallback_desc: "Patch set that reduces startup time by 20-30%, memory by 15%, and fixes assorted issues.",
        loaders: &["fabric", "forge", "neoforge"],
        tags: &[],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "immediatelyfast",
        name: "ImmediatelyFast",
        fallback_desc: "Optimizes font, HUD, and entity UI rendering for 10-30% more FPS.",
        loaders: &["fabric", "quilt", "forge", "neoforge"],
        tags: &[],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "entityculling",
        name: "Entity Culling",
        fallback_desc: "Asynchronously skips off-screen entity rendering for 10-30% more FPS in dense areas.",
        loaders: &["fabric", "quilt", "forge", "neoforge"],
        tags: &[],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 60,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "modmenu",
        name: "Mod Menu",
        fallback_desc: "Adds a clean in-game mod list and configuration entry point for Fabric and Quilt.",
        loaders: &["fabric", "quilt"],
        tags: &["quality-of-life", "configuration"],
        default_enabled: true,
        install_rank: 0,
        keep_priority: 90,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "cloth-config",
        name: "Cloth Config API",
        fallback_desc: "Common configuration library used by many Fabric, Quilt, Forge, and NeoForge mods.",
        loaders: &["fabric", "quilt", "forge", "neoforge"],
        tags: &["library", "configuration"],
        default_enabled: true,
        install_rank: 0,
        keep_priority: 90,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "dynamic-fps",
        name: "Dynamic FPS",
        fallback_desc: "Reduces FPS while the game window is unfocused, improving battery life, thermals, and background resource usage.",
        loaders: &["fabric", "quilt", "forge", "neoforge"],
        tags: &["background", "quality-of-life"],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 60,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "debugify",
        name: "Debugify",
        fallback_desc: "Fixes documented Minecraft bugs from the Mojang bug tracker while keeping each fix configurable.",
        loaders: &["fabric", "quilt", "forge"],
        tags: &["bugfix", "quality-of-life"],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 60,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "moreculling",
        name: "More Culling",
        fallback_desc: "Adds extra render culling to reduce wasted draw calls. Pairs well with Sodium and Cloth Config.",
        loaders: &["fabric", "quilt", "neoforge"],
        tags: &["rendering", "configuration"],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 60,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "sodium-extra",
        name: "Sodium Extra",
        fallback_desc: "Adds advanced Sodium video options, visual controls, FPS display, and OptiFine-like toggles.",
        loaders: &["fabric", "quilt", "neoforge"],
        tags: &["rendering", "quality-of-life"],
        default_enabled: true,
        install_rank: 1,
        keep_priority: 70,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "reeses-sodium-options",
        name: "Reese's Sodium Options",
        fallback_desc: "Replaces Sodium's video settings with a clearer, more scalable options screen.",
        loaders: &["fabric", "quilt", "neoforge"],
        tags: &["rendering", "quality-of-life"],
        default_enabled: true,
        install_rank: 1,
        keep_priority: 40,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "iris",
        name: "Iris Shaders",
        fallback_desc: "Adds shader-pack support designed to work with Sodium.",
        loaders: &["fabric", "quilt"],
        tags: &["shaders", "rendering"],
        default_enabled: true,
        install_rank: 1,
        keep_priority: 80,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "krypton",
        name: "Krypton",
        fallback_desc: "Optimizes Minecraft's networking stack for multiplayer and packet-heavy sessions.",
        loaders: &["fabric", "quilt"],
        tags: &["networking"],
        default_enabled: true,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "c2me-fabric",
        name: "C2ME",
        fallback_desc: "Parallelizes chunk generation and loading. Powerful, but best kept opt-in for easier crash isolation.",
        loaders: &["fabric", "quilt"],
        tags: &["opt-in", "worldgen", "experimental"],
        default_enabled: false,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "noisium",
        name: "Noisium",
        fallback_desc: "Optimizes world generation noise calculations. Useful for worldgen-heavy play, but opt-in by default.",
        loaders: &["fabric", "quilt", "neoforge"],
        tags: &["opt-in", "worldgen"],
        default_enabled: false,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "scalablelux",
        name: "ScalableLux",
        fallback_desc: "Optimizes lighting performance. Kept opt-in because lighting changes can be harder to diagnose.",
        loaders: &["fabric", "quilt"],
        tags: &["opt-in", "lighting"],
        default_enabled: false,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
    ModDef {
        slug: "badoptimizations",
        name: "BadOptimizations",
        fallback_desc: "Applies smaller targeted optimizations outside the main rendering stack. Useful, but best opt-in.",
        loaders: &["fabric", "forge", "neoforge"],
        tags: &["opt-in", "advanced"],
        default_enabled: false,
        install_rank: 2,
        keep_priority: 50,
        min_mc_version: None,
        max_mc_version: None,
    },
];

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AutoMod {
    pub project_id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default = "default_install_rank")]
    pub install_rank: u8,
    #[serde(default = "default_keep_priority")]
    pub keep_priority: u8,
    #[serde(default)]
    pub min_mc_version: Option<String>,
    #[serde(default)]
    pub max_mc_version: Option<String>,
}

fn default_install_rank() -> u8 {
    2
}

fn default_keep_priority() -> u8 {
    50
}

pub async fn get_recommended_mods(loader: &str) -> Vec<RecommendedMod> {
    let filtered: Vec<&ModDef> = BUILTIN_MODS
        .iter()
        .filter(|m| m.loaders.contains(&loader))
        .collect();

    if filtered.is_empty() {
        return vec![];
    }

    let client = match modrinth_client() {
        Ok(client) => client,
        Err(_) => {
            return filtered
                .iter()
                .map(|definition| recommended_from_def(definition))
                .collect();
        }
    };

    let slugs: Vec<String> = filtered.iter().map(|m| m.slug.to_string()).collect();
    let projects = fetch_projects(&client, &slugs).await;

    filtered
        .iter()
        .map(|m| recommended_from_project(m, &projects))
        .collect()
}

pub async fn get_all_recommended_mods() -> Vec<RecommendedMod> {
    let client = match modrinth_client() {
        Ok(client) => client,
        Err(_) => return BUILTIN_MODS.iter().map(recommended_from_def).collect(),
    };

    let slugs: Vec<String> = BUILTIN_MODS.iter().map(|m| m.slug.to_string()).collect();
    let projects = fetch_projects(&client, &slugs).await;

    BUILTIN_MODS
        .iter()
        .map(|m| recommended_from_project(m, &projects))
        .collect()
}

pub fn load_auto_mods(path: &std::path::Path) -> Result<Vec<AutoMod>, String> {
    Ok(load_auto_mods_checked(path)?
        .map(|mods| merge_builtin_auto_mods(mods, None))
        .unwrap_or_default())
}

const POLICY_LIMIT: u64 = 4 * 1024 * 1024;
const JOURNAL: &str = "auto_mods.pending.json";
const SMART_PROFILES: [&str; 2] = ["smart:latest-plus", "smart:snapshot-plus"];

#[cfg(test)]
mod policy_tests {
    use super::*;

    fn entry(enabled: bool) -> Result<AutoMod, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "project_id":"sodium", "name":"Sodium", "description":"fixture",
            "enabled":enabled, "loaders":["fabric"]
        }))
    }

    #[test]
    fn incomplete_goal_survives_restart_and_identical_retry()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("auto_mods.json");
        let desired = vec![entry(true)?];
        let mut update = PolicyUpdate::open(&path, Some(&desired))?;
        assert_eq!(update.pending().len(), 2);
        assert!(PolicyUpdate::open(&path, None).is_err());
        update.complete_profile(SMART_PROFILES[0])?;
        drop(update); // Simulates termination before the second profile commits.
        assert_eq!(load_auto_mods_checked(&path)?, Some(desired.clone()));
        let mut retry = PolicyUpdate::open(&path, Some(&desired))?;
        let pending = retry.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, SMART_PROFILES[1]);
        assert_eq!(pending[0].1, desired);
        retry.complete_profile(SMART_PROFILES[1])?;
        retry.finish()?;
        assert!(!temp.path().join(JOURNAL).exists());
        assert_eq!(load_auto_mods_checked(&path)?, Some(desired));
        Ok(())
    }

    #[test]
    fn newer_goal_supersedes_old_pending_choice_and_corruption_blocks_replacement()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("auto_mods.json");
        drop(PolicyUpdate::open(&path, Some(&[entry(true)?]))?);
        let retry = PolicyUpdate::open(&path, Some(&[entry(false)?]))?;
        for (_, changes) in retry.pending() {
            assert_eq!(changes.len(), 1);
            assert!(!changes[0].enabled);
        }
        drop(retry);
        std::fs::write(temp.path().join(JOURNAL), b"{")?;
        let before = std::fs::read(&path)?;
        assert!(PolicyUpdate::open(&path, Some(&[entry(true)?])).is_err());
        assert!(load_auto_mods_checked(&path).is_err());
        assert_eq!(std::fs::read(&path)?, before);
        assert_eq!(std::fs::read(temp.path().join(JOURNAL))?, b"{");
        Ok(())
    }

    #[tokio::test]
    async fn initialization_does_not_replace_invalid_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("auto_mods.json");
        for bytes in [b"{".as_slice(), b"null", b"[{}]"] {
            std::fs::write(&path, bytes)?;
            assert!(init_auto_mods(&path, "unknown").await.is_err());
            assert_eq!(std::fs::read(&path)?, bytes);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyGoal {
    schema: u32,
    desired: Vec<AutoMod>,
    pending: BTreeMap<String, Vec<AutoMod>>,
}

fn validate_policy(mods: &[AutoMod]) -> Result<(), String> {
    let mut projects = std::collections::BTreeSet::new();
    if mods.len() > 4096 {
        return Err("recommendation policy has too many projects".into());
    }
    for entry in mods {
        if !super::mod_intent::valid_project(&entry.project_id)
            || !projects.insert(&entry.project_id)
        {
            return Err("invalid or duplicate recommendation project".into());
        }
    }
    Ok(())
}

fn read_policy<T: serde::de::DeserializeOwned>(
    root: &cap_std::fs::Dir,
    name: &Path,
) -> Result<Option<T>, String> {
    if managed_fs::inspect(root, name)?.is_none() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(root, name)?
        .take(POLICY_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read recommendation policy: {error}"))?;
    if bytes.len() as u64 > POLICY_LIMIT {
        return Err("recommendation policy exceeds size limit".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("invalid recommendation policy: {error}"))
}

fn read_goal(root: &cap_std::fs::Dir) -> Result<Option<PolicyGoal>, String> {
    let goal: Option<PolicyGoal> = read_policy(root, Path::new(JOURNAL))?;
    if let Some(goal) = &goal {
        if goal.schema != 1
            || goal
                .pending
                .keys()
                .any(|id| !SMART_PROFILES.contains(&id.as_str()))
        {
            return Err("invalid recommendation propagation goal".into());
        }
        validate_policy(&goal.desired)?;
        for changes in goal.pending.values() {
            validate_policy(changes)?;
        }
    }
    Ok(goal)
}

/// A pending goal is authoritative until its complete policy has reached every target.
pub fn load_auto_mods_checked(path: &Path) -> Result<Option<Vec<AutoMod>>, String> {
    let parent = path.parent().ok_or("recommendation policy has no parent")?;
    let root = managed_fs::root(parent)?;
    if let Some(goal) = read_goal(&root)? {
        return Ok(Some(goal.desired));
    }
    let name = path
        .file_name()
        .ok_or("recommendation policy has no filename")?;
    let mods: Option<Vec<AutoMod>> = read_policy(&root, Path::new(name))?;
    if let Some(mods) = &mods {
        validate_policy(mods)?;
    }
    Ok(mods)
}

/// Serializes policy writers across processes; profile edits never acquire this lock.
pub(crate) struct PolicyUpdate {
    root: cap_std::fs::Dir,
    _lock: std::fs::File,
    goal: Option<PolicyGoal>,
}

impl PolicyUpdate {
    pub(crate) fn open(path: &Path, desired: Option<&[AutoMod]>) -> Result<Self, String> {
        let parent = path.parent().ok_or("recommendation policy has no parent")?;
        let root = managed_fs::root(parent)?;
        let lock_name = Path::new(".auto_mods.policy.lock");
        managed_fs::inspect(&root, lock_name)?;
        let lock = root
            .open_with(
                lock_name,
                cap_std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true),
            )
            .map_err(|error| format!("failed to open recommendation policy lock: {error}"))?
            .into_std();
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|error| format!("recommendation policy is busy or unavailable: {error}"))?;
        let goal = read_goal(&root)?;
        let mut update = Self {
            root,
            _lock: lock,
            goal,
        };
        if let Some(desired) = desired {
            validate_policy(desired)?;
            let previous = match &update.goal {
                Some(goal) => goal.desired.clone(),
                None => load_auto_mods_checked(path)?.unwrap_or_default(),
            };
            let mut changes: Vec<_> = desired
                .iter()
                .filter(|new| {
                    previous
                        .iter()
                        .find(|old| old.project_id == new.project_id)
                        .is_none_or(|old| old.enabled != new.enabled)
                })
                .cloned()
                .collect();
            for old in &previous {
                if !desired.iter().any(|new| new.project_id == old.project_id) {
                    let mut removed = old.clone();
                    removed.enabled = false;
                    changes.push(removed);
                }
            }
            let mut pending = match update.goal.take() {
                Some(goal) => goal.pending,
                None => BTreeMap::new(),
            };
            if !changes.is_empty() {
                for id in SMART_PROFILES {
                    let target = pending.entry(id.into()).or_default();
                    for change in &changes {
                        target.retain(|old| old.project_id != change.project_id);
                        target.push(change.clone());
                    }
                }
            }
            update.goal = Some(PolicyGoal {
                schema: 1,
                desired: desired.to_vec(),
                pending,
            });
            update.persist()?;
        }
        if let Some(goal) = &update.goal {
            save_auto_mods_to_file(path, &goal.desired)?;
        }
        Ok(update)
    }

    fn persist(&self) -> Result<(), String> {
        let goal = self
            .goal
            .as_ref()
            .ok_or("no recommendation propagation goal")?;
        let bytes = serde_json::to_vec(goal).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > POLICY_LIMIT {
            return Err("recommendation propagation goal exceeds size limit".into());
        }
        managed_fs::write(&self.root, Path::new(JOURNAL), &bytes, true)
    }

    pub(crate) fn pending(&self) -> Vec<(String, Vec<AutoMod>)> {
        match &self.goal {
            Some(goal) => goal
                .pending
                .iter()
                .map(|(id, changes)| (id.clone(), changes.clone()))
                .collect(),
            None => Vec::new(),
        }
    }

    pub(crate) fn complete_profile(&mut self, id: &str) -> Result<(), String> {
        let goal = self
            .goal
            .as_mut()
            .ok_or("no recommendation propagation goal")?;
        goal.pending.remove(id);
        self.persist()
    }

    pub(crate) fn finish(self) -> Result<(), String> {
        if let Some(goal) = &self.goal {
            if !goal.pending.is_empty() {
                return Err("recommendation propagation is incomplete".into());
            }
            self.root
                .remove_file(JOURNAL)
                .map_err(|error| format!("failed to finish recommendation propagation: {error}"))?;
        }
        Ok(())
    }
}

pub fn save_auto_mods_to_file(path: &std::path::Path, mods: &[AutoMod]) -> Result<(), String> {
    validate_policy(mods)?;
    let json = serde_json::to_string_pretty(mods)
        .map_err(|e| format!("auto_mods.json serialize failed: {}", e))?;
    let root = managed_fs::root(path.parent().ok_or("recommendation policy has no parent")?)?;
    managed_fs::write(
        &root,
        Path::new(
            path.file_name()
                .ok_or("recommendation policy has no filename")?,
        ),
        json.as_bytes(),
        true,
    )
}

pub async fn init_auto_mods(
    path: &std::path::Path,
    gpu_vendor: &str,
) -> Result<Vec<AutoMod>, String> {
    let update = PolicyUpdate::open(path, None)?;
    if let Some(existing) = load_auto_mods_checked(path)? {
        return Ok(existing);
    }
    let is_nvidia = gpu_vendor == "nvidia";
    let fallback = || -> Vec<AutoMod> {
        BUILTIN_MODS
            .iter()
            .map(|m| auto_mod_from_def(m, is_nvidia, None))
            .collect()
    };

    let client = match modrinth_client() {
        Ok(client) => client,
        Err(_) => {
            let mods = fallback();
            save_auto_mods_to_file(path, &mods)?;
            return Ok(mods);
        }
    };

    let slugs: Vec<String> = BUILTIN_MODS.iter().map(|m| m.slug.to_string()).collect();
    let projects = fetch_projects(&client, &slugs).await;

    let mods: Vec<AutoMod> = BUILTIN_MODS
        .iter()
        .map(|m| {
            let project = projects.iter().find(|p| p.slug == m.slug || p.id == m.slug);
            auto_mod_from_def(m, is_nvidia, project)
        })
        .collect();

    save_auto_mods_to_file(path, &mods)?;
    update.finish()?;
    Ok(mods)
}

fn merge_builtin_auto_mods(mut saved: Vec<AutoMod>, is_nvidia: Option<bool>) -> Vec<AutoMod> {
    for def in BUILTIN_MODS {
        if let Some(existing) = saved
            .iter()
            .position(|mod_entry| mod_entry.project_id == def.slug || mod_entry.name == def.name)
        {
            hydrate_builtin_metadata(&mut saved[existing], def, is_nvidia);
            continue;
        }
        let auto_mod = match is_nvidia {
            Some(value) => auto_mod_from_def(def, value, None),
            None => auto_mod_from_def(def, !def.tags.contains(&"nvidia-only"), None),
        };
        saved.push(auto_mod);
    }
    saved
}

fn hydrate_builtin_metadata(entry: &mut AutoMod, def: &ModDef, is_nvidia: Option<bool>) {
    if entry.tags.is_empty() {
        entry.tags = def.tags.iter().map(|tag| tag.to_string()).collect();
    }
    if def.tags.contains(&"nvidia-only")
        && matches!(is_nvidia, Some(false))
        && !entry.tags.iter().any(|tag| tag == "unsupported-gpu")
    {
        entry.tags.push("unsupported-gpu".to_string());
    }
    if entry.loaders.is_empty() {
        entry.loaders = def
            .loaders
            .iter()
            .map(|loader| loader.to_string())
            .collect();
    }
    entry.install_rank = def.install_rank;
    entry.keep_priority = def.keep_priority;
    if entry.min_mc_version.is_none() {
        entry.min_mc_version = def.min_mc_version.map(|value| value.to_string());
    }
    if entry.max_mc_version.is_none() {
        entry.max_mc_version = def.max_mc_version.map(|value| value.to_string());
    }
}

pub(super) async fn fetch_projects(
    client: &reqwest::Client,
    ids: &[String],
) -> Vec<ModrinthProject> {
    if ids.is_empty() {
        return vec![];
    }
    let ids_json = match serde_json::to_string(ids) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let url = format!(
        "https://api.modrinth.com/v2/projects?ids={}",
        urlencoding::encode(&ids_json)
    );
    match client.get(&url).send().await {
        Ok(r) => r.json::<Vec<ModrinthProject>>().await.unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn modrinth_client() -> Result<reqwest::Client, String> {
    crate::http::builder()?
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|error| error.to_string())
}

fn recommended_from_def(def: &ModDef) -> RecommendedMod {
    RecommendedMod {
        project_id: def.slug.to_string(),
        name: def.name.to_string(),
        description: def.fallback_desc.to_string(),
        icon_url: None,
        tags: def.tags.iter().map(|t| t.to_string()).collect(),
        default_enabled: def.default_enabled,
        install_rank: def.install_rank,
        keep_priority: def.keep_priority,
        min_mc_version: def.min_mc_version.map(|value| value.to_string()),
        max_mc_version: def.max_mc_version.map(|value| value.to_string()),
    }
}

fn recommended_from_project(def: &ModDef, projects: &[ModrinthProject]) -> RecommendedMod {
    let project = projects
        .iter()
        .find(|p| p.slug == def.slug || p.id == def.slug);
    RecommendedMod {
        project_id: project
            .map(|p| p.id.clone())
            .unwrap_or_else(|| def.slug.to_string()),
        name: def.name.to_string(),
        description: project
            .filter(|p| !p.description.is_empty())
            .map(|p| p.description.clone())
            .unwrap_or_else(|| def.fallback_desc.to_string()),
        icon_url: project.and_then(|p| p.icon_url.clone()),
        tags: def.tags.iter().map(|t| t.to_string()).collect(),
        default_enabled: def.default_enabled,
        install_rank: def.install_rank,
        keep_priority: def.keep_priority,
        min_mc_version: def.min_mc_version.map(|value| value.to_string()),
        max_mc_version: def.max_mc_version.map(|value| value.to_string()),
    }
}

fn auto_mod_from_def(def: &ModDef, is_nvidia: bool, project: Option<&ModrinthProject>) -> AutoMod {
    let mut tags: Vec<String> = def.tags.iter().map(|t| t.to_string()).collect();
    if def.tags.contains(&"nvidia-only") && !is_nvidia {
        tags.push("unsupported-gpu".to_string());
    }

    AutoMod {
        project_id: project
            .map(|p| p.id.clone())
            .unwrap_or_else(|| def.slug.to_string()),
        name: def.name.to_string(),
        description: project
            .filter(|p| !p.description.is_empty())
            .map(|p| p.description.clone())
            .unwrap_or_else(|| def.fallback_desc.to_string()),
        icon_url: project.and_then(|p| p.icon_url.clone()),
        enabled: def.default_enabled && (!def.tags.contains(&"nvidia-only") || is_nvidia),
        tags,
        loaders: def.loaders.iter().map(|l| l.to_string()).collect(),
        install_rank: def.install_rank,
        keep_priority: def.keep_priority,
        min_mc_version: def.min_mc_version.map(|value| value.to_string()),
        max_mc_version: def.max_mc_version.map(|value| value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{AutoMod, BUILTIN_MODS, merge_builtin_auto_mods};

    #[test]
    fn experience_mods_are_enabled_by_default() {
        for slug in ["dynamic-fps", "modmenu", "cloth-config"] {
            let entry = BUILTIN_MODS
                .iter()
                .find(|item| item.slug == slug)
                .unwrap_or_else(|| panic!("{slug} definition"));
            assert!(entry.default_enabled, "{slug} should be enabled by default");
        }
    }

    #[test]
    fn advanced_opt_in_mods_are_present_but_disabled() {
        for slug in [
            "moonrise-opt",
            "nvidium",
            "c2me-fabric",
            "noisium",
            "scalablelux",
            "badoptimizations",
        ] {
            let entry = BUILTIN_MODS
                .iter()
                .find(|item| item.slug == slug)
                .unwrap_or_else(|| panic!("{slug} definition"));
            assert!(!entry.default_enabled, "{slug} must be opt-in");
        }
    }

    #[test]
    fn nvidia_only_mod_is_not_auto_enabled_just_because_gpu_matches() {
        let nvidium = BUILTIN_MODS
            .iter()
            .find(|item| item.slug == "nvidium")
            .expect("nvidium definition");
        let auto_mod = super::auto_mod_from_def(nvidium, true, None);

        assert!(!auto_mod.enabled);
        assert!(auto_mod.tags.iter().any(|tag| tag == "nvidia-only"));
    }

    #[test]
    fn saved_auto_mods_are_extended_without_overwriting_user_choices() {
        let merged = merge_builtin_auto_mods(
            vec![AutoMod {
                project_id: "sodium".to_string(),
                name: "Custom Sodium".to_string(),
                description: "User edited description".to_string(),
                icon_url: None,
                enabled: false,
                tags: vec![],
                loaders: vec!["fabric".to_string()],
                install_rank: 2,
                keep_priority: 50,
                min_mc_version: None,
                max_mc_version: None,
            }],
            None,
        );

        let sodium = merged
            .iter()
            .find(|item| item.project_id == "sodium")
            .expect("existing sodium setting");
        let cloth_config = merged
            .iter()
            .find(|item| item.project_id == "cloth-config")
            .expect("new cloth config default");

        assert!(!sodium.enabled);
        assert_eq!(sodium.name, "Custom Sodium");
        assert_eq!(sodium.description, "User edited description");
        assert_eq!(sodium.loaders, vec!["fabric"]);
        assert!(cloth_config.enabled);
    }
}
