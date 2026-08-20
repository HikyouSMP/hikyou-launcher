//! Mod 管理モジュール

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
        artifact_satisfies_all_predicates, artifact_satisfies_predicate, compare_release_versions,
        fetch_modrinth_versions_for_project, resolve_dependency_project_id,
        resolve_modrinth_project_from_mod_id, select_compatible_file, select_compatible_versions,
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
    cache,
    mod_files::{is_safe_filename, load_meta, mods_dir, save_meta},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio::fs;

// ── キャッシュ定数 ─────────────────────────────────────────────────────────────
const CACHE_MOD_SEARCH: &str = "modrinth_mod_search";
const TTL_SEARCH: i64 = 300; // 5 分
const AUTO_MOD_RESOLVER_VERSION: u8 = 5;

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

    let client = reqwest::Client::builder()
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

pub async fn read_auto_mod_sync_state(game_dir: &Path) -> Option<serde_json::Value> {
    mod_sync_state::read_status(game_dir).await
}

struct ActiveModManifest {
    project_id: String,
    manifest: InstalledModManifest,
}

/// メイン Mod と必須依存 Mod を全てインストールし、表示名/アイコンをキャッシュする。
pub async fn install_modrinth_mod(
    game_dir: &Path,
    project_id: &str,
    mc_version: &str,
    loader: &str,
    display_name: Option<String>,
    icon_url: Option<String>,
) -> Result<Vec<ModFile>, String> {
    let plan = resolve_modrinth_mod_install_plan(game_dir, project_id, mc_version, loader).await?;
    commit_mod_install_plan(game_dir, project_id, display_name, icon_url, plan).await
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

    let client = reqwest::Client::builder()
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let dir = mods_dir(game_dir);
    fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("mods failed to create directory: {}", e))?;

    let mut pending_installs: Vec<PendingModInstall> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<(String, bool, Vec<String>)> =
        vec![(project_id.to_string(), false, Vec::new())];

    while let Some((pid, is_dependency, predicates)) = queue.pop() {
        if !visited.insert(pid.clone()) {
            continue;
        }
        let is_main = pid == project_id;
        let mut effective_predicates = predicates;
        if !is_dependency {
            effective_predicates.extend(
                installed_dependency_predicates_for_project(
                    &client, game_dir, &pid, mc_version, loader_str,
                )
                .await,
            );
            effective_predicates.sort();
            effective_predicates.dedup();
        }

        if let Some((existing, jar_dependencies)) = existing_project_install(
            &client,
            game_dir,
            &pid,
            mc_version,
            loader_str,
            &effective_predicates,
        )
        .await
        {
            for dep in jar_dependencies {
                if dep.project_id != pid && !visited.contains(&dep.project_id) {
                    let predicates = dependency_project_predicates(&dep);
                    queue.push((dep.project_id, true, predicates));
                }
            }
            pending_installs.push(existing);
            continue;
        }

        let versions =
            fetch_modrinth_versions_for_project(&client, &pid, mc_version, loader_str, is_main)
                .await?;

        let mut selected: Option<SelectedModCandidate> = None;
        for version in select_compatible_versions(
            versions,
            mc_version,
            loader_str,
            !is_dependency,
            &effective_predicates,
        ) {
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

            let dest = dir.join(&file.filename);
            if dest.exists() {
                match fs::read(&dest).await {
                    Ok(bytes) => {
                        if mod_jar_is_loadable_for_loader(&bytes, loader_str)
                            && !mod_jar_conflicts_with_installed(
                                &bytes, game_dir, mc_version, loader_str,
                            )
                            .await
                        {
                            let jar_dependencies = required_modrinth_dependencies_from_mod_jar(
                                &client, &bytes, mc_version, loader_str,
                            )
                            .await;
                            let size = bytes.len() as u64;
                            selected = Some((version, file, None, size, jar_dependencies));
                            break;
                        }
                        log::warn!(
                            "[mods] Removing incompatible cached mod candidate: {:?}",
                            dest
                        );
                        let _ = fs::remove_file(&dest).await;
                    }
                    Err(_) => {
                        let _ = fs::remove_file(&dest).await;
                    }
                }
            }

            let dl_resp = client.get(&file.url).send().await;
            let bytes = match dl_resp {
                Err(e) => {
                    return Err(if is_main {
                        format!("download failed: {}", e)
                    } else {
                        format!("required dependency download failed for {}: {}", pid, e)
                    });
                }
                Ok(r) => match r.bytes().await {
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
                    Ok(b) => b.to_vec(),
                },
            };

            if !mod_jar_is_loadable_for_loader(&bytes, loader_str) {
                log::warn!(
                    "[mods] Skipping incompatible mod metadata candidate: {}",
                    file.filename
                );
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
            .await;
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

            if !visited.contains(&dep_id) {
                queue.push((dep_id, true, Vec::new()));
            }
        }
        for dep in jar_dependencies {
            if dep.project_id != pid && !visited.contains(&dep.project_id) {
                let predicates = dependency_project_predicates(&dep);
                queue.push((dep.project_id, true, predicates));
            }
        }

        pending_installs.push(PendingModInstall {
            project_id: pid.clone(),
            filename: file.filename.clone(),
            bytes,
            size_bytes,
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
        return Ok(());
    }

    let mut mods_to_install: Vec<AutoMod> = load_auto_mods(&auto_mods_file)
        .into_iter()
        .filter(|mod_def| auto_mod_applies_to_loader_and_version(mod_def, mc_version, loader))
        .collect();

    mods_to_install.sort_by_key(auto_mod_install_rank);

    let signature = mod_sync_state::signature(&mods_to_install, AUTO_MOD_RESOLVER_VERSION);
    let mod_dir_signature = mod_sync_state::dir_signature(game_dir).await;
    if mod_sync_state::is_fresh(game_dir, mc_version, loader, &signature, &mod_dir_signature).await
    {
        log::info!(
            "[mods] Auto mod sync skipped; cached state is fresh for {} {}",
            loader,
            mc_version
        );
        return Ok(());
    }

    let mut plan_set =
        resolve_auto_mod_plan_set(game_dir, &mods_to_install, mc_version, loader).await;

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
    error.contains("No compatible loadable")
        || error.contains("required dependency")
        || error.contains("incompatible")
}

async fn resolve_auto_mod_plan_set(
    game_dir: &Path,
    mods_to_install: &[AutoMod],
    mc_version: &str,
    loader: &str,
) -> AutoModPlanSet {
    let mut suppressed_project_ids = HashSet::new();
    let mut items = Vec::new();

    for _ in 0..3 {
        items.clear();
        for mod_def in mods_to_install {
            if suppressed_project_ids.contains(&mod_def.project_id) {
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
                    suppressed_project_ids.insert(mod_def.project_id.clone());
                    if auto_mod_install_error_should_disable_existing(&error) {
                        remove_project_files(game_dir, &mod_def.project_id).await;
                    }
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

    AutoModPlanSet {
        items,
        suppressed_project_ids,
    }
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
    let dir = mods_dir(game_dir);
    let mut manifests = Vec::new();

    for item in plan_items {
        for pending in &item.plan.installs {
            let bytes = match &pending.bytes {
                Some(bytes) => bytes.clone(),
                None => match fs::read(dir.join(&pending.filename)).await {
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

fn auto_mod_applies_to_loader_and_version(
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
) -> Option<(PendingModInstall, Vec<RequiredModrinthDependency>)> {
    let mut meta = load_meta(game_dir).await;
    let dir = mods_dir(game_dir);

    for (filename, entry) in meta.clone() {
        if entry.project_id != project_id || !is_safe_filename(&filename) {
            continue;
        }
        let active_filename = filename
            .strip_suffix(".disabled")
            .unwrap_or(&filename)
            .to_string();
        if !artifact_is_usable_for_mc(&active_filename, mc_version) {
            continue;
        }
        if !artifact_satisfies_all_predicates(&active_filename, predicates) {
            continue;
        }

        let path = dir.join(&filename);
        let bytes = fs::read(&path).await.ok()?;
        if !mod_jar_is_loadable_for_loader(&bytes, loader) {
            continue;
        }

        if filename.ends_with(".disabled") {
            let active_path = dir.join(&active_filename);
            let _ = fs::remove_file(&active_path).await;
            fs::rename(&path, &active_path).await.ok()?;
            if let Some(entry) = meta.remove(&filename) {
                meta.insert(active_filename.clone(), entry);
                save_meta(game_dir, &meta).await;
            }
        }

        let jar_dependencies =
            required_modrinth_dependencies_from_mod_jar(client, &bytes, mc_version, loader).await;

        return Some((
            PendingModInstall {
                project_id: project_id.to_string(),
                filename: active_filename,
                size_bytes: bytes.len() as u64,
                bytes: None,
            },
            jar_dependencies,
        ));
    }

    None
}

async fn installed_dependency_predicates_for_project(
    client: &reqwest::Client,
    game_dir: &Path,
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Vec<String> {
    let target_mod_ids = installed_mod_ids_for_project(game_dir, project_id).await;
    let dir = mods_dir(game_dir);
    let mut rd = match fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
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
        let Ok(bytes) = fs::read(entry.path()).await else {
            continue;
        };
        if !target_mod_ids.is_empty()
            && let Ok(dependencies) = fabric_mod_json_required_dependencies(&bytes)
        {
            for dependency in dependencies {
                if target_mod_ids.contains(&dependency.mod_id)
                    && let Some(predicate) = dependency.predicate
                {
                    predicates.push(predicate);
                }
            }
            continue;
        }
        for dependency in
            required_modrinth_dependencies_from_mod_jar(client, &bytes, mc_version, loader).await
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
    predicates
}

async fn installed_mod_ids_for_project(game_dir: &Path, project_id: &str) -> HashSet<String> {
    let meta = load_meta(game_dir).await;
    let dir = mods_dir(game_dir);
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
        let Ok(bytes) = fs::read(dir.join(&filename)).await else {
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
        let Ok(bytes) = fs::read(entry.path()).await else {
            continue;
        };
        if !mod_jar_is_loadable_for_loader(&bytes, loader) {
            continue;
        }
        if let Ok(Some(manifest)) = read_installed_mod_manifest(&active_filename, &bytes) {
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
        let Ok(bytes) = fs::read(entry.path()).await else {
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
    loader: &str,
) -> Result<Vec<String>, String> {
    if loader != "fabric" && loader != "quilt" {
        return Ok(Vec::new());
    }

    let dir = mods_dir(game_dir);
    let mut rd = match fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("failed to inspect mods directory: {}", e)),
    };
    let mut meta = load_meta(game_dir).await;
    let mut quarantined = Vec::new();
    let mut manifests = Vec::new();

    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !name.ends_with(".jar") || !is_safe_filename(&name) {
            continue;
        }

        let file_meta = match entry.metadata().await {
            Ok(file_meta) if file_meta.is_file() => file_meta,
            _ => continue,
        };
        if file_meta.len() == 0 {
            continue;
        }

        let path = entry.path();
        let bytes = match fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("[mods] Failed to inspect mod JAR {}: {}", name, e);
                continue;
            }
        };

        if mod_jar_is_loadable_for_loader(&bytes, loader) {
            if let Ok(Some(manifest)) = read_installed_mod_manifest(&name, &bytes) {
                manifests.push(manifest);
            }
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

    let available_versions: HashMap<String, Option<String>> = manifests
        .iter()
        .map(|manifest| (manifest.id.clone(), manifest.version.clone()))
        .collect();
    let available_ids = expand_available_mod_ids(available_versions.keys().cloned().collect());
    let unresolved: Vec<String> = manifests
        .iter()
        .filter(|manifest| {
            manifest.required_dependencies.iter().any(|dependency| {
                !dependency_is_available(&dependency.mod_id, &available_ids)
                    || !installed_dependency_version_is_compatible(dependency, &available_versions)
            })
        })
        .map(|manifest| manifest.filename.clone())
        .collect();
    for filename in unresolved {
        if disable_mod_file(&dir, &mut meta, &filename).await {
            log::warn!(
                "[mods] Disabled mod with unresolved required dependencies: {}",
                filename
            );
            quarantined.push(filename);
        }
    }

    save_meta(game_dir, &meta).await;

    Ok(quarantined)
}

fn expand_available_mod_ids(mut ids: HashSet<String>) -> HashSet<String> {
    ids.extend(["minecraft", "java", "fabricloader", "quilt_loader"].map(str::to_string));
    if ids.contains("fabric-api")
        || ids.contains("fabric-api-base")
        || ids.iter().any(|id| id.starts_with("fabric-"))
    {
        ids.insert("fabric".to_string());
    }
    ids
}

fn dependency_is_available(dependency: &str, available_ids: &HashSet<String>) -> bool {
    available_ids.contains(dependency)
        || (dependency.starts_with("fabric-")
            && (available_ids.contains("fabric-api") || available_ids.contains("fabric")))
}

fn installed_dependency_version_is_compatible(
    dependency: &FabricDependency,
    available_versions: &HashMap<String, Option<String>>,
) -> bool {
    let Some(predicate) = dependency.predicate.as_deref() else {
        return true;
    };
    if dependency.mod_id.starts_with("fabric-") || dependency.mod_id == "fabric" {
        return true;
    }
    let Some(Some(version)) = available_versions.get(&dependency.mod_id) else {
        return true;
    };
    artifact_satisfies_predicate(version, predicate)
}

async fn required_modrinth_dependencies_from_mod_jar(
    client: &reqwest::Client,
    bytes: &[u8],
    mc_version: &str,
    loader: &str,
) -> Vec<RequiredModrinthDependency> {
    if loader != "fabric" && loader != "quilt" {
        return Vec::new();
    }

    let dependencies = match fabric_mod_json_required_dependencies(bytes) {
        Ok(dependencies) => dependencies,
        Err(error) => {
            log::warn!("[mods] Failed to read mod dependencies from JAR: {}", error);
            return Vec::new();
        }
    };

    let mut projects = Vec::new();
    for dependency in dependencies {
        if let Some(project_id) =
            resolve_modrinth_project_from_mod_id(client, &dependency.mod_id, mc_version, loader)
                .await
        {
            projects.push(RequiredModrinthDependency {
                project_id,
                mod_id: dependency.mod_id,
                predicate: dependency.predicate,
            });
        } else {
            log::warn!(
                "[mods] Could not resolve required mod id '{}' to a Modrinth project",
                dependency.mod_id
            );
        }
    }
    projects.sort_by(|a, b| {
        a.project_id
            .cmp(&b.project_id)
            .then_with(|| a.mod_id.cmp(&b.mod_id))
            .then_with(|| a.predicate.cmp(&b.predicate))
    });
    projects.dedup();
    projects
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
    project_id == FABRIC_API_PROJECT_ID && (mod_id == "fabric" || mod_id.starts_with("fabric-"))
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
            project_id: None,
            version_number: version_number.to_string(),
            version_type: Some(_version_type.to_string()),
            game_versions: game_versions.iter().map(|v| v.to_string()).collect(),
            loaders: loaders.iter().map(|v| v.to_string()).collect(),
            files: vec![ModrinthFile {
                url: "https://example.invalid/mod.jar".to_string(),
                filename: filename.to_string(),
                primary: true,
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
