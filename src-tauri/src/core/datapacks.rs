use serde::Serialize;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::core::worlds;

const MAX_DATAPACK_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatapackItem {
    pub name: String,
    pub size_bytes: u64,
    pub directory: bool,
}

pub async fn list(
    root: &Path,
    profile_ref: &str,
    world_id: &str,
) -> Result<Vec<DatapackItem>, String> {
    let dir = worlds::world_dir(root, profile_ref, world_id)?.join("datapacks");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("failed to read datapacks: {e}"))?;
    let mut items = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("failed to read datapack entry: {e}"))?
    {
        let metadata = entry
            .metadata()
            .await
            .map_err(|e| format!("failed to inspect datapack: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || (!metadata.is_dir() && !name.to_ascii_lowercase().ends_with(".zip"))
        {
            continue;
        }
        items.push(DatapackItem {
            name,
            size_bytes: metadata.len(),
            directory: metadata.is_dir(),
        });
    }
    items.sort_by_key(|item| item.name.to_lowercase());
    Ok(items)
}

pub async fn import(
    root: &Path,
    profile_ref: &str,
    world_id: &str,
    filename: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let filename = safe_name(filename)?;
    if !filename.to_ascii_lowercase().ends_with(".zip") {
        return Err("datapacks must be ZIP files".to_string());
    }
    if bytes.is_empty() || bytes.len() > MAX_DATAPACK_BYTES {
        return Err("datapack is empty or exceeds 256 MiB".to_string());
    }
    validate_datapack(&bytes)?;
    let world = worlds::world_dir(root, profile_ref, world_id)?;
    worlds::ensure_not_locked(&world)?;
    let dir = world.join("datapacks");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed to create datapacks directory: {e}"))?;
    let destination = dir.join(filename);
    if destination.exists() {
        return Err("a datapack with this name already exists".to_string());
    }
    create_checkpoint(&world)?;
    let temporary = dir.join(format!(".{filename}.{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|e| format!("failed to stage datapack: {e}"))?;
    tokio::fs::rename(&temporary, destination)
        .await
        .map_err(|e| format!("failed to commit datapack: {e}"))
}

pub async fn remove(
    root: &Path,
    profile_ref: &str,
    world_id: &str,
    name: &str,
) -> Result<(), String> {
    let name = safe_name(name)?;
    let world = worlds::world_dir(root, profile_ref, world_id)?;
    worlds::ensure_not_locked(&world)?;
    let source = world.join("datapacks").join(name);
    if !source.exists() {
        return Err("datapack was not found".to_string());
    }
    create_checkpoint(&world)?;
    let trash = world.join(".hikyou").join("trash").join("datapacks");
    tokio::fs::create_dir_all(&trash)
        .await
        .map_err(|e| format!("failed to create datapack trash: {e}"))?;
    let destination = trash.join(format!("{}-{name}", chrono::Utc::now().timestamp_millis()));
    tokio::fs::rename(source, destination)
        .await
        .map_err(|e| format!("failed to move datapack to trash: {e}"))
}

fn safe_name(name: &str) -> Result<&str, String> {
    let value = name.trim();
    if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\\', '\0']) {
        return Err("invalid datapack filename".to_string());
    }
    Ok(value)
}

fn validate_datapack(bytes: &[u8]) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "invalid datapack ZIP".to_string())?;
    if archive.len() > 50_000 {
        return Err("datapack contains too many files".to_string());
    }
    let mut has_metadata = false;
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| "invalid datapack entry".to_string())?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| "datapack contains an unsafe path".to_string())?;
        if enclosed == Path::new("pack.mcmeta") {
            let mut metadata = String::new();
            file.by_ref()
                .take(1024 * 1024)
                .read_to_string(&mut metadata)
                .map_err(|_| "pack.mcmeta is not valid UTF-8".to_string())?;
            serde_json::from_str::<serde_json::Value>(&metadata)
                .map_err(|_| "pack.mcmeta is not valid JSON".to_string())?;
            has_metadata = true;
        }
        expanded = expanded.saturating_add(file.size());
        if expanded > 2 * 1024 * 1024 * 1024 {
            return Err("datapack expands beyond 2 GiB".to_string());
        }
    }
    if !has_metadata {
        return Err("datapack does not contain pack.mcmeta at its root".to_string());
    }
    Ok(())
}

