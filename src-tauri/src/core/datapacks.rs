use serde::Serialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

use crate::core::{archive, managed_fs, profile, worlds};
use cap_std::fs::Dir;

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
    let (world, _) = open_world(root, profile_ref, world_id)?;
    if managed_fs::inspect(&world, Path::new("datapacks"))?.is_none() {
        return Ok(Vec::new());
    }
    let dir = managed_fs::directory(&world, Path::new("datapacks"), false)?;
    let entries = dir
        .entries()
        .map_err(|e| format!("failed to read datapacks: {e}"))?;
    let mut items = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to read datapack entry: {e}"))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "datapack name is not UTF-8")?;
        let metadata =
            managed_fs::inspect(&dir, Path::new(&name))?.ok_or("datapack disappeared")?;
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
    let (world, path) = open_world(root, profile_ref, world_id)?;
    worlds::ensure_not_locked(&path)?;
    let destination = Path::new("datapacks").join(filename);
    if managed_fs::inspect(&world, &destination)?.is_some() {
        return Err("a datapack with this name already exists".to_string());
    }
    create_checkpoint(&world)?;
    managed_fs::write(&world, &destination, &bytes, false)
}

pub async fn remove(
    root: &Path,
    profile_ref: &str,
    world_id: &str,
    name: &str,
) -> Result<(), String> {
    let name = safe_name(name)?;
    let (world, path) = open_world(root, profile_ref, world_id)?;
    worlds::ensure_not_locked(&path)?;
    let source = Path::new("datapacks").join(name);
    if managed_fs::inspect(&world, &source)?.is_none() {
        return Err("datapack was not found".to_string());
    }
    let trash_path = Path::new(".hikyou/trash/datapacks");
    managed_fs::inspect(&world, trash_path)?;
    create_checkpoint(&world)?;
    let datapacks = managed_fs::directory(&world, Path::new("datapacks"), false)?;
    let trash = managed_fs::directory(&world, trash_path, true)?;
    datapacks
        .rename(name, &trash, format!("{}-{name}", uuid::Uuid::new_v4()))
        .map_err(|e| format!("failed to move datapack to trash: {e}"))
}

fn open_world(root: &Path, profile_ref: &str, world_id: &str) -> Result<(Dir, PathBuf), String> {
    worlds::validate_world_id(world_id)?;
    let game = profile::profile_game_dir_for_ref(root, profile_ref)?;
    let relative = Path::new("saves").join(world_id);
    let world = managed_fs::directory(&managed_fs::root(&game)?, &relative, false)?;
    if !managed_fs::inspect(&world, Path::new("level.dat"))?.is_some_and(|m| m.is_file()) {
        return Err("world was not found".to_string());
    }
    Ok((world, game.join(relative)))
}

fn safe_name(name: &str) -> Result<&str, String> {
    let value = name.trim();
    if !crate::core::paths::is_safe_component(value) {
        return Err("invalid datapack filename".to_string());
    }
    Ok(value)
}

fn validate_datapack(bytes: &[u8]) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "invalid datapack ZIP".to_string())?;
    archive::preflight_zip(&mut archive, 50_000, 2 * 1024 * 1024 * 1024)?;
    let mut has_metadata = false;
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
    }
    if !has_metadata {
        return Err("datapack does not contain pack.mcmeta at its root".to_string());
    }
    Ok(())
}

fn create_checkpoint(world: &Dir) -> Result<PathBuf, String> {
    let mut inputs = Vec::new();
    let mut budget = CheckpointBudget::default();
    if managed_fs::inspect(world, Path::new("level.dat"))?.is_some() {
        collect_file(world, Path::new("level.dat"), &mut budget, &mut inputs)?;
    }
    if managed_fs::inspect(world, Path::new("datapacks"))?.is_some() {
        collect_tree(world, Path::new("datapacks"), &mut budget, &mut inputs)?;
    }
    let path = Path::new(".hikyou/checkpoints").join(format!(
        "{}-{}.zip",
        chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f"),
        uuid::Uuid::new_v4()
    ));
    managed_fs::write_with(world, &path, false, |output| {
        let mut zip = zip::ZipWriter::new(output);
        for (path, size) in &inputs {
            let mut input = managed_fs::read_file(world, path)?.take(size + 1);
            let name = path
                .to_str()
                .ok_or("checkpoint path is not UTF-8")?
                .replace('\\', "/");
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .map_err(|e| format!("failed to add checkpoint file: {e}"))?;
            let actual = std::io::copy(&mut input, &mut zip)
                .map_err(|e| format!("failed to copy checkpoint input: {e}"))?;
            if actual != *size {
                return Err("checkpoint input changed size during backup".to_string());
            }
        }
        zip.finish()
            .map_err(|e| format!("failed to finish world checkpoint: {e}"))?;
        Ok(())
    })?;
    Ok(path)
}

