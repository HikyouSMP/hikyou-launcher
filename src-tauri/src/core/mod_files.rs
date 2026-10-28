//! Installed mod file operations and metadata backfill.

use crate::core::managed_fs;
use crate::core::mod_recommendations::fetch_projects;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

pub(super) const MAX_MOD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_META_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn read_bounded(reader: impl std::io::Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(reader, limit.saturating_add(1)),
        &mut bytes,
    )
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("mod input exceeds supported size limit".to_string());
    }
    Ok(bytes)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModFile {
    #[serde(default)]
    pub project_id: Option<String>,
    pub filename: String,
    pub size_bytes: u64,
    pub display_name: Option<String>,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub version_number: Option<String>,
    #[serde(default)]
    pub hashes: Option<HashMap<String, String>>,
}

// ── メタデータキャッシュ ───────────────────────────────────────────────────────
// mods/.mod_meta.json に { filename → { display_name, icon_url, project_id } } を保存

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct ModMetaEntry {
    pub(super) display_name: String,
    pub(super) icon_url: Option<String>,
    pub(super) project_id: String,
    #[serde(default)]
    pub(super) version_id: Option<String>,
    #[serde(default)]
    pub(super) version_number: Option<String>,
    #[serde(default)]
    pub(super) hashes: Option<HashMap<String, String>>,
    #[serde(default)]
    pub(super) game_versions: Vec<String>,
    #[serde(default)]
    pub(super) loaders: Vec<String>,
}

pub(super) type ModMeta = HashMap<String, ModMetaEntry>;

pub(super) fn is_safe_filename(filename: &str) -> bool {
    crate::core::paths::is_safe_component(filename)
}

pub(super) fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

pub(super) fn managed_mods(game_dir: &Path) -> Result<cap_std::fs::Dir, String> {
    open_mods(game_dir, false)
}

pub(super) fn open_mods(game_dir: &Path, create: bool) -> Result<cap_std::fs::Dir, String> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if std::fs::symlink_metadata(game_dir)
            .map_err(|e| e.to_string())?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err("reparse-point game directories are not supported".to_string());
        }
    }
    let root = managed_fs::root(game_dir)?;
    inspect_mod_path(&root, Path::new("mods"))?;
    managed_fs::directory(&root, Path::new("mods"), create)
}

pub(super) fn inspect_mod_path(
    dir: &cap_std::fs::Dir,
    path: &Path,
) -> Result<Option<cap_std::fs::Metadata>, String> {
    let metadata = managed_fs::inspect(dir, path)?;
    #[cfg(windows)]
    if let Some(metadata) = &metadata {
        use cap_std::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("reparse-point mod paths are not supported".to_string());
        }
    }
    Ok(metadata)
}

pub(super) fn read_mod_file(dir: &cap_std::fs::Dir, path: &Path) -> Result<std::fs::File, String> {
    inspect_mod_path(dir, path)?;
    let file = managed_fs::read_file(dir, path)?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_MOD_BYTES {
        return Err("mod input exceeds supported size limit".to_string());
    }
    Ok(file)
}

pub(super) async fn load_meta(game_dir: &Path) -> ModMeta {
    managed_mods(game_dir)
        .map(|dir| load_meta_in(&dir))
        .unwrap_or_default()
}

pub(super) fn load_meta_in(dir: &cap_std::fs::Dir) -> ModMeta {
    load_meta_checked_in(dir).unwrap_or_default()
}

