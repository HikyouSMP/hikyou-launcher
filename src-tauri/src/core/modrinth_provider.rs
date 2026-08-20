use super::mod_metadata::read_installed_mod_manifest;
use crate::core::{cache, mod_files::is_safe_filename};
use serde::{Deserialize, Serialize};

const CACHE_MOD_VERSIONS: &str = "modrinth_mod_versions";
const TTL_MOD_VERSIONS: i64 = 6 * 60 * 60;

pub(super) const FABRIC_API_PROJECT_ID: &str = "P7dR8mSH";

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct ModrinthDep {
    pub(super) project_id: Option<String>,
    pub(super) version_id: Option<String>,
    pub(super) dependency_type: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct ModrinthVersion {
    #[serde(default)]
    pub(super) project_id: Option<String>,
    #[serde(default)]
    pub(super) version_number: String,
    #[serde(default)]
    pub(super) version_type: Option<String>,
    #[serde(default)]
    pub(super) game_versions: Vec<String>,
    #[serde(default)]
    pub(super) loaders: Vec<String>,
    pub(super) files: Vec<ModrinthFile>,
    #[serde(default)]
    pub(super) dependencies: Vec<ModrinthDep>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct ModrinthFile {
    pub(super) url: String,
    pub(super) filename: String,
    pub(super) primary: bool,
}

pub(super) async fn fetch_modrinth_versions_for_project(
    client: &reqwest::Client,
    project_id: &str,
    mc_version: &str,
    loader: &str,
    is_main: bool,
) -> Result<Vec<ModrinthVersion>, String> {
    let alias = format!("{}|{}|{}", project_id, mc_version, loader);
    if let Some(c) = cache::get()
        && let Some(cached) = c
            .get::<Vec<ModrinthVersion>>(CACHE_MOD_VERSIONS, &alias)
            .await
    {
        return Ok(cached);
    }

    let versions_url = format!(
        "https://api.modrinth.com/v2/project/{}/version?game_versions=[\"{}\"]&loaders=[\"{}\"]",
        project_id, mc_version, loader
    );

    let resp = client.get(&versions_url).send().await;
    let versions: Vec<ModrinthVersion> = match resp {
        Err(e) => {
            return Err(if is_main {
                format!("failed to fetch versions: {}", e)
            } else {
                format!(
                    "failed to fetch required dependency versions for {}: {}",
                    project_id, e
                )
            });
        }
        Ok(r) => match r.json::<Vec<ModrinthVersion>>().await {
            Err(e) => {
                return Err(if is_main {
                    format!("failed to parse version data: {}", e)
                } else {
                    format!(
                        "failed to parse required dependency version data for {}: {}",
                        project_id, e
                    )
                });
            }
            Ok(v) => v,
        },
    };

    if let Some(c) = cache::get() {
        let _ = c
            .set(CACHE_MOD_VERSIONS, &alias, &versions, TTL_MOD_VERSIONS)
            .await;
    }

    Ok(versions)
}

pub(super) fn compare_release_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let left_parts = parse_release_parts(left)?;
    let right_parts = parse_release_parts(right)?;
    for i in 0..left_parts.len().max(right_parts.len()) {
        let l = *left_parts.get(i).unwrap_or(&0);
        let r = *right_parts.get(i).unwrap_or(&0);
        match l.cmp(&r) {
            std::cmp::Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }
    Some(std::cmp::Ordering::Equal)
}

fn parse_release_parts(value: &str) -> Option<Vec<u16>> {
    let parts: Vec<u16> = value
        .split('.')
        .take(3)
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.is_empty() { None } else { Some(parts) }
}

pub(super) async fn resolve_dependency_project_id(
    client: &reqwest::Client,
    dep: &ModrinthDep,
) -> Option<String> {
    if let Some(project_id) = &dep.project_id {
        return Some(project_id.clone());
    }

    let version_id = dep.version_id.as_ref()?;
    let url = format!("https://api.modrinth.com/v2/version/{}", version_id);
    client
        .get(url)
        .send()
        .await
        .ok()?
        .json::<ModrinthVersion>()
        .await
        .ok()?
        .project_id
}

pub(super) async fn resolve_modrinth_project_from_mod_id(
    client: &reqwest::Client,
    mod_id: &str,
    mc_version: &str,
    loader: &str,
) -> Option<String> {
    for candidate in modrinth_slug_candidates_for_mod_id(mod_id) {
        if let Some(project_id) = fetch_modrinth_project_id(client, &candidate).await {
            return Some(project_id);
        }
    }

    search_modrinth_project_for_mod_id(client, mod_id, mc_version, loader).await
}

pub(super) fn modrinth_slug_candidates_for_mod_id(mod_id: &str) -> Vec<String> {
    if mod_id == "fabric" || mod_id.starts_with("fabric-") {
        return vec!["fabric-api".to_string(), mod_id.to_string()];
    }

    let mut candidates = vec![mod_id.to_string()];
    let hyphenated = mod_id.replace('_', "-");
    candidates.push(hyphenated.clone());

    if let Some(stripped) = mod_id
        .strip_suffix("_v3")
        .or_else(|| mod_id.strip_suffix("_v2"))
    {
        candidates.push(stripped.to_string());
        candidates.push(stripped.replace('_', "-"));
    }
    if let Some(stripped) = hyphenated
        .strip_suffix("-v3")
        .or_else(|| hyphenated.strip_suffix("-v2"))
    {
        candidates.push(stripped.to_string());
    }
    let base = mod_id
        .strip_suffix("_v3")
        .or_else(|| mod_id.strip_suffix("_v2"))
        .unwrap_or(mod_id);
    let acronym: String = base
        .split(['_', '-'])
        .filter_map(|part| part.chars().next())
        .collect();
    if acronym.len() >= 2 {
        candidates.push(acronym);
    }

    candidates.dedup();
    candidates
}

#[derive(Deserialize)]
struct ModrinthProjectLookup {
    id: String,
}

async fn fetch_modrinth_project_id(client: &reqwest::Client, slug_or_id: &str) -> Option<String> {
    let url = format!("https://api.modrinth.com/v2/project/{}", slug_or_id);
    client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<ModrinthProjectLookup>()
        .await
        .ok()
        .map(|project| project.id)
}

#[derive(Deserialize)]
struct ModrinthProjectSearchResponse {
    hits: Vec<ModrinthProjectSearchHit>,
}

#[derive(Deserialize)]
struct ModrinthProjectSearchHit {
    project_id: String,
    slug: String,
    title: String,
}

async fn search_modrinth_project_for_mod_id(
    client: &reqwest::Client,
    mod_id: &str,
    mc_version: &str,
    loader: &str,
) -> Option<String> {
    for query in modrinth_slug_candidates_for_mod_id(mod_id) {
        if let Some(project_id) =
            search_modrinth_project_for_query(client, &query, mod_id, mc_version, loader).await
        {
            return Some(project_id);
        }
    }
    None
}

async fn search_modrinth_project_for_query(
    client: &reqwest::Client,
    query: &str,
    mod_id: &str,
    mc_version: &str,
    loader: &str,
) -> Option<String> {
    let facets = format!(
        r#"[["project_type:mod"],["categories:{}"],["versions:{}"]]"#,
        loader, mc_version
    );
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&facets={}&limit=10",
        urlencoding::encode(query),
        urlencoding::encode(&facets)
    );
    let response = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<ModrinthProjectSearchResponse>()
        .await
        .ok()?;

    for hit in &response.hits {
        if !modrinth_search_hit_matches_dependency_query(hit, mod_id, query) {
            continue;
        }
        if project_provides_fabric_mod_id(client, &hit.project_id, mod_id, mc_version, loader).await
        {
            return Some(hit.project_id.clone());
        }
    }

    None
}

fn modrinth_search_hit_matches_dependency_query(
    hit: &ModrinthProjectSearchHit,
    mod_id: &str,
    query: &str,
) -> bool {
    hit.slug == mod_id
        || hit.slug.replace('-', "_") == mod_id
        || hit.slug == query
        || hit.slug.replace('-', "_") == query.replace('-', "_")
        || hit.title.eq_ignore_ascii_case(mod_id)
        || hit.title.eq_ignore_ascii_case(query)
}

async fn project_provides_fabric_mod_id(
    client: &reqwest::Client,
    project_id: &str,
    mod_id: &str,
    mc_version: &str,
    loader: &str,
) -> bool {
    if loader != "fabric" && loader != "quilt" {
        return true;
    }
    let versions_url = format!(
        "https://api.modrinth.com/v2/project/{}/version?game_versions=[\"{}\"]&loaders=[\"{}\"]",
        project_id, mc_version, loader
    );
    let Ok(response) = client.get(&versions_url).send().await else {
        return false;
    };
    let Ok(versions) = response.json::<Vec<ModrinthVersion>>().await else {
        return false;
    };

    for version in select_compatible_versions(versions, mc_version, loader, true, &[]) {
        let Some(file) = select_compatible_file(&version, mc_version) else {
            continue;
        };
        let Ok(response) = client.get(&file.url).send().await else {
            continue;
        };
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if let Ok(Some(manifest)) = read_installed_mod_manifest(&file.filename, &bytes) {
            return manifest.id == mod_id;
        }
    }
    false
}

#[cfg(test)]
pub(super) fn select_compatible_version(
    versions: Vec<ModrinthVersion>,
    mc_version: &str,
    loader: &str,
) -> Option<ModrinthVersion> {
    select_compatible_versions(versions, mc_version, loader, true, &[])
        .into_iter()
        .next()
}

pub(super) fn select_compatible_versions(
    versions: Vec<ModrinthVersion>,
    mc_version: &str,
    loader: &str,
    prefer_release: bool,
    predicates: &[String],
) -> Vec<ModrinthVersion> {
    let mut compatible: Vec<ModrinthVersion> = versions
        .into_iter()
        .filter(|version| is_exact_modrinth_match(version, mc_version, loader))
        .filter(|version| version_satisfies_all_predicates(version, predicates))
        .collect();

    if prefer_release {
        compatible.sort_by_key(|version| {
            (
                version_target_rank(version, mc_version),
                version_type_rank(version.version_type.as_deref()),
            )
        });
    } else {
        compatible.sort_by_key(|version| version_target_rank(version, mc_version));
    }

    compatible
}

fn version_satisfies_all_predicates(version: &ModrinthVersion, predicates: &[String]) -> bool {
    predicates
        .iter()
        .all(|predicate| version_satisfies_predicate(version, predicate))
}

pub(super) fn artifact_satisfies_all_predicates(value: &str, predicates: &[String]) -> bool {
    predicates
        .iter()
        .all(|predicate| artifact_satisfies_predicate(value, predicate))
}

fn version_satisfies_predicate(version: &ModrinthVersion, predicate: &str) -> bool {
    candidates_satisfy_predicate(&modrinth_version_candidates(version), predicate)
}

pub(super) fn artifact_satisfies_predicate(value: &str, predicate: &str) -> bool {
    candidates_satisfy_predicate(&extract_version_like_tokens(value), predicate)
}

fn candidates_satisfy_predicate(candidates: &[String], predicate: &str) -> bool {
    let predicate = predicate.trim();
    if predicate.is_empty() || predicate == "*" {
        return true;
    }

    if predicate.ends_with(".x") {
        let prefix = predicate.trim_end_matches('x');
        return candidates
            .iter()
            .any(|candidate| candidate.starts_with(prefix));
    }

    for (operator, expected) in [
        (">=", predicate.strip_prefix(">=")),
        ("<=", predicate.strip_prefix("<=")),
        (">", predicate.strip_prefix(">")),
        ("<", predicate.strip_prefix("<")),
        ("=", predicate.strip_prefix("=")),
    ] {
        let Some(expected) = expected else {
            continue;
        };
        let expected = expected.trim();
        return candidates.iter().any(|candidate| {
            let Some(ordering) = compare_version_like(candidate, expected) else {
                return false;
            };
            match operator {
                ">=" => ordering >= 0,
                "<=" => ordering <= 0,
                ">" => ordering > 0,
                "<" => ordering < 0,
                "=" => ordering == 0,
                _ => false,
            }
        });
    }

    if predicate
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        return candidates
            .iter()
            .any(|candidate| compare_version_like(candidate, predicate) == Some(0));
    }

    true
}

