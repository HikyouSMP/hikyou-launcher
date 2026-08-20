//! Fabric Mod ローダーのサポート
//!
//! Fabric Meta API からローダー情報を取得し、
//! バニラの VersionJson に Fabric のライブラリ・mainClass をマージして返す。

use crate::core::cache;
use crate::core::manifest::{
    ArgumentValue, Arguments, Library, LibraryArtifact, LibraryDownloads, VersionJson,
};
use crate::core::paths::LauncherPaths;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";
const CACHE_TYPE: &str = "fabric_loader";
const CACHE_TTL: i64 = 3600; // 1 時間

// ────────────────────────────────────────────────────────────────────────────
// 公開型
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FabricLoaderVersion {
    pub version: String,
    pub stable: bool,
}

// ────────────────────────────────────────────────────────────────────────────
// Fabric Meta API のレスポンス型 (内部用)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct FabricMetaEntry {
    loader: FabricLoaderMeta,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderMeta {
    version: String,
    stable: bool,
}

#[derive(Debug, Deserialize)]
struct FabricProfile {
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    libraries: Vec<FabricLibrary>,
    arguments: Option<FabricArguments>,
}

#[derive(Debug, Deserialize)]
struct FabricArguments {
    #[serde(default)]
    game: Vec<String>,
    #[serde(default)]
    jvm: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FabricLibrary {
    name: String,
    url: String,
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// 指定 MC バージョンで利用可能な Fabric ローダーバージョン一覧を返す（1 時間キャッシュ）。
pub async fn fetch_loader_versions(mc_version: &str) -> Result<Vec<FabricLoaderVersion>, String> {
    if let Some(c) = cache::get()
        && let Some(cached) = c
            .get::<Vec<FabricLoaderVersion>>(CACHE_TYPE, mc_version)
            .await
    {
        return Ok(cached);
    }

    let client = Client::new();
    let url = format!("{}/versions/loader/{}", FABRIC_META_BASE, mc_version);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Fabric Meta API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Minecraft {} is not supported by Fabric (HTTP {})",
            mc_version,
            resp.status()
        ));
    }

    let entries: Vec<FabricMetaEntry> = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse Fabric Meta API response: {}", e))?;

    if entries.is_empty() {
        return Err(format!(
            "Minecraft {} is not supported by Fabric",
            mc_version
        ));
    }

    let versions: Vec<FabricLoaderVersion> = entries
        .into_iter()
        .map(|e| FabricLoaderVersion {
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

/// バニラ VersionJson に Fabric の情報をマージして返す。
/// ダウンロード・起動はこのマージ済み JSON をそのまま使える。
pub async fn build_fabric_version_json(
    vanilla_json: &VersionJson,
    loader_version: &str,
    paths: &LauncherPaths,
) -> Result<VersionJson, String> {
    let mc_version = &vanilla_json.id;
    let profile = fetch_fabric_profile(mc_version, loader_version, paths).await?;

    let mut merged = vanilla_json.clone();

    // mainClass を Fabric のものに差し替え
    merged.main_class = profile.main_class;

    // Fabric のライブラリをバニラの先頭に追加
    // (クラスパスで Fabric が先に来る必要がある)
    let fabric_libs = fabric_libs_to_vanilla(profile.libraries)?;
    let mut all_libs = fabric_libs;
    all_libs.extend(merged.libraries.clone());
    merged.libraries = all_libs;

    // Fabric の arguments をマージ
    if let Some(fabric_args) = profile.arguments {
        let vanilla_args = merged.arguments.get_or_insert(Arguments {
            game: Vec::new(),
            jvm: Vec::new(),
        });
        for s in fabric_args.game {
            vanilla_args.game.push(ArgumentValue::Simple(s));
        }
        for s in fabric_args.jvm {
            vanilla_args.jvm.push(ArgumentValue::Simple(s));
        }
    }

    log::info!(
        "Fabric merge complete: MC {} + Loader {}",
        mc_version,
        loader_version
    );
    Ok(merged)
}

// ────────────────────────────────────────────────────────────────────────────
// 内部関数
// ────────────────────────────────────────────────────────────────────────────

async fn fetch_fabric_profile(
    mc_version: &str,
    loader_version: &str,
    paths: &LauncherPaths,
) -> Result<FabricProfile, String> {
    // キャッシュ確認
    let cache_dir = paths.fabric_dir();
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("failed to create Fabric cache directory: {}", e))?;
    let cache_path = cache_dir.join(format!("{}-{}.json", mc_version, loader_version));

    if loader_json_is_fresh(&cache_path)
        && let Ok(content) = std::fs::read_to_string(&cache_path)
    {
        if let Ok(profile) = serde_json::from_str::<FabricProfile>(&content) {
            return Ok(profile);
        }
        // 破損していたら削除して再取得
        let _ = std::fs::remove_file(&cache_path);
    }

    let url = format!(
        "{}/versions/loader/{}/{}/profile/json",
        FABRIC_META_BASE, mc_version, loader_version
    );

    let resp = Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Fabric profile request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "failed to fetch Fabric profile (HTTP {}): {} / {}",
            resp.status(),
            mc_version,
            loader_version
        ));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read Fabric profile: {}", e))?;

    let profile: FabricProfile = serde_json::from_str(&text).map_err(|e| {
        format!(
            "failed to parse Fabric profile: {} (body: {})",
            e,
            &text[..text.len().min(200)]
        )
    })?;

    let _ = std::fs::write(&cache_path, &text);
    log::info!("Cached Fabric profile: {:?}", cache_path);

    Ok(profile)
}

/// Fabric の Maven ライブラリ → バニラ形式の Library に変換
fn fabric_libs_to_vanilla(libs: Vec<FabricLibrary>) -> Result<Vec<Library>, String> {
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
                        // Fabric Meta は sha1 を提供しない → 空文字でスキップ
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

/// ローダー JSON キャッシュが 30 日以内に書かれたかどうかを返す。
/// ファイルが存在しない・mtime 取得失敗の場合は false。
fn loader_json_is_fresh(path: &std::path::Path) -> bool {
    const TTL_SECS: u64 = 30 * 24 * 3600;
    path.metadata()
        .and_then(|m| m.modified())
        .and_then(|t| t.elapsed().map_err(|_| std::io::Error::other("")))
        .map(|age| age.as_secs() < TTL_SECS)
        .unwrap_or(false)
}

/// Maven 座標 → ファイルパス変換
/// "net.fabricmc:fabric-loader:0.16.9"
///   → "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
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
