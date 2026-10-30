//! Requested roots are mandatory. Search is read-only; only a complete plan is committed.
//! Fabric metadata is checked directly (including nested jars and provided aliases).
//! Runtime predicates participate in search. Other loaders retain provider-only
//! dependency checks and JAR integrity checks, not a claim of native graph support.

use super::*;
use crate::core::mod_files::{ModMeta, load_meta_checked_in};
use crate::core::mod_intent::{DependencyEdge, Requirement, valid_project};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_PROJECTS: usize = 128;
const MAX_REQUIREMENTS: usize = 4096;
const MAX_CANDIDATES: usize = 64;
const MAX_STATES: usize = 20_000;
const MAX_NODES: usize = 4096;
const MAX_EDGES: usize = 32768;
const MAX_PREPARED_BYTES: u64 = 512 * 1024 * 1024;

struct Candidate {
    install: PendingModInstall,
    nodes: Vec<LocalModNode>,
    dependencies: Vec<ProjectDependency>,
}

#[derive(Clone)]
struct ProjectDependency {
    project: String,
    exact: Option<String>,
    incompatible: bool,
}

struct FixedJar {
    owner: String,
    nodes: Vec<LocalModNode>,
}

struct RuntimeVersions {
    loader: String,
    loader_version: Option<String>,
    java_major: Option<u32>,
    bundled: Vec<LocalModNode>,
}

#[derive(Default)]
struct Snapshot {
    roots: BTreeSet<String>,
    disabled: BTreeSet<String>,
    disabled_ids: BTreeSet<String>,
    protected_files: BTreeSet<String>,
    fixed: Vec<FixedJar>,
    pinned: BTreeMap<String, Option<String>>,
    obsolete: BTreeSet<String>,
    meta: ModMeta,
    runtime: Option<RuntimeVersions>,
}

trait Source {
    fn truncated_catalog(&self) -> bool {
        false
    }
    fn versions(
        &mut self,
        project: &str,
        exact: Option<&str>,
    ) -> impl Future<Output = Result<Vec<ModrinthVersion>, String>> + Send;
    fn prepare(
        &mut self,
        project: &str,
        version: &ModrinthVersion,
    ) -> impl Future<Output = Result<Option<Arc<Candidate>>, String>> + Send;
    fn projects_for_mod(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Vec<String>, String>> + Send;
}

struct Provider<'a> {
    client: reqwest::Client,
    game_dir: &'a Path,
    mc: &'a str,
    loader: &'a str,
    versions: BTreeMap<String, Vec<ModrinthVersion>>,
    exact: BTreeMap<String, ModrinthVersion>,
    prepared: BTreeMap<(String, String), Option<Arc<Candidate>>>,
    aliases: BTreeMap<String, Vec<String>>,
    bytes: u64,
    truncated: bool,
}

impl Provider<'_> {
    async fn exact_version(&mut self, id: &str) -> Result<ModrinthVersion, String> {
        if !valid_project(id) {
            return Err("invalid provider version identity".into());
        }
        if let Some(version) = self.exact.get(id) {
            return Ok(version.clone());
        }
        if self.exact.len() >= MAX_PROJECTS * MAX_CANDIDATES {
            return Err(limit("exact versions"));
        }
        let version = fetch_modrinth_version_by_id(&self.client, id).await?;
        self.exact.insert(id.into(), version.clone());
        Ok(version)
    }
}

impl Source for Provider<'_> {
    fn truncated_catalog(&self) -> bool {
        self.truncated
    }
    async fn versions(
        &mut self,
        project: &str,
        exact: Option<&str>,
    ) -> Result<Vec<ModrinthVersion>, String> {
        if !valid_project(project) {
            return Err("invalid provider project identity".into());
        }
        let versions = if let Some(id) = exact {
            vec![self.exact_version(id).await?]
        } else if let Some(versions) = self.versions.get(project) {
            versions.clone()
        } else {
            if self.versions.len() >= MAX_PROJECTS {
                return Err(limit("projects"));
            }
            let versions = fetch_modrinth_versions_for_project(
                &self.client,
                project,
                self.mc,
                self.loader,
                true,
            )
            .await?;
            self.versions.insert(project.into(), versions.clone());
            versions
        };
        for version in &versions {
            if version.project_id.as_deref() != Some(project)
                || !version.id.as_deref().is_some_and(valid_project)
            {
                return Err("provider version project/identity mismatch".into());
            }
        }
        let (versions, truncated) = compatible_candidate_window(versions, self.mc, self.loader);
        self.truncated |= truncated;
        Ok(versions)
    }

    async fn prepare(
        &mut self,
        project: &str,
        version: &ModrinthVersion,
    ) -> Result<Option<Arc<Candidate>>, String> {
        let id = version
            .id
            .as_deref()
            .ok_or("provider version identity missing")?;
        let key = (project.to_string(), id.to_string());
        if let Some(candidate) = self.prepared.get(&key) {
            return Ok(candidate.clone());
        }
        if self.prepared.len() >= MAX_PROJECTS * MAX_CANDIDATES {
            return Err(limit("prepared candidates"));
        }
        let Some(file) = select_compatible_file(version, self.mc).cloned() else {
            return Ok(None);
        };
        if !is_safe_filename(&file.filename)
            || !file.filename.ends_with(".jar")
            || !has_upstream_digest(&file)
        {
            return Err("provider artifact has unsafe filename or no supported digest".into());
        }
        let reservation = file.size.unwrap_or(super::super::mod_files::MAX_MOD_BYTES);
        if reservation > MAX_PREPARED_BYTES.saturating_sub(self.bytes) {
            return Err(limit("prepared bytes"));
        }
        let cached = match managed_mods(self.game_dir) {
            Ok(dir) if inspect_mod_path(&dir, Path::new(&file.filename))?.is_some() => {
                let bytes = read_mod_bytes_in(&dir, &file.filename)?;
                verify_mod_bytes(&bytes, &file.hashes, file.size)
                    .is_ok()
                    .then_some(bytes)
            }
            _ => None,
        };
        let bytes = match cached {
            Some(bytes) => bytes,
            None => {
                let url =
                    url::Url::parse(&file.url).map_err(|_| "invalid provider artifact URL")?;
                if url.scheme() != "https"
                    || url.host_str() != Some("cdn.modrinth.com")
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err("untrusted provider artifact URL".into());
                }
                let response = self
                    .client
                    .get(url)
                    .send()
                    .await
                    .map_err(|_| "provider artifact download failed")?;
                download_mod_bytes(response, file.size).await?
            }
        };
        verify_mod_bytes(&bytes, &file.hashes, file.size)?;
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| limit("prepared bytes"))?;
        if self.bytes > MAX_PREPARED_BYTES {
            return Err(limit("prepared bytes"));
        }
        let mut nodes = Vec::new();
        let inspected = check_mod_jar_structure(&file.filename, &bytes).and_then(|()| {
            if supports_dependency_validation(self.loader) {
                collect_local_mods(&file.filename, &bytes, self.loader, 0, &mut nodes)?;
                if nodes.is_empty() {
                    return Err("no client Mod metadata".to_string());
                }
            }
            Ok(())
        });
        if let Err(error) = inspected {
            log::debug!("[mods] Rejected provider candidate {project}/{id}: {error}");
            self.prepared.insert(key, None);
            return Ok(None);
        }
        let mut dependencies = Vec::new();
        if version.dependencies.len() > MAX_EDGES {
            return Err(limit("provider dependencies"));
        }
        for dep in &version.dependencies {
            match dep.dependency_type.as_str() {
                "optional" | "embedded" => continue,
                "required" | "incompatible" => {}
                _ => return Err("unsupported provider dependency kind".into()),
            }
            let mut pid = dep.project_id.clone();
            if let Some(exact) = &dep.version_id {
                let resolved = self.exact_version(exact).await?;
                if pid
                    .as_ref()
                    .is_some_and(|pid| Some(pid) != resolved.project_id.as_ref())
                {
                    return Err("provider dependency version project mismatch".into());
                }
                pid = resolved.project_id;
            }
            let project = pid
                .filter(|id| valid_project(id))
                .ok_or("provider dependency project missing or invalid")?;
            dependencies.push(ProjectDependency {
                project,
                exact: dep.version_id.clone(),
                incompatible: dep.dependency_type == "incompatible",
            });
        }
        dependencies.sort_by(|a, b| {
            (&a.project, &a.exact, a.incompatible).cmp(&(&b.project, &b.exact, b.incompatible))
        });
        let candidate = Arc::new(Candidate {
            install: PendingModInstall {
                project_id: project.into(),
                filename: file.filename,
                size_bytes: bytes.len() as u64,
                bytes: Some(bytes),
                version_id: version.id.clone(),
                version_number: Some(version.version_number.clone()),
                hashes: file.hashes,
                advertised_size: file.size,
                game_versions: version.game_versions.clone(),
                loaders: version.loaders.clone(),
            },
            nodes,
            dependencies,
        });
        self.prepared.insert(key, Some(candidate.clone()));
        Ok(Some(candidate))
    }

    async fn projects_for_mod(&mut self, id: &str) -> Result<Vec<String>, String> {
        if let Some(projects) = self.aliases.get(id) {
            return Ok(projects.clone());
        }
        if !valid_project(id) {
            return Err("unsupported required mod identity".into());
        }
        if self.aliases.len() >= MAX_NODES {
            return Err(limit("mod aliases"));
        }
        // Resolve canonical IDs, not search hits with unverified artifact downloads.
        // A guessed slug is only a hint: the selected JAR must actually provide id.
        let mut projects = Vec::new();
        let slugs = crate::core::modrinth_provider::modrinth_slug_candidates_for_mod_id(id);
        if slugs.len() > 32 {
            return Err(limit("project aliases"));
        }
        for slug in slugs {
            let Some(project) = canonical_project_with_client(&self.client, &slug).await? else {
                continue;
            };
            if !projects.contains(&project) {
                projects.push(project);
            }
        }
        self.aliases.insert(id.into(), projects.clone());
        Ok(projects)
    }
}

