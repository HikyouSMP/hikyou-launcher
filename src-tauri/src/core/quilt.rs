//! Quilt Mod ローダーのサポート
//!
//! Quilt Meta API (v3) からローダー情報を取得し、
//! バニラの VersionJson に Quilt のライブラリ・mainClass をマージして返す。
//! API 構造は Fabric Meta v2 とほぼ同一。

use crate::core::cache;
use crate::core::manifest::{
    ArgumentValue, Arguments, Library, LibraryArtifact, LibraryDownloads, VersionJson,
};
use crate::core::paths::LauncherPaths;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const QUILT_META_BASE: &str = "https://meta.quiltmc.org/v3";
const CACHE_TYPE: &str = "quilt_loader";
const CACHE_TTL: i64 = 3600;

// ────────────────────────────────────────────────────────────────────────────
// 公開型
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuiltLoaderVersion {
    pub version: String,
    /// Quilt Meta は "stable" フィールドを持つ
    pub stable: bool,
}

// ────────────────────────────────────────────────────────────────────────────
// Quilt Meta API のレスポンス型 (内部用)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct QuiltMetaEntry {
    loader: QuiltLoaderMeta,
}

#[derive(Debug, Deserialize)]
struct QuiltLoaderMeta {
    version: String,
    #[serde(default)]
    stable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MainClassValue {
    Simple(String),
    /// Quilt は { "client": "...", "server": "..." } の形式の場合がある
    Split {
        client: String,
        #[allow(dead_code)]
        server: Option<String>,
    },
}

impl MainClassValue {
    fn into_client(self) -> String {
        match self {
            MainClassValue::Simple(s) => s,
            MainClassValue::Split { client, .. } => client,
        }
    }
}

#[derive(Debug, Deserialize)]
struct QuiltProfile {
    #[serde(rename = "mainClass")]
    main_class: MainClassValue,
    #[serde(default)]
    libraries: Vec<QuiltLibrary>,
    arguments: Option<QuiltArguments>,
}

#[derive(Debug, Deserialize)]
struct QuiltArguments {
    #[serde(default)]
    game: Vec<String>,
    #[serde(default)]
    jvm: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct QuiltLibrary {
    name: String,
    url: String,
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// 指定 MC バージョンで利用可能な Quilt ローダーバージョン一覧を返す（1 時間キャッシュ）。
pub async fn fetch_loader_versions(mc_version: &str) -> Result<Vec<QuiltLoaderVersion>, String> {
    if let Some(c) = cache::get()
        && let Some(cached) = c
            .get::<Vec<QuiltLoaderVersion>>(CACHE_TYPE, mc_version)
            .await
    {
        return Ok(cached);
    }

    let client = Client::new();
    let url = format!("{}/versions/loader/{}", QUILT_META_BASE, mc_version);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Quilt Meta API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Minecraft {} is not supported by Quilt (HTTP {})",
            mc_version,
            resp.status()
        ));
    }

    let entries: Vec<QuiltMetaEntry> = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse Quilt Meta API response: {}", e))?;

    if entries.is_empty() {
        return Err(format!(
            "Minecraft {} is not supported by Quilt",
            mc_version
        ));
    }

    let versions: Vec<QuiltLoaderVersion> = entries
        .into_iter()
        .map(|e| QuiltLoaderVersion {
            version: e.loader.version,
            stable: e.loader.stable,
        })
        .collect();

    if let Some(c) = cache::get() {
        c.set(CACHE_TYPE, mc_version, &versions, CACHE_TTL)
            .await
            .ok();
    }
    Ok(versions)
}