fn modrinth_version_candidates(version: &ModrinthVersion) -> Vec<String> {
    std::iter::once(version.version_number.as_str())
        .chain(version.files.iter().map(|file| file.filename.as_str()))
        .flat_map(extract_version_like_tokens)
        .collect()
}

fn extract_version_like_tokens(value: &str) -> Vec<String> {
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            if i >= 2 && &lower[i - 2..i] == "mc" {
                i += 1;
                continue;
            }
            let start = i;
            let mut end = i + 1;
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b'_') {
                    end += 1;
                } else {
                    break;
                }
            }
            let token = lower[start..end]
                .trim_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_string();
            if token.contains('.') {
                tokens.push(token);
            }
            i = end;
        } else {
            i += 1;
        }
    }
    tokens
}

fn compare_version_like(left: &str, right: &str) -> Option<i8> {
    let left = ParsedVersionLike::parse(left)?;
    let right = ParsedVersionLike::parse(right)?;
    for i in 0..left.release.len().max(right.release.len()) {
        let l = *left.release.get(i).unwrap_or(&0);
        let r = *right.release.get(i).unwrap_or(&0);
        if l > r {
            return Some(1);
        }
        if l < r {
            return Some(-1);
        }
    }
    match (&left.pre, &right.pre) {
        (None, None) => Some(0),
        (None, Some(_)) => Some(1),
        (Some(_), None) => Some(-1),
        (Some(left_pre), Some(right_pre)) => compare_prerelease(left_pre, right_pre),
    }
}

