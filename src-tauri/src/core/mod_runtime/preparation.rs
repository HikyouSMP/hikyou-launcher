//! Bounded newest-first target search. Unknown failures never justify a downgrade.

use super::*;
use crate::core::mod_intent::Requirement;

const RECENT_MINECRAFT_TARGETS: usize = 8;
const RECENT_LOADER_TARGETS: usize = 3;

fn unsatisfiable(error: &str) -> bool {
    error.starts_with("mod_resolution_unsatisfiable:")
}

fn minecraft_candidates(
    manifest: &manifest::VersionManifest,
    key: &str,
    head: Option<&ModRevision>,
) -> Result<Vec<String>, String> {
    let (latest, kind) = match key {
        "latest-plus" => (&manifest.latest.release, manifest::VersionType::Release),
        "snapshot-plus" => (&manifest.latest.snapshot, manifest::VersionType::Snapshot),
        _ => return Err("not a smart profile".into()),
    };
    let mut entries: Vec<_> = manifest
        .versions
        .iter()
        .filter(|entry| entry.version_type == kind)
        .collect();
    entries.sort_by(|a, b| b.release_time.cmp(&a.release_time));
    let mut result = vec![latest.clone()];
    for entry in entries {
        if result.len() >= RECENT_MINECRAFT_TARGETS {
            break;
        }
        if !result.contains(&entry.id) {
            result.push(entry.id.clone());
        }
    }
    if let Some(head) = head
        && !result.contains(&head.target.mc_version)
    {
        result.push(head.target.mc_version.clone());
    }
    Ok(result)
}

fn loader_candidates(
    mc_version: &str,
    versions: &[fabric::FabricLoaderVersion],
    head: Option<&ModRevision>,
) -> Vec<ModTarget> {
    let mut loaders = Vec::new();
    // Stable releases precede previews, preserving the provider's newest-first order.
    for stable in [true, false] {
        for version in versions.iter().filter(|version| version.stable == stable) {
            if loaders.len() < RECENT_LOADER_TARGETS && !loaders.contains(&version.version) {
                loaders.push(version.version.clone());
            }
        }
    }
    if let Some(head) = head
        && head.target.mc_version == mc_version
        && head.target.loader == "fabric"
        && let Some(loader) = &head.target.loader_version
        && !loaders.contains(loader)
    {
        loaders.push(loader.clone());
    }
    loaders
        .into_iter()
        .map(|loader| ModTarget {
            mc_version: mc_version.into(),
            loader: "fabric".into(),
            loader_version: Some(loader),
        })
        .collect()
}

fn validate_loader(target: &ModTarget, intent: &Intent) -> Result<(), String> {
    if target.loader == "vanilla" && intent.requirements.iter().any(|request| request.enabled) {
        return Err("mod_resolution_unsatisfiable: Vanilla cannot load the requested Mods".into());
    }
    Ok(())
}

