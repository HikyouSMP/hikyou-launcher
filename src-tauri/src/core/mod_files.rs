//! Installed mod file operations and metadata backfill.

use crate::core::mod_recommendations::fetch_projects;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModFile {
    pub filename: String,
    pub size_bytes: u64,
    pub display_name: Option<String>,
    pub icon_url: Option<String>,
}

// ── メタデータキャッシュ ───────────────────────────────────────────────────────
// mods/.mod_meta.json に { filename → { display_name, icon_url, project_id } } を保存

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct ModMetaEntry {
    pub(super) display_name: String,
    pub(super) icon_url: Option<String>,
    pub(super) project_id: String,
}

pub(super) type ModMeta = HashMap<String, ModMetaEntry>;

/// ファイル名が安全か検証する。
/// Path::components() を使うことで、OS固有の区切り文字・`.`・`..`・
/// 絶対パス・NULバイトを一括して拒否する。
pub(super) fn is_safe_filename(filename: &str) -> bool {
    if filename.is_empty() || filename.contains('\0') {
        return false;
    }
    let mut components = std::path::Path::new(filename).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

pub(super) fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

fn meta_path(game_dir: &Path) -> PathBuf {
    mods_dir(game_dir).join(".mod_meta.json")
}

pub(super) async fn load_meta(game_dir: &Path) -> ModMeta {
    let path = meta_path(game_dir);
    match fs::read_to_string(&path).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => ModMeta::new(),
    }
}

pub(super) async fn save_meta(game_dir: &Path, meta: &ModMeta) {
    let path = meta_path(game_dir);
    if let Ok(s) = serde_json::to_string_pretty(meta) {
        let _ = fs::write(path, s).await;
    }
}

// ── インストール済み Mod 一覧 ─────────────────────────────────────────────────

pub async fn list_mods(game_dir: &Path) -> Vec<ModFile> {
    let dir = mods_dir(game_dir);
    let meta = load_meta(game_dir).await;
    let mut result = Vec::new();
    let mut rd = match fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(_) => return result,
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let file_meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !file_meta.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue; // .mod_meta.json など非表示ファイルを除外
        }
        if name.ends_with(".jar") || name.ends_with(".disabled") {
            let cached = meta.get(&name);
            result.push(ModFile {
                filename: name.clone(),
                size_bytes: file_meta.len(),
                display_name: cached.map(|e| e.display_name.clone()),
                icon_url: cached.and_then(|e| e.icon_url.clone()),
            });
        }
    }
    result.sort_by(|a, b| a.filename.cmp(&b.filename));
    result
}

pub async fn remove_mod(game_dir: &Path, filename: &str) -> Result<(), String> {
    if !is_safe_filename(filename) {
        return Err("invalid filename".to_string());
    }
    let path = mods_dir(game_dir).join(filename);
    if !path.exists() {
        return Err(format!("Mod file was not found: {}", filename));
    }
    fs::remove_file(&path)
        .await
        .map_err(|e| format!("failed to delete: {}", e))?;
    // メタデータからも削除
    let mut meta = load_meta(game_dir).await;
    meta.remove(filename);
    save_meta(game_dir, &meta).await;
    Ok(())
}

// ── Mod 有効/無効切り替え ─────────────────────────────────────────────────