struct ParsedVersionLike {
    release: Vec<u16>,
    pre: Option<Vec<PreReleasePart>>,
}

#[derive(PartialEq, Eq)]
enum PreReleasePart {
    Number(u16),
    Text(String),
}

impl ParsedVersionLike {
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim().split('+').next().unwrap_or(value);
        let (release, pre) = value
            .split_once('-')
            .map_or((value, None), |(release, pre)| (release, Some(pre)));
        let release = version_compare_parts(release)?;
        let pre = pre
            .map(parse_prerelease_parts)
            .filter(|parts| !parts.is_empty());
        Some(Self { release, pre })
    }
}

fn parse_prerelease_parts(value: &str) -> Vec<PreReleasePart> {
    value
        .split(|ch: char| !(ch.is_ascii_alphanumeric()))
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<u16>()
                .map(PreReleasePart::Number)
                .unwrap_or_else(|_| PreReleasePart::Text(part.to_ascii_lowercase()))
        })
        .collect()
}

fn compare_prerelease(left: &[PreReleasePart], right: &[PreReleasePart]) -> Option<i8> {
    for i in 0..left.len().max(right.len()) {
        let Some(l) = left.get(i) else {
            return Some(-1);
        };
        let Some(r) = right.get(i) else {
            return Some(1);
        };
        let ordering = match (l, r) {
            (PreReleasePart::Number(l), PreReleasePart::Number(r)) => l.cmp(r),
            (PreReleasePart::Text(l), PreReleasePart::Text(r)) => l.cmp(r),
            (PreReleasePart::Number(_), PreReleasePart::Text(_)) => std::cmp::Ordering::Less,
            (PreReleasePart::Text(_), PreReleasePart::Number(_)) => std::cmp::Ordering::Greater,
        };
        match ordering {
            std::cmp::Ordering::Greater => return Some(1),
            std::cmp::Ordering::Less => return Some(-1),
            std::cmp::Ordering::Equal => {}
        }
    }
    Some(0)
}

