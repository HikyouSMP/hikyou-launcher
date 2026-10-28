//! Mod 管理モジュール

#[path = "mod_solver.rs"]
mod requested_solver;
pub use requested_solver::{canonical_modrinth_project_id, reconcile_requested_mods};

#[cfg(test)]
use super::mod_metadata::{
    fabric_mod_json_has_invalid_wildcard_predicate, fabric_mod_json_incompatible_dependencies,
};
#[cfg(test)]
use super::modrinth_provider::{
    is_exact_modrinth_match, modrinth_slug_candidates_for_mod_id, select_compatible_version,
};
use super::{
    mod_installer::{
        ModInstallPlan, PendingModInstall, commit_mod_install_plan, disable_mod_file,
        remove_project_files,
    },
    mod_metadata::{
        FabricDependency, InstalledModManifest, fabric_mod_json_required_dependencies,
        mod_jar_is_loadable_for_loader, read_installed_mod_manifest,
    },
    mod_sync_state,
    modrinth_provider::{
        FABRIC_API_PROJECT_ID, ModrinthFile, ModrinthVersion, artifact_is_usable_for_mc,
        artifact_satisfies_predicate, compare_release_versions, download_mod_bytes,
        fetch_modrinth_version_by_id, fetch_modrinth_versions_for_project,
        minecraft_version_satisfies_fabric_predicate, normalize_minecraft_version_for_fabric,
        resolve_dependency_project_id, resolve_modrinth_project_from_mod_id,
        select_compatible_file, select_compatible_versions, verify_mod_bytes,
    },
};
pub use crate::core::mod_files::{ModFile, backfill_metadata, list_mods, remove_mod, toggle_mod};
pub use crate::core::mod_recommendations::{
    AutoMod, RecommendedMod, get_all_recommended_mods, get_recommended_mods, init_auto_mods,
    load_auto_mods, save_auto_mods_to_file,
};
pub use crate::core::modpacks::{
    ModpackVersionInfo, get_modpack_versions, install_modpack, search_modpacks,
};
use crate::core::{
    cache, managed_fs,
    mod_files::{
        inspect_mod_path, is_safe_filename, load_meta, managed_mods, mods_dir, open_mods,
        read_mod_file, save_meta,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::fs;

// ── キャッシュ定数 ─────────────────────────────────────────────────────────────
const CACHE_MOD_SEARCH: &str = "modrinth_mod_search";
const TTL_SEARCH: i64 = 300; // 5 分
const AUTO_MOD_RESOLVER_VERSION: u8 = 6;

type SelectedModCandidate = (
    ModrinthVersion,
    ModrinthFile,
    Option<Vec<u8>>,
    u64,
    Vec<RequiredModrinthDependency>,
);

// ── 公開型 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModSearchResult {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub slug: String,
}

// ── Modrinth 検索 ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthHit>,
}

#[derive(Deserialize)]
struct ModrinthHit {
    project_id: String,
    title: String,
    description: String,
    downloads: u64,
    icon_url: Option<String>,
    slug: String,
}