fn create_checkpoint(world: &Path) -> Result<PathBuf, String> {
    let checkpoints = world.join(".hikyou").join("checkpoints");
    std::fs::create_dir_all(&checkpoints)
        .map_err(|e| format!("failed to create checkpoint directory: {e}"))?;
    let path = checkpoints.join(format!(
        "{}.zip",
        chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f")
    ));
    let output = std::fs::File::create(&path)
        .map_err(|e| format!("failed to create world checkpoint: {e}"))?;
    let mut zip = zip::ZipWriter::new(output);
    let mut budget = CheckpointBudget::default();
    if world.join("level.dat").is_file() {
        add_file(&mut zip, &world.join("level.dat"), "level.dat", &mut budget)?;
    }
    let datapacks = world.join("datapacks");
    if datapacks.is_dir() {
        add_tree(&mut zip, &datapacks, &datapacks, "datapacks", &mut budget)?;
    }
    zip.finish()
        .map_err(|e| format!("failed to finish world checkpoint: {e}"))?;
    Ok(path)
}

#[derive(Default)]
struct CheckpointBudget {
    files: usize,
    bytes: u64,
}

fn add_tree(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    current: &Path,
    prefix: &str,
    budget: &mut CheckpointBudget,
) -> Result<(), String> {
    for entry in
        std::fs::read_dir(current).map_err(|e| format!("failed to read checkpoint input: {e}"))?
    {
        let entry = entry.map_err(|e| format!("failed to read checkpoint input: {e}"))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "invalid checkpoint path".to_string())?;
        let name = format!("{prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("failed to inspect checkpoint input: {e}"))?;
        if metadata.file_type().is_symlink() {
            return Err("datapack checkpoint contains a symbolic link".to_string());
        }
        if metadata.is_dir() {
            add_tree(zip, root, &path, prefix, budget)?;
        } else if metadata.is_file() {
            add_file(zip, &path, &name, budget)?;
        }
    }
    Ok(())
}

fn add_file(
    zip: &mut zip::ZipWriter<std::fs::File>,
    source: &Path,
    name: &str,
    budget: &mut CheckpointBudget,
) -> Result<(), String> {
    let size = std::fs::metadata(source)
        .map_err(|e| format!("failed to inspect checkpoint input: {e}"))?
        .len();
    budget.files = budget.files.saturating_add(1);
    budget.bytes = budget.bytes.saturating_add(size);
    if budget.files > 50_000 || budget.bytes > 2 * 1024 * 1024 * 1024 {
        return Err("datapack checkpoint exceeds its safety limit".to_string());
    }
    zip.start_file::<_, ()>(name, zip::write::FileOptions::default())
        .map_err(|e| format!("failed to add checkpoint file: {e}"))?;
    let mut file =
        std::fs::File::open(source).map_err(|e| format!("failed to open checkpoint input: {e}"))?;
    std::io::copy(&mut file, zip).map_err(|e| format!("failed to write checkpoint file: {e}"))?;
    zip.flush()
        .map_err(|e| format!("failed to flush checkpoint: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{safe_name, validate_datapack};
    use std::io::Write;

    #[test]
    fn datapack_names_are_single_components() {
        assert!(safe_name("example.zip").is_ok());
        assert!(safe_name("../example.zip").is_err());
    }

    #[test]
    fn datapack_metadata_must_be_valid_and_at_root() {
        fn archive(name: &str, body: &[u8]) -> Vec<u8> {
            let mut bytes = Vec::new();
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            zip.start_file::<_, ()>(name, zip::write::FileOptions::default())
                .unwrap();
            zip.write_all(body).unwrap();
            zip.finish().unwrap();
            bytes
        }

        assert!(
            validate_datapack(&archive("pack.mcmeta", br#"{"pack":{"pack_format":1}}"#)).is_ok()
        );
        assert!(validate_datapack(&archive("nested/pack.mcmeta", br#"{"pack":{}}"#)).is_err());
        assert!(validate_datapack(&archive("pack.mcmeta", b"not json")).is_err());
    }
}
