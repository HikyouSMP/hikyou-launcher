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
        .filter(|entry| {
            entry.version_type == kind
                || (key == "snapshot-plus" && entry.version_type == manifest::VersionType::Release)
        })
        .collect();
    entries.sort_by(|a, b| b.release_time.cmp(&a.release_time));
    let mut result = if key == "snapshot-plus" {
        Vec::new()
    } else {
        vec![latest.clone()]
    };
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
    if result.is_empty() {
        return Err("No known channel targets are available".into());
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
    let effective = selection::for_target(paths, intent, &target)?;
    attempt_plan(
        paths,
        id,
        Attempt {
            game,
            head,
            intent: &effective,
            target,
            aliases,
            requires_decision: false,
        },
    )
    .await
}

struct Attempt<'a> {
    game: &'a Path,
    head: Option<&'a ModRevision>,
    intent: &'a Intent,
    target: ModTarget,
    aliases: &'a std::collections::BTreeMap<String, String>,
    requires_decision: bool,
}

async fn attempt_plan(
    paths: &LauncherPaths,
    id: &str,
    plan: Attempt<'_>,
) -> Result<ModRevision, String> {
    let Attempt {
        game,
        head,
        intent,
        target,
        aliases,
        requires_decision,
    } = plan;
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
    seal_plan(
        paths,
        id,
        working.path().to_path_buf(),
        SealedPlan {
            target,
            parent: head.map(|head| head.id.clone()),
            intent: intent.clone(),
            edges,
            requires_decision,
        },
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
        let target = normal_target(paths, id).await?;
        return match attempt(paths, id, game, head, intent, target.clone(), aliases).await {
            Ok(revision) => Ok(revision),
            Err(error) if unsatisfiable(&error) => {
                if let Some(revision) =
                    alternative(paths, id, game, head, intent, &[target], aliases).await?
                {
                    return Ok(revision);
                }
                Err(error)
            }
            Err(error) => Err(error),
        };
    };
    let manifest = manifest::refresh_manifest(paths).await?;
    let mut last =
        "mod_resolution_unsatisfiable: no supported target satisfies the requested configuration"
            .to_string();
    for (index, mc) in minecraft_candidates(&manifest, key, head)?
        .into_iter()
        .enumerate()
    {
        let versions = match fabric::fetch_loader_versions(&mc).await {
            Ok(versions) => versions,
            // The provider distinguishes an empty supported-version result from HTTP failure.
            Err(error) if error == format!("Minecraft {mc} is not supported by Fabric") => {
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        let mut targets = loader_candidates(&mc, &versions, head);
        if targets.is_empty() && !intent.independent {
            targets.push(ModTarget {
                mc_version: mc,
                loader: "vanilla".into(),
                loader_version: None,
            });
        }
        for target in &targets {
            log::debug!(
                "[mods] Preparing requested graph for Minecraft {} / {} {}",
                target.mc_version,
                target.loader,
                target.loader_version.as_deref().unwrap_or("none")
            );
            match attempt(paths, id, game, head, intent, target.clone(), aliases).await {
                Ok(revision) => return Ok(revision),
                Err(error) if unsatisfiable(&error) => last = error,
                Err(error) => return Err(error),
            }
        }
        // Only offer a tradeoff for the newest target, never a hidden downgrade.
        // Try every full-graph loader candidate before considering omissions.
        if index == 0
            && let Some(revision) =
                alternative(paths, id, game, head, intent, &targets, aliases).await?
        {
            return Ok(revision);
        }
    }
    Err(last)
}

pub(super) fn without(intent: &Intent, omitted: &[String]) -> Result<Intent, String> {
    let mut effective = intent.clone();
    let selected: std::collections::BTreeSet<_> = omitted.iter().collect();
    if selected.len() != omitted.len() || omitted.is_empty() {
        return Err("Invalid Mod omission set".into());
    }
    for key in &selected {
        let request = effective
            .requirements
            .iter_mut()
            .find(|r| r.key() == **key && r.enabled)
            .ok_or("Only an enabled Mod wish can be temporarily omitted")?;
        request.enabled = false;
        if let Some(name) = &mut request.filename
            && !name.ends_with(".disabled")
        {
            name.push_str(".disabled");
        }
        // The reduced resolution records an allowed absence, not a new durable
        // exclusion or a rewrite of the user's explicit inclusion provenance.
        effective.choice_provenance.remove(*key);
    }
    effective.validate(&intent.profile_id)?;
    Ok(effective)
}

async fn alternative(
    paths: &LauncherPaths,
    id: &str,
    game: &Path,
    head: Option<&ModRevision>,
    intent: &Intent,
    targets: &[ModTarget],
    aliases: &std::collections::BTreeMap<String, String>,
) -> Result<Option<ModRevision>, String> {
    let recommendations = if intent.independent {
        Vec::new()
    } else {
        mods::load_auto_mods(&paths.auto_mods_file())?
    };
    for target in targets {
        let target_intent = selection::for_target(paths, intent, target)?;
        let mut roots: Vec<_> = target_intent
            .requirements
            .iter()
            .filter(|request| request.enabled)
            .collect();
        // Important recommendations get their chance to return first. This is an
        // ordering heuristic, never permission to drop an unlisted root.
        roots.sort_by_key(|request| {
            (
                std::cmp::Reverse(
                    recommendations
                        .iter()
                        .find(|entry| request.project_id.as_ref() == Some(&entry.project_id))
                        .map(|entry| entry.keep_priority)
                        .unwrap_or(100),
                ),
                request.key(),
            )
        });
        let roots: Vec<_> = roots.into_iter().map(|request| request.key()).collect();
        let result = alternatives::find(&roots, 24, |omitted| {
            let target = target.clone();
            let target_intent = &target_intent;
            async move {
                let effective = without(target_intent, &omitted)?;
                match attempt_plan(
                    paths,
                    id,
                    Attempt {
                        game,
                        head,
                        intent: &effective,
                        target,
                        aliases,
                        requires_decision: true,
                    },
                )
                .await
                {
                    Ok(revision) => Ok(alternatives::Trial::Feasible(revision)),
                    Err(error) if unsatisfiable(&error) => Ok(alternatives::Trial::Conflict),
                    Err(error) => Err(error),
                }
            }
        })
        .await?;
        if let Some((_omitted, candidate)) = result {
            let root = paths.root();
            let selected = candidate.clone();
            tokio::task::spawn_blocking(move || ModRepository::new(&root).materialize(&selected))
                .await
                .map_err(|e| e.to_string())??;
            // The coordinator rechecks input versions under its short edit
            // lease before offering or activating this candidate.
            return Ok(Some(candidate));
        }
    }
    Ok(None)
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
    fn snapshot_tracks_the_newest_publication_even_when_it_is_a_release() -> Result<(), String> {
        let manifest = manifest::VersionManifest {
            latest: manifest::LatestVersions {
                release: "new-release".into(),
                snapshot: "older-snapshot".into(),
            },
            versions: [
                (
                    "older-snapshot",
                    manifest::VersionType::Snapshot,
                    "2026-10-01T00:00:00Z",
                ),
                (
                    "new-release",
                    manifest::VersionType::Release,
                    "2026-10-02T00:00:00Z",
                ),
            ]
            .into_iter()
            .map(|(id, version_type, date)| manifest::VersionEntry {
                id: id.into(),
                version_type,
                url: String::new(),
                sha1: String::new(),
                time: None,
                release_time: Some(date.into()),
            })
            .collect(),
        };
        assert_eq!(
            minecraft_candidates(&manifest, "snapshot-plus", None)?,
            ["new-release", "older-snapshot"]
        );
        assert_eq!(
            minecraft_candidates(&manifest, "latest-plus", None)?,
            ["new-release"]
        );
        Ok(())
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
            choice_sequence: 0,
            choice_provenance: Default::default(),
            profile_id: "smart:latest-plus".into(),
            recommendations_initialized: false,
            independent: false,
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