fn version_compare_parts(value: &str) -> Option<Vec<u16>> {
    let parts: Vec<u16> = value
        .split(|c: char| !(c.is_ascii_digit()))
        .filter(|part| !part.is_empty())
        .take(4)
        .filter_map(|part| part.parse::<u16>().ok())
        .collect();
    if parts.is_empty() { None } else { Some(parts) }
}

pub(super) fn is_exact_modrinth_match(
    version: &ModrinthVersion,
    mc_version: &str,
    loader: &str,
) -> bool {
    version.game_versions.iter().any(|v| v == mc_version)
        && version.loaders.iter().any(|l| l == loader)
        && !version_explicitly_targets_other_mc_version(version, mc_version)
        && version
            .files
            .iter()
            .any(|file| is_safe_filename(&file.filename))
}

pub(super) fn select_compatible_file<'a>(
    version: &'a ModrinthVersion,
    mc_version: &str,
) -> Option<&'a ModrinthFile> {
    version
        .files
        .iter()
        .filter(|file| is_safe_filename(&file.filename))
        .min_by_key(|file| {
            (
                file_target_rank(file, mc_version),
                if file.primary { 0 } else { 1 },
            )
        })
}

fn version_type_rank(version_type: Option<&str>) -> u8 {
    match version_type {
        Some("release") => 0,
        Some("beta") => 1,
        Some("alpha") => 2,
        _ => 3,
    }
}

