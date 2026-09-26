use flate2::read::GzDecoder;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
};

use crate::core::profile;

const MAX_WORLD_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;
const MAX_WORLD_FILES: usize = 200_000;

#[derive(Debug, Default, Deserialize)]
struct LevelRoot {
    #[serde(rename = "Data")]
    data: LevelData,
}

#[derive(Debug, Default, Deserialize)]
struct LevelData {
    #[serde(rename = "LevelName")]
    level_name: Option<String>,
    #[serde(rename = "LastPlayed")]
    last_played: Option<i64>,
    #[serde(rename = "GameType")]
    game_type: Option<i32>,
    hardcore: Option<i8>,
    #[serde(rename = "Version")]
    version: Option<WorldVersion>,
}

#[derive(Debug, Deserialize)]
struct WorldVersion {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Id")]
    id: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSummary {
    pub id: String,
    pub name: String,
    pub last_played: Option<i64>,
    pub game_mode: Option<String>,
    pub hardcore: bool,
    pub version_name: Option<String>,
    pub data_version: Option<i32>,
    pub locked: bool,
    pub icon_available: bool,
}

pub fn saves_dir(root: &Path, profile_ref: &str) -> Result<PathBuf, String> {
    profile::validate_profile_ref(profile_ref)?;
    Ok(profile::profile_game_dir_for_ref(root, profile_ref)?.join("saves"))
}

pub fn world_dir(root: &Path, profile_ref: &str, world_id: &str) -> Result<PathBuf, String> {
    validate_world_id(world_id)?;
    let saves = saves_dir(root, profile_ref)?;
    let world = saves.join(world_id);
    if !world.join("level.dat").is_file() {
        return Err("world was not found".to_string());
    }
    let saves_canonical = saves
        .canonicalize()
        .map_err(|e| format!("failed to resolve saves directory: {e}"))?;
    let world_canonical = world
        .canonicalize()
        .map_err(|e| format!("failed to resolve world directory: {e}"))?;
    if !world_canonical.starts_with(saves_canonical) {
        return Err("world path escaped the profile".to_string());
    }
    Ok(world_canonical)
}

fn validate_world_id(world_id: &str) -> Result<(), String> {
    if world_id.is_empty()
        || world_id == "."
        || world_id == ".."
        || world_id.contains(['/', '\\', '\0'])
    {
        return Err("invalid world id".to_string());
    }
    Ok(())
}

pub async fn list(root: &Path, profile_ref: &str) -> Result<Vec<WorldSummary>, String> {
    let saves = saves_dir(root, profile_ref)?;
    if !saves.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(&saves)
        .await
        .map_err(|e| format!("failed to read saves directory: {e}"))?;
    let mut worlds = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("failed to read world entry: {e}"))?
    {
        let metadata = entry
            .metadata()
            .await
            .map_err(|e| format!("failed to inspect world entry: {e}"))?;
        if !metadata.is_dir() || !entry.path().join("level.dat").is_file() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let data = read_level_data(&entry.path().join("level.dat")).unwrap_or_default();
        worlds.push(WorldSummary {
            name: data
                .level_name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            last_played: data.last_played,
            game_mode: data.game_type.map(game_mode_name),
            hardcore: data.hardcore.unwrap_or_default() != 0,
            version_name: data
                .version
                .as_ref()
                .and_then(|version| version.name.clone()),
            data_version: data.version.and_then(|version| version.id),
            locked: is_locked(&entry.path()),
            icon_available: entry.path().join("icon.png").is_file(),
        });
    }
    worlds.sort_by(|a, b| {
        b.last_played
            .cmp(&a.last_played)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(worlds)
}

fn read_level_data(path: &Path) -> Result<LevelData, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("failed to open level.dat: {e}"))?;
    let decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder
        .take(32 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("failed to decompress level.dat: {e}"))?;
    let root: LevelRoot =
        fastnbt::from_bytes(&bytes).map_err(|e| format!("failed to parse level.dat: {e}"))?;
    Ok(root.data)
}

fn game_mode_name(value: i32) -> String {
    match value {
        0 => "Survival",
        1 => "Creative",
        2 => "Adventure",
        3 => "Spectator",
        _ => "Unknown",
    }
    .to_string()
}

pub fn ensure_not_locked(world: &Path) -> Result<(), String> {
    if is_locked(world) {
        Err("world is currently in use by Minecraft".to_string())
    } else {
        Ok(())
    }
}