fn canonical_project_identity(requested: &str, json: &serde_json::Value) -> Result<String, String> {
    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| valid_project(id))
        .ok_or("provider project identity missing or invalid")?;
    if requested != id
        && !json
            .get("slug")
            .and_then(|v| v.as_str())
            .is_some_and(|slug| slug.eq_ignore_ascii_case(requested))
    {
        return Err("provider project alias identity mismatch".into());
    }
    Ok(id.into())
}

async fn canonical_project_with_client(
    client: &reqwest::Client,
    project: &str,
) -> Result<Option<String>, String> {
    if !valid_project(project) {
        return Err("invalid provider project identity".into());
    }
    let response = client
        .get(format!(
            "https://api.modrinth.com/v2/project/{}",
            urlencoding::encode(project)
        ))
        .send()
        .await
        .map_err(|_| "provider project lookup failed")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let mut response = response
        .error_for_status()
        .map_err(|_| "provider project lookup failed")?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "provider project lookup failed")?
    {
        if chunk.len() > (1024 * 1024usize).saturating_sub(bytes.len()) {
            return Err(limit("project metadata bytes"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid provider project metadata")?;
    canonical_project_identity(project, &json).map(Some)
}

/// Normalize a legacy slug before persisting intent. Reconciliation deliberately
/// does not rename caller-owned requirements or silently change their identity.
pub async fn canonical_modrinth_project_id(project: &str) -> Result<String, String> {
    let client = crate::http::builder()?
        .user_agent("HikyouLauncher/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    canonical_project_with_client(&client, project)
        .await?
        .ok_or_else(|| "provider project was not found".into())
}

fn compatible_candidate_window(
    versions: Vec<ModrinthVersion>,
    mc: &str,
    loader: &str,
) -> (Vec<ModrinthVersion>, bool) {
    let mut versions: Vec<_> = versions
        .into_iter()
        .filter(|version| {
            crate::core::modrinth_provider::is_exact_modrinth_match(version, mc, loader)
        })
        .collect();
    let truncated = versions.len() > MAX_CANDIDATES;
    versions.truncate(MAX_CANDIDATES);
    (versions, truncated)
}

fn limit(kind: &str) -> String {
    format!("mod_resolution_limit: {kind}; no partial solution accepted")
}

fn object_id(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn former_dependencies(
    requirements: &[Requirement],
    previous: &[DependencyEdge],
) -> Result<BTreeSet<String>, String> {
    if previous.len() > MAX_EDGES {
        return Err(limit("previous edges"));
    }
    let roots: BTreeSet<_> = requirements
        .iter()
        .filter_map(|r| r.project_id.as_deref())
        .collect();
    let mut former = BTreeSet::new();
    for edge in previous {
        if !valid_project(&edge.requester)
            || !valid_project(&edge.dependency)
            || edge
                .exact_version
                .as_deref()
                .is_some_and(|id| !valid_project(id))
            || edge.predicate.as_ref().is_some_and(|p| p.len() > 4096)
        {
            return Err("invalid previous dependency edge".into());
        }
        if !roots.contains(edge.dependency.as_str()) {
            former.insert(edge.dependency.clone());
        }
    }
    Ok(former)
}

fn snapshot(
    game_dir: &Path,
    requirements: &[Requirement],
    previous: &[DependencyEdge],
    mc: &str,
    loader: &str,
) -> Result<Snapshot, String> {
    if requirements.len() > MAX_REQUIREMENTS {
        return Err(limit("requested records"));
    }
    if requirements.iter().filter(|r| r.enabled).count() > MAX_PROJECTS {
        return Err(limit("requested roots"));
    }
    let mut state = Snapshot::default();
    let former = former_dependencies(requirements, previous)?;
    let mut local = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for requirement in requirements {
        if !identities.insert(requirement.key()) {
            return Err("duplicate requested Mod identity".into());
        }
        match (
            &requirement.project_id,
            &requirement.object_id,
            &requirement.filename,
        ) {
            (Some(project), None, None) if valid_project(project) => {
                if requirement.enabled {
                    state.roots.insert(project.clone());
                } else {
                    state.disabled.insert(project.clone());
                }
            }
            (None, Some(id), Some(name))
                if id.len() == 64
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && is_safe_filename(name) =>
            {
                let active = is_active_mod_filename(name);
                if !(active || name.ends_with(".jar.disabled")) || active != requirement.enabled {
                    return Err("local requested Mod state does not match staged filename".into());
                }
                local.insert(name.clone(), (id.clone(), requirement.enabled));
                state.protected_files.insert(name.clone());
            }
            _ => return Err("invalid requested Mod identity".into()),
        }
    }
    let root = managed_fs::root(game_dir)?;
    if inspect_mod_path(&root, Path::new("mods"))?.is_none() {
        if !local.is_empty() {
            return Err("requested local Mod is missing from stage".into());
        }
        return Ok(state);
    }
    let dir = managed_mods(game_dir)?;
    state.meta = load_meta_checked_in(&dir)?;
    let mut names = dir
        .entries()
        .map_err(|e| e.to_string())?
        .map(|entry| {
            entry
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    if names.len() > MAX_NODES {
        return Err(limit("staged files"));
    }
    let mut seen_local = BTreeSet::new();
    let mut total = 0u64;
    for name in names {
        let active = is_active_mod_filename(&name);
        if !active && !name.ends_with(".jar.disabled") {
            continue;
        }
        let bytes = read_mod_bytes_in(&dir, &name)?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| limit("staged bytes"))?;
        if total > MAX_PREPARED_BYTES {
            return Err(limit("staged bytes"));
        }
        let entry = state.meta.get(&name);
        let project = entry
            .map(|e| e.project_id.as_str())
            .filter(|id| valid_project(id));
        let local_root = local.get(&name);
        if let Some((id, enabled)) = local_root {
            if object_id(&bytes) != *id || *enabled != active {
                return Err("requested local Mod identity changed".into());
            }
            seen_local.insert(name.clone());
        }
        if let Some(hashes) = entry.and_then(|e| e.hashes.as_ref()) {
            verify_mod_bytes(&bytes, hashes, None)?;
        }
        if local_root.is_none() && project.is_some_and(|id| state.disabled.contains(id)) && active {
            return Err("disabled requested project is active in stage".into());
        }
        // Explicit project roots may change versions; former dependencies may disappear.
        // An explicit local requirement always wins over cached project ownership.
        if local_root.is_none()
            && project.is_some_and(|p| state.roots.contains(p) || former.contains(p))
        {
            if project.is_some_and(|p| former.contains(p)) {
                state.obsolete.insert(name);
            }
            continue;
        }
        let mut nodes = Vec::new();
        if !active {
            // Disabled jars do not participate in the load graph. Inspect aliases
            // where possible, without requiring inactive jars to be Fabric mods.
            let _ = collect_local_mods(&name, &bytes, loader, 0, &mut nodes);
            state
                .disabled_ids
                .extend(nodes.iter().flat_map(|n| n.ids.iter().cloned()));
            if let Some(project) = project {
                state.disabled.insert(project.into());
            }
            state.protected_files.insert(name);
            continue;
        }
        check_mod_jar_structure(&name, &bytes)?;
        if supports_dependency_validation(loader) {
            collect_local_mods(&name, &bytes, loader, 0, &mut nodes)?;
        }
        if let Some(entry) = entry
            && ((!entry.game_versions.is_empty() && !entry.game_versions.iter().any(|v| v == mc))
                || (!entry.loaders.is_empty()
                    && !entry
                        .loaders
                        .iter()
                        .any(|v| v == loader || (loader == "quilt" && v == "fabric"))))
        {
            return Err("fixed Mod provider targeting mismatch".into());
        }
        let owner = local_root
            .map(|(id, _)| id.clone())
            .or_else(|| project.map(str::to_string))
            .unwrap_or_else(|| object_id(&bytes));
        if let Some(project) = project
            && state
                .pinned
                .insert(project.into(), entry.and_then(|e| e.version_id.clone()))
                .is_some()
        {
            return Err("multiple fixed files claim the same project".into());
        }
        state.protected_files.insert(name);
        state.fixed.push(FixedJar { owner, nodes });
    }
    if local.keys().any(|name| !seen_local.contains(name)) {
        return Err("requested local Mod is missing from stage".into());
    }
    Ok(state)
}

type Selection = BTreeMap<String, Arc<Candidate>>;

enum Analysis {
    Complete(Vec<DependencyEdge>),
    Project(String, Option<String>),
    Mod(String),
    Conflict(String),
}

fn runtime_id(id: &str) -> bool {
    matches!(
        id,
        "java" | "fabricloader" | "fabric-loader" | "quilt_loader" | "quilt-loader"
    )
}

fn runtime_predicate_matches(
    snapshot: &Snapshot,
    id: &str,
    predicate: &serde_json::Value,
) -> Result<bool, String> {
    let Some(runtime) = &snapshot.runtime else {
        return Ok(true);
    };
    let java = runtime.java_major.map(|v| v.to_string());
    let version = if id == "java" {
        java.as_deref()
    } else if matches!(id, "quilt_loader" | "quilt-loader") && runtime.loader != "quilt" {
        return Ok(false);
    } else {
        runtime.loader_version.as_deref()
    };
    check_runtime_version_known(id, version, predicate)?;
    local_predicate_matches(version, predicate)
}

fn check_runtime(
    snapshot: &Snapshot,
    mc: &str,
    id: &str,
    predicate: &serde_json::Value,
) -> Result<super::local_graph::RuntimeCheck, String> {
    use super::local_graph::RuntimeCheck;
    if id == "minecraft" {
        local_minecraft_predicate_matches(mc, predicate).map(RuntimeCheck::Matches)
    } else if runtime_id(id) {
        if snapshot.runtime.is_none() {
            Ok(RuntimeCheck::Unchecked)
        } else {
            runtime_predicate_matches(snapshot, id, predicate).map(RuntimeCheck::Matches)
        }
    } else {
        Ok(RuntimeCheck::NotRuntime)
    }
}

fn analyze(snapshot: &Snapshot, selected: &Selection, mc: &str) -> Result<Analysis, String> {
    let all: Vec<_> = snapshot
        .fixed
        .iter()
        .map(|f| (f.owner.as_str(), &f.nodes))
        .chain(selected.iter().map(|(pid, c)| (pid.as_str(), &c.nodes)))
        .flat_map(|(owner, nodes)| nodes.iter().map(move |node| (owner, node)))
        .collect();
    let owners: HashMap<*const LocalModNode, &str> = all
        .iter()
        .map(|(owner, node)| (*node as *const LocalModNode, *owner))
        .collect();
    let input: Vec<_> = all.iter().map(|(_, node)| *node).collect();
    let runtime =
        |id: &str, predicate: &serde_json::Value| check_runtime(snapshot, mc, id, predicate);
    let nodes = match super::local_graph::select(&input, true, &snapshot.disabled_ids, &runtime)? {
        super::local_graph::Resolution::Selected(nodes) => nodes,
        super::local_graph::Resolution::Conflict(error) => return Ok(Analysis::Conflict(error)),
    };
    let nodes: Vec<_> = nodes
        .into_iter()
        .map(|node| (owners[&(node as *const LocalModNode)], node))
        .collect();
    let mut edges = Vec::new();
    let mut missing = None;
    let mut available = BTreeMap::<&str, (&str, &LocalModNode)>::new();
    for &(owner, node) in &nodes {
        for id in &node.ids {
            available.insert(id, (owner, node));
        }
    }
    for (project, candidate) in selected {
        if snapshot.disabled.contains(project) {
            return Ok(Analysis::Conflict(format!(
                "required dependency {project} is disabled"
            )));
        }
        for dep in &candidate.dependencies {
            let version = selected
                .get(&dep.project)
                .map(|c| &c.install.version_id)
                .or_else(|| snapshot.pinned.get(&dep.project));
            if dep.exact.is_some() && matches!(version, Some(None)) {
                return Err(format!(
                    "mod_resolution_unsupported: exact provider version is unknown for fixed project {}",
                    dep.project
                ));
            }
            let matches = version.is_some_and(|v| dep.exact.is_none() || *v == dep.exact);
            if dep.incompatible {
                if matches {
                    return Ok(Analysis::Conflict(format!(
                        "incompatible provider dependency {}",
                        dep.project
                    )));
                }
                continue;
            }
            if snapshot.disabled.contains(&dep.project) {
                return Ok(Analysis::Conflict(format!(
                    "required dependency {} is disabled",
                    dep.project
                )));
            }
            if let Some(version) = version {
                if dep.exact.is_some() && *version != dep.exact {
                    return Ok(Analysis::Conflict(format!(
                        "exact version conflict for {}",
                        dep.project
                    )));
                }
            } else if missing.is_none() {
                missing = Some(Analysis::Project(dep.project.clone(), dep.exact.clone()));
            }
            edges.push(DependencyEdge {
                requester: project.clone(),
                dependency: dep.project.clone(),
                predicate: None,
                exact_version: dep.exact.clone(),
            });
        }
    }
    for &(owner, node) in &nodes {
        for (id, predicate) in &node.depends {
            if runtime_id(id) || id == "minecraft" {
                continue;
            }
            if let Some((provider, _)) = available.get(id.as_str()) {
                if *provider != owner
                    && *provider != loader_bundle::OWNER
                    && owner != loader_bundle::OWNER
                {
                    let predicate = Some(match predicate.as_str() {
                        Some(predicate) => predicate.to_string(),
                        None => serde_json::to_string(predicate).map_err(|e| e.to_string())?,
                    });
                    if predicate.as_ref().is_some_and(|p| p.len() > 4096) {
                        return Err(limit("predicate bytes"));
                    }
                    edges.push(DependencyEdge {
                        requester: owner.into(),
                        dependency: (*provider).into(),
                        predicate,
                        exact_version: None,
                    });
                }
            } else if missing.is_none() {
                missing = Some(Analysis::Mod(id.clone()));
            }
        }
        if edges.len() > MAX_EDGES {
            return Err(limit("graph edges"));
        }
    }
    if let Some(missing) = missing {
        return Ok(missing);
    }
    edges.sort_by(|a, b| {
        (&a.requester, &a.dependency, &a.predicate, &a.exact_version).cmp(&(
            &b.requester,
            &b.dependency,
            &b.predicate,
            &b.exact_version,
        ))
    });
    edges.dedup();
    Ok(Analysis::Complete(edges))
}

struct Search<'a, S> {
    source: &'a mut S,
    snapshot: &'a Snapshot,
    mc: &'a str,
    states: usize,
    last_conflict: String,
    uncertain: Option<String>,
    catalogs: BTreeMap<String, Vec<ModrinthVersion>>,
    nogoods: BTreeSet<Vec<(String, String)>>,
}

// Only mandatory top-level nodes can prove a conflict that adding another
// project cannot repair. Nested candidates must not produce learned clauses.
fn mandatory_conflict(
    snapshot: &Snapshot,
    selected: &Selection,
    mc: &str,
) -> Option<(String, Vec<(String, String)>)> {
    let roots: Vec<_> = snapshot
        .fixed
        .iter()
        .map(|f| (f.owner.as_str(), &f.nodes))
        .chain(selected.iter().map(|(owner, c)| (owner.as_str(), &c.nodes)))
        .flat_map(|(owner, nodes)| {
            nodes
                .iter()
                .filter(|node| node.depth == 0)
                .map(move |node| (owner, node))
        })
        .collect();
    let clause = |owners: &[&str]| -> Option<Vec<(String, String)>> {
        let mut clause = Vec::new();
        for &owner in owners {
            if let Some(candidate) = selected.get(owner) {
                clause.push((owner.to_string(), candidate.install.version_id.clone()?));
            }
        }
        clause.sort();
        clause.dedup();
        Some(clause)
    };
    let mut providers = BTreeMap::new();
    for &(owner, node) in &roots {
        for id in &node.ids {
            if let Some((other, _)) = providers.insert(id, (owner, node)) {
                return Some((
                    format!("duplicate mandatory Mod identity {id}"),
                    clause(&[owner, other])?,
                ));
            }
        }
    }
    for &(owner, node) in &roots {
        for (id, predicate) in &node.depends {
            if matches!(
                check_runtime(snapshot, mc, id, predicate),
                Ok(super::local_graph::RuntimeCheck::Matches(false))
            ) || snapshot.disabled_ids.contains(id)
            {
                return Some((
                    format!("mandatory Mod runtime/disabled dependency conflict for {id}"),
                    clause(&[owner])?,
                ));
            }
            if let Some(&(provider, provided)) = providers.get(id)
                && local_predicate_matches(provided.version.as_deref(), predicate) == Ok(false)
            {
                return Some((
                    format!("mandatory Mod dependency conflict for {id}"),
                    clause(&[owner, provider])?,
                ));
            }
        }
        for (id, predicate) in &node.incompatible {
            if matches!(
                check_runtime(snapshot, mc, id, predicate),
                Ok(super::local_graph::RuntimeCheck::Matches(true))
            ) {
                return Some((
                    format!("mandatory Mod breaks runtime {id}"),
                    clause(&[owner])?,
                ));
            }
            if let Some(&(provider, provided)) = providers.get(id)
                && local_predicate_matches(provided.version.as_deref(), predicate) == Ok(true)
            {
                return Some((
                    format!("mandatory Mod breaks dependency {id}"),
                    clause(&[owner, provider])?,
                ));
            }
        }
    }
    for (owner, candidate) in selected {
        for dependency in &candidate.dependencies {
            if !dependency.incompatible && snapshot.disabled.contains(&dependency.project) {
                return Some((
                    format!("required dependency {} is disabled", dependency.project),
                    clause(&[owner])?,
                ));
            }
            let version = selected
                .get(&dependency.project)
                .map(|c| &c.install.version_id)
                .or_else(|| snapshot.pinned.get(&dependency.project));
            if let Some(version) = version {
                if dependency.exact.is_some() && version.is_none() {
                    continue;
                }
                let matches = dependency.exact.is_none() || *version == dependency.exact;
                if (dependency.incompatible && matches) || (!dependency.incompatible && !matches) {
                    let reason = if dependency.incompatible {
                        "incompatible provider dependency"
                    } else {
                        "exact version conflict for"
                    };
                    return Some((
                        format!("{reason} {}", dependency.project),
                        clause(&[owner, &dependency.project])?,
                    ));
                }
            }
        }
    }
    None
}

type SearchResult = Result<Option<(Selection, Vec<DependencyEdge>)>, String>;
type SearchFuture<'a> = Pin<Box<dyn Future<Output = SearchResult> + Send + 'a>>;

impl<S: Source + Send> Search<'_, S> {
    fn excluded(&self, selected: &Selection, project: &str, version: &ModrinthVersion) -> bool {
        self.nogoods.iter().any(|clause| {
            clause.iter().all(|(owner, id)| {
                if owner == project {
                    version.id.as_ref() == Some(id)
                } else {
                    selected
                        .get(owner)
                        .is_some_and(|c| c.install.version_id.as_ref() == Some(id))
                }
            })
        })
    }

    async fn next_root(
        &mut self,
        selected: &Selection,
    ) -> Result<Option<(String, Vec<ModrinthVersion>)>, String> {
        let mut next: Option<(String, Vec<ModrinthVersion>)> = None;
        for project in &self.snapshot.roots {
            if selected.contains_key(project) {
                continue;
            }
            if !self.catalogs.contains_key(project) {
                let versions = self.source.versions(project, None).await?;
                if versions.len() > MAX_CANDIDATES {
                    return Err(limit("candidates per project"));
                }
                self.catalogs.insert(project.clone(), versions);
            }
            let versions: Vec<_> = self.catalogs[project]
                .iter()
                .filter(|version| !self.excluded(selected, project, version))
                .cloned()
                .collect();
            if versions.is_empty() {
                return Ok(Some((project.clone(), versions)));
            }
            if next
                .as_ref()
                .is_none_or(|(_, previous)| versions.len() < previous.len())
            {
                next = Some((project.clone(), versions));
            }
        }
        Ok(next)
    }

    fn solve<'a>(&'a mut self, selected: Selection) -> SearchFuture<'a> {
        Box::pin(async move {
            tokio::task::yield_now().await;
            self.states += 1;
            if self.states > MAX_STATES || selected.len() > MAX_PROJECTS {
                let kind = if selected.len() > MAX_PROJECTS {
                    "projects"
                } else {
                    "search states"
                };
                return Err(format!(
                    "{}; states={}; projects={}; roots={}",
                    limit(kind),
                    self.states,
                    selected.len(),
                    self.snapshot.roots.len()
                ));
            }
            let analysis = match analyze(self.snapshot, &selected, self.mc) {
                Ok(Analysis::Conflict(error)) => {
                    self.last_conflict = error;
                    return Ok(None);
                }
                Ok(analysis) => analysis,
                Err(error) if error.starts_with("mod_resolution_limit:") => return Err(error),
                Err(error) => {
                    self.uncertain.get_or_insert(error);
                    return Ok(None);
                }
            };
            // Provider project dependencies already have exact ownership. Resolve
            // them early; defer heuristic Mod-ID lookup until all roots are present.
            let next_root = if matches!(analysis, Analysis::Project(_, _)) {
                None
            } else {
                self.next_root(&selected).await?
            };
            let (choices, required_mod) = if let Analysis::Project(project, exact) = &analysis {
                (vec![(project.clone(), exact.clone())], None)
            } else if let Some((root, versions)) = &next_root {
                if versions.is_empty() {
                    self.last_conflict =
                        format!("no compatible candidate for required project {root}");
                    return Ok(None);
                }
                (vec![(root.clone(), None)], None)
            } else {
                match analysis {
                    Analysis::Complete(edges) => return Ok(Some((selected, edges))),
                    Analysis::Conflict(error) => {
                        self.last_conflict = error;
                        return Ok(None);
                    }
                    Analysis::Project(project, exact) => (vec![(project, exact)], None),
                    Analysis::Mod(id) => {
                        let projects = self.source.projects_for_mod(&id).await?;
                        let choices: Vec<_> = projects
                            .into_iter()
                            .filter(|project| {
                                !selected.contains_key(project)
                                    && !self.snapshot.pinned.contains_key(project)
                            })
                            .map(|project| (project, None))
                            .collect();
                        if choices.is_empty() {
                            self.last_conflict = format!("unresolved required Mod {id}");
                            self.uncertain.get_or_insert_with(|| format!("mod_resolution_unsupported: no verified provider mapping for {id}"));
                            return Ok(None);
                        }
                        (choices, Some(id))
                    }
                }
            };
            for (project, exact) in choices {
                if self.snapshot.disabled.contains(&project) {
                    self.last_conflict = format!("required dependency {project} is disabled");
                    continue;
                }
                let versions = if let Some((root, versions)) = &next_root
                    && root == &project
                    && exact.is_none()
                {
                    versions.clone()
                } else {
                    self.source.versions(&project, exact.as_deref()).await?
                };
                if versions.len() > MAX_CANDIDATES {
                    return Err(limit("candidates per project"));
                }
                for version in versions {
                    if self.excluded(&selected, &project, &version) {
                        continue;
                    }
                    if exact
                        .as_ref()
                        .is_some_and(|id| version.id.as_ref() != Some(id))
                    {
                        continue;
                    }
                    let Some(candidate) = self.source.prepare(&project, &version).await? else {
                        continue;
                    };
                    if required_mod
                        .as_ref()
                        .is_some_and(|id| !candidate.nodes.iter().any(|node| node.ids.contains(id)))
                    {
                        self.last_conflict = format!(
                            "candidate for {project} does not provide the required Mod identity"
                        );
                        continue;
                    }
                    let filename = &candidate.install.filename;
                    if self.snapshot.pinned.contains_key(&project)
                        || self
                            .snapshot
                            .protected_files
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(filename))
                        || self.snapshot.meta.iter().any(|(name, entry)| {
                            (entry.project_id == project
                                && self.snapshot.protected_files.contains(name))
                                || (name.eq_ignore_ascii_case(filename)
                                    && entry.project_id != project)
                        })
                        || selected
                            .values()
                            .any(|c| c.install.filename.eq_ignore_ascii_case(filename))
                    {
                        self.last_conflict =
                            format!("candidate for {project} would overwrite another root");
                        continue;
                    }
                    let mut branch = selected.clone();
                    branch.insert(project.clone(), candidate);
                    if let Some((reason, clause)) =
                        mandatory_conflict(self.snapshot, &branch, self.mc)
                    {
                        self.last_conflict = reason;
                        self.nogoods.insert(clause);
                        continue;
                    }
                    if let Some(solution) = self.solve(branch).await? {
                        return Ok(Some(solution));
                    }
                }
                if self.last_conflict.is_empty() {
                    self.last_conflict =
                        format!("no compatible candidate for required project {project}");
                }
            }
            Ok(None)
        })
    }
}

/// Reconcile a disposable stage only. The caller retains desired intent on error
/// Project roots must use canonical IDs (normalize legacy slugs before persisting).
/// Other loaders validate known provider constraints/integrity only; callers must
/// not represent those results as a validated Forge/Quilt native dependency graph.
pub async fn reconcile_requested_mods(
    game_dir: &Path,
    requirements: &[Requirement],
    previous_edges: &[DependencyEdge],
    mc_version: &str,
    loader: &str,
    loader_version: Option<&str>,
    java_major: Option<u32>,
) -> Result<Vec<DependencyEdge>, String> {
    if !matches!(loader, "fabric" | "quilt" | "forge" | "neoforge") {
        return Err(format!("unsupported Mod reconciliation loader: {loader}"));
    }
    let client = crate::http::builder()?
        .user_agent("HikyouLauncher/1.0")
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let mut provider = Provider {
        client,
        game_dir,
        mc: mc_version,
        loader,
        versions: BTreeMap::new(),
        exact: BTreeMap::new(),
        prepared: BTreeMap::new(),
        aliases: BTreeMap::new(),
        bytes: 0,
        truncated: false,
    };
    tokio::time::timeout(std::time::Duration::from_secs(300), async {
        let bundled = loader_bundle::load(loader, loader_version).await?;
        reconcile_with_source_and_runtime(
            game_dir,
            requirements,
            previous_edges,
            mc_version,
            loader,
            &mut provider,
            Some(RuntimeVersions {
                loader: loader.into(),
                loader_version: loader_version.map(str::to_string),
                java_major,
                bundled,
            }),
        )
        .await
    })
    .await
    .map_err(|_| limit("wall time"))?
}

#[cfg(test)]
async fn reconcile_with_source<S: Source + Send>(
    game_dir: &Path,
    requirements: &[Requirement],
    previous: &[DependencyEdge],
    mc: &str,
    loader: &str,
    source: &mut S,
) -> Result<Vec<DependencyEdge>, String> {
    reconcile_with_source_and_runtime(game_dir, requirements, previous, mc, loader, source, None)
        .await
}

async fn reconcile_with_source_and_runtime<S: Source + Send>(
    game_dir: &Path,
    requirements: &[Requirement],
    previous: &[DependencyEdge],
    mc: &str,
    loader: &str,
    source: &mut S,
    mut runtime: Option<RuntimeVersions>,
) -> Result<Vec<DependencyEdge>, String> {
    let mut snapshot = snapshot(game_dir, requirements, previous, mc, loader)?;
    if let Some(runtime) = &mut runtime
        && !runtime.bundled.is_empty()
    {
        snapshot.fixed.push(FixedJar {
            owner: loader_bundle::OWNER.into(),
            nodes: std::mem::take(&mut runtime.bundled),
        });
    }
    snapshot.runtime = runtime;
    let mut search = Search {
        source,
        snapshot: &snapshot,
        mc,
        states: 0,
        last_conflict: String::new(),
        uncertain: None,
        catalogs: BTreeMap::new(),
        nogoods: BTreeSet::new(),
    };
    let result = search.solve(BTreeMap::new()).await;
    log::debug!(
        "[mods] Resolution for Minecraft {mc}/{loader}: states={}, requested={}, learned_conflicts={}",
        search.states,
        snapshot.roots.len(),
        search.nogoods.len()
    );
    let Some((selected, edges)) = result? else {
        if search.source.truncated_catalog() {
            return Err(limit("newest compatible candidate window"));
        }
        if let Some(error) = search.uncertain {
            return Err(error);
        }
        return Err(format!(
            "mod_resolution_unsatisfiable: {}; all requested roots retained",
            search.last_conflict
        ));
    };
    let mut names = BTreeSet::new();
    for (project, candidate) in &selected {
        if !names.insert(candidate.install.filename.to_ascii_lowercase())
            || snapshot.pinned.contains_key(project)
            || snapshot.meta.iter().any(|(name, entry)| {
                entry.project_id == *project && snapshot.protected_files.contains(name)
            })
            || snapshot
                .protected_files
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&candidate.install.filename))
        {
            return Err("resolved plan would overwrite a fixed/local root".into());
        }
        if let Ok(dir) = managed_mods(game_dir)
            && inspect_mod_path(&dir, Path::new(&candidate.install.filename))?.is_some()
            && !snapshot
                .meta
                .get(&candidate.install.filename)
                .is_some_and(|e| e.project_id == *project)
        {
            return Err(
                "resolved artifact destination belongs to an unknown/different root".into(),
            );
        }
    }
    // One existing transactional commit for the entire selected graph. Nothing
    // above this point writes files, including unsatisfiable/exhausted searches.
    if let Some(first) = selected.keys().next() {
        let root = requirements
            .iter()
            .find(|r| r.project_id.as_ref() == Some(first));
        commit_mod_install_plan(
            game_dir,
            first,
            root.and_then(|r| r.display_name.clone()),
            root.and_then(|r| r.icon_url.clone()),
            ModInstallPlan {
                installs: selected.values().map(|c| c.install.clone()).collect(),
            },
        )
        .await?;
    }
    if !snapshot.obsolete.is_empty() {
        let dir = managed_mods(game_dir)?;
        let mut meta = load_meta_checked_in(&dir)?;
        for name in &snapshot.obsolete {
            let Some(entry) = snapshot.meta.get(name) else {
                continue;
            };
            if selected.contains_key(&entry.project_id) || snapshot.protected_files.contains(name) {
                continue;
            }
            if inspect_mod_path(&dir, Path::new(name))?.is_some() {
                read_mod_file(&dir, Path::new(name))?;
                dir.remove_file(name).map_err(|e| e.to_string())?;
            }
            meta.remove(name);
        }
        replace_stage_metadata(&dir, &meta)?;
    }
    if supports_dependency_validation(loader)
        && let Some(runtime) = &snapshot.runtime
    {
        validate_mod_set_with_known_runtime(
            game_dir,
            mc,
            loader,
            runtime.loader_version.as_deref(),
            runtime.java_major,
            snapshot
                .fixed
                .iter()
                .filter(|jar| jar.owner == loader_bundle::OWNER)
                .flat_map(|jar| jar.nodes.iter().cloned())
                .collect(),
        )
        .await?;
    } else {
        validate_mod_set(game_dir, mc, loader).await?;
    }
    Ok(edges)
}