/// バニラ VersionJson に Quilt の情報をマージして返す。
pub async fn build_quilt_version_json(
    vanilla_json: &VersionJson,
    loader_version: &str,
    paths: &LauncherPaths,
) -> Result<VersionJson, String> {
    let mc_version = &vanilla_json.id;
    let profile = fetch_quilt_profile(mc_version, loader_version, paths).await?;

    let mut merged = vanilla_json.clone();
    merged.main_class = profile.main_class.into_client();

    let quilt_libs = quilt_libs_to_vanilla(profile.libraries)?;
    let mut all_libs = quilt_libs;
    all_libs.extend(merged.libraries.clone());
    merged.libraries = all_libs;

    if let Some(quilt_args) = profile.arguments {
        let vanilla_args = merged.arguments.get_or_insert(Arguments {
            game: Vec::new(),
            jvm: Vec::new(),
        });
        for s in quilt_args.game {
            vanilla_args.game.push(ArgumentValue::Simple(s));
        }
        for s in quilt_args.jvm {
            vanilla_args.jvm.push(ArgumentValue::Simple(s));
        }
    }

    log::info!(
        "Quilt merge complete: MC {} + Loader {}",
        mc_version,
        loader_version
    );
    Ok(merged)
}

// ────────────────────────────────────────────────────────────────────────────
// 内部関数
// ────────────────────────────────────────────────────────────────────────────

async fn fetch_quilt_profile(
    mc_version: &str,
    loader_version: &str,
    paths: &LauncherPaths,
) -> Result<QuiltProfile, String> {
    let cache_dir = paths.quilt_dir();
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("failed to create Quilt cache directory: {}", e))?;
    let cache_path = cache_dir.join(format!("{}-{}.json", mc_version, loader_version));

    if loader_json_is_fresh(&cache_path)
        && let Ok(content) = std::fs::read_to_string(&cache_path)
    {
        if let Ok(profile) = serde_json::from_str::<QuiltProfile>(&content) {
            return Ok(profile);
        }
        let _ = std::fs::remove_file(&cache_path);
    }

    let url = format!(
        "{}/versions/loader/{}/{}/profile/json",
        QUILT_META_BASE, mc_version, loader_version
    );

    let resp = Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Quilt profile request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "failed to fetch Quilt profile (HTTP {}): {} / {}",
            resp.status(),
            mc_version,
            loader_version
        ));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read Quilt profile: {}", e))?;

    let profile: QuiltProfile = serde_json::from_str(&text).map_err(|e| {
        format!(
            "failed to parse Quilt profile: {} (body: {})",
            e,
            &text[..text.len().min(200)]
        )
    })?;

    let _ = std::fs::write(&cache_path, &text);
    log::info!("Cached Quilt profile: {:?}", cache_path);
    Ok(profile)
}

fn loader_json_is_fresh(path: &std::path::Path) -> bool {
    const TTL_SECS: u64 = 30 * 24 * 3600;
    path.metadata()
        .and_then(|m| m.modified())
        .and_then(|t| t.elapsed().map_err(|_| std::io::Error::other("")))
        .map(|age| age.as_secs() < TTL_SECS)
        .unwrap_or(false)
}

fn quilt_libs_to_vanilla(libs: Vec<QuiltLibrary>) -> Result<Vec<Library>, String> {
    libs.into_iter()
        .map(|lib| {
            let path = maven_coord_to_path(&lib.name)?;
            let url = format!(
                "{}/{}",
                lib.url.trim_end_matches('/'),
                path.replace('\\', "/")
            );
            Ok(Library {
                name: lib.name,
                downloads: Some(LibraryDownloads {
                    artifact: Some(LibraryArtifact {
                        path,
                        url,
                        sha1: String::new(),
                        size: 0,
                    }),
                    classifiers: None,
                }),
                rules: None,
                natives: None,
                extract: None,
            })
        })
        .collect()
}

fn maven_coord_to_path(coord: &str) -> Result<String, String> {
    let parts: Vec<&str> = coord.splitn(4, ':').collect();
    if parts.len() < 3 {
        return Err(format!("invalid Maven coordinates: {}", coord));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let jar = if parts.len() == 4 {
        format!("{}-{}-{}.jar", artifact, version, parts[3])
    } else {
        format!("{}-{}.jar", artifact, version)
    };
    Ok(format!("{}/{}/{}/{}", group, artifact, version, jar))
}