pub async fn search_modrinth(
    query: &str,
    loader: &str,
    mc_version: &str,
) -> Result<Vec<ModSearchResult>, String> {
    let alias = format!("{}|{}|{}", query, loader, mc_version);

    // 有効なキャッシュがあれば即返す
    if let Some(c) = cache::get()
        && let Some(cached) = c
            .get::<Vec<ModSearchResult>>(CACHE_MOD_SEARCH, &alias)
            .await
    {
        return Ok(cached);
    }

    // 期限切れキャッシュから ETag を取得（条件付きリクエスト用）
    let stale = if let Some(c) = cache::get() {
        c.get_stale_with_etag::<Vec<ModSearchResult>>(CACHE_MOD_SEARCH, &alias)
            .await
    } else {
        None
    };

    // ローダー・バージョンが空の場合はフィルターをスキップ（自動導入Mod設定画面用）
    let facets = {
        let mut parts = vec![r#"["project_type:mod"]"#.to_string()];
        if !loader.is_empty() {
            let lf = match loader {
                "fabric" => "fabric",
                "quilt" => "quilt",
                "forge" => "forge",
                "neoforge" => "neoforge",
                _ => "fabric",
            };
            parts.push(format!(r#"["categories:{}"]"#, lf));
        }
        if !mc_version.is_empty() {
            parts.push(format!(r#"["versions:{}"]"#, mc_version));
        }
        parts.push(
            r#"["client_side:required","client_side:optional","client_side:unsupported"]"#
                .to_string(),
        );
        format!("[{}]", parts.join(","))
    };
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&facets={}&limit=12",
        urlencoding::encode(query),
        urlencoding::encode(&facets),
    );

    let client = crate::http::builder()?
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(&url);
    if let Some((_, Some(ref etag))) = stale {
        req = req.header("If-None-Match", etag.as_str());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Modrinth search failed: {}", e))?;

    // 304: データ未変更 → TTL を更新して既存データを返す
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED
        && let Some((data, etag)) = stale
    {
        if let Some(c) = cache::get() {
            c.set_with_etag(CACHE_MOD_SEARCH, &alias, &data, TTL_SEARCH, etag.as_deref())
                .await
                .ok();
        }
        return Ok(data);
    }

    let new_etag = resp
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let parsed: ModrinthSearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse Modrinth response: {}", e))?;

    let results: Vec<ModSearchResult> = parsed
        .hits
        .into_iter()
        .map(|h| ModSearchResult {
            project_id: h.project_id,
            title: h.title,
            description: h.description,
            downloads: h.downloads,
            icon_url: h.icon_url,
            slug: h.slug,
        })
        .collect();

    if let Some(c) = cache::get() {
        c.set_with_etag(
            CACHE_MOD_SEARCH,
            &alias,
            &results,
            TTL_SEARCH,
            new_etag.as_deref(),
        )
        .await
        .ok();
    }
    Ok(results)
}

// ── Modrinth インストール ─────────────────────────────────────────────────────

#[derive(Clone)]
struct AutoModPlanItem {
    project_id: String,
    display_name: Option<String>,
    icon_url: Option<String>,
    keep_priority: u8,
    plan: ModInstallPlan,
}

struct AutoModPlanSet {
    items: Vec<AutoModPlanItem>,
    suppressed_project_ids: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RequiredModrinthDependency {
    project_id: String,
    mod_id: String,
    predicate: Option<String>,
}

struct ActiveModManifest {
    project_id: String,
    manifest: InstalledModManifest,
}

async fn resolve_modrinth_mod_install_plan(
    game_dir: &Path,
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Result<ModInstallPlan, String> {
    let loader_str = match loader {
        "fabric" | "quilt" | "forge" | "neoforge" => loader,
        _ => return Err("this loader does not support mod installation".to_string()),
    };

    let client = crate::http::builder()?
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let dir = open_mods(game_dir, true)?;

    let mut pending_installs: Vec<PendingModInstall> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<(String, bool, Vec<String>, Option<String>)> =
        vec![(project_id.to_string(), false, Vec::new(), None)];
    let mut selected_versions = HashMap::<String, Option<String>>::new();

    while let Some((pid, is_dependency, predicates, exact_version)) = queue.pop() {
        if is_dependency && project_is_disabled(game_dir, &pid)? {
            return Err(format!("required dependency {pid} is disabled by the user"));
        }
        if !visited.insert(pid.clone()) {
            if exact_version.is_some() && selected_versions.get(&pid) != Some(&exact_version) {
                return Err(format!(
                    "required dependency exact version conflict for {pid}"
                ));
            }
            continue;
        }
        let is_main = pid == project_id;
        let mut effective_predicates = predicates;
        if !is_dependency {
            effective_predicates.extend(
                installed_dependency_predicates_for_project(
                    &client, game_dir, &pid, mc_version, loader_str,
                )
                .await?,
            );
            effective_predicates.sort();
            effective_predicates.dedup();
        }

        if exact_version.is_none()
            && let Some((existing, jar_dependencies)) = existing_project_install(
                &client,
                game_dir,
                &pid,
                mc_version,
                loader_str,
                &effective_predicates,
            )
            .await?
        {
            selected_versions.insert(pid.clone(), existing.version_id.clone());
            for dep in jar_dependencies {
                if dep.project_id != pid && !visited.contains(&dep.project_id) {
                    let predicates = dependency_project_predicates(&dep);
                    queue.push((dep.project_id, true, predicates, None));
                }
            }
            pending_installs.push(existing);
            continue;
        }

        let versions = if let Some(id) = exact_version.as_deref() {
            let version = fetch_modrinth_version_by_id(&client, id).await?;
            if version.project_id.as_deref() != Some(pid.as_str()) {
                return Err(format!(
                    "required dependency version project mismatch for {pid}"
                ));
            }
            vec![version]
        } else {
            fetch_modrinth_versions_for_project(&client, &pid, mc_version, loader_str, is_main)
                .await?
        };

        let mut selected: Option<SelectedModCandidate> = None;
        let no_versions = versions.is_empty();
        let candidates =
            select_compatible_versions(versions, mc_version, loader_str, !is_dependency, &[]);
        if no_versions && is_main && effective_predicates.is_empty() {
            return Err(format!(
                "optional_mod_unavailable: {pid} ({loader_str} {mc_version})"
            ));
        }
        for version in candidates {
            if exact_version
                .as_ref()
                .is_some_and(|id| version.id.as_ref() != Some(id))
            {
                continue;
            }
            let Some(file) = select_compatible_file(&version, mc_version).cloned() else {
                continue;
            };

            if !is_safe_filename(&file.filename) {
                log::warn!(
                    "[mods] Skipping suspicious filename from Modrinth: {}",
                    file.filename
                );
                continue;
            }

            if inspect_mod_path(&dir, Path::new(&file.filename))?.is_some() {
                match read_mod_bytes_in(&dir, &file.filename) {
                    Ok(bytes) => {
                        if has_upstream_digest(&file)
                            && verify_mod_bytes(&bytes, &file.hashes, file.size).is_ok()
                            && mod_jar_is_loadable_for_loader(&bytes, loader_str)
                            && jar_satisfies_dependency_predicates(&bytes, &effective_predicates)?
                            && !mod_jar_conflicts_with_installed(
                                &bytes, game_dir, mc_version, loader_str,
                            )
                            .await
                        {
                            let jar_dependencies = required_modrinth_dependencies_from_mod_jar(
                                &client, &bytes, mc_version, loader_str,
                            )
                            .await?;
                            let size = bytes.len() as u64;
                            selected = Some((version, file, None, size, jar_dependencies));
                            break;
                        }
                        log::warn!(
                            "[mods] Ignoring unverified or incompatible cached mod candidate: {}",
                            file.filename
                        );
                    }
                    Err(error) => return Err(error),
                }
            }

            let dl_resp = client
                .get(&file.url)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status);
            let bytes = match dl_resp {
                Err(e) => {
                    return Err(if is_main {
                        format!("download failed: {}", e)
                    } else {
                        format!("required dependency download failed for {}: {}", pid, e)
                    });
                }
                Ok(r) => match download_mod_bytes(r, file.size).await {
                    Err(e) => {
                        return Err(if is_main {
                            format!("failed to read bytes: {}", e)
                        } else {
                            format!(
                                "failed to read required dependency bytes for {}: {}",
                                pid, e
                            )
                        });
                    }
                    Ok(b) => b,
                },
            };

            verify_mod_bytes(&bytes, &file.hashes, file.size)?;

            if !mod_jar_is_loadable_for_loader(&bytes, loader_str) {
                log::warn!(
                    "[mods] Skipping incompatible mod metadata candidate: {}",
                    file.filename
                );
                continue;
            }
            if !jar_satisfies_dependency_predicates(&bytes, &effective_predicates)? {
                continue;
            }
            if mod_jar_conflicts_with_installed(&bytes, game_dir, mc_version, loader_str).await {
                log::info!(
                    "[mods] Skipping mod candidate with declared conflicts: {}",
                    file.filename
                );
                continue;
            }

            let jar_dependencies = required_modrinth_dependencies_from_mod_jar(
                &client, &bytes, mc_version, loader_str,
            )
            .await?;
            let size = bytes.len() as u64;
            selected = Some((version, file, Some(bytes), size, jar_dependencies));
            break;
        }

        let (version, file, bytes, size_bytes, jar_dependencies) = match selected {
            Some(selected) => selected,
            None => {
                return Err(if is_main {
                    format!(
                        "No compatible loadable file was found for {} ({})",
                        loader_str, mc_version
                    )
                } else {
                    format!(
                        "No compatible loadable required dependency file was found for {} ({} {})",
                        pid, loader_str, mc_version
                    )
                });
            }
        };

        for dep in &version.dependencies {
            if dep.dependency_type != "required" {
                continue;
            }

            let dep_id = match resolve_dependency_project_id(&client, dep).await {
                Some(dep_id) => dep_id,
                None => {
                    return Err(format!(
                        "required dependency could not be resolved for {}",
                        pid
                    ));
                }
            };

            queue.push((dep_id, true, Vec::new(), dep.version_id.clone()));
        }
        for dep in jar_dependencies {
            if dep.project_id != pid && !visited.contains(&dep.project_id) {
                let predicates = dependency_project_predicates(&dep);
                queue.push((dep.project_id, true, predicates, None));
            }
        }

        selected_versions.insert(pid.clone(), version.id.clone());
        pending_installs.push(PendingModInstall {
            project_id: pid.clone(),
            filename: file.filename.clone(),
            bytes,
            size_bytes,
            version_id: version.id.clone(),
            version_number: (!version.version_number.is_empty())
                .then(|| version.version_number.clone()),
            hashes: file.hashes.clone(),
            advertised_size: file.size,
            game_versions: version.game_versions.clone(),
            loaders: version.loaders.clone(),
        });
    }

    Ok(ModInstallPlan {
        installs: pending_installs,
    })
}

pub async fn sync_auto_mods_for_launch(
    game_dir: &Path,
    auto_mods_file: PathBuf,
    mc_version: &str,
    loader: &str,
) -> Result<(), String> {
    if loader == "vanilla" {
        return validate_mod_set(game_dir, mc_version, loader).await;
    }

    let mut mods_to_install: Vec<AutoMod> = load_auto_mods(&auto_mods_file)
        .into_iter()
        .filter(|mod_def| auto_mod_applies_to_loader_and_version(mod_def, mc_version, loader))
        .collect();

    mods_to_install.sort_by_key(auto_mod_install_rank);

    let signature = mod_sync_state::signature(&mods_to_install, AUTO_MOD_RESOLVER_VERSION);
    let mod_dir_signature = mod_sync_state::dir_signature(game_dir).await;
    install_missing_installed_mod_dependencies(game_dir, mc_version, loader).await?;
    if mod_sync_state::is_fresh(game_dir, mc_version, loader, &signature, &mod_dir_signature).await
    {
        log::info!(
            "[mods] Auto mod sync skipped; cached state is fresh for {} {}",
            loader,
            mc_version
        );
        return validate_mod_set(game_dir, mc_version, loader).await;
    }

    let mut plan_set =
        resolve_auto_mod_plan_set(game_dir, &mods_to_install, mc_version, loader).await?;

    for item in plan_set.items {
        commit_mod_install_plan(
            game_dir,
            &item.project_id,
            item.display_name,
            item.icon_url,
            item.plan,
        )
        .await?;
    }

    repair_auto_mod_active_set_conflicts(
        game_dir,
        mc_version,
        loader,
        &mods_to_install,
        &mut plan_set.suppressed_project_ids,
    )
    .await;

    validate_mod_set(game_dir, mc_version, loader).await?;

    let mod_dir_signature = mod_sync_state::dir_signature(game_dir).await;
    mod_sync_state::save(game_dir, mc_version, loader, &signature, &mod_dir_signature).await;

    Ok(())
}

pub async fn mark_auto_mod_sync_current(
    game_dir: &Path,
    auto_mods_file: PathBuf,
    mc_version: &str,
    loader: &str,
) {
    if loader == "vanilla" {
        return;
    }
    let mut mods_to_install: Vec<AutoMod> = load_auto_mods(&auto_mods_file)
        .into_iter()
        .filter(|mod_def| auto_mod_applies_to_loader_and_version(mod_def, mc_version, loader))
        .collect();
    mods_to_install.sort_by_key(auto_mod_install_rank);
    let signature = mod_sync_state::signature(&mods_to_install, AUTO_MOD_RESOLVER_VERSION);
    let mod_dir_signature = mod_sync_state::dir_signature(game_dir).await;
    mod_sync_state::save(game_dir, mc_version, loader, &signature, &mod_dir_signature).await;
}

fn auto_mod_install_error_should_disable_existing(error: &str) -> bool {
    error.starts_with("optional_mod_unavailable:")
}

async fn resolve_auto_mod_plan_set(
    game_dir: &Path,
    mods_to_install: &[AutoMod],
    mc_version: &str,
    loader: &str,
) -> Result<AutoModPlanSet, String> {
    let mut suppressed_project_ids = HashSet::new();
    let mut items = Vec::new();

    for _ in 0..3 {
        items.clear();
        for mod_def in mods_to_install {
            if suppressed_project_ids.contains(&mod_def.project_id) {
                continue;
            }
            if project_is_disabled(game_dir, &mod_def.project_id)? {
                suppressed_project_ids.insert(mod_def.project_id.clone());
                continue;
            }
            match resolve_modrinth_mod_install_plan(
                game_dir,
                &mod_def.project_id,
                mc_version,
                loader,
            )
            .await
            {
                Ok(plan) => items.push(AutoModPlanItem {
                    project_id: mod_def.project_id.clone(),
                    display_name: Some(mod_def.name.clone()),
                    icon_url: mod_def.icon_url.clone(),
                    keep_priority: mod_def.keep_priority,
                    plan,
                }),
                Err(error) => {
                    if !auto_mod_install_error_should_disable_existing(&error) {
                        return Err(error);
                    }
                    suppressed_project_ids.insert(mod_def.project_id.clone());
                    // Missing optional provider candidates do not remove a working install.
                    log::info!(
                        "[mods] Skipped optional auto mod {} for {} {}: {}",
                        mod_def.name,
                        loader,
                        mc_version,
                        error
                    );
                }
            }
        }

        let rejected = validate_auto_mod_plan(game_dir, mc_version, loader, &items).await;
        if rejected.is_empty() {
            break;
        }
        items.retain(|item| !rejected.contains(&item.project_id));
        for project_id in rejected {
            if suppressed_project_ids.insert(project_id.clone()) {
                remove_project_files(game_dir, &project_id).await;
                log::info!(
                    "[mods] Removed optional auto mod project {} before commit because the whole plan was incompatible",
                    project_id
                );
            }
        }
    }

    if !validate_auto_mod_plan(game_dir, mc_version, loader, &items)
        .await
        .is_empty()
    {
        return Err("unresolved incompatible auto mod plan".to_string());
    }
    Ok(AutoModPlanSet {
        items,
        suppressed_project_ids,
    })
}

async fn validate_auto_mod_plan(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
    plan_items: &[AutoModPlanItem],
) -> HashSet<String> {
    if loader != "fabric" && loader != "quilt" {
        return HashSet::new();
    }

    let mut planned_project_ids = HashSet::new();
    for item in plan_items {
        planned_project_ids.insert(item.project_id.as_str());
        for pending in &item.plan.installs {
            planned_project_ids.insert(pending.project_id.as_str());
        }
    }

    let mut manifests = planned_auto_mod_manifests(game_dir, plan_items).await;
    let mut installed = active_mod_manifests(game_dir, mc_version, loader).await;
    installed.retain(|entry| !planned_project_ids.contains(entry.project_id.as_str()));
    manifests.extend(installed.into_iter().map(|entry| PlannedModManifest {
        project_id: entry.project_id,
        keep_priority: u8::MAX,
        manifest: entry.manifest,
        planned: false,
    }));

    let mut rejected = HashSet::new();
    for left in &manifests {
        for right in &manifests {
            if left.project_id == right.project_id || left.manifest.id == right.manifest.id {
                continue;
            }
            let conflicts = dependency_list_matches_mod(
                &left.manifest.incompatible_dependencies,
                &right.manifest,
            ) || dependency_list_matches_mod(
                &right.manifest.incompatible_dependencies,
                &left.manifest,
            );
            if !conflicts {
                continue;
            }

            let reject = if left.planned && right.planned {
                if left.keep_priority <= right.keep_priority {
                    left.project_id.clone()
                } else {
                    right.project_id.clone()
                }
            } else if left.planned {
                left.project_id.clone()
            } else if right.planned {
                right.project_id.clone()
            } else {
                continue;
            };
            rejected.insert(reject);
        }
    }

    rejected
}

struct PlannedModManifest {
    project_id: String,
    keep_priority: u8,
    manifest: InstalledModManifest,
    planned: bool,
}

async fn planned_auto_mod_manifests(
    game_dir: &Path,
    plan_items: &[AutoModPlanItem],
) -> Vec<PlannedModManifest> {
    let mut manifests = Vec::new();

    for item in plan_items {
        for pending in &item.plan.installs {
            let bytes = match &pending.bytes {
                Some(bytes) => bytes.clone(),
                None => match managed_mods(game_dir)
                    .and_then(|dir| read_mod_bytes_in(&dir, &pending.filename))
                {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                },
            };
            if let Ok(Some(manifest)) = read_installed_mod_manifest(&pending.filename, &bytes) {
                manifests.push(PlannedModManifest {
                    project_id: item.project_id.clone(),
                    keep_priority: item.keep_priority,
                    manifest,
                    planned: true,
                });
            }
        }
    }

    manifests
}

async fn repair_auto_mod_active_set_conflicts(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
    auto_mods: &[AutoMod],
    suppressed_project_ids: &mut HashSet<String>,
) -> bool {
    let active = active_mod_manifests(game_dir, mc_version, loader).await;
    let auto_mod_by_project: HashMap<&str, &AutoMod> = auto_mods
        .iter()
        .map(|mod_def| (mod_def.project_id.as_str(), mod_def))
        .collect();

    for breaker in &active {
        for broken in &active {
            if breaker.project_id == broken.project_id {
                continue;
            }
            if !dependency_list_matches_mod(
                &breaker.manifest.incompatible_dependencies,
                &broken.manifest,
            ) {
                continue;
            }

            if let Some(requester) = find_optional_requester_for_installed_mod(
                &active,
                &breaker.manifest,
                &auto_mod_by_project,
            ) {
                if suppressed_project_ids.insert(requester.project_id.clone()) {
                    remove_project_files(game_dir, &requester.project_id).await;
                }
                if auto_mod_by_project.contains_key(breaker.project_id.as_str()) {
                    remove_project_files(game_dir, &breaker.project_id).await;
                }
                log::info!(
                    "[mods] Disabled optional auto mod {} because it forced an incompatible {} version",
                    auto_mod_by_project
                        .get(requester.project_id.as_str())
                        .map(|mod_def| mod_def.name.as_str())
                        .unwrap_or(requester.project_id.as_str()),
                    breaker.manifest.id
                );
                return true;
            }

            let disabled_project = choose_conflict_project_to_disable(
                &breaker.project_id,
                &broken.project_id,
                &auto_mod_by_project,
            );
            if let Some(project_id) = disabled_project
                && suppressed_project_ids.insert(project_id.clone())
            {
                remove_project_files(game_dir, &project_id).await;
                log::info!(
                    "[mods] Removed optional auto mod project {} to repair an incompatible active mod set",
                    project_id
                );
                return true;
            }
        }
    }

    false
}

fn find_optional_requester_for_installed_mod<'a>(
    active: &'a [ActiveModManifest],
    installed: &InstalledModManifest,
    auto_mod_by_project: &HashMap<&str, &AutoMod>,
) -> Option<&'a ActiveModManifest> {
    active
        .iter()
        .filter(|candidate| {
            candidate.manifest.id != installed.id
                && auto_mod_by_project.contains_key(candidate.project_id.as_str())
                && candidate
                    .manifest
                    .required_dependencies
                    .iter()
                    .any(|dependency| {
                        dependency.mod_id == installed.id
                            && dependency_predicate_matches_version(
                                dependency,
                                installed.version.as_deref(),
                            )
                    })
        })
        .max_by_key(|candidate| {
            auto_mod_by_project
                .get(candidate.project_id.as_str())
                .map(|mod_def| auto_mod_disable_priority(mod_def))
                .unwrap_or(0)
        })
}

fn choose_conflict_project_to_disable(
    left_project_id: &str,
    right_project_id: &str,
    auto_mod_by_project: &HashMap<&str, &AutoMod>,
) -> Option<String> {
    let left = auto_mod_by_project.get(left_project_id)?;
    let right = auto_mod_by_project.get(right_project_id)?;
    if auto_mod_keep_priority(left) <= auto_mod_keep_priority(right) {
        Some(left_project_id.to_string())
    } else {
        Some(right_project_id.to_string())
    }
}

fn auto_mod_keep_priority(mod_def: &AutoMod) -> u8 {
    mod_def.keep_priority
}

fn auto_mod_disable_priority(mod_def: &AutoMod) -> u8 {
    100u8.saturating_sub(auto_mod_keep_priority(mod_def))
}

pub(super) fn auto_mod_applies_to_loader_and_version(
    mod_def: &AutoMod,
    mc_version: &str,
    loader: &str,
) -> bool {
    mod_def.enabled
        && !mod_def.tags.iter().any(|tag| tag == "unsupported-gpu")
        && (mod_def.loaders.is_empty() || mod_def.loaders.iter().any(|item| item == loader))
        && auto_mod_version_allowed(mod_def, mc_version)
}

fn auto_mod_version_allowed(mod_def: &AutoMod, mc_version: &str) -> bool {
    if let Some(min) = &mod_def.min_mc_version
        && compare_release_versions(mc_version, min) == Some(std::cmp::Ordering::Less)
    {
        return false;
    }
    if let Some(max) = &mod_def.max_mc_version
        && compare_release_versions(mc_version, max) == Some(std::cmp::Ordering::Greater)
    {
        return false;
    }
    true
}

fn auto_mod_install_rank(mod_def: &AutoMod) -> u8 {
    mod_def.install_rank
}

async fn existing_project_install(
    client: &reqwest::Client,
    game_dir: &Path,
    project_id: &str,
    mc_version: &str,
    loader: &str,
    predicates: &[String],
) -> Result<Option<(PendingModInstall, Vec<RequiredModrinthDependency>)>, String> {
    let meta = load_meta(game_dir).await;
    let dir = managed_mods(game_dir)?;
    if !meta.values().any(|entry| entry.project_id == project_id) {
        return Ok(None);
    }
    let versions =
        fetch_modrinth_versions_for_project(client, project_id, mc_version, loader, true).await?;

    for (filename, entry) in meta.clone() {
        if entry.project_id != project_id || !is_active_mod_filename(&filename) {
            continue;
        }
        let active_filename = filename
            .strip_suffix(".disabled")
            .unwrap_or(&filename)
            .to_string();
        if !artifact_is_usable_for_mc(&active_filename, mc_version) {
            continue;
        }
        let Some(version) = versions.iter().find(|v| {
            v.game_versions.iter().any(|mc| mc == mc_version)
                && v.loaders.iter().any(|l| l == loader)
                && entry
                    .version_id
                    .as_ref()
                    .is_none_or(|id| v.id.as_ref() == Some(id))
                && v.files.iter().any(|f| f.filename == active_filename)
        }) else {
            continue;
        };
        // The full resolver must honor provider-level required/exact dependencies.
        if version
            .dependencies
            .iter()
            .any(|dep| dep.dependency_type == "required")
        {
            continue;
        }
        let file = version
            .files
            .iter()
            .find(|f| f.filename == active_filename)
            .unwrap();
        if !has_upstream_digest(file) {
            continue;
        }
        let bytes = read_mod_bytes_in(&dir, &filename)?;
        if verify_mod_bytes(&bytes, &file.hashes, file.size).is_err() {
            continue;
        }
        if !mod_jar_is_loadable_for_loader(&bytes, loader) {
            continue;
        }
        if !jar_satisfies_dependency_predicates(&bytes, predicates)? {
            continue;
        }
        if let Ok(Some(manifest)) = read_installed_mod_manifest(&active_filename, &bytes)
            && !mod_manifest_supports_minecraft_version(&manifest, mc_version)
        {
            continue;
        }

        let jar_dependencies =
            required_modrinth_dependencies_from_mod_jar(client, &bytes, mc_version, loader).await?;

        return Ok(Some((
            PendingModInstall {
                project_id: project_id.to_string(),
                filename: active_filename,
                size_bytes: bytes.len() as u64,
                bytes: Some(bytes),
                version_id: version.id.clone(),
                version_number: (!version.version_number.is_empty())
                    .then(|| version.version_number.clone()),
                hashes: file.hashes.clone(),
                advertised_size: file.size,
                game_versions: version.game_versions.clone(),
                loaders: version.loaders.clone(),
            },
            jar_dependencies,
        )));
    }

    Ok(None)
}

async fn installed_dependency_predicates_for_project(
    client: &reqwest::Client,
    game_dir: &Path,
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Result<Vec<String>, String> {
    let target_mod_ids = installed_mod_ids_for_project(game_dir, project_id).await;
    let dir = mods_dir(game_dir);
    let mut rd = match fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(error) => return Err(error.to_string()),
    };
    let mut predicates = Vec::new();

    while let Ok(Some(entry)) = rd.next_entry().await {
        let filename = entry.file_name().to_string_lossy().to_string();
        if !is_active_mod_filename(&filename) {
            continue;
        }
        let active_filename = filename
            .strip_suffix(".disabled")
            .unwrap_or(&filename)
            .to_string();
        if !active_filename.ends_with(".jar")
            || !artifact_is_usable_for_mc(&active_filename, mc_version)
        {
            continue;
        }
        let Ok(bytes) = read_mod_bytes(game_dir, &filename) else {
            continue;
        };
        if !target_mod_ids.is_empty() {
            let mut nodes = Vec::new();
            collect_local_mods(&filename, &bytes, loader, 0, &mut nodes)?;
            for node in nodes {
                for (mod_id, predicate) in node.depends {
                    if target_mod_ids.contains(&mod_id) {
                        predicates.push(
                            predicate
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| predicate.to_string()),
                        );
                    }
                }
            }
            continue;
        }
        for dependency in
            required_modrinth_dependencies_from_mod_jar(client, &bytes, mc_version, loader).await?
        {
            if dependency.project_id == project_id
                && let Some(predicate) = dependency.predicate
            {
                predicates.push(predicate);
            }
        }
    }

    predicates.sort();
    predicates.dedup();
    Ok(predicates)
}

async fn installed_mod_ids_for_project(game_dir: &Path, project_id: &str) -> HashSet<String> {
    let meta = load_meta(game_dir).await;
    let mut ids = HashSet::new();

    for (filename, entry) in meta {
        if entry.project_id != project_id || !is_active_mod_filename(&filename) {
            continue;
        }
        let active_filename = filename
            .strip_suffix(".disabled")
            .unwrap_or(&filename)
            .to_string();
        if !active_filename.ends_with(".jar") {
            continue;
        }
        let Ok(bytes) = read_mod_bytes(game_dir, &filename) else {
            continue;
        };
        if let Ok(Some(manifest)) = read_installed_mod_manifest(&active_filename, &bytes) {
            ids.insert(manifest.id);
        }
    }

    ids
}

async fn mod_jar_conflicts_with_installed(
    bytes: &[u8],
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
) -> bool {
    if loader != "fabric" && loader != "quilt" {
        return false;
    }

    let candidate = match read_installed_mod_manifest("<candidate>", bytes) {
        Ok(Some(manifest)) => manifest,
        _ => return false,
    };
    let installed = installed_manifests(game_dir, mc_version, loader).await;
    installed.into_iter().any(|manifest| {
        if manifest.id == candidate.id {
            return false;
        }
        dependency_list_matches_mod(&candidate.incompatible_dependencies, &manifest)
            || dependency_list_matches_mod(&manifest.incompatible_dependencies, &candidate)
    })
}

async fn installed_manifests(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
) -> Vec<InstalledModManifest> {
    let dir = mods_dir(game_dir);
    let mut rd = match fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut manifests = Vec::new();

    while let Ok(Some(entry)) = rd.next_entry().await {
        let filename = entry.file_name().to_string_lossy().to_string();
        if !is_active_mod_filename(&filename) {
            continue;
        }
        let active_filename = filename
            .strip_suffix(".disabled")
            .unwrap_or(&filename)
            .to_string();
        if !active_filename.ends_with(".jar")
            || !artifact_is_usable_for_mc(&active_filename, mc_version)
        {
            continue;
        }
        let Ok(bytes) = read_mod_bytes(game_dir, &filename) else {
            continue;
        };
        if !mod_jar_is_loadable_for_loader(&bytes, loader) {
            continue;
        }
        if let Ok(Some(manifest)) = read_installed_mod_manifest(&active_filename, &bytes)
            && mod_manifest_supports_minecraft_version(&manifest, mc_version)
        {
            manifests.push(manifest);
        }
    }

    manifests
}

async fn active_mod_manifests(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
) -> Vec<ActiveModManifest> {
    let meta = load_meta(game_dir).await;
    let dir = mods_dir(game_dir);
    let mut rd = match fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut manifests = Vec::new();

    while let Ok(Some(entry)) = rd.next_entry().await {
        let filename = entry.file_name().to_string_lossy().to_string();
        if !is_active_mod_filename(&filename) || !artifact_is_usable_for_mc(&filename, mc_version) {
            continue;
        }
        let Some(meta_entry) = meta.get(&filename) else {
            continue;
        };
        let Ok(bytes) = read_mod_bytes(game_dir, &filename) else {
            continue;
        };
        if !mod_jar_is_loadable_for_loader(&bytes, loader) {
            continue;
        }
        if let Ok(Some(manifest)) = read_installed_mod_manifest(&filename, &bytes) {
            manifests.push(ActiveModManifest {
                project_id: meta_entry.project_id.clone(),
                manifest,
            });
        }
    }

    manifests
}

fn dependency_list_matches_mod(
    dependencies: &[FabricDependency],
    manifest: &InstalledModManifest,
) -> bool {
    dependencies.iter().any(|dependency| {
        dependency.mod_id == manifest.id
            && dependency_predicate_matches_version(dependency, manifest.version.as_deref())
    })
}

fn dependency_predicate_matches_version(
    dependency: &FabricDependency,
    version: Option<&str>,
) -> bool {
    let Some(predicate) = dependency.predicate.as_deref() else {
        return true;
    };
    if predicate == "*" {
        return true;
    }
    let Some(version) = version else {
        return true;
    };
    artifact_satisfies_predicate(version, predicate)
}

pub async fn quarantine_unloadable_mods(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
) -> Result<Vec<String>, String> {
    if loader != "fabric" && loader != "quilt" {
        return Ok(Vec::new());
    }

    let root = managed_fs::root(game_dir)?;
    if inspect_mod_path(&root, Path::new("mods"))?.is_none() {
        return Ok(Vec::new());
    }
    let confined = managed_mods(game_dir)?;
    let dir = mods_dir(game_dir);
    let entries = confined.entries().map_err(|e| e.to_string())?;
    let mut meta = load_meta(game_dir).await;
    let mut quarantined = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !name.ends_with(".jar") || !is_safe_filename(&name) {
            continue;
        }

        let bytes = read_mod_bytes_in(&confined, &name)?;

        let manifest = read_installed_mod_manifest(&name, &bytes).ok().flatten();
        let incompatible_minecraft = manifest
            .as_ref()
            .and_then(|manifest| manifest.minecraft_predicates.as_ref())
            .is_some_and(|predicates| {
                !predicates.iter().any(|predicate| {
                    local_minecraft_predicate_matches(
                        mc_version,
                        &serde_json::Value::String(predicate.clone()),
                    )
                    .unwrap_or(true)
                })
            });

        if incompatible_minecraft {
            if disable_mod_file(&dir, &mut meta, &name).await {
                log::warn!(
                    "[mods] Disabled mod that does not support Minecraft {}: {}",
                    mc_version,
                    name
                );
                quarantined.push(name);
            }
            continue;
        }

        if mod_jar_is_loadable_for_loader(&bytes, loader) {
            continue;
        }

        if disable_mod_file(&dir, &mut meta, &name).await {
            log::warn!(
                "[mods] Disabled mod with incompatible loader metadata: {}",
                name
            );
            quarantined.push(name);
        }
    }

    // Required dependency failures belong to validate_mod_set, not quarantine.
    save_meta(game_dir, &meta).await;

    Ok(quarantined)
}

#[cfg(test)]
fn expand_available_mod_ids(mut ids: HashSet<String>) -> HashSet<String> {
    ids.extend(["minecraft", "java", "fabricloader", "quilt_loader"].map(str::to_string));
    if ids.contains("fabric-api")
        || ids.contains("fabric-api-base")
        || ids.iter().any(|id| is_fabric_api_module_id(id))
    {
        ids.insert("fabric".to_string());
    }
    ids
}

fn is_fabric_api_module_id(mod_id: &str) -> bool {
    mod_id == "fabric" || mod_id.starts_with("fabric-") || mod_id.starts_with("fabric_")
}

fn mod_manifest_supports_minecraft_version(
    manifest: &InstalledModManifest,
    mc_version: &str,
) -> bool {
    manifest
        .minecraft_predicates
        .as_ref()
        .is_none_or(|predicates| {
            predicates.iter().any(|predicate| {
                local_minecraft_predicate_matches(
                    mc_version,
                    &serde_json::Value::String(predicate.clone()),
                )
                .unwrap_or(true)
            })
        })
}

async fn install_missing_installed_mod_dependencies(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
) -> Result<(), String> {
    if !supports_dependency_validation(loader) {
        return Ok(());
    }

    let client = crate::http::builder()?
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|error| error.to_string())?;
    let mut nodes = Vec::new();
    let root = managed_fs::root(game_dir)?;
    if inspect_mod_path(&root, Path::new("mods"))?.is_none() {
        return Ok(());
    }
    let dir = managed_mods(game_dir)?;
    for entry in dir.entries().map_err(|e| e.to_string())? {
        let name = entry
            .map_err(|e| e.to_string())?
            .file_name()
            .to_string_lossy()
            .to_string();
        if is_active_mod_filename(&name) {
            collect_local_mods(
                &name,
                &read_mod_bytes_in(&dir, &name)?,
                loader,
                0,
                &mut nodes,
            )?;
        }
    }
    let available_versions: HashMap<String, Option<String>> = nodes
        .iter()
        .flat_map(|node| node.ids.iter().map(|id| (id.clone(), node.version.clone())))
        .collect();
    let mut missing = HashSet::<String>::new();

    for node in &nodes {
        for (mod_id, predicate) in &node.depends {
            if matches!(
                mod_id.as_str(),
                "minecraft"
                    | "java"
                    | "fabricloader"
                    | "fabric-loader"
                    | "quilt_loader"
                    | "quilt-loader"
            ) {
                continue;
            }
            if let Some(version) = available_versions.get(mod_id)
                && local_predicate_matches(version.as_deref(), predicate)?
            {
                continue;
            }

            let project_id = if is_fabric_api_module_id(mod_id) {
                Some(FABRIC_API_PROJECT_ID.to_string())
            } else {
                resolve_modrinth_project_from_mod_id(&client, mod_id, mc_version, loader).await
            };
            let Some(project_id) = project_id else {
                return Err(format!(
                    "required dependency '{}' for '{}' could not be resolved",
                    mod_id, node.filename
                ));
            };

            missing.insert(project_id);
        }
    }

    for project_id in missing {
        if project_is_disabled(game_dir, &project_id)? {
            return Err(format!(
                "required dependency {project_id} is disabled by the user"
            ));
        }
        match resolve_modrinth_mod_install_plan(game_dir, &project_id, mc_version, loader).await {
            Ok(plan) => {
                commit_mod_install_plan(game_dir, &project_id, None, None, plan).await?;
                log::info!(
                    "[mods] Installed required dependency project {}",
                    project_id
                );
            }
            Err(error) => return Err(format!("required dependency {project_id}: {error}")),
        }
    }

    Ok(())
}

async fn required_modrinth_dependencies_from_mod_jar(
    client: &reqwest::Client,
    bytes: &[u8],
    mc_version: &str,
    loader: &str,
) -> Result<Vec<RequiredModrinthDependency>, String> {
    if loader != "fabric" && loader != "quilt" {
        return Ok(Vec::new());
    }

    let dependencies = match fabric_mod_json_required_dependencies(bytes) {
        Ok(dependencies) => dependencies,
        Err(error) => {
            return Err(format!(
                "failed to read required dependency metadata: {error}"
            ));
        }
    };

    let mut projects = Vec::new();
    if dependencies.is_empty() {
        return Ok(projects);
    }
    let mut bundled = Vec::new();
    collect_local_mods("<candidate>", bytes, loader, 0, &mut bundled)?;
    for dependency in dependencies {
        if bundled
            .iter()
            .any(|node| node.ids.contains(&dependency.mod_id))
        {
            continue;
        }
        if let Some(project_id) =
            resolve_modrinth_project_from_mod_id(client, &dependency.mod_id, mc_version, loader)
                .await
        {
            let predicate = bundled
                .first()
                .and_then(|node| node.depends.get(&dependency.mod_id))
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string())
                })
                .or(dependency.predicate);
            projects.push(RequiredModrinthDependency {
                project_id,
                mod_id: dependency.mod_id,
                predicate,
            });
        } else {
            return Err(format!(
                "required dependency '{}' could not be resolved to a Modrinth project",
                dependency.mod_id
            ));
        }
    }
    projects.sort_by(|a, b| {
        a.project_id
            .cmp(&b.project_id)
            .then_with(|| a.mod_id.cmp(&b.mod_id))
            .then_with(|| a.predicate.cmp(&b.predicate))
    });
    projects.dedup();
    Ok(projects)
}