pub(super) fn load_meta_checked_in(dir: &cap_std::fs::Dir) -> Result<ModMeta, String> {
    if inspect_mod_path(dir, Path::new(".mod_meta.json"))?.is_none() {
        return Ok(ModMeta::new());
    }
    let file = read_mod_file(dir, Path::new(".mod_meta.json"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_META_BYTES {
        return Err("mod metadata exceeds supported size limit".to_string());
    }
    serde_json::from_slice(&read_bounded(file, MAX_META_BYTES)?)
        .map_err(|e| format!("invalid mod metadata cache: {e}"))
}

pub(super) async fn save_meta(game_dir: &Path, meta: &ModMeta) {
    if let Ok(dir) = managed_mods(game_dir) {
        save_meta_in(&dir, meta);
    }
}

fn save_meta_in(dir: &cap_std::fs::Dir, meta: &ModMeta) {
    if let Err(error) = inspect_mod_path(dir, Path::new(".mod_meta.json")) {
        log::warn!("Mod metadata path rejected: {error}");
        return;
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(meta)
        && let Err(error) = managed_fs::write(dir, Path::new(".mod_meta.json"), &bytes, true)
    {
        log::warn!("Mod metadata was not saved: {error}");
    }
}

// ── インストール済み Mod 一覧 ─────────────────────────────────────────────────

pub async fn list_mods(game_dir: &Path) -> Vec<ModFile> {
    match managed_mods(game_dir) {
        Ok(dir) => list_mods_in(&dir),
        Err(_) => Vec::new(),
    }
}

fn list_mods_in(dir: &cap_std::fs::Dir) -> Vec<ModFile> {
    let meta = load_meta_in(dir);
    let mut result = Vec::new();
    let rd = match dir.entries() {
        Ok(rd) => rd,
        Err(_) => return result,
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue; // .mod_meta.json など非表示ファイルを除外
        }
        if name.ends_with(".jar") || name.ends_with(".disabled") {
            let file_meta = match inspect_mod_path(dir, Path::new(&name)) {
                Ok(Some(metadata)) if metadata.is_file() => metadata,
                _ => continue,
            };
            let cached = meta.get(&name);
            result.push(ModFile {
                project_id: cached
                    .map(|e| e.project_id.clone())
                    .filter(|id| !id.is_empty()),
                filename: name.clone(),
                size_bytes: file_meta.len(),
                display_name: cached.map(|e| e.display_name.clone()),
                icon_url: cached.and_then(|e| e.icon_url.clone()),
                version_id: cached.and_then(|e| e.version_id.clone()),
                version_number: cached.and_then(|e| e.version_number.clone()),
                hashes: cached.and_then(|e| e.hashes.clone()),
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
    let dir = managed_mods(game_dir)?;
    if inspect_mod_path(&dir, Path::new(filename))?.is_none() {
        return Err(format!("Mod file was not found: {}", filename));
    }
    dir.remove_file(filename)
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
    let dir = managed_mods(game_dir)?;
    let file_meta = inspect_mod_path(&dir, Path::new(filename))?;
    if !file_meta.as_ref().is_some_and(|m| m.is_file()) {
        return Err(format!("Mod file was not found: {}", filename));
    }

    let new_filename = toggled_filename(filename)?;
    if inspect_mod_path(&dir, Path::new(&new_filename))?.is_some() {
        return Err("the target mod filename already exists".to_string());
    }
    dir.rename(filename, &dir, &new_filename)
        .map_err(|e| format!("failed to rename: {}", e))?;

    // メタデータのキーを更新
    let mut meta = load_meta(game_dir).await;
    if let Some(entry) = meta.remove(filename) {
        meta.insert(new_filename.clone(), entry);
        save_meta(game_dir, &meta).await;
    }

    let file_meta = file_meta.unwrap();
    let cached = meta.get(&new_filename);
    Ok(ModFile {
        project_id: cached
            .map(|e| e.project_id.clone())
            .filter(|id| !id.is_empty()),
        filename: new_filename,
        size_bytes: file_meta.len(),
        display_name: cached.map(|e| e.display_name.clone()),
        icon_url: cached.and_then(|e| e.icon_url.clone()),
        version_id: cached.and_then(|e| e.version_id.clone()),
        version_number: cached.and_then(|e| e.version_number.clone()),
        hashes: cached.and_then(|e| e.hashes.clone()),
    })
}

fn toggled_filename(filename: &str) -> Result<String, String> {
    let name = if let Some(enabled_name) = filename.strip_suffix(".disabled") {
        enabled_name.to_string()
    } else if filename.ends_with(".jar") {
        format!("{filename}.disabled")
    } else {
        return Err("unsupported file type".to_string());
    };
    if !is_safe_filename(&name) {
        return Err("invalid target mod filename".to_string());
    }
    Ok(name)
}

#[cfg(test)]
mod filename_tests {
    use super::{is_safe_filename, toggled_filename};

    #[test]
    fn bounded_reader_rejects_oversized_and_accepts_exact_inputs() {
        assert_eq!(
            super::read_bounded(std::io::Cursor::new(b"abc"), 3).unwrap(),
            b"abc"
        );
        assert!(super::read_bounded(std::io::Cursor::new(b"abcd"), 3).is_err());
    }

    #[test]
    fn toggling_validates_the_result_as_well_as_the_source() {
        assert_eq!(toggled_filename("mod.jar").unwrap(), "mod.jar.disabled");
        assert_eq!(toggled_filename("mod.jar.disabled").unwrap(), "mod.jar");
        assert!(toggled_filename("..disabled").is_err());
        assert!(toggled_filename("...disabled").is_err());
        assert!(!is_safe_filename("a.jar/."));
        assert!(!is_safe_filename("base.jar:stream.jar"));
    }

    #[test]
    fn legacy_metadata_and_display_deserialize_without_provenance() {
        let entry: super::ModMetaEntry = serde_json::from_str(
            r#"{"display_name":"Name","icon_url":null,"project_id":"project"}"#,
        )
        .unwrap();
        assert!(entry.version_id.is_none());
        assert!(entry.version_number.is_none());
        assert!(entry.hashes.is_none());
        let file: super::ModFile = serde_json::from_str(
            r#"{"filename":"a.jar","size_bytes":1,"display_name":null,"icon_url":null}"#,
        )
        .unwrap();
        assert!(file.version_id.is_none());
        assert!(file.version_number.is_none());
        assert!(file.hashes.is_none());
    }
}

// ── ハッシュによる自動メタデータ補完 ──────────────────────────────────────

async fn sha1_of_file(dir: &cap_std::fs::Dir, name: &str) -> Option<String> {
    let mut file = tokio::fs::File::from_std(read_mod_file(dir, Path::new(name)).ok()?);
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
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    version_number: Option<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
}

/// メタデータがない手動インストール Mod に対し Modrinth のハッシュ API で自動補完する。
/// 戻り値は補完後の最新 mod 一覧。
pub async fn backfill_metadata(game_dir: &Path) -> Vec<ModFile> {
    match managed_mods(game_dir) {
        Ok(dir) => backfill_metadata_in(&dir).await,
        Err(_) => Vec::new(),
    }
}

pub(super) async fn backfill_metadata_in(dir: &cap_std::fs::Dir) -> Vec<ModFile> {
    let mut meta = load_meta_in(dir);

    let client = match crate::http::builder().and_then(|builder| {
        builder
            .user_agent("HikyouLauncher/1.0")
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(c) => c,
        Err(_) => return list_mods_in(dir),
    };

    // メタデータが未キャッシュのファイルを収集
    let mut missing = Vec::new();
    let rd = match dir.entries() {
        Ok(r) => r,
        Err(_) => return list_mods_in(dir),
    };
    for entry in rd.flatten() {
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
        missing.push(name);
    }

    if missing.is_empty() {
        return list_mods_in(dir);
    }

    // SHA1 を並列計算
    let mut hash_to_name: HashMap<String, String> = HashMap::new();
    for name in &missing {
        if let Some(h) = sha1_of_file(dir, name).await {
            hash_to_name.insert(h, name.clone());
        }
    }

    if hash_to_name.is_empty() {
        return list_mods_in(dir);
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
        Err(_) => return list_mods_in(dir),
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
                    version_id: version_map.iter().find_map(|(hash, version)| {
                        (hash_to_name.get(hash) == Some(filename))
                            .then(|| version.id.clone())
                            .flatten()
                    }),
                    version_number: version_map.iter().find_map(|(hash, version)| {
                        (hash_to_name.get(hash) == Some(filename))
                            .then(|| version.version_number.clone())
                            .flatten()
                    }),
                    hashes: hash_to_name.iter().find_map(|(hash, name)| {
                        (name == filename)
                            .then(|| HashMap::from([("sha1".to_string(), hash.clone())]))
                    }),
                    game_versions: version_map
                        .iter()
                        .find_map(|(hash, version)| {
                            (hash_to_name.get(hash) == Some(filename))
                                .then(|| version.game_versions.clone())
                        })
                        .unwrap_or_default(),
                    loaders: version_map
                        .iter()
                        .find_map(|(hash, version)| {
                            (hash_to_name.get(hash) == Some(filename))
                                .then(|| version.loaders.clone())
                        })
                        .unwrap_or_default(),
                },
            );
        }
    }

    save_meta_in(dir, &meta);
    list_mods_in(dir)
}