async fn attempt(
    paths: &LauncherPaths,
    id: &str,
    game: &Path,
    head: Option<&ModRevision>,
    intent: &Intent,
    target: ModTarget,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Result<ModRevision, String> {
    validate_loader(&target, intent)?;
    let root = paths.root();
    let source = game.to_path_buf();
    let working =
        tokio::task::spawn_blocking(move || workspace::stage(&LauncherPaths::new(root), &source))
            .await
            .map_err(|error| error.to_string())??;
    requests::normalize_stage(working.path(), aliases)?;
    let mut previous_edges = requests::stage_requests(paths, working.path(), head, intent).await?;
    for edge in &mut previous_edges {
        if let Some(canonical) = aliases.get(&edge.requester) {
            edge.requester = canonical.clone();
        }
        if let Some(canonical) = aliases.get(&edge.dependency) {
            edge.dependency = canonical.clone();
        }
    }
    let mut requirements = intent.requirements.clone();
    requirements.extend(
        intent
            .excluded_projects
            .iter()
            .map(|project| Requirement::project(project, false, None, None, "user")),
    );
    let edges = if target.loader != "vanilla" {
        let java = preparation_java(paths, &target).await?;
        mods::reconcile_requested_mods(
            working.path(),
            &requirements,
            &previous_edges,
            &target.mc_version,
            &target.loader,
            target.loader_version.as_deref(),
            Some(java),
        )
        .await?
    } else {
        Vec::new()
    };
    validate_inputs(paths, working.path(), &target, true).await?;
    seal_requested(
        paths,
        id,
        working.path().to_path_buf(),
        target,
        head.map(|head| head.id.clone()),
        intent.clone(),
        edges,
    )
    .await
}

pub(super) async fn resolve(
    paths: &LauncherPaths,
    id: &str,
    game: &Path,
    head: Option<&ModRevision>,
    intent: &Intent,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Result<ModRevision, String> {
    tokio::time::timeout(
        Duration::from_secs(300),
        resolve_inner(paths, id, game, head, intent, aliases),
    )
    .await
    .map_err(|_| "mod_resolution_limit: target preparation wall time".to_string())?
}

async fn resolve_inner(
    paths: &LauncherPaths,
    id: &str,
    game: &Path,
    head: Option<&ModRevision>,
    intent: &Intent,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Result<ModRevision, String> {
    let Some(key) = profile::smart_key_from_ref(id) else {
        return attempt(
            paths,
            id,
            game,
            head,
            intent,
            normal_target(paths, id).await?,
            aliases,
        )
        .await;
    };
    let manifest = manifest::refresh_manifest(paths).await?;
    let mut last =
        "mod_resolution_unsatisfiable: no supported target satisfies the requested configuration"
            .to_string();
    for mc in minecraft_candidates(&manifest, key, head)? {
        let versions = match fabric::fetch_loader_versions(&mc).await {
            Ok(versions) => versions,
            // The provider distinguishes an empty supported-version result from HTTP failure.
            Err(error) if error == format!("Minecraft {mc} is not supported by Fabric") => {
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        let mut targets = loader_candidates(&mc, &versions, head);
        if targets.is_empty() {
            targets.push(ModTarget {
                mc_version: mc,
                loader: "vanilla".into(),
                loader_version: None,
            });
        }
        for target in targets {
            log::debug!(
                "[mods] Preparing requested graph for Minecraft {} / {} {}",
                target.mc_version,
                target.loader,
                target.loader_version.as_deref().unwrap_or("none")
            );
            match attempt(paths, id, game, head, intent, target, aliases).await {
                Ok(revision) => return Ok(revision),
                Err(error) if unsatisfiable(&error) => last = error,
                Err(error) => return Err(error),
            }
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_proven_conflicts_allow_fallback() {
        assert!(unsatisfiable("mod_resolution_unsatisfiable: conflict"));
        assert!(!unsatisfiable("mod_resolution_limit: search exhausted"));
        assert!(!unsatisfiable("HTTP request failed"));
    }

    #[test]
    fn vanilla_never_claims_to_load_enabled_mods() {
        let target = ModTarget {
            mc_version: "1.21.1".into(),
            loader: "vanilla".into(),
            loader_version: None,
        };
        let mut intent = Intent {
            schema: 1,
            profile_id: "smart:latest-plus".into(),
            recommendations_initialized: false,
            requirements: Vec::new(),
            excluded_projects: Vec::new(),
        };
        assert!(validate_loader(&target, &intent).is_ok());
        intent
            .requirements
            .push(Requirement::project("example", true, None, None, "user"));
        assert!(validate_loader(&target, &intent).is_err());
        intent.requirements[0].enabled = false;
        assert!(validate_loader(&target, &intent).is_ok());
    }

    #[test]
    fn target_order_is_bounded_and_preserves_the_current_fallback() {
        let manifest = manifest::VersionManifest {
            latest: manifest::LatestVersions {
                release: "20".into(),
                snapshot: "snapshot".into(),
            },
            versions: (0..20)
                .rev()
                .map(|version| manifest::VersionEntry {
                    id: version.to_string(),
                    version_type: manifest::VersionType::Release,
                    url: String::new(),
                    sha1: String::new(),
                    time: None,
                    release_time: Some(format!("2026-10-{version:02}")),
                })
                .collect(),
        };
        let candidates = minecraft_candidates(&manifest, "latest-plus", None).unwrap();
        assert_eq!(candidates.len(), RECENT_MINECRAFT_TARGETS);
        assert_eq!(&candidates[..3], ["20", "19", "18"]);
        assert!(!candidates.contains(&"snapshot".into()));
        let versions = vec![
            fabric::FabricLoaderVersion {
                version: "preview".into(),
                stable: false,
            },
            fabric::FabricLoaderVersion {
                version: "stable".into(),
                stable: true,
            },
        ];
        assert_eq!(
            loader_candidates("20", &versions, None)[0]
                .loader_version
                .as_deref(),
            Some("stable")
        );
    }
}