fn read_mod_bytes_in(dir: &cap_std::fs::Dir, filename: &str) -> Result<Vec<u8>, String> {
    super::mod_files::read_bounded(
        read_mod_file(dir, Path::new(filename))?,
        super::mod_files::MAX_MOD_BYTES,
    )
}

fn has_upstream_digest(file: &ModrinthFile) -> bool {
    file.hashes.contains_key("sha512") || file.hashes.contains_key("sha1")
}

fn read_mod_bytes(game_dir: &Path, filename: &str) -> Result<Vec<u8>, String> {
    read_mod_bytes_in(&managed_mods(game_dir)?, filename)
}

fn project_is_disabled(game_dir: &Path, project_id: &str) -> Result<bool, String> {
    let root = managed_fs::root(game_dir)?;
    if inspect_mod_path(&root, Path::new("mods"))?.is_none() {
        return Ok(false);
    }
    let dir = managed_mods(game_dir)?;
    let meta = crate::core::mod_files::load_meta_checked_in(&dir)?;
    let mut disabled = false;
    for (name, entry) in meta {
        if entry.project_id != project_id {
            continue;
        }
        if inspect_mod_path(&dir, Path::new(&name))?.is_none() {
            continue;
        }
        if is_active_mod_filename(&name) {
            return Ok(false);
        }
        disabled |= name.ends_with(".jar.disabled");
    }
    Ok(disabled)
}