fn is_locked(world: &Path) -> bool {
    let lock = world.join("session.lock");
    if !lock.exists() {
        return false;
    }
    let Ok(file) = OpenOptions::new().read(true).write(true).open(lock) else {
        return true;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(_) => true,
    }
}

pub async fn import_zip(
    root: &Path,
    profile_ref: &str,
    filename: &str,
    bytes: Vec<u8>,
) -> Result<String, String> {
    if bytes.is_empty() || bytes.len() > MAX_WORLD_ARCHIVE_BYTES {
        return Err("world archive is empty or exceeds 512 MiB".to_string());
    }
    let game_dir = profile::profile_game_dir_for_ref(root, profile_ref)?;
    let staging = game_dir
        .join(".hikyou")
        .join("imports")
        .join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(|e| format!("failed to create world staging directory: {e}"))?;
    let result = extract_world_archive(&bytes, &staging).and_then(|world_root| {
        let saves = game_dir.join("saves");
        std::fs::create_dir_all(&saves)
            .map_err(|e| format!("failed to create saves directory: {e}"))?;
        let base = sanitize_world_name(filename);
        let destination = unique_destination(&saves, &base);
        std::fs::rename(&world_root, &destination)
            .map_err(|e| format!("failed to commit imported world: {e}"))?;
        Ok(destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string())
    });
    let _ = tokio::fs::remove_dir_all(&staging).await;
    result
}

fn extract_world_archive(bytes: &[u8], staging: &Path) -> Result<PathBuf, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "invalid world ZIP archive".to_string())?;
    if archive.len() > MAX_WORLD_FILES {
        return Err("world archive contains too many files".to_string());
    }
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| "invalid ZIP entry".to_string())?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| "world archive contains an unsafe path".to_string())?
            .to_path_buf();
        expanded = expanded.saturating_add(file.size());
        if expanded > 16 * 1024 * 1024 * 1024 {
            return Err("world archive expands beyond 16 GiB".to_string());
        }
        let output = staging.join(enclosed);
        if file.is_dir() {
            std::fs::create_dir_all(&output)
                .map_err(|e| format!("failed to extract world: {e}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed to extract world: {e}"))?;
        }
        let mut target = std::fs::File::create(&output)
            .map_err(|e| format!("failed to extract world file: {e}"))?;
        std::io::copy(&mut file, &mut target)
            .map_err(|e| format!("failed to extract world file: {e}"))?;
    }
    find_world_root(staging)
}

fn find_world_root(staging: &Path) -> Result<PathBuf, String> {
    if staging.join("level.dat").is_file() {
        return Ok(staging.to_path_buf());
    }
    let candidates = std::fs::read_dir(staging)
        .map_err(|e| format!("failed to inspect imported world: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("level.dat").is_file())
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [world] => Ok(world.clone()),
        [] => Err("archive does not contain a Minecraft world".to_string()),
        _ => Err("archive contains multiple Minecraft worlds".to_string()),
    }
}

fn sanitize_world_name(filename: &str) -> String {
    let stem = filename.strip_suffix(".zip").unwrap_or(filename);
    let value = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ' ') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.trim().is_empty() {
        "Imported World".to_string()
    } else {
        value
    }
}

fn unique_destination(parent: &Path, base: &str) -> PathBuf {
    let direct = parent.join(base);
    if !direct.exists() {
        return direct;
    }
    for index in 2..10_000 {
        let candidate = parent.join(format!("{base} {index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(uuid::Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use super::{extract_world_archive, validate_world_id};
    use std::io::Write;

    #[test]
    fn world_ids_are_single_path_components() {
        assert!(validate_world_id("My World").is_ok());
        assert!(validate_world_id("../world").is_err());
        assert!(validate_world_id("folder/world").is_err());
    }

    #[test]
    fn world_archive_rejects_parent_traversal() {
        let outside_name = format!("hikyou-outside-{}.txt", uuid::Uuid::new_v4());
        let mut bytes = Vec::new();
        {
            let mut archive = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            archive
                .start_file::<_, ()>(
                    format!("../{outside_name}"),
                    zip::write::FileOptions::default(),
                )
                .unwrap();
            archive.write_all(b"outside").unwrap();
            archive.finish().unwrap();
        }
        let staging = std::env::temp_dir().join(format!("hikyou-world-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&staging).unwrap();
        assert!(extract_world_archive(&bytes, &staging).is_err());
        assert!(!staging.parent().unwrap().join(outside_name).exists());
        std::fs::remove_dir_all(staging).unwrap();
    }
}