fn version_target_rank(version: &ModrinthVersion, mc_version: &str) -> u8 {
    if artifact_targets_mc_version(&version.version_number, mc_version)
        || version
            .files
            .iter()
            .any(|file| artifact_targets_mc_version(&file.filename, mc_version))
    {
        0
    } else {
        1
    }
}

fn file_target_rank(file: &ModrinthFile, mc_version: &str) -> u8 {
    if artifact_targets_mc_version(&file.filename, mc_version) {
        0
    } else {
        1
    }
}

fn artifact_targets_mc_version(value: &str, mc_version: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let version = mc_version.to_ascii_lowercase();
    let compact = version.replace('.', "");
    [
        format!("mc{}", version),
        format!("minecraft-{}", version),
        format!("minecraft_{}", version),
        format!("+{}", version),
        format!("-{}", version),
        format!("_{}", version),
        format!("mc{}", compact),
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub(super) fn artifact_is_usable_for_mc(value: &str, mc_version: &str) -> bool {
    let targets = extract_artifact_mc_targets(value);
    targets.is_empty()
        || targets
            .iter()
            .any(|target| same_mc_version(target, mc_version))
}

fn version_explicitly_targets_other_mc_version(
    version: &ModrinthVersion,
    mc_version: &str,
) -> bool {
    let explicit_targets: Vec<String> = std::iter::once(version.version_number.as_str())
        .chain(version.files.iter().map(|file| file.filename.as_str()))
        .flat_map(extract_artifact_mc_targets)
        .collect();

    !explicit_targets.is_empty()
        && !explicit_targets
            .iter()
            .any(|target| same_mc_version(target, mc_version))
}

fn extract_artifact_mc_targets(value: &str) -> Vec<String> {
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut targets = Vec::new();
    let mut i = 0;

    while i + 2 <= bytes.len() {
        if lower[i..].starts_with("mc") {
            let start = i + 2;
            if let Some((target, end)) = read_dotted_version(&lower, start) {
                targets.push(target);
                i = end;
                continue;
            }
        }

        i += 1;
    }

    targets
}

fn read_dotted_version(value: &str, start: usize) -> Option<(String, usize)> {
    let bytes = value.as_bytes();
    let mut end = start;
    let mut dot_count = 0;

    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_digit() {
            end += 1;
        } else if b == b'.' {
            dot_count += 1;
            end += 1;
        } else {
            break;
        }
    }

    if end == start || dot_count == 0 {
        return None;
    }

    let target = value[start..end].trim_matches('.').to_string();
    if target.is_empty() {
        None
    } else {
        Some((target, end))
    }
}

fn same_mc_version(left: &str, right: &str) -> bool {
    left == right
}