fn jar_satisfies_dependency_predicates(
    bytes: &[u8],
    predicates: &[String],
) -> Result<bool, String> {
    if predicates.is_empty() {
        return Ok(true);
    }
    let manifest = read_installed_mod_manifest("<candidate>", bytes)?
        .ok_or("required dependency has no local version metadata")?;
    for predicate in predicates {
        let predicate = if predicate.starts_with('[') {
            serde_json::from_str(predicate)
                .map_err(|e| format!("invalid required dependency predicate: {e}"))?
        } else {
            serde_json::Value::String(predicate.clone())
        };
        if !local_predicate_matches(manifest.version.as_deref(), &predicate)? {
            return Ok(false);
        }
    }
    Ok(true)
}

struct LocalModNode {
    filename: String,
    ids: Vec<String>,
    version: Option<String>,
    depends: serde_json::Map<String, serde_json::Value>,
    incompatible: serde_json::Map<String, serde_json::Value>,
    digest: String,
    nested: bool,
}

fn collect_local_mods(
    filename: &str,
    bytes: &[u8],
    loader: &str,
    depth: usize,
    nodes: &mut Vec<LocalModNode>,
) -> Result<(), String> {
    if depth > 16 || nodes.len() >= 4096 {
        return Err("mod nesting limit exceeded".to_string());
    }
    let mut jar = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("invalid mod JAR {filename}: {e}"))?;
    let text = match jar.by_name("fabric.mod.json") {
        Ok(entry) => {
            let bytes = super::mod_files::read_bounded(entry, 8 * 1024 * 1024)?;
            String::from_utf8(bytes).map_err(|e| e.to_string())?
        }
        Err(_) => {
            return Err(format!(
                "unsupported or missing Fabric metadata in {filename} ({loader}); cannot validate dependency graph"
            ));
        }
    };
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid metadata in {filename}: {e}"))?;
    match json.get("environment").and_then(|v| v.as_str()) {
        Some("server") => return Ok(()),
        None | Some("client" | "*") => {}
        _ => return Err(format!("unsupported mod environment in {filename}")),
    }
    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("missing mod id in {filename}"))?;
    if !mod_jar_is_loadable_for_loader(bytes, loader) {
        return Err(format!("unloadable mod: {filename}"));
    }
    let mut ids = vec![id.to_string()];
    if let Some(provides) = json.get("provides").and_then(|v| v.as_array()) {
        for provided in provides {
            ids.push(
                provided
                    .as_str()
                    .ok_or("invalid provided mod id")?
                    .to_string(),
            );
        }
    }
    ids.sort();
    ids.dedup();
    let depends = json
        .get("depends")
        .map(|v| {
            v.as_object()
                .cloned()
                .ok_or("invalid required dependencies")
        })
        .transpose()?
        .unwrap_or_default();
    let incompatible = json
        .get("breaks")
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or("invalid incompatible dependencies")
        })
        .transpose()?
        .unwrap_or_default();
    nodes.push(LocalModNode {
        filename: filename.to_string(),
        ids,
        version: json
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        depends,
        incompatible,
        digest: {
            use sha2::{Digest, Sha512};
            format!("{:x}", Sha512::digest(bytes))
        },
        nested: depth > 0,
    });
    if let Some(nested) = json.get("jars").and_then(|v| v.as_array()) {
        for entry in nested {
            let name = entry
                .get("file")
                .and_then(|v| v.as_str())
                .ok_or("invalid nested JAR entry")?;
            let mut bytes = Vec::new();
            jar.by_name(name)
                .map_err(|e| format!("missing nested JAR {name}: {e}"))?
                .take(128 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 128 * 1024 * 1024 {
                return Err("nested mod JAR is too large".to_string());
            }
            collect_local_mods(
                &format!("{filename}!{name}"),
                &bytes,
                loader,
                depth + 1,
                nodes,
            )?;
        }
    }
    Ok(())
}