fn replace_stage_metadata(dir: &cap_std::fs::Dir, meta: &ModMeta) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?;
    let name = ".mod_meta.json";
    let backup = format!(".mod-solver-meta-{}", uuid::Uuid::new_v4());
    let existed = inspect_mod_path(dir, Path::new(name))?.is_some();
    if existed {
        read_mod_file(dir, Path::new(name))?;
        dir.rename(name, dir, &backup)
            .map_err(|e| format!("stage metadata backup failed: {e}"))?;
    }
    // A confined exclusive create also works on Windows, where replacing an
    // existing cap-std destination with rename can fail with AccessDenied.
    if let Err(error) = managed_fs::write(dir, Path::new(name), &bytes, false) {
        if existed {
            dir.rename(&backup, dir, name)
                .map_err(|_| format!("{error}; stage metadata rollback failed"))?;
        }
        return Err(error);
    }
    if existed {
        dir.remove_file(&backup)
            .map_err(|e| format!("stage metadata backup cleanup failed: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[derive(Default)]
    struct Fixtures {
        versions: BTreeMap<String, Vec<ModrinthVersion>>,
        candidates: BTreeMap<String, Arc<Candidate>>,
        aliases: BTreeMap<String, Vec<String>>,
        queried: Vec<String>,
        truncated: bool,
        failure: Option<String>,
    }

    fn jar(json: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file::<_, ()>("fabric.mod.json", zip::write::FileOptions::default())
            .unwrap();
        writer.write_all(json.as_bytes()).unwrap();
        writer.finish().unwrap().into_inner()
    }

    impl Fixtures {
        fn add_bundle(&mut self, project: &str, id: &str, library: &str, predicate: &str) {
            let metadata = serde_json::json!({"id": project, "version": "1", "depends": {"fabric-api-base": predicate}});
            self.add(project, id, &metadata.to_string(), &[]);
            let mut metadata = metadata;
            metadata["jars"] = serde_json::json!([{"file":"nested/library.jar"}]);
            let library = jar(&format!(
                r#"{{"id":"fabric-api-base","version":"{library}"}}"#
            ));
            let bytes = crate::core::test_support::zip(
                &[
                    ("fabric.mod.json", metadata.to_string().as_bytes()),
                    ("nested/library.jar", &library),
                ],
                zip::CompressionMethod::Stored,
            );
            let candidate = Arc::get_mut(self.candidates.get_mut(id).unwrap()).unwrap();
            candidate.install.size_bytes = bytes.len() as u64;
            candidate.nodes.clear();
            collect_local_mods(
                &candidate.install.filename,
                &bytes,
                "fabric",
                0,
                &mut candidate.nodes,
            )
            .unwrap();
            candidate.install.bytes = Some(bytes);
        }

        fn add(&mut self, project: &str, id: &str, json: &str, deps: &[(&str, Option<&str>)]) {
            let filename = format!("{id}.jar");
            let bytes = jar(json);
            let version = ModrinthVersion {
                id: Some(id.into()),
                project_id: Some(project.into()),
                version_number: id.into(),
                version_type: Some("release".into()),
                game_versions: vec!["1.21.6".into()],
                loaders: vec!["fabric".into()],
                files: vec![ModrinthFile {
                    url: String::new(),
                    filename: filename.clone(),
                    primary: true,
                    hashes: HashMap::new(),
                    size: Some(bytes.len() as u64),
                }],
                dependencies: vec![],
            };
            let mut nodes = Vec::new();
            collect_local_mods(&filename, &bytes, "fabric", 0, &mut nodes).unwrap();
            let candidate = Arc::new(Candidate {
                install: PendingModInstall {
                    project_id: project.into(),
                    filename,
                    size_bytes: bytes.len() as u64,
                    bytes: Some(bytes),
                    version_id: Some(id.into()),
                    version_number: Some(id.into()),
                    hashes: HashMap::new(),
                    advertised_size: None,
                    game_versions: version.game_versions.clone(),
                    loaders: version.loaders.clone(),
                },
                nodes,
                dependencies: deps
                    .iter()
                    .map(|(p, v)| ProjectDependency {
                        project: (*p).into(),
                        exact: v.map(str::to_string),
                        incompatible: false,
                    })
                    .collect(),
            });
            self.versions
                .entry(project.into())
                .or_default()
                .push(version);
            self.candidates.insert(id.into(), candidate);
        }
    }

    impl Source for Fixtures {
        fn truncated_catalog(&self) -> bool {
            self.truncated
        }
        async fn versions(
            &mut self,
            project: &str,
            exact: Option<&str>,
        ) -> Result<Vec<ModrinthVersion>, String> {
            self.queried.push(project.into());
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            let versions = self
                .versions
                .get(project)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|v| exact.is_none_or(|id| v.id.as_deref() == Some(id)))
                .collect();
            let (versions, truncated) = compatible_candidate_window(versions, "1.21.6", "fabric");
            self.truncated |= truncated;
            Ok(versions)
        }
        async fn prepare(
            &mut self,
            _project: &str,
            version: &ModrinthVersion,
        ) -> Result<Option<Arc<Candidate>>, String> {
            Ok(self.candidates.get(version.id.as_deref().unwrap()).cloned())
        }
        async fn projects_for_mod(&mut self, id: &str) -> Result<Vec<String>, String> {
            Ok(self.aliases.get(id).cloned().unwrap_or_default())
        }
    }

    fn roots(projects: &[&str]) -> Snapshot {
        Snapshot {
            roots: projects.iter().map(|p| (*p).into()).collect(),
            ..Snapshot::default()
        }
    }

    async fn solve(
        fixture: &mut Fixtures,
        snapshot: &Snapshot,
    ) -> Option<(Selection, Vec<DependencyEdge>)> {
        Search {
            source: fixture,
            snapshot,
            mc: "1.21.6",
            states: 0,
            last_conflict: String::new(),
            uncertain: None,
            catalogs: BTreeMap::new(),
            nogoods: BTreeSet::new(),
        }
        .solve(BTreeMap::new())
        .await
        .unwrap()
    }

    fn request(project: &str, enabled: bool) -> Requirement {
        Requirement::project(project, enabled, None, None, "user")
    }

    fn local_request(name: &str, bytes: &[u8]) -> Requirement {
        Requirement {
            project_id: None,
            object_id: Some(object_id(bytes)),
            filename: Some(name.into()),
            enabled: true,
            display_name: None,
            icon_url: None,
            origin: "imported".into(),
        }
    }

    fn edge(requester: &str, dependency: &str) -> DependencyEdge {
        DependencyEdge {
            requester: requester.into(),
            dependency: dependency.into(),
            predicate: None,
            exact_version: None,
        }
    }

    fn stage(files: &[(&str, &str, Option<&str>)]) -> tempfile::TempDir {
        let game = tempfile::tempdir().unwrap();
        std::fs::create_dir(game.path().join("mods")).unwrap();
        let mut meta = serde_json::Map::new();
        for (name, json, project) in files {
            std::fs::write(game.path().join("mods").join(name), jar(json)).unwrap();
            if let Some(project) = project {
                meta.insert(
                    (*name).into(),
                    serde_json::json!({"project_id": project, "display_name": project}),
                );
            }
        }
        std::fs::write(
            game.path().join("mods/.mod_meta.json"),
            serde_json::to_vec(&meta).unwrap(),
        )
        .unwrap();
        game
    }

    fn contents(game: &Path) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(game.join("mods"))
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn runtime_conflicts_do_not_multiply_unrelated_root_versions() -> Result<(), String> {
        for condition in [
            serde_json::json!({"depends":{"minecraft":"<1.21"}}),
            serde_json::json!({"depends":{"java":">=26"}}),
            serde_json::json!({"depends":{"fabricloader":">=0.20"}}),
            serde_json::json!({"breaks":{"java":"<=25"}}),
        ] {
            let mut fixture = Fixtures::default();
            let mut projects = Vec::new();
            for index in 0..15 {
                let project = format!("a{index:02}");
                for version in [2, 1] {
                    fixture.add(
                        &project,
                        &format!("{project}v{version}"),
                        &format!(r#"{{"id":"{project}","version":"{version}"}}"#),
                        &[],
                    );
                }
                projects.push(project);
            }
            for version in [3, 2, 1] {
                let mut metadata = condition.clone();
                metadata["id"] = serde_json::json!("z");
                metadata["version"] = serde_json::json!(version.to_string());
                fixture.add("z", &format!("z{version}"), &metadata.to_string(), &[]);
            }
            projects.push("z".into());
            let mut snapshot = roots(&projects.iter().map(String::as_str).collect::<Vec<_>>());
            snapshot.runtime = Some(RuntimeVersions {
                loader: "fabric".into(),
                loader_version: Some("0.19.5".into()),
                java_major: Some(25),
                bundled: Vec::new(),
            });
            let mut search = Search {
                source: &mut fixture,
                snapshot: &snapshot,
                mc: "1.21.6",
                states: 0,
                last_conflict: String::new(),
                uncertain: None,
                catalogs: BTreeMap::new(),
                nogoods: BTreeSet::new(),
            };
            assert!(search.solve(BTreeMap::new()).await?.is_none());
            assert!(
                search.states < 100,
                "fixed runtime conflict explored {} states",
                search.states
            );
            eprintln!(
                "fixed constraint {condition}: states={}, learned_conflicts={}",
                search.states,
                search.nogoods.len()
            );
            assert!(!search.nogoods.is_empty());
        }
        Ok(())
    }

    #[tokio::test]
    async fn provider_conflicts_are_learned_without_discarding_requested_roots()
    -> Result<(), String> {
        let mut fixture = Fixtures::default();
        fixture.add("a", "a2", r#"{"id":"a","version":"2"}"#, &[]);
        fixture.add("a", "a1", r#"{"id":"a","version":"1"}"#, &[]);
        let mut projects = vec!["a".to_string()];
        for index in 0..15 {
            let project = format!("b{index:02}");
            for version in [2, 1] {
                fixture.add(
                    &project,
                    &format!("{project}v{version}"),
                    &format!(r#"{{"id":"{project}","version":"{version}"}}"#),
                    &[],
                );
            }
            projects.push(project);
        }
        for version in [3, 2, 1] {
            fixture.add(
                "z",
                &format!("z{version}"),
                &format!(r#"{{"id":"z","version":"{version}"}}"#),
                &[("a", Some("a1"))],
            );
        }
        projects.push("z".into());
        let snapshot = roots(&projects.iter().map(String::as_str).collect::<Vec<_>>());
        let mut search = Search {
            source: &mut fixture,
            snapshot: &snapshot,
            mc: "1.21.6",
            states: 0,
            last_conflict: String::new(),
            uncertain: None,
            catalogs: BTreeMap::new(),
            nogoods: BTreeSet::new(),
        };
        let (selected, _) = search
            .solve(BTreeMap::new())
            .await?
            .ok_or("valid provider graph rejected")?;
        assert_eq!(selected.len(), 17);
        assert_eq!(selected["a"].install.version_id.as_deref(), Some("a1"));
        assert!(
            search.states < 100,
            "provider conflict explored {} states",
            search.states
        );
        Ok(())
    }

    #[tokio::test]
    async fn unknown_fixed_provider_version_is_not_a_proven_conflict() -> Result<(), String> {
        let local_json = r#"{"id":"b","version":"1"}"#;
        for incompatible in [false, true] {
            let game = stage(&[("b.jar", local_json, Some("b"))]);
            let before = contents(game.path());
            let requirements = [request("a", true), local_request("b.jar", &jar(local_json))];
            let mut fixture = Fixtures::default();
            fixture.add(
                "a",
                "a1",
                r#"{"id":"a","version":"1"}"#,
                &[("b", Some("b1"))],
            );
            fixture
                .candidates
                .get_mut("a1")
                .and_then(Arc::get_mut)
                .ok_or("fixture candidate is not unique")?
                .dependencies
                .first_mut()
                .ok_or("fixture dependency missing")?
                .incompatible = incompatible;
            let mut state = snapshot(game.path(), &requirements, &[], "1.21.6", "fabric")?;
            assert_eq!(state.pinned.get("b"), Some(&None));
            assert_eq!(state.fixed[0].owner, object_id(&jar(local_json)));
            let candidate = fixture
                .candidates
                .get("a1")
                .cloned()
                .ok_or("fixture missing")?;
            let selected = BTreeMap::from([("a".into(), candidate)]);
            assert!(mandatory_conflict(&state, &selected, "1.21.6").is_none());
            let error = reconcile_with_source(
                game.path(),
                &requirements,
                &[],
                "1.21.6",
                "fabric",
                &mut fixture,
            )
            .await
            .err()
            .ok_or("unknown exact provenance was accepted")?;
            assert!(error.starts_with("mod_resolution_unsupported:"), "{error}");
            assert_eq!(contents(game.path()), before);
            for known in ["b1", "b2"] {
                state.pinned.insert("b".into(), Some(known.into()));
                let conflict = mandatory_conflict(&state, &selected, "1.21.6");
                assert_eq!(conflict.is_some(), (known == "b1") == incompatible);
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn learned_mandatory_conflicts_prune_unrelated_version_combinations() {
        let mut f = Fixtures::default();
        f.add("a", "a2", r#"{"id":"sodium","version":"2"}"#, &[]);
        f.add("a", "a1", r#"{"id":"sodium","version":"1"}"#, &[]);
        f.add(
            "z",
            "z2",
            r#"{"id":"addon","version":"2","depends":{"sodium":"<2"}}"#,
            &[],
        );
        f.add(
            "z",
            "z1",
            r#"{"id":"addon","version":"1","depends":{"sodium":"<2"}}"#,
            &[],
        );
        let mut projects = vec!["a".to_string(), "z".to_string()];
        for index in 0..15 {
            let project = format!("b{index:02}");
            for version in [2, 1] {
                f.add(
                    &project,
                    &format!("{project}v{version}"),
                    &format!(r#"{{"id":"{project}","version":"{version}"}}"#),
                    &[],
                );
            }
            projects.push(project);
        }
        let snapshot = roots(&projects.iter().map(String::as_str).collect::<Vec<_>>());
        let mut search = Search {
            source: &mut f,
            snapshot: &snapshot,
            mc: "1.21.6",
            states: 0,
            last_conflict: String::new(),
            uncertain: None,
            catalogs: BTreeMap::new(),
            nogoods: BTreeSet::new(),
        };
        let (selected, _) = search.solve(BTreeMap::new()).await.unwrap().unwrap();
        assert_eq!(selected.len(), 17);
        assert_eq!(selected["a"].install.version_id.as_deref(), Some("a1"));
        assert!(
            search.states < 100,
            "unrelated roots must not produce exponential exploration: {}",
            search.states
        );
        assert!(!search.nogoods.is_empty());
    }

    #[tokio::test]
    async fn bundled_api_versions_resolve_and_pass_the_final_validator() {
        let game = stage(&[]);
        let mut f = Fixtures::default();
        f.add_bundle("a", "a1", "1", ">=1");
        f.add_bundle("b", "b1", "2", ">=2");
        let requirements = [request("a", true), request("b", true)];
        reconcile_with_source(game.path(), &requirements, &[], "1.21.6", "fabric", &mut f)
            .await
            .unwrap();
        validate_mod_set_with_known_runtime(
            game.path(),
            "1.21.6",
            "fabric",
            Some("0.19.5"),
            Some(21),
            Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(list_mods(game.path()).await.len(), 2);
        assert_eq!(f.queried, ["a", "b"]);
    }

    #[tokio::test]
    async fn loader_owned_library_is_resolved_without_becoming_a_requested_project() {
        let game = stage(&[]);
        let zip = crate::core::test_support::zip;
        let child = jar(r#"{"id":"mixinextras","version":"5"}"#);
        let loader = zip(
            &[
                (
                    "fabric.mod.json",
                    br#"{"id":"fabricloader","version":"1","jars":[{"file":"lib.jar"}]}"#,
                ),
                ("lib.jar", &child),
            ],
            zip::CompressionMethod::Stored,
        );
        let bundled = loader_bundle::inspect("1", &loader).unwrap();
        let mut f = Fixtures::default();
        f.add(
            "a",
            "a1",
            r#"{"id":"a","version":"1","depends":{"mixinextras":">=5"}}"#,
            &[],
        );
        let edges = reconcile_with_source_and_runtime(
            game.path(),
            &[request("a", true)],
            &[],
            "1.21.6",
            "fabric",
            &mut f,
            Some(RuntimeVersions {
                loader: "fabric".into(),
                loader_version: Some("1".into()),
                java_major: Some(21),
                bundled: bundled.clone(),
            }),
        )
        .await
        .unwrap();
        assert!(
            edges.is_empty(),
            "Loader ownership must not become a user project edge"
        );
        assert_eq!(f.queried, ["a"]);
        assert_eq!(list_mods(game.path()).await.len(), 1);
        validate_mod_set_with_known_runtime(
            game.path(),
            "1.21.6",
            "fabric",
            Some("1"),
            Some(21),
            bundled,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn alternative_candidate_yields_one_solution_for_all_required_roots() {
        let mut f = Fixtures::default();
        f.add(
            "a",
            "a2",
            r#"{"id":"a","version":"2"}"#,
            &[("lib", Some("lib2"))],
        );
        f.add(
            "a",
            "a1",
            r#"{"id":"a","version":"1"}"#,
            &[("lib", Some("lib1"))],
        );
        f.add(
            "b",
            "b1",
            r#"{"id":"b","version":"1"}"#,
            &[("lib", Some("lib1"))],
        );
        f.add("lib", "lib2", r#"{"id":"lib","version":"2"}"#, &[]);
        f.add("lib", "lib1", r#"{"id":"lib","version":"1"}"#, &[]);
        let (selected, edges) = solve(&mut f, &roots(&["a", "b"])).await.unwrap();
        assert_eq!(selected["a"].install.version_id.as_deref(), Some("a1"));
        assert_eq!(selected["b"].install.version_id.as_deref(), Some("b1"));
        assert_eq!(selected["lib"].install.version_id.as_deref(), Some("lib1"));
        assert_eq!(edges.len(), 2);
        assert!(
            edges
                .iter()
                .all(|e| e.exact_version.as_deref() == Some("lib1"))
        );
    }

    #[tokio::test]
    async fn unsatisfiable_exact_versions_retain_all_roots_and_do_not_mutate_stage() {
        let game = stage(&[
            ("a.jar", r#"{"id":"a","version":"0"}"#, Some("a")),
            ("b.jar", r#"{"id":"b","version":"0"}"#, Some("b")),
        ]);
        let before = contents(game.path());
        let requirements = vec![request("a", true), request("b", true)];
        let mut f = Fixtures::default();
        f.add(
            "a",
            "a1",
            r#"{"id":"a","version":"1"}"#,
            &[("lib", Some("lib1"))],
        );
        f.add(
            "b",
            "b1",
            r#"{"id":"b","version":"1"}"#,
            &[("lib", Some("lib2"))],
        );
        f.add("lib", "lib1", r#"{"id":"lib","version":"1"}"#, &[]);
        f.add("lib", "lib2", r#"{"id":"lib","version":"2"}"#, &[]);
        let error =
            reconcile_with_source(game.path(), &requirements, &[], "1.21.6", "fabric", &mut f)
                .await
                .unwrap_err();
        assert!(error.contains("exact version conflict"), "{error}");
        assert_eq!(contents(game.path()), before);
        assert_eq!(requirements.len(), 2);
        assert!(requirements.iter().all(|r| r.enabled));
    }

    #[tokio::test]
    async fn disabled_dependency_is_rejected_without_enabling_or_downloading_it() {
        let game = stage(&[("b.jar.disabled", r#"{"id":"b","version":"1"}"#, Some("b"))]);
        let before = contents(game.path());
        let mut f = Fixtures::default();
        f.add("a", "a1", r#"{"id":"a","version":"1"}"#, &[("b", None)]);
        let error = reconcile_with_source(
            game.path(),
            &[request("a", true), request("b", false)],
            &[],
            "1.21.6",
            "fabric",
            &mut f,
        )
        .await
        .unwrap_err();
        assert!(error.contains("disabled"), "{error}");
        assert_eq!(f.queried, vec!["a"]);
        assert_eq!(contents(game.path()), before);
    }

    #[tokio::test]
    async fn jar_predicates_breaks_and_later_root_providers_drive_backtracking() {
        let mut f = Fixtures::default();
        f.add(
            "a",
            "a2",
            r#"{"id":"a","version":"2","depends":{"alias":["1.x",">=2"]},"breaks":{"b":">=2"}}"#,
            &[],
        );
        f.add(
            "a",
            "a1",
            r#"{"id":"a","version":"1","depends":{"alias":["1.x",">=2"]}}"#,
            &[],
        );
        f.add(
            "b",
            "b2",
            r#"{"id":"b","version":"2","provides":["alias"]}"#,
            &[],
        );
        let (selected, edges) = solve(&mut f, &roots(&["a", "b"])).await.unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected["a"].install.version_id.as_deref(), Some("a1"));
        assert!(edges.iter().any(|e| e.requester == "a"
            && e.dependency == "b"
            && e.predicate.as_deref() == Some(r#"["1.x",">=2"]"#)));
        assert!(!f.queried.iter().any(|p| p == "alias"));
    }

    #[tokio::test]
    async fn fixed_local_root_constrains_candidates_and_uses_object_id_in_edges() {
        let game = stage(&[(
            "local.jar",
            r#"{"id":"local","version":"1","depends":{"a":"1.x"}}"#,
            Some("backfilled"),
        )]);
        let bytes = std::fs::read(game.path().join("mods/local.jar")).unwrap();
        let before = contents(game.path());
        let snapshot = snapshot(
            game.path(),
            &[local_request("local.jar", &bytes), request("a", true)],
            &[],
            "1.21.6",
            "fabric",
        )
        .unwrap();
        let mut f = Fixtures::default();
        f.add("a", "a2", r#"{"id":"a","version":"2"}"#, &[]);
        f.add("a", "a1", r#"{"id":"a","version":"1.2"}"#, &[]);
        let (selected, edges) = solve(&mut f, &snapshot).await.unwrap();
        assert_eq!(selected["a"].install.version_id.as_deref(), Some("a1"));
        assert_eq!(
            edges,
            vec![DependencyEdge {
                requester: object_id(&bytes),
                dependency: "a".into(),
                predicate: Some("1.x".into()),
                exact_version: None
            }]
        );
        assert_eq!(contents(game.path()), before);
    }

    #[tokio::test]
    async fn alternative_project_hints_must_actually_provide_the_required_id() {
        let game = stage(&[(
            "local.jar",
            r#"{"id":"local","version":"1","depends":{"alias":"*"}}"#,
            None,
        )]);
        let snapshot = snapshot(game.path(), &[], &[], "1.21.6", "fabric").unwrap();
        let mut f = Fixtures::default();
        f.aliases
            .insert("alias".into(), vec!["wrong".into(), "correct".into()]);
        f.add("wrong", "wrong1", r#"{"id":"wrong","version":"1"}"#, &[]);
        f.add(
            "correct",
            "correct1",
            r#"{"id":"provider","version":"1","provides":["alias"]}"#,
            &[],
        );
        let (selected, edges) = solve(&mut f, &snapshot).await.unwrap();
        assert_eq!(
            selected.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["correct"]
        );
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].dependency, "correct");
    }

    #[tokio::test]
    async fn absent_disabled_project_is_not_fetched_as_a_dependency() {
        let game = stage(&[]);
        let before = contents(game.path());
        let mut f = Fixtures::default();
        f.add("a", "a1", r#"{"id":"a","version":"1"}"#, &[("b", None)]);
        let error = reconcile_with_source(
            game.path(),
            &[request("a", true), request("b", false)],
            &[],
            "1.21.6",
            "fabric",
            &mut f,
        )
        .await
        .unwrap_err();
        assert!(error.contains("disabled"));
        assert_eq!(f.queried, vec!["a"]);
        assert_eq!(contents(game.path()), before);
    }

    #[tokio::test]
    async fn java_and_exact_loader_versions_participate_in_cross_root_backtracking() {
        let mut f = Fixtures::default();
        f.add(
            "a",
            "a2",
            r#"{"id":"a","version":"2","depends":{"java":">=26","fabricloader":">=0.19"}}"#,
            &[],
        );
        f.add(
            "a",
            "a1",
            r#"{"id":"a","version":"1","depends":{"java":">=25","fabricloader":">=0.18"}}"#,
            &[],
        );
        f.add(
            "b",
            "b1",
            r#"{"id":"b","version":"1","depends":{"a":"*","java":">=25"}}"#,
            &[],
        );
        for (java, loader) in [(25, "0.19"), (26, "0.18")] {
            let mut snapshot = roots(&["a", "b"]);
            snapshot.runtime = Some(RuntimeVersions {
                loader: "fabric".into(),
                loader_version: Some(loader.into()),
                java_major: Some(java),
                bundled: Vec::new(),
            });
            let (selected, edges) = solve(&mut f, &snapshot).await.unwrap();
            assert_eq!(selected["a"].install.version_id.as_deref(), Some("a1"));
            assert!(selected.contains_key("b"));
            assert!(
                edges
                    .iter()
                    .any(|e| e.requester == "b" && e.dependency == "a")
            );
        }
    }

    #[tokio::test]
    async fn runtime_breaks_and_missing_runtime_are_not_ignored() {
        let mut f = Fixtures::default();
        f.add(
            "a",
            "a2",
            r#"{"id":"a","version":"2","breaks":{"java":"<=25"}}"#,
            &[],
        );
        f.add(
            "a",
            "a1",
            r#"{"id":"a","version":"1","depends":{"java":">=25"}}"#,
            &[],
        );
        let mut snapshot = roots(&["a"]);
        snapshot.runtime = Some(RuntimeVersions {
            loader: "fabric".into(),
            loader_version: Some("0.18".into()),
            java_major: Some(25),
            bundled: Vec::new(),
        });
        assert_eq!(
            solve(&mut f, &snapshot).await.unwrap().0["a"]
                .install
                .version_id
                .as_deref(),
            Some("a1")
        );
        snapshot.runtime.as_mut().unwrap().java_major = None;
        assert!(solve(&mut f, &snapshot).await.is_none());
    }

    #[tokio::test]
    async fn historic_catalog_is_filtered_then_windowed_and_exhaustion_is_not_unsatisfiable() {
        let game = stage(&[]);
        let before = contents(game.path());
        let mut f = Fixtures::default();
        for n in 0..65 {
            f.add(
                "a",
                &format!("a{n}"),
                r#"{"id":"a","version":"1","depends":{"unmapped":"*"}}"#,
                &[],
            );
        }
        // A 65th compatible version is outside the supported newest-first window.
        let error = reconcile_with_source(
            game.path(),
            &[request("a", true)],
            &[],
            "1.21.6",
            "fabric",
            &mut f,
        )
        .await
        .unwrap_err();
        assert!(error.starts_with("mod_resolution_limit:"), "{error}");
        assert_eq!(contents(game.path()), before);
        let mut versions = f.versions["a"].clone();
        let mut wrong = versions[0].clone();
        wrong.game_versions = vec!["1.20.1".into()];
        versions.insert(0, wrong);
        let (window, truncated) = compatible_candidate_window(versions, "1.21.6", "fabric");
        assert!(truncated);
        assert_eq!(window.len(), 64);
        assert_eq!(window[0].id.as_deref(), Some("a0"));
        assert_eq!(window[63].id.as_deref(), Some("a63"));
    }

    #[tokio::test]
    async fn provider_failures_and_unknown_predicates_are_not_proven_unsatisfiable() {
        let game = stage(&[]);
        let before = contents(game.path());
        let mut f = Fixtures {
            failure: Some("provider network failed".into()),
            ..Fixtures::default()
        };
        let error = reconcile_with_source(
            game.path(),
            &[request("a", true)],
            &[],
            "1.21.6",
            "fabric",
            &mut f,
        )
        .await
        .unwrap_err();
        assert_eq!(error, "provider network failed");
        f.failure = None;
        f.add(
            "a",
            "a1",
            r#"{"id":"a","version":"1","depends":{"java":"unknown syntax"}}"#,
            &[],
        );
        let error = reconcile_with_source_and_runtime(
            game.path(),
            &[request("a", true)],
            &[],
            "1.21.6",
            "fabric",
            &mut f,
            Some(RuntimeVersions {
                loader: "fabric".into(),
                loader_version: Some("0.18".into()),
                java_major: Some(25),
                bundled: Vec::new(),
            }),
        )
        .await
        .unwrap_err();
        assert!(
            !error.starts_with("mod_resolution_unsatisfiable:"),
            "{error}"
        );
        assert!(!error.starts_with("mod_resolution_limit:"), "{error}");
        assert_eq!(contents(game.path()), before);
    }

    #[tokio::test]
    async fn non_fabric_loaders_keep_integrity_without_claiming_native_graph_support() {
        let game = stage(&[(
            "local.jar",
            r#"{"id":"local","version":"1","depends":{"missing":"*","java":">=999"}}"#,
            None,
        )]);
        let bytes = std::fs::read(game.path().join("mods/local.jar")).unwrap();
        let before = contents(game.path());
        for loader in ["forge", "neoforge", "quilt"] {
            assert!(!supports_dependency_validation(loader));
            let edges = reconcile_requested_mods(
                game.path(),
                &[local_request("local.jar", &bytes)],
                &[],
                "1.21.6",
                loader,
                Some("1"),
                Some(25),
            )
            .await
            .unwrap();
            assert!(edges.is_empty());
            assert_eq!(contents(game.path()), before);
        }
    }

    #[test]
    fn legacy_slug_normalization_requires_a_verified_project_identity() {
        let project = serde_json::json!({"id":"AANobbMI", "slug":"sodium"});
        assert_eq!(
            canonical_project_identity("sodium", &project).unwrap(),
            "AANobbMI"
        );
        assert_eq!(
            canonical_project_identity("AANobbMI", &project).unwrap(),
            "AANobbMI"
        );
        assert!(canonical_project_identity("unrelated", &project).is_err());
        assert!(
            canonical_project_identity(
                "sodium",
                &serde_json::json!({"id":"../path","slug":"sodium"})
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn removes_only_former_transitives_not_roots_disabled_or_unknown_legacy() {
        let root_json = r#"{"id":"root","version":"1"}"#;
        let root_bytes = jar(root_json);
        let game = stage(&[
            ("root.jar", root_json, Some("root")),
            ("dep.jar", r#"{"id":"dep","version":"1"}"#, Some("dep")),
            (
                "disabled.jar.disabled",
                r#"{"id":"disabled","version":"1"}"#,
                Some("disabled"),
            ),
            ("legacy.jar", r#"{"id":"legacy","version":"1"}"#, None),
        ]);
        let requirements = vec![
            local_request("root.jar", &root_bytes),
            request("disabled", false),
        ];
        let previous = vec![
            edge("old", "dep"),
            edge("old", "root"),
            edge("old", "disabled"),
        ];
        let mut f = Fixtures::default();
        let edges = reconcile_with_source(
            game.path(),
            &requirements,
            &previous,
            "1.21.6",
            "fabric",
            &mut f,
        )
        .await
        .unwrap();
        let after = contents(game.path());
        assert!(edges.is_empty());
        assert_eq!(after["root.jar"], root_bytes);
        assert!(after.contains_key("legacy.jar"));
        assert!(after.contains_key("disabled.jar.disabled"));
        assert!(!after.contains_key("dep.jar"));
        assert!(f.queried.is_empty());
        assert!(
            !load_meta_checked_in(&managed_mods(game.path()).unwrap())
                .unwrap()
                .contains_key("dep.jar")
        );
    }

    #[tokio::test]
    async fn exhausted_search_fails_closed() {
        let mut f = Fixtures::default();
        let snapshot = roots(&["a"]);
        let error = Search {
            source: &mut f,
            snapshot: &snapshot,
            mc: "1.21.6",
            states: MAX_STATES,
            last_conflict: String::new(),
            uncertain: None,
            catalogs: BTreeMap::new(),
            nogoods: BTreeSet::new(),
        }
        .solve(BTreeMap::new())
        .await
        .err()
        .unwrap();
        assert!(error.starts_with("mod_resolution_limit:"));
        assert!(f.queried.is_empty());
    }

    #[tokio::test]
    async fn unsupported_loader_and_noncanonical_previous_edges_fail_before_mutation() {
        let game = stage(&[]);
        let before = contents(game.path());
        assert!(
            reconcile_requested_mods(game.path(), &[], &[], "1.21.6", "unknown", None, None)
                .await
                .unwrap_err()
                .starts_with("unsupported Mod reconciliation loader:")
        );
        let mut f = Fixtures::default();
        assert!(
            reconcile_with_source(
                game.path(),
                &[],
                &[edge("a", "../dep")],
                "1.21.6",
                "fabric",
                &mut f
            )
            .await
            .is_err()
        );
        assert_eq!(contents(game.path()), before);
    }
}
#[tokio::test]
#[ignore = "downloads public Modrinth artifacts into an isolated temporary directory; no accounts or game launch"]
async fn live_requested_mod_graph() {
    let game = tempfile::tempdir().unwrap();
    std::fs::create_dir(game.path().join("mods")).unwrap();
    let sodium = canonical_modrinth_project_id("sodium").await.unwrap();
    assert_eq!(sodium, "AANobbMI");
    let entity_culling = canonical_modrinth_project_id("entityculling")
        .await
        .unwrap();
    let requirements = vec![
        Requirement::project(&sodium, true, None, None, "user"),
        Requirement::project(&entity_culling, true, None, None, "user"),
    ];
    let edges = reconcile_requested_mods(
        game.path(),
        &requirements,
        &[],
        "1.21.1",
        "fabric",
        Some("0.19.5"),
        Some(21),
    )
    .await
    .unwrap();
    let installed = list_mods(game.path()).await;
    assert!(
        installed
            .iter()
            .any(|entry| entry.project_id.as_deref() == Some(&sodium))
    );
    assert!(
        installed
            .iter()
            .any(|entry| entry.project_id.as_deref() == Some(&entity_culling))
    );
    assert!(
        !edges.is_empty(),
        "fixture must exercise real required dependencies"
    );
    validate_mod_set_with_runtime(game.path(), "1.21.1", "fabric", Some("0.19.5"), Some(21))
        .await
        .unwrap();
    eprintln!(
        "public provider graph validated: {} roots, {} artifacts, {} dependency edges",
        requirements.len(),
        installed.len(),
        edges.len()
    );
}

#[tokio::test]
#[ignore = "downloads the complete default Mod recipe from public Modrinth into a temporary directory; no account or game launch"]
async fn live_default_requested_mod_graph() -> Result<(), Box<dyn std::error::Error>> {
    let game = crate::core::test_support::physical_tempdir();
    std::fs::create_dir(game.path().join("mods"))?;
    let projects: Vec<String> = match std::env::var("HIKYOU_TEST_REQUESTED_PROJECTS") {
        Ok(json) => serde_json::from_str(&json)?,
        Err(std::env::VarError::NotPresent) => {
            init_auto_mods(&game.path().join("auto-mods.json"), "unknown")
                .await?
                .into_iter()
                .filter(|entry| entry.enabled)
                .map(|entry| entry.project_id)
                .collect()
        }
        Err(error) => return Err(error.into()),
    };
    if projects.len() > MAX_PROJECTS || projects.iter().any(|project| !valid_project(project)) {
        return Err("invalid public-data test project list".into());
    }
    let mut requirements = Vec::new();
    for project in projects {
        let project = canonical_modrinth_project_id(&project).await?;
        requirements.push(Requirement::project(
            &project,
            true,
            None,
            None,
            "recommended",
        ));
    }
    assert!(
        requirements.len() > 10,
        "must exercise the full enabled default recipe"
    );
    let mc = std::env::var("HIKYOU_TEST_MC_VERSION").unwrap_or_else(|_| "1.21.1".into());
    let java = std::env::var("HIKYOU_TEST_JAVA_MAJOR")
        .ok()
        .map(|s| s.parse::<u32>())
        .transpose()?
        .unwrap_or(21);
    let edges = reconcile_requested_mods(
        game.path(),
        &requirements,
        &[],
        &mc,
        "fabric",
        Some("0.19.5"),
        Some(java),
    )
    .await?;
    validate_mod_set_with_runtime(game.path(), &mc, "fabric", Some("0.19.5"), Some(java)).await?;
    let installed = list_mods(game.path()).await;
    for requirement in &requirements {
        assert!(
            installed
                .iter()
                .any(|entry| entry.project_id == requirement.project_id),
            "a requested default Mod disappeared"
        );
    }
    eprintln!(
        "full public default graph validated: {} roots, {} artifacts, {} edges, Minecraft {}",
        requirements.len(),
        installed.len(),
        edges.len(),
        mc
    );
    Ok(())
}
