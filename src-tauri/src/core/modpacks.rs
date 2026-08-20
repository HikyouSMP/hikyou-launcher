use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::core::cache;
use crate::core::mod_files::{ModFile, backfill_metadata, is_safe_filename, load_meta, mods_dir};
use crate::core::mod_recommendations::ModrinthProject;
use crate::core::mods::{ModSearchResult, ModpackInstallResult};

const CACHE_MODPACK_SEARCH: &str = "modrinth_modpack_search";
const CACHE_MODPACK_VERSIONS: &str = "modrinth_modpack_versions";
const TTL_SEARCH: i64 = 300;
const TTL_VERSIONS: i64 = 1800;

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

#[derive(Deserialize)]
struct ModrinthVersion {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    files: Vec<ModrinthFile>,
}

#[derive(Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    primary: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModpackVersionInfo {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
}
// ── Modrinth ModPack 検索 ─────────────────────────────────────────────────

pub async fn search_modpacks(
    query: &str,
    mc_version: &str,
) -> Result<Vec<ModSearchResult>, String> {
    let alias = format!("{}|{}", query, mc_version);

    if let Some(c) = cache::get()
        && let Some(cached) = c
            .get::<Vec<ModSearchResult>>(CACHE_MODPACK_SEARCH, &alias)
            .await
    {
        return Ok(cached);
    }

    let stale = if let Some(c) = cache::get() {
        c.get_stale_with_etag::<Vec<ModSearchResult>>(CACHE_MODPACK_SEARCH, &alias)
            .await
    } else {
        None
    };

    let facets = if mc_version.is_empty() {
        r#"[["project_type:modpack"]]"#.to_string()
    } else {
        format!(r#"[["project_type:modpack"],["versions:{}"]]"#, mc_version)
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

    if resp.status() == reqwest::StatusCode::NOT_MODIFIED
        && let Some((data, etag)) = stale
    {
        if let Some(c) = cache::get() {
            c.set_with_etag(
                CACHE_MODPACK_SEARCH,
                &alias,
                &data,
                TTL_SEARCH,
                etag.as_deref(),
            )
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
            CACHE_MODPACK_SEARCH,
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

// ── Modrinth ModPack インストール ─────────────────────────────────────────

#[derive(Deserialize)]
struct MrpackIndex {
    files: Vec<MrpackFile>,
    #[serde(default)]
    dependencies: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct MrpackFile {
    path: String,
    downloads: Vec<String>,
    #[serde(rename = "fileSize")]
    file_size: Option<u64>,
    #[serde(default)]
    env: Option<MrpackEnv>,
}

#[derive(Deserialize)]
struct MrpackEnv {
    client: Option<String>,
}

pub async fn get_modpack_versions(project_id: &str) -> Result<Vec<ModpackVersionInfo>, String> {
    // 1. 有効なキャッシュがあればそのまま返す
    if let Some(c) = cache::get()
        && let Some(cached) = c
            .get::<Vec<ModpackVersionInfo>>(CACHE_MODPACK_VERSIONS, project_id)
            .await
    {
        return Ok(cached);
    }

    // 2. 期限切れでも stale データと ETag を取得
    let (stale, etag) = if let Some(c) = cache::get() {
        c.get_stale_with_etag::<Vec<ModpackVersionInfo>>(CACHE_MODPACK_VERSIONS, project_id)
            .await
            .map(|(v, e)| (Some(v), e))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    let client = reqwest::Client::builder()
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("https://api.modrinth.com/v2/project/{}/version", project_id);
    let mut req = client.get(&url);
    if let Some(ref tag) = etag {
        req = req.header("If-None-Match", tag.as_str());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("failed to fetch versions: {}", e))?;

    // 3. 304 Not Modified → stale データの TTL を更新して返す
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED
        && let (Some(data), Some(c)) = (stale, cache::get())
    {
        c.set_with_etag(
            CACHE_MODPACK_VERSIONS,
            project_id,
            &data,
            TTL_VERSIONS,
            etag.as_deref(),
        )
        .await
        .ok();
        return Ok(data);
    }

    // 4. 新しい ETag を取得
    let new_etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let versions: Vec<ModrinthVersion> = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse version data: {}", e))?;

    let result: Vec<ModpackVersionInfo> = versions
        .into_iter()
        .map(|v| ModpackVersionInfo {
            id: v.id,
            name: v.name,
            version_number: v.version_number,
            game_versions: v.game_versions,
        })
        .collect();

    if let Some(c) = cache::get() {
        c.set_with_etag(
            CACHE_MODPACK_VERSIONS,
            project_id,
            &result,
            TTL_VERSIONS,
            new_etag.as_deref(),
        )
        .await
        .ok();
    }
    Ok(result)
}

pub async fn install_modpack(
    game_dir: &Path,
    project_id: &str,
    mc_version: &str,
    version_id: Option<&str>,
) -> Result<ModpackInstallResult, String> {
    let client = reqwest::Client::builder()
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    // Fetch project info for the name
    let project_resp = client
        .get(format!(
            "https://api.modrinth.com/v2/project/{}",
            project_id
        ))
        .send()
        .await;
    let project_title: String = match project_resp {
        Ok(r) => r
            .json::<ModrinthProject>()
            .await
            .map(|p| p.title)
            .unwrap_or_else(|_| project_id.to_string()),
        Err(_) => project_id.to_string(),
    };

    // バージョンを取得 (version_id 指定優先、なければ mc_version でフィルタ)
    let version: ModrinthVersion = if let Some(vid) = version_id {
        let url = format!("https://api.modrinth.com/v2/version/{}", vid);
        client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("failed to fetch versions: {}", e))?
            .json()
            .await
            .map_err(|e| format!("failed to parse version data: {}", e))?
    } else {
        let versions_url = format!(
            "https://api.modrinth.com/v2/project/{}/version?game_versions=[\"{}\"]",
            project_id, mc_version
        );
        let versions: Vec<ModrinthVersion> = client
            .get(&versions_url)
            .send()
            .await
            .map_err(|e| format!("failed to fetch versions: {}", e))?
            .json()
            .await
            .map_err(|e| format!("failed to parse version data: {}", e))?;
        versions
            .into_iter()
            .next()
            .ok_or_else(|| format!("No compatible version was found for {}", mc_version))?
    };

    // .mrpack ファイルを取得
    let mrpack_file = version
        .files
        .iter()
        .find(|f| f.primary || f.filename.ends_with(".mrpack"))
        .or_else(|| version.files.first())
        .ok_or("ModPack file was not found")?;

    let mrpack_bytes = client
        .get(&mrpack_file.url)
        .send()
        .await
        .map_err(|e| format!("ModPack download failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("ModPack failed to read: {}", e))?;

    // ZIP を解凍して modrinth.index.json を取得
    let cursor = std::io::Cursor::new(&mrpack_bytes[..]);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to parse ZIP: {}", e))?;

    let index: MrpackIndex = {
        let mut index_file = archive
            .by_name("modrinth.index.json")
            .map_err(|_| "modrinth.index.json was not found")?;
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut index_file, &mut buf)
            .map_err(|e| format!("index failed to read: {}", e))?;
        serde_json::from_str(&buf).map_err(|e| format!("failed to parse index: {}", e))?
    };

    let dir = mods_dir(game_dir);
    fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("mods failed to create directory: {}", e))?;

    let mut installed: Vec<ModFile> = Vec::new();
    let mut hashes_for_backfill: Vec<String> = Vec::new();

    for pack_file in &index.files {
        // mods/ 以外（config, resourcepack 等）はスキップ
        if !pack_file.path.starts_with("mods/") {
            continue;
        }
        // サーバー専用はスキップ
        if let Some(env) = &pack_file.env
            && env.client.as_deref() == Some("unsupported")
        {
            continue;
        }

        let filename = std::path::Path::new(&pack_file.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !is_safe_filename(&filename) {
            log::warn!(
                "[modpack] Skipping file with unsafe mod filename: {}",
                pack_file.path
            );
            continue;
        }

        let dest = dir.join(&filename);

        if dest.exists() {
            if let Ok(m) = fs::metadata(&dest).await {
                installed.push(ModFile {
                    filename: filename.clone(),
                    size_bytes: m.len(),
                    display_name: None,
                    icon_url: None,
                });
            }
            continue;
        }

        let dl_url = match pack_file.downloads.first() {
            Some(u) => u,
            None => continue,
        };
        if !dl_url.starts_with("https://") {
            log::warn!("[modpack] Skipping non-HTTPS download URL for {}", filename);
            continue;
        }

        let bytes = match client.get(dl_url).send().await {
            Ok(r) => match r.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("[modpack] Download failed {}: {}", filename, e);
                    continue;
                }
            },
            Err(e) => {
                log::warn!("[modpack] Download failed {}: {}", filename, e);
                continue;
            }
        };

        if let Err(e) = fs::write(&dest, &bytes).await {
            log::warn!("[modpack] Save failed {}: {}", filename, e);
            continue;
        }

        log::info!("[modpack] Installed: {}", filename);
        hashes_for_backfill.push(filename.clone());
        installed.push(ModFile {
            filename: filename.clone(),
            size_bytes: pack_file.file_size.unwrap_or(bytes.len() as u64),
            display_name: None,
            icon_url: None,
        });
    }

    // overrides/ と client-overrides/ を展開する（設定・リソースパック等）
    for prefix in &["overrides/", "client-overrides/"] {
        for i in 0..archive.len() {
            let mut entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let raw_name = entry.name().to_string();
            let relative = match raw_name.strip_prefix(prefix) {
                Some(r) if !r.is_empty() => r.to_string(),
                _ => continue,
            };
            // ディレクトリエントリはスキップ
            if relative.ends_with('/') {
                continue;
            }
            // ZIPエントリのパスを検証: Normal以外のコンポーネント（`..`・絶対パス等）を拒否
            if std::path::Path::new(&relative)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                log::warn!(
                    "[modpack] Skipping path with possible traversal: {}",
                    relative
                );
                continue;
            }
            let dest = game_dir.join(&relative);
            if let Some(parent) = dest.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                log::warn!(
                    "[modpack] overrides directory creation failed {}: {}",
                    parent.display(),
                    e
                );
                continue;
            }
            let mut buf = Vec::new();
            if let Err(e) = std::io::Read::read_to_end(&mut entry, &mut buf) {
                log::warn!("[modpack] overrides read failed {}: {}", relative, e);
                continue;
            }
            if let Err(e) = std::fs::write(&dest, &buf) {
                log::warn!("[modpack] overrides write failed {}: {}", relative, e);
                continue;
            }
            log::info!("[modpack] overrides extracted: {}", relative);
        }
    }

    // メタデータをバックフィル
    let updated = backfill_metadata(game_dir).await;
    // installed に display_name / icon_url を反映
    let meta = load_meta(game_dir).await;
    for f in &mut installed {
        if let Some(entry) = meta.get(&f.filename) {
            f.display_name = Some(entry.display_name.clone());
            f.icon_url = entry.icon_url.clone();
        }
    }

    let _ = updated;

    let actual_mc_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .unwrap_or_else(|| mc_version.to_string());

    let (loader, loader_version) = if let Some(v) = index.dependencies.get("fabric-loader") {
        ("fabric".to_string(), Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("neoforge") {
        ("neoforge".to_string(), Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("forge") {
        ("forge".to_string(), Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("quilt-loader") {
        ("quilt".to_string(), Some(v.clone()))
    } else {
        ("fabric".to_string(), None)
    };

    Ok(ModpackInstallResult {
        profile_name: project_title,
        mc_version: actual_mc_version,
        loader,
        loader_version,
        mods: installed,
    })
}