fn local_predicate_matches(
    version: Option<&str>,
    predicate: &serde_json::Value,
) -> Result<bool, String> {
    match predicate {
        serde_json::Value::String(predicate) => {
            if predicate == "*" || predicate.is_empty() {
                return Ok(true);
            }
            let version = version.ok_or("dependency version is unknown")?;
            if version == predicate {
                return Ok(true);
            }
            if !local_version_syntax_supported(version)
                || !local_predicate_syntax_supported(predicate)
            {
                return Err(format!(
                    "unsupported dependency predicate {predicate:?} for {version:?}"
                ));
            }
            minecraft_version_satisfies_fabric_predicate(version, predicate).ok_or_else(|| {
                format!("unsupported dependency predicate {predicate:?} for {version:?}")
            })
        }
        serde_json::Value::Array(predicates) => {
            let mut unknown = None;
            for predicate in predicates {
                match local_predicate_matches(version, predicate) {
                    Ok(true) => return Ok(true),
                    Ok(false) => {}
                    Err(error) => unknown = Some(error),
                }
            }
            match unknown {
                Some(error) => Err(error),
                None => Ok(false),
            }
        }
        _ => Err("invalid dependency predicate".to_string()),
    }
}

fn local_minecraft_predicate_matches(
    version: &str,
    predicate: &serde_json::Value,
) -> Result<bool, String> {
    if predicate
        .as_str()
        .is_some_and(|p| p == version || p == "*" || p.is_empty())
    {
        return Ok(true);
    }
    if let Some(predicates) = predicate.as_array() {
        let mut unknown = None;
        for predicate in predicates {
            match local_minecraft_predicate_matches(version, predicate) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(error) => unknown = Some(error),
            }
        }
        return match unknown {
            Some(error) => Err(error),
            None => Ok(false),
        };
    }
    let normalized = normalize_minecraft_version_for_fabric(version)
        .ok_or_else(|| format!("unsupported Minecraft version namespace: {version}; normalized release target is required"))?;
    local_predicate_matches(Some(&normalized), predicate)
}

fn local_version_syntax_supported(version: &str) -> bool {
    let version = version.split('+').next().unwrap_or(version);
    let (release, pre) = version
        .split_once('-')
        .map_or((version, None), |(r, p)| (r, Some(p)));
    !release.is_empty()
        && release
            .split('.')
            .all(|part| !part.is_empty() && part.parse::<u16>().is_ok())
        && pre.is_none_or(|p| {
            p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        })
}

fn local_predicate_syntax_supported(predicate: &str) -> bool {
    predicate.split_whitespace().all(|clause| {
        let bound = [">=", "<=", ">", "<", "=", "~", "^"]
            .into_iter()
            .find_map(|op| clause.strip_prefix(op))
            .unwrap_or(clause);
        let bound = bound
            .strip_suffix(".x")
            .or_else(|| bound.strip_suffix(".*"))
            .unwrap_or(bound);
        local_version_syntax_supported(bound)
    })
}

/// Whether this module can validate a loader's complete local mod graph.
/// Forge/NeoForge and Quilt-native metadata retain native-loader dependency enforcement.
pub fn supports_dependency_validation(loader: &str) -> bool {
    loader == "fabric"
}

pub const MISSING_JAVA_RUNTIME_ERROR: &str = "mod_validation_runtime_missing:java";
pub const MISSING_LOADER_RUNTIME_ERROR: &str = "mod_validation_runtime_missing:loader";

fn check_runtime_version_known(
    id: &str,
    version: Option<&str>,
    predicate: &serde_json::Value,
) -> Result<(), String> {
    if version.is_some() || local_predicate_matches(None, predicate).is_ok() {
        return Ok(());
    }
    match id {
        "java" => Err(MISSING_JAVA_RUNTIME_ERROR.to_string()),
        "fabricloader" | "fabric-loader" | "quilt_loader" | "quilt-loader" => {
            Err(MISSING_LOADER_RUNTIME_ERROR.to_string())
        }
        _ => Ok(()),
    }
}

/// Check local integrity and advertised targeting, plus the Fabric graph where supported.
/// Java/loader version predicates are skipped by this compatibility wrapper.
/// Vanilla checks confinement/known hashes but does not load or target-check stored JARs.
/// For Forge/NeoForge/Quilt, success only means
/// JAR integrity and known provider targeting, NOT a validated dependency graph.
pub async fn validate_mod_set(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
) -> Result<(), String> {
    validate_mod_set_inner(game_dir, mc_version, loader, None).await
}

/// Strict ready-set validation including actual Java and loader versions.
/// Missing runtime versions fail if a declared predicate needs them. Unsupported
/// loader metadata returns `dependency_validation_unsupported`, never graph-ready.
pub async fn validate_mod_set_with_runtime(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
    loader_version: Option<&str>,
    java_major: Option<u32>,
) -> Result<(), String> {
    if !supports_dependency_validation(loader) {
        return Err(format!("dependency_validation_unsupported: {loader}"));
    }
    validate_mod_set_inner(
        game_dir,
        mc_version,
        loader,
        Some((loader_version, java_major)),
    )
    .await
}