#[derive(Default)]
struct CheckpointBudget {
    files: usize,
    bytes: u64,
}

fn collect_tree(
    world: &Dir,
    current: &Path,
    budget: &mut CheckpointBudget,
    inputs: &mut Vec<(PathBuf, u64)>,
) -> Result<(), String> {
    if current.components().count() > 128 {
        return Err("datapack checkpoint directory nesting is too deep".to_string());
    }
    let directory = managed_fs::directory(world, current, false)?;
    for entry in directory
        .entries()
        .map_err(|e| format!("failed to read checkpoint input: {e}"))?
    {
        let entry = entry.map_err(|e| format!("failed to read checkpoint input: {e}"))?;
        let path = current.join(entry.file_name());
        let metadata = managed_fs::inspect(world, &path)?.ok_or("checkpoint input disappeared")?;
        if metadata.is_dir() {
            collect_tree(world, &path, budget, inputs)?;
        } else if metadata.is_file() {
            collect_file(world, &path, budget, inputs)?;
        } else {
            return Err("checkpoint input is not a regular file or directory".to_string());
        }
    }
    Ok(())
}

fn collect_file(
    world: &Dir,
    source: &Path,
    budget: &mut CheckpointBudget,
    inputs: &mut Vec<(PathBuf, u64)>,
) -> Result<(), String> {
    let size = managed_fs::read_file(world, source)?
        .metadata()
        .map_err(|e| format!("failed to inspect checkpoint input: {e}"))?
        .len();
    budget.files = budget.files.saturating_add(1);
    budget.bytes = budget.bytes.saturating_add(size);
    if budget.files > 50_000 || budget.bytes > 2 * 1024 * 1024 * 1024 {
        return Err("datapack checkpoint exceeds its safety limit".to_string());
    }
    inputs.push((source.to_path_buf(), size));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{create_checkpoint, safe_name, validate_datapack};
    use std::{
        io::{Read, Write},
        path::{Path, PathBuf},
    };

    fn world_fixture(root: &Path) -> PathBuf {
        let game =
            crate::core::profile::profile_game_dir_for_ref(root, "smart:latest-plus").unwrap();
        let world = game.join("saves/world");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), b"world metadata").unwrap();
        world
    }

    fn valid_pack() -> Vec<u8> {
        crate::core::test_support::zip(
            &[("pack.mcmeta", br#"{"pack":{"pack_format":1}}"#)],
            zip::CompressionMethod::Deflated,
        )
    }

    #[tokio::test]
    async fn normal_datapack_roundtrip_keeps_complete_checkpoints() {
        let temp = tempfile::tempdir().unwrap();
        let world = world_fixture(temp.path());
        let bytes = valid_pack();
        super::import(
            temp.path(),
            "smart:latest-plus",
            "world",
            "pack.zip",
            bytes.clone(),
        )
        .await
        .unwrap();
        assert_eq!(
            super::list(temp.path(), "smart:latest-plus", "world")
                .await
                .unwrap()[0]
                .name,
            "pack.zip"
        );
        super::remove(temp.path(), "smart:latest-plus", "world", "pack.zip")
            .await
            .unwrap();
        assert!(
            super::list(temp.path(), "smart:latest-plus", "world")
                .await
                .unwrap()
                .is_empty()
        );
        let checkpoints = std::fs::read_dir(world.join(".hikyou/checkpoints"))
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(checkpoints.len(), 2);
        let mut found_pack = false;
        for checkpoint in checkpoints {
            let mut archive =
                zip::ZipArchive::new(std::fs::File::open(checkpoint.unwrap().path()).unwrap())
                    .unwrap();
            assert!(archive.by_name("level.dat").is_ok());
            if let Ok(mut file) = archive.by_name("datapacks/pack.zip") {
                let mut stored = Vec::new();
                file.read_to_end(&mut stored).unwrap();
                assert_eq!(stored, bytes);
                found_pack = true;
            }
        }
        assert!(found_pack);
    }

    #[tokio::test]
    async fn datapack_boundary_blocks_list_import_and_remove_through_root_link() {
        let temp = tempfile::tempdir().unwrap();
        let world = world_fixture(temp.path());
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("keep.zip"), b"outside").unwrap();
        crate::core::test_support::directory_link(&outside, &world.join("datapacks"));
        assert!(
            super::list(temp.path(), "smart:latest-plus", "world")
                .await
                .is_err()
        );
        assert!(
            super::import(
                temp.path(),
                "smart:latest-plus",
                "world",
                "new.zip",
                valid_pack()
            )
            .await
            .is_err()
        );
        assert!(
            super::remove(temp.path(), "smart:latest-plus", "world", "keep.zip")
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(outside.join("keep.zip")).unwrap(), b"outside");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
        assert!(!world.join(".hikyou").exists());
    }

    #[tokio::test]
    async fn datapack_boundary_blocks_checkpoint_and_trash_directory_links() {
        for boundary in [".hikyou", ".hikyou/checkpoints", ".hikyou/trash"] {
            let temp = tempfile::tempdir().unwrap();
            let world = world_fixture(temp.path());
            let outside = temp.path().join("outside");
            std::fs::create_dir(&outside).unwrap();
            if boundary == ".hikyou/trash" {
                super::import(
                    temp.path(),
                    "smart:latest-plus",
                    "world",
                    "pack.zip",
                    valid_pack(),
                )
                .await
                .unwrap();
            }
            let link = world.join(boundary);
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            crate::core::test_support::directory_link(&outside, &link);
            if boundary == ".hikyou/trash" {
                assert!(
                    super::remove(temp.path(), "smart:latest-plus", "world", "pack.zip")
                        .await
                        .is_err()
                );
                assert!(world.join("datapacks/pack.zip").is_file());
            } else {
                assert!(
                    super::import(
                        temp.path(),
                        "smart:latest-plus",
                        "world",
                        "pack.zip",
                        valid_pack()
                    )
                    .await
                    .is_err()
                );
                assert!(!world.join("datapacks/pack.zip").exists());
            }
            assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        }
    }

    #[tokio::test]
    async fn unsupported_datapack_is_rejected_before_any_world_change() {
        let temp = tempfile::tempdir().unwrap();
        let world = world_fixture(temp.path());
        let bytes = crate::core::test_support::unsupported_zip(&[
            ("pack.mcmeta", br#"{"pack":{}}"#),
            ("data/content", b"unsupported"),
        ]);
        let error = super::import(temp.path(), "smart:latest-plus", "world", "pack.zip", bytes)
            .await
            .unwrap_err();
        assert!(error.contains("Deflate") && error.contains("data/content"));
        assert_eq!(std::fs::read_dir(&world).unwrap().count(), 1);
    }

    #[test]
    fn datapack_boundary_rejects_linked_checkpoint_input() {
        let temp = tempfile::tempdir().unwrap();
        let world = temp.path().join("world");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&world).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(world.join("level.dat"), b"world").unwrap();
        std::fs::write(outside.join("secret.txt"), b"outside content").unwrap();
        crate::core::test_support::directory_link(&outside, &world.join("datapacks"));
        let result = create_checkpoint(&crate::core::managed_fs::root(&world).unwrap());
        assert!(
            result.is_err(),
            "checkpoint followed external datapacks directory: {result:?}"
        );
    }

    #[test]
    fn datapack_names_are_single_components() {
        assert!(safe_name("example.zip").is_ok());
        assert!(safe_name("../example.zip").is_err());
        assert!(safe_name("C:example.zip").is_err());
        assert!(safe_name("pack.zip:stream.zip").is_err());
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