pub async fn toggle_mod(game_dir: &Path, filename: &str) -> Result<ModFile, String> {
    if !is_safe_filename(filename) {
        return Err("invalid filename".to_string());
    }
    let dir = mods_dir(game_dir);
    let src = dir.join(filename);
    if !src.exists() {
        return Err(format!("Mod file was not found: {}", filename));
    }

    let new_filename = if let Some(enabled_name) = filename.strip_suffix(".disabled") {
        enabled_name.to_string()
    } else if filename.ends_with(".jar") {
        format!("{}.disabled", filename)
    } else {
        return Err("unsupported file type".to_string());
    };

    let dest = dir.join(&new_filename);
    fs::rename(&src, &dest)
        .await
        .map_err(|e| format!("failed to rename: {}", e))?;

    // メタデータのキーを更新
    let mut meta = load_meta(game_dir).await;
    if let Some(entry) = meta.remove(filename) {
        meta.insert(new_filename.clone(), entry);
        save_meta(game_dir, &meta).await;
    }

    let file_meta = fs::metadata(&dest)
        .await
        .map_err(|e| format!("failed to read metadata: {}", e))?;
    let cached = meta.get(&new_filename);
    Ok(ModFile {
        filename: new_filename,
        size_bytes: file_meta.len(),
        display_name: cached.map(|e| e.display_name.clone()),
        icon_url: cached.and_then(|e| e.icon_url.clone()),
    })
}

// ── ハッシュによる自動メタデータ補完 ──────────────────────────────────────

async fn sha1_of_file(path: &std::path::Path) -> Option<String> {
    let mut file = tokio::fs::File::open(path).await.ok()?;
    let mut hasher = sha1_smol::Sha1::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = file.read(&mut buf).await.ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hasher.digest().to_string())
}

#[derive(Deserialize)]
struct ModrinthVersionFileEntry {
    project_id: String,
}

/// メタデータがない手動インストール Mod に対し Modrinth のハッシュ API で自動補完する。
/// 戻り値は補完後の最新 mod 一覧。
pub async fn backfill_metadata(game_dir: &Path) -> Vec<ModFile> {
    let dir = mods_dir(game_dir);
    let mut meta = load_meta(game_dir).await;

    let client = match reqwest::Client::builder()
        .user_agent("HikyouLauncher/1.0")
        .build()
    {
        Ok(c) => c,
        Err(_) => return list_mods(game_dir).await,
    };

    // メタデータが未キャッシュのファイルを収集
    let mut missing: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut rd = match fs::read_dir(&dir).await {
        Ok(r) => r,
        Err(_) => return list_mods(game_dir).await,
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if !name.ends_with(".jar") && !name.ends_with(".disabled") {
            continue;
        }
        if meta.contains_key(&name) {
            continue;
        }
        missing.push((name, entry.path()));
    }

    if missing.is_empty() {
        return list_mods(game_dir).await;
    }

    // SHA1 を並列計算
    let mut hash_to_name: HashMap<String, String> = HashMap::new();
    for (name, path) in &missing {
        if let Some(h) = sha1_of_file(path).await {
            hash_to_name.insert(h, name.clone());
        }
    }

    if hash_to_name.is_empty() {
        return list_mods(game_dir).await;
    }

    // Modrinth bulk hash lookup: POST /v2/version_files
    #[derive(serde::Serialize)]
    struct HashLookupReq<'a> {
        hashes: Vec<&'a String>,
        algorithm: &'static str,
    }
    let req_body = HashLookupReq {
        hashes: hash_to_name.keys().collect(),
        algorithm: "sha1",
    };

    let version_map: HashMap<String, ModrinthVersionFileEntry> = match client
        .post("https://api.modrinth.com/v2/version_files")
        .json(&req_body)
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(_) => return list_mods(game_dir).await,
    };

    // プロジェクト ID を収集
    let mut pid_to_filename: HashMap<String, String> = HashMap::new();
    for (hash, version) in &version_map {
        if let Some(filename) = hash_to_name.get(hash) {
            pid_to_filename.insert(version.project_id.clone(), filename.clone());
        }
    }

    // プロジェクト情報を一括取得
    let pids: Vec<String> = pid_to_filename.keys().cloned().collect();
    let projects = fetch_projects(&client, &pids).await;

    for proj in projects {
        if let Some(filename) = pid_to_filename.get(&proj.id) {
            meta.insert(
                filename.clone(),
                ModMetaEntry {
                    display_name: proj.title.clone(),
                    icon_url: proj.icon_url.clone(),
                    project_id: proj.id.clone(),
                },
            );
        }
    }

    save_meta(game_dir, &meta).await;
    list_mods(game_dir).await
}