async fn validate_mod_set_inner(
    game_dir: &Path,
    mc_version: &str,
    loader: &str,
    runtime: Option<(Option<&str>, Option<u32>)>,
) -> Result<(), String> {
    let root = managed_fs::root(game_dir)?;
    if inspect_mod_path(&root, Path::new("mods"))?.is_none() {
        return Ok(());
    }
    let dir = managed_mods(game_dir)?;
    let meta = crate::core::mod_files::load_meta_checked_in(&dir)?;
    let mut nodes = Vec::new();
    for entry in dir.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_active_mod_filename(&name) {
            continue;
        }
        let bytes = read_mod_bytes_in(&dir, &name)?;
        if let Some(hashes) = meta.get(&name).and_then(|entry| entry.hashes.as_ref()) {
            verify_mod_bytes(&bytes, hashes, None)?;
        }
        if loader == "vanilla" {
            continue;
        }
        check_mod_jar_structure(&name, &bytes)?;
        if let Some(entry) = meta.get(&name) {
            if !entry.game_versions.is_empty()
                && !entry.game_versions.iter().any(|v| v == mc_version)
            {
                return Err(format!("provider Minecraft targeting mismatch: {name}"));
            }
            if !entry.loaders.is_empty()
                && !entry
                    .loaders
                    .iter()
                    .any(|v| v == loader || (loader == "quilt" && v == "fabric"))
            {
                return Err(format!("provider loader targeting mismatch: {name}"));
            }
        }
        if supports_dependency_validation(loader) {
            collect_local_mods(&name, &bytes, loader, 0, &mut nodes)?;
        }
    }
    let mut available = HashMap::<String, Option<String>>::new();
    let mut owners = HashMap::<&str, &LocalModNode>::new();
    for node in &nodes {
        for id in &node.ids {
            if let Some(previous) = owners.get(id.as_str()) {
                if (previous.nested || node.nested) && previous.digest == node.digest {
                    continue;
                }
                return Err(format!("duplicate mod id {id} in {}", node.filename));
            }
            if matches!(
                id.as_str(),
                "minecraft" | "java" | "fabricloader" | "fabric-loader"
            ) {
                return Err(format!("mod shadows runtime id {id} in {}", node.filename));
            }
            owners.insert(id, node);
            available.insert(id.clone(), node.version.clone());
        }
    }
    available.insert("minecraft".to_string(), Some(mc_version.to_string()));
    let (loader_version, java_major) = runtime.unwrap_or((None, None));
    available.insert("java".to_string(), java_major.map(|v| v.to_string()));
    if loader == "fabric" || loader == "quilt" {
        available.insert(
            "fabricloader".to_string(),
            loader_version.map(str::to_string),
        );
        available.insert(
            "fabric-loader".to_string(),
            loader_version.map(str::to_string),
        );
    }
    if loader == "quilt" {
        available.insert(
            "quilt_loader".to_string(),
            loader_version.map(str::to_string),
        );
        available.insert(
            "quilt-loader".to_string(),
            loader_version.map(str::to_string),
        );
    }
    for node in &nodes {
        for (id, predicate) in &node.depends {
            let version = available.get(id).ok_or_else(|| {
                format!("unresolved required dependency {id} for {}", node.filename)
            })?;
            if runtime.is_none()
                && matches!(
                    id.as_str(),
                    "java" | "fabricloader" | "fabric-loader" | "quilt_loader" | "quilt-loader"
                )
            {
                continue;
            }
            if runtime.is_some() {
                check_runtime_version_known(id, version.as_deref(), predicate)?;
            }
            let matches = if id == "minecraft" {
                local_minecraft_predicate_matches(mc_version, predicate)?
            } else {
                local_predicate_matches(version.as_deref(), predicate)?
            };
            if !matches {
                return Err(format!(
                    "incompatible required dependency {id} for {}",
                    node.filename
                ));
            }
        }
        for (id, predicate) in &node.incompatible {
            if runtime.is_none()
                && matches!(
                    id.as_str(),
                    "java" | "fabricloader" | "fabric-loader" | "quilt_loader" | "quilt-loader"
                )
            {
                continue;
            }
            if let Some(version) = available.get(id) {
                if runtime.is_some() {
                    check_runtime_version_known(id, version.as_deref(), predicate)?;
                }
                let matches = if id == "minecraft" {
                    local_minecraft_predicate_matches(mc_version, predicate)?
                } else {
                    local_predicate_matches(version.as_deref(), predicate)?
                };
                if !matches {
                    continue;
                }
                return Err(format!(
                    "incompatible dependency {id} declared by {}",
                    node.filename
                ));
            }
        }
    }
    Ok(())
}

fn check_mod_jar_structure(filename: &str, bytes: &[u8]) -> Result<(), String> {
    let mut jar = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("invalid mod JAR {filename}: {e}"))?;
    let mut total = 0u64;
    for index in 0..jar.len() {
        let mut entry = jar.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let remaining = 512 * 1024 * 1024u64 - total;
        let read = std::io::copy(
            &mut entry.by_ref().take(remaining + 1),
            &mut std::io::sink(),
        )
        .map_err(|e| format!("corrupt mod JAR {filename}: {e}"))?;
        total += read;
        if total > 512 * 1024 * 1024 {
            return Err(format!("mod JAR expansion limit exceeded: {filename}"));
        }
    }
    Ok(())
}

fn dependency_project_predicates(dependency: &RequiredModrinthDependency) -> Vec<String> {
    if fabric_api_module_is_provided_by_aggregate_project(
        &dependency.mod_id,
        &dependency.project_id,
    ) {
        return Vec::new();
    }
    dependency.predicate.clone().into_iter().collect()
}

fn fabric_api_module_is_provided_by_aggregate_project(mod_id: &str, project_id: &str) -> bool {
    project_id == FABRIC_API_PROJECT_ID && is_fabric_api_module_id(mod_id)
}

fn is_active_mod_filename(name: &str) -> bool {
    !name.starts_with('.')
        && name.ends_with(".jar")
        && !name.ends_with(".jar.disabled")
        && is_safe_filename(name)
}

// ── ModPack インストール結果 ───────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModpackInstallResult {
    pub profile_name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub mods: Vec<ModFile>,
}

#[cfg(test)]
mod tests {
    use super::{
        FABRIC_API_PROJECT_ID, ModrinthFile, ModrinthVersion, RequiredModrinthDependency,
        artifact_is_usable_for_mc, artifact_satisfies_predicate, dependency_project_predicates,
        expand_available_mod_ids, fabric_mod_json_has_invalid_wildcard_predicate,
        fabric_mod_json_incompatible_dependencies, fabric_mod_json_required_dependencies,
        is_active_mod_filename, is_exact_modrinth_match, modrinth_slug_candidates_for_mod_id,
        select_compatible_version, select_compatible_versions,
    };
    use std::collections::HashSet;
    use std::io::{Cursor, Write};

    fn version(
        version_number: &str,
        _version_type: &str,
        game_versions: &[&str],
        loaders: &[&str],
        filename: &str,
    ) -> ModrinthVersion {
        ModrinthVersion {
            id: None,
            project_id: None,
            version_number: version_number.to_string(),
            version_type: Some(_version_type.to_string()),
            game_versions: game_versions.iter().map(|v| v.to_string()).collect(),
            loaders: loaders.iter().map(|v| v.to_string()).collect(),
            files: vec![ModrinthFile {
                url: "https://example.invalid/mod.jar".to_string(),
                filename: filename.to_string(),
                primary: true,
                hashes: Default::default(),
                size: None,
            }],
            dependencies: vec![],
        }
    }

    fn jar_with_fabric_mod_json(json: &str) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        zip.start_file::<_, ()>("fabric.mod.json", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(json.as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn local_game(mods: &[(&str, &str)]) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("mods")).unwrap();
        for (name, json) in mods {
            std::fs::write(
                temp.path().join("mods").join(name),
                jar_with_fabric_mod_json(json),
            )
            .unwrap();
        }
        temp
    }

