//! アセット（サウンド・テクスチャ等）のダウンロード管理

use crate::core::downloader::DownloadProgress;
use crate::core::manifest::{AssetIndex, AssetIndexFile};
use crate::core::paths::LauncherPaths;
use reqwest::Client;
use sha1_smol::Sha1;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinSet;

const RESOURCE_BASE_URL: &str = "https://resources.download.minecraft.net";
const PARALLEL_LIMIT: usize = 32;

pub async fn download_assets(
    asset_index: &AssetIndex,
    paths: &LauncherPaths,
    app: &AppHandle,
) -> Result<(), String> {
    let index_file = fetch_asset_index(asset_index, paths).await?;

    // 1.7.2以前は virtual/legacy/ にもファイルを配置する必要がある
    let needs_virtual = asset_index.id == "legacy" || asset_index.id == "pre-1.6";
    let virtual_dir = paths.assets().join("virtual").join("legacy");

    if needs_virtual {
        fs::create_dir_all(&virtual_dir)
            .map_err(|e| format!("failed to create virtual directory: {}", e))?;
        log::info!("virtual/legacy asset mode enabled (id={})", asset_index.id);
    }

    // (asset_name, hash) のペアで収集する
    let objects: Vec<(String, String)> = index_file
        .objects
        .into_iter()
        .map(|(name, obj)| (name, obj.hash))
        .collect();
    let total = objects.len();
    log::info!("Asset count: {}", total);

    let objects_dir = paths.assets().join("objects");
    if assets_are_present(
        &objects,
        &objects_dir,
        needs_virtual.then_some(&virtual_dir),
    ) {
        log::info!(
            "Asset check skipped; all {} cached objects are present",
            total
        );
        return Ok(());
    }

    let client = Client::new();
    let mut completed = 0usize;

    for chunk in objects.chunks(PARALLEL_LIMIT) {
        let mut set: JoinSet<Result<(), String>> = JoinSet::new();

        for (asset_name, hash) in chunk {
            let hash = hash.clone();
            let asset_name = asset_name.clone();
            let client = client.clone();
            let objects_dir = objects_dir.clone();
            // needs_virtual のときだけ virtual_dir を渡す
            let vdir = if needs_virtual {
                Some(virtual_dir.clone())
            } else {
                None
            };

            set.spawn(async move {
                download_asset_object(&client, &hash, &objects_dir, vdir, asset_name).await
            });
        }

        while let Some(result) = set.join_next().await {
            result.map_err(|e| format!("task failed: {}", e))??;
            completed += 1;
            let _ = app.emit(
                "download://progress",
                DownloadProgress {
                    completed,
                    total,
                    current_file: String::new(),
                    bytes_downloaded: 0,
                    bytes_total: 0,
                    phase: "assets".to_string(),
                },
            );
        }
    }

    log::info!("Asset download complete ({} files)", total);
    Ok(())
}

fn assets_are_present(
    objects: &[(String, String)],
    objects_dir: &Path,
    virtual_dir: Option<&PathBuf>,
) -> bool {
    objects.iter().all(|(asset_name, hash)| {
        if hash.len() < 2 {
            return false;
        }
        let object_path = objects_dir.join(&hash[..2]).join(hash);
        let object_ok = object_path
            .metadata()
            .map(|metadata| metadata.is_file() && metadata.len() > 0)
            .unwrap_or(false);
        if !object_ok {
            return false;
        }
        virtual_dir
            .map(|dir| dir.join(asset_name).is_file())
            .unwrap_or(true)
    })
}

// ────────────────────────────────────────────────────────────────────────────
// 内部関数
// ────────────────────────────────────────────────────────────────────────────

async fn fetch_asset_index(
    asset_index: &AssetIndex,
    paths: &LauncherPaths,
) -> Result<AssetIndexFile, String> {
    let indexes_dir = paths.assets().join("indexes");
    fs::create_dir_all(&indexes_dir)
        .map_err(|e| format!("failed to create indexes directory: {}", e))?;

    let cache_path = indexes_dir.join(format!("{}.json", asset_index.id));

    if cache_path.exists()
        && let Ok(content) = fs::read_to_string(&cache_path)
        && let Ok(index) = serde_json::from_str(&content)
    {
        return Ok(index);
    }

    let client = Client::new();
    let res = client
        .get(&asset_index.url)
        .send()
        .await
        .map_err(|e| format!("failed to fetch asset index: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("asset index HTTP request failed: {}", res.status()));
    }

    let text = res
        .text()
        .await
        .map_err(|e| format!("failed to read asset index response: {}", e))?;

    fs::write(&cache_path, &text)
        .map_err(|e| format!("failed to write asset index cache: {}", e))?;

    serde_json::from_str(&text).map_err(|e| format!("failed to parse asset index: {}", e))
}

/// アセットを1件ダウンロードする。
/// `vdir` が Some のとき、objects/ に保存後に virtual/legacy/{asset_name} にもコピーする。
async fn download_asset_object(
    client: &Client,
    hash: &str,
    objects_dir: &Path,
    vdir: Option<PathBuf>,
    asset_name: String,
) -> Result<(), String> {
    let prefix = &hash[..2];
    let dest_dir = objects_dir.join(prefix);
    let dest = dest_dir.join(hash);

    // objects/ に既に正しいファイルがあればスキップ
    if dest.exists() {
        let bytes = fs::read(&dest).map_err(|e| format!("failed to read file: {}", e))?;
        if compute_sha1(&bytes) == hash {
            if let Some(ref vd) = vdir {
                copy_to_virtual(&dest, &asset_name, vd)?;
            }
            return Ok(());
        }
    }

    fs::create_dir_all(&dest_dir).map_err(|e| format!("failed to create directory: {}", e))?;

    let url = format!("{}/{}/{}", RESOURCE_BASE_URL, prefix, hash);
    let res = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("asset download failed {}: {}", hash, e))?;

    if !res.status().is_success() {
        return Err(format!(
            "asset download HTTP request failed {} ({})",
            hash,
            res.status()
        ));
    }

    let bytes = res
        .bytes()
        .await
        .map_err(|e| format!("failed to read asset response: {}", e))?;

    if compute_sha1(&bytes) != hash {
        return Err(format!("asset SHA1 mismatch {}", hash));
    }

    fs::write(&dest, &bytes).map_err(|e| format!("failed to write asset {:?}: {}", dest, e))?;

    if let Some(ref vd) = vdir {
        copy_to_virtual(&dest, &asset_name, vd)?;
    }

    Ok(())
}

fn copy_to_virtual(source: &std::path::Path, asset_name: &str, vdir: &Path) -> Result<(), String> {
    let vdest = vdir.join(asset_name);
    if vdest.exists() {
        return Ok(());
    }
    if let Some(parent) = vdest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create virtual subdirectory: {}", e))?;
    }
    fs::copy(source, &vdest)
        .map_err(|e| format!("failed to copy virtual asset {}: {}", asset_name, e))?;
    Ok(())
}

fn compute_sha1(data: &[u8]) -> String {
    format!("{}", Sha1::from(data).digest())
}
