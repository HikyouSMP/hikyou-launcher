use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize, Clone)]
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

pub fn load_auto_mods(path: &std::path::Path) -> Vec<AutoMod> {
    match std::fs::read_to_string(path) {
        Ok(s) => merge_builtin_auto_mods(serde_json::from_str(&s).unwrap_or_default(), None),
        Err(_) => vec![],
    }
}

pub fn save_auto_mods_to_file(path: &std::path::Path, mods: &[AutoMod]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(mods)
        .map_err(|e| format!("auto_mods.json serialize failed: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("auto_mods.json write failed: {}", e))
}

pub async fn init_auto_mods(path: &std::path::Path, gpu_vendor: &str) -> Vec<AutoMod> {
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
            let _ = save_auto_mods_to_file(path, &mods);
            return mods;
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

    let _ = save_auto_mods_to_file(path, &mods);
    mods
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

fn modrinth_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent("HikyouLauncher/1.0")
        .build()
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