    #[tokio::test]
    async fn auto_update_preserves_user_disabled_project_without_fetching() {
        let game = local_game(&[]);
        let bytes = jar_with_fabric_mod_json(r#"{"id":"disabled_mod","version":"1.0.0"}"#);
        std::fs::write(game.path().join("mods/a.jar.disabled"), &bytes).unwrap();
        let metadata =
            br#"{"a.jar.disabled":{"project_id":"disabled-project","display_name":"Disabled"}}"#;
        std::fs::write(game.path().join("mods/.mod_meta.json"), metadata).unwrap();
        let recipe: super::AutoMod = serde_json::from_value(serde_json::json!({
            "project_id":"disabled-project", "name":"Disabled", "description":"", "enabled":true
        }))
        .unwrap();
        let plan = super::resolve_auto_mod_plan_set(game.path(), &[recipe], "26.3", "fabric")
            .await
            .unwrap();
        assert!(plan.items.is_empty());
        assert!(plan.suppressed_project_ids.contains("disabled-project"));
        assert_eq!(
            std::fs::read(game.path().join("mods/a.jar.disabled")).unwrap(),
            bytes
        );
        assert_eq!(
            std::fs::read(game.path().join("mods/.mod_meta.json")).unwrap(),
            metadata
        );
        assert!(!game.path().join("mods/a.jar").exists());
    }

    #[test]
    fn candidate_predicates_use_manifest_versions_not_provider_or_filename_tokens() {
        let bytes = jar_with_fabric_mod_json(r#"{"id":"library","version":"0.9.2+mc26.3"}"#);
        assert!(
            super::jar_satisfies_dependency_predicates(&bytes, &[">=0.9.1".to_string()]).unwrap()
        );
        assert!(
            !super::jar_satisfies_dependency_predicates(&bytes, &[">=1.0.0".to_string()]).unwrap()
        );
        assert!(
            super::jar_satisfies_dependency_predicates(
                &bytes,
                &[r#"[">=1.0.0",">=0.9.1"]"#.to_string()]
            )
            .unwrap()
        );
        assert!(super::auto_mod_install_error_should_disable_existing(
            "optional_mod_unavailable: project"
        ));
        for error in [
            "failed to fetch versions",
            "failed to read cached file",
            "No compatible loadable file was found",
            "required dependency download failed",
        ] {
            assert!(!super::auto_mod_install_error_should_disable_existing(
                error
            ));
        }
    }

    #[tokio::test]
    async fn modern_snapshot_uses_minecraft_normalized_namespace_only() {
        let game = local_game(&[(
            "a.jar",
            r#"{"id":"a","version":"1.0.0","depends":{"minecraft":"~26.3-","java":">=25","fabricloader":">=0.19.0"}}"#,
        )]);
        super::validate_mod_set_with_runtime(
            game.path(),
            "26.3-snapshot-2",
            "fabric",
            Some("0.19.3"),
            Some(25),
        )
        .await
        .unwrap();
        assert!(
            super::local_minecraft_predicate_matches(
                "26.3-snapshot-2",
                &serde_json::json!("<26.3-alpha.3")
            )
            .unwrap()
        );
        assert!(
            !super::local_minecraft_predicate_matches(
                "26.3-snapshot-2",
                &serde_json::json!(">=26.3")
            )
            .unwrap()
        );
        assert!(
            super::local_minecraft_predicate_matches(
                "25w45a",
                &serde_json::json!(["25w45a", ">=1.21.11"])
            )
            .unwrap()
        );
        assert!(
            super::local_minecraft_predicate_matches("25w45a", &serde_json::json!(">=1.21.11"))
                .is_err()
        );
        assert!(
            super::local_predicate_matches(Some("0.20.0"), &serde_json::json!("^0.19.0")).unwrap()
        );
        assert!(
            super::local_predicate_matches(Some("26.3-rc.3"), &serde_json::json!("~26.3-rc.2"))
                .unwrap()
        );
    }

    #[tokio::test]
    async fn missing_runtime_errors_are_classified_separately_from_incompatibility() {
        let game = local_game(&[(
            "a.jar",
            r#"{"id":"a","version":"1.0.0","depends":{"java":">=25","fabricloader":">=0.19.0"}}"#,
        )]);
        let unknown_java = super::validate_mod_set_with_runtime(
            game.path(),
            "26.3",
            "fabric",
            Some("0.19.3"),
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(unknown_java, super::MISSING_JAVA_RUNTIME_ERROR);
        let unknown_loader =
            super::validate_mod_set_with_runtime(game.path(), "26.3", "fabric", None, Some(25))
                .await
                .unwrap_err();
        assert_eq!(unknown_loader, super::MISSING_LOADER_RUNTIME_ERROR);
        let incompatible = super::validate_mod_set_with_runtime(
            game.path(),
            "26.3",
            "fabric",
            Some("0.19.3"),
            Some(21),
        )
        .await
        .unwrap_err();
        assert!(incompatible.contains("incompatible required dependency java"));
    }

    #[tokio::test]
    async fn local_validation_requires_real_dependencies_and_compatible_versions() {
        let game = local_game(&[(
            "main.jar",
            r#"{"id":"main","version":"1.0.0","depends":{"library":">=2.0.0"}}"#,
        )]);
        let error = super::validate_mod_set(game.path(), "1.21.6", "fabric")
            .await
            .unwrap_err();
        assert!(error.contains("unresolved required dependency library"));
        std::fs::write(
            game.path().join("mods/library.jar"),
            jar_with_fabric_mod_json(r#"{"id":"library","version":"1.0.0"}"#),
        )
        .unwrap();
        assert!(
            super::validate_mod_set(game.path(), "1.21.6", "fabric")
                .await
                .unwrap_err()
                .contains("incompatible required dependency")
        );
        std::fs::write(
            game.path().join("mods/library.jar"),
            jar_with_fabric_mod_json(r#"{"id":"library","version":"2.1.0"}"#),
        )
        .unwrap();
        super::validate_mod_set(game.path(), "1.21.6", "fabric")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn advisory_conflicts_do_not_reject_a_loadable_set() {
        let metadata = r#"{"id":"main","version":"1.0.0","conflicts":{"other":"*"}}"#;
        let game = local_game(&[
            ("main.jar", metadata),
            ("other.jar", r#"{"id":"other","version":"1.0.0"}"#),
        ]);
        assert!(
            fabric_mod_json_incompatible_dependencies(&jar_with_fabric_mod_json(metadata))
                .unwrap()
                .is_empty()
        );
        super::validate_mod_set(game.path(), "1.21.6", "fabric")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn local_validation_rejects_conflicts_duplicates_and_wrong_minecraft() {
        let game = local_game(&[
            (
                "main.jar",
                r#"{"id":"main","version":"1.0.0","breaks":{"other":">=1.0.0"}}"#,
            ),
            ("other.jar", r#"{"id":"other","version":"2.0.0"}"#),
        ]);
        assert!(
            super::validate_mod_set(game.path(), "1.21.6", "fabric")
                .await
                .unwrap_err()
                .contains("incompatible dependency")
        );
        std::fs::write(
            game.path().join("mods/other.jar"),
            jar_with_fabric_mod_json(r#"{"id":"main","version":"2.0.0"}"#),
        )
        .unwrap();
        assert!(
            super::validate_mod_set(game.path(), "1.21.6", "fabric")
                .await
                .unwrap_err()
                .contains("duplicate mod id")
        );
        let wrong_mc = local_game(&[(
            "a.jar",
            r#"{"id":"a","version":"1.0.0","depends":{"minecraft":">=1.22"}}"#,
        )]);
        assert!(
            super::validate_mod_set(wrong_mc.path(), "1.21.6", "fabric")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn empty_and_optional_only_sets_do_not_require_fabric_api() {
        let empty = local_game(&[]);
        super::validate_mod_set(empty.path(), "1.21.6", "fabric")
            .await
            .unwrap();
        let optional = local_game(&[(
            "a.jar",
            r#"{"id":"a","version":"1.0.0","suggests":{"missing":"*"},"recommends":{"also_missing":"*"}}"#,
        )]);
        super::validate_mod_set(optional.path(), "1.21.6", "fabric")
            .await
            .unwrap();
        assert!(!super::auto_mod_install_error_should_disable_existing(
            "required dependency download failed"
        ));
        assert!(!super::auto_mod_install_error_should_disable_existing(
            "failed to fetch versions"
        ));
    }

    #[tokio::test]
    async fn required_modules_cannot_be_inferred_from_aggregate_name() {
        let game = local_game(&[
            (
                "main.jar",
                r#"{"id":"main","version":"1.0.0","depends":{"fabric-missing-module":"*"}}"#,
            ),
            ("api.jar", r#"{"id":"fabric-api","version":"1.0.0"}"#),
        ]);
        assert!(
            super::validate_mod_set(game.path(), "1.21.6", "fabric")
                .await
                .unwrap_err()
                .contains("fabric-missing-module")
        );
    }

    #[tokio::test]
    async fn local_validation_reads_nested_jars_and_provided_aliases() {
        let game = local_game(&[(
            "main.jar",
            r#"{"id":"main","version":"1.0.0","depends":{"alias":["1.x",">=2.0.0"]}}"#,
        )]);
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file::<_, ()>("fabric.mod.json", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"bundle","version":"1.0.0","jars":[{"file":"nested/library.jar"}]}"#,
        )
        .unwrap();
        zip.start_file::<_, ()>("nested/library.jar", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(&jar_with_fabric_mod_json(
            r#"{"id":"library","version":"2.1.0","provides":["alias"]}"#,
        ))
        .unwrap();
        std::fs::write(
            game.path().join("mods/bundle.jar"),
            zip.finish().unwrap().into_inner(),
        )
        .unwrap();
        super::validate_mod_set(game.path(), "1.21.6", "fabric")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn validator_rejects_linked_mod_directory() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        crate::core::test_support::directory_link(outside.path(), &temp.path().join("mods"));
        assert!(
            super::validate_mod_set(temp.path(), "1.21.6", "fabric")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn strict_runtime_validation_checks_java_and_loader_versions() {
        let game = local_game(&[(
            "runtime.jar",
            r#"{"id":"runtime","version":"1.0.0","depends":{"java":">=21","fabricloader":">=0.17.0"}}"#,
        )]);
        super::validate_mod_set(game.path(), "1.21.6", "fabric")
            .await
            .unwrap();
        super::validate_mod_set_with_runtime(
            game.path(),
            "1.21.6",
            "fabric",
            Some("0.17.3"),
            Some(21),
        )
        .await
        .unwrap();
        for (loader, java) in [
            (Some("0.16.9"), Some(21)),
            (Some("0.17.3"), Some(17)),
            (None, Some(21)),
            (Some("0.17.3"), None),
        ] {
            assert!(
                super::validate_mod_set_with_runtime(game.path(), "1.21.6", "fabric", loader, java)
                    .await
                    .is_err()
            );
        }
        let unsupported = local_game(&[(
            "a.jar",
            r#"{"id":"a","version":"1.0.0","depends":{"java":"unsupported-predicate"}}"#,
        )]);
        assert!(
            super::validate_mod_set_with_runtime(
                unsupported.path(),
                "1.21.6",
                "fabric",
                Some("0.17.3"),
                Some(21)
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn other_loaders_keep_integrity_checks_without_claiming_graph_support() {
        let game = local_game(&[]);
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file::<_, ()>("META-INF/mods.toml", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(b"modLoader=\"javafml\"\n").unwrap();
        std::fs::write(
            game.path().join("mods/forge.jar"),
            zip.finish().unwrap().into_inner(),
        )
        .unwrap();
        assert!(!super::supports_dependency_validation("forge"));
        assert!(!super::supports_dependency_validation("neoforge"));
        assert!(!super::supports_dependency_validation("quilt"));
        assert!(super::supports_dependency_validation("fabric"));
        super::validate_mod_set(game.path(), "1.21.6", "forge")
            .await
            .unwrap();
        super::validate_mod_set(game.path(), "1.21.6", "neoforge")
            .await
            .unwrap();
        let error =
            super::validate_mod_set_with_runtime(game.path(), "1.21.6", "forge", None, None)
                .await
                .unwrap_err();
        assert!(error.starts_with("dependency_validation_unsupported:"));
        std::fs::write(game.path().join("mods/forge.jar"), b"corrupt").unwrap();
        assert!(
            super::validate_mod_set(game.path(), "1.21.6", "forge")
                .await
                .is_err()
        );
        super::validate_mod_set(game.path(), "1.21.6", "vanilla")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn non_fabric_integrity_checks_advertised_provider_targeting_and_hashes() {
        let game = local_game(&[("a.jar", r#"{"id":"a","version":"1.0.0"}"#)]);
        let bytes = std::fs::read(game.path().join("mods/a.jar")).unwrap();
        let meta = serde_json::json!({"a.jar": {"display_name":"A","project_id":"project","game_versions":["1.21.6"],"loaders":["forge"],"hashes":{"sha1":sha1_smol::Sha1::from(bytes.as_slice()).digest().to_string()}}});
        std::fs::write(
            game.path().join("mods/.mod_meta.json"),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        super::validate_mod_set(game.path(), "1.21.6", "forge")
            .await
            .unwrap();
        assert!(
            super::validate_mod_set(game.path(), "1.20.1", "forge")
                .await
                .unwrap_err()
                .contains("targeting mismatch")
        );
        assert!(
            super::validate_mod_set(game.path(), "1.21.6", "neoforge")
                .await
                .unwrap_err()
                .contains("targeting mismatch")
        );
        std::fs::write(
            game.path().join("mods/a.jar"),
            jar_with_fabric_mod_json(r#"{"id":"a","version":"1.0.1"}"#),
        )
        .unwrap();
        assert!(
            super::validate_mod_set(game.path(), "1.21.6", "forge")
                .await
                .unwrap_err()
                .contains("hash mismatch")
        );
    }

    #[test]
    fn rejects_neighbor_minecraft_versions() {
        let candidate = version(
            "mc1.21.8-0.7.3-fabric",
            "release",
            &["1.21.8"],
            &["fabric"],
            "sodium.jar",
        );

        assert!(!is_exact_modrinth_match(&candidate, "1.21.6", "fabric"));
    }

    #[test]
    fn top_level_selection_prefers_release_before_beta() {
        let selected = select_compatible_version(
            vec![
                version(
                    "mc1.21.6-1.0.1-beta",
                    "beta",
                    &["1.21.6"],
                    &["fabric"],
                    "newer-beta.jar",
                ),
                version(
                    "mc1.21.6-1.0.0",
                    "release",
                    &["1.21.6"],
                    &["fabric"],
                    "older-release.jar",
                ),
                version(
                    "mc1.21.8-1.0.0",
                    "release",
                    &["1.21.8"],
                    &["fabric"],
                    "wrong.jar",
                ),
            ],
            "1.21.6",
            "fabric",
        )
        .expect("exact compatible version");

        assert_eq!(selected.files[0].filename, "older-release.jar");
    }

    #[test]
    fn dependency_selection_keeps_modrinth_newest_order() {
        let selected = select_compatible_versions(
            vec![
                version(
                    "mc1.21.6-1.0.1-beta",
                    "beta",
                    &["1.21.6"],
                    &["fabric"],
                    "newer-beta.jar",
                ),
                version(
                    "mc1.21.6-1.0.0",
                    "release",
                    &["1.21.6"],
                    &["fabric"],
                    "older-release.jar",
                ),
            ],
            "1.21.6",
            "fabric",
            false,
            &[],
        )
        .into_iter()
        .next()
        .expect("exact compatible dependency version");

        assert_eq!(selected.files[0].filename, "newer-beta.jar");
    }

    #[test]
    fn dependency_selection_respects_minimum_version_predicate() {
        let selected = select_compatible_versions(
            vec![
                version(
                    "0.9.0",
                    "release",
                    &["26.2"],
                    &["fabric"],
                    "sodium-fabric-0.9.0+mc26.2.jar",
                ),
                version(
                    "0.9.1-beta.3",
                    "beta",
                    &["26.2"],
                    &["fabric"],
                    "sodium-fabric-0.9.1-beta.3+mc26.2.jar",
                ),
            ],
            "26.2",
            "fabric",
            false,
            &[">=0.9.1-beta.3".to_string()],
        )
        .into_iter()
        .next()
        .expect("version satisfying dependency predicate");

        assert_eq!(
            selected.files[0].filename,
            "sodium-fabric-0.9.1-beta.3+mc26.2.jar"
        );
    }

    #[test]
    fn artifact_predicate_rejects_old_sodium_version() {
        assert!(!artifact_satisfies_predicate(
            "0.9.0+mc26.2",
            ">=0.9.1-beta.3"
        ));
        assert!(artifact_satisfies_predicate(
            "0.9.1-beta.3+mc26.2",
            ">=0.9.1-beta.3"
        ));
    }

    #[test]
    fn version_predicates_handle_prerelease_and_upper_bounds() {
        assert!(artifact_satisfies_predicate("1.11.1+mc26.2", "<=1.11.1"));
        assert!(!artifact_satisfies_predicate("1.11.2+mc26.2", "<=1.11.1"));
        assert!(artifact_satisfies_predicate("0.9.1", ">=0.9.1-beta.3"));
        assert!(!artifact_satisfies_predicate(
            "0.9.1-beta.2",
            ">=0.9.1-beta.3"
        ));
    }

    #[test]
    fn extracts_fabric_breaks_as_incompatible_dependencies() {
        let bytes = jar_with_fabric_mod_json(
            r#"{
              "schemaVersion": 1,
              "id": "sodium",
              "version": "0.9.1-beta.3+mc26.2",
              "breaks": {
                "iris": "<=1.11.1",
                "fabric-api": "<0.145.1"
              }
            }"#,
        );

        let dependencies = fabric_mod_json_incompatible_dependencies(&bytes).unwrap();
        assert!(dependencies.iter().any(|dependency| {
            dependency.mod_id == "iris" && dependency.predicate.as_deref() == Some("<=1.11.1")
        }));
    }

    #[test]
    fn disabled_jars_are_not_part_of_active_mod_graph() {
        assert!(is_active_mod_filename("sodium-fabric-0.9.0+mc26.2.jar"));
        assert!(!is_active_mod_filename(
            "reeses-sodium-options-fabric-2.2.2+mc26.2.jar.disabled"
        ));
    }

    #[test]
    fn fabric_api_module_predicate_does_not_filter_aggregate_project_version() {
        let dependency = RequiredModrinthDependency {
            project_id: FABRIC_API_PROJECT_ID.to_string(),
            mod_id: "fabric-rendering-fluids-v1".to_string(),
            predicate: Some(">=2.0.0".to_string()),
        };

        assert!(dependency_project_predicates(&dependency).is_empty());
    }

    #[test]
    fn normal_dependency_predicate_filters_project_version() {
        let dependency = RequiredModrinthDependency {
            project_id: "AANobbMI".to_string(),
            mod_id: "sodium".to_string(),
            predicate: Some(">=0.9.1-beta.3".to_string()),
        };

        assert_eq!(
            dependency_project_predicates(&dependency),
            vec![">=0.9.1-beta.3".to_string()]
        );
    }

    #[test]
    fn rejects_artifact_that_explicitly_targets_another_minecraft_version() {
        let candidate = version(
            "mc1.21.8-2.0.0-fabric",
            "release",
            &["1.21.6", "1.21.7", "1.21.8"],
            &["fabric"],
            "example-fabric-2.0.0+mc1.21.8.jar",
        );

        assert!(!is_exact_modrinth_match(&candidate, "1.21.6", "fabric"));
    }

    #[test]
    fn does_not_treat_library_version_numbers_as_minecraft_targets() {
        let candidate = version(
            "19.0.147+fabric",
            "release",
            &["1.21.6", "1.21.7", "1.21.8"],
            &["fabric"],
            "cloth-config-19.0.147-fabric.jar",
        );

        assert!(is_exact_modrinth_match(&candidate, "1.21.6", "fabric"));
    }

    #[test]
    fn ignores_mod_version_suffix_when_file_also_contains_minecraft_version() {
        let candidate = version(
            "1.4.0-beta.1",
            "beta",
            &["1.21.6", "1.21.7", "1.21.8"],
            &["fabric"],
            "moreculling-fabric-1.21.6-1.4.0-beta.1.jar",
        );

        assert!(is_exact_modrinth_match(&candidate, "1.21.6", "fabric"));
    }

    #[test]
    fn prefers_artifact_matching_requested_minecraft_version() {
        let selected = select_compatible_version(
            vec![
                version(
                    "mc1.21.8-2.0.0-fabric",
                    "release",
                    &["1.21.6", "1.21.7", "1.21.8"],
                    &["fabric"],
                    "example-fabric-2.0.0+mc1.21.8.jar",
                ),
                version(
                    "mc1.21.6-1.0.0-fabric",
                    "release",
                    &["1.21.6", "1.21.7", "1.21.8"],
                    &["fabric"],
                    "example-fabric-1.0.0+mc1.21.6.jar",
                ),
            ],
            "1.21.6",
            "fabric",
        )
        .expect("compatible version");

        assert_eq!(selected.version_number, "mc1.21.6-1.0.0-fabric");
    }

    #[test]
    fn detects_fabric_dependency_wildcard_range_that_loader_rejects() {
        let bytes = jar_with_fabric_mod_json(
            r#"{
              "schemaVersion": 1,
              "id": "example",
              "version": "1.0.0",
              "depends": {
                "sodium": ">=0.3.x"
              }
            }"#,
        );

        assert!(fabric_mod_json_has_invalid_wildcard_predicate(&bytes).unwrap());
    }

    #[test]
    fn accepts_fabric_dependency_plain_wildcard_predicate() {
        let bytes = jar_with_fabric_mod_json(
            r#"{
              "schemaVersion": 1,
              "id": "example",
              "version": "1.0.0",
              "depends": {
                "sodium": "0.3.x"
              }
            }"#,
        );

        assert!(!fabric_mod_json_has_invalid_wildcard_predicate(&bytes).unwrap());
    }

    #[test]
    fn extracts_required_fabric_dependencies_from_mod_json() {
        let bytes = jar_with_fabric_mod_json(
            r#"{
              "schemaVersion": 1,
              "id": "example",
              "version": "1.0.0",
              "depends": {
                "fabricloader": ">=0.14.0",
                "minecraft": "1.16.x",
                "fabric": "*",
                "sodium": "*"
              }
            }"#,
        );

        assert_eq!(
            fabric_mod_json_required_dependencies(&bytes)
                .unwrap()
                .into_iter()
                .map(|dependency| dependency.mod_id)
                .collect::<Vec<_>>(),
            vec!["fabric".to_string(), "sodium".to_string()]
        );
    }

    #[test]
    fn resolves_fabric_mod_id_to_fabric_api_slug_candidate() {
        assert_eq!(
            modrinth_slug_candidates_for_mod_id("fabric"),
            vec!["fabric-api", "fabric"]
        );
        assert_eq!(
            modrinth_slug_candidates_for_mod_id("sodium"),
            vec!["sodium"]
        );
    }

    #[test]
    fn resolves_library_mod_id_acronym_slug_candidates() {
        let candidates = modrinth_slug_candidates_for_mod_id("yet_another_config_lib_v3");
        assert!(candidates.contains(&"yet-another-config-lib".to_string()));
        assert!(candidates.contains(&"yacl".to_string()));
    }

    #[test]
    fn library_version_number_is_not_treated_as_minecraft_target() {
        assert!(artifact_is_usable_for_mc(
            "fabric-language-kotlin-1.13.12+kotlin.2.4.0.jar",
            "26.2"
        ));
        assert!(!artifact_is_usable_for_mc(
            "sodium-fabric-0.9.1+mc26.1.jar",
            "26.2"
        ));
        assert!(artifact_is_usable_for_mc(
            "sodium-fabric-0.9.1+mc26.2.jar",
            "26.2"
        ));
    }

    #[test]
    fn treats_fabric_api_modules_as_fabric_dependency_provider() {
        let ids = expand_available_mod_ids(HashSet::from(["fabric-api-base".to_string()]));
        assert!(ids.contains("fabric"));
    }
}
