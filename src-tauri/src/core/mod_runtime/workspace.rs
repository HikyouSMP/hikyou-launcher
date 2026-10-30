//! Confined working copies and recoverable checkout, separate from object identity.

use super::super::{
    managed_fs,
    mod_files::MAX_MOD_BYTES,
    mod_repository::{ModRepository, ModRevision},
    paths::LauncherPaths,
};
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

const JOURNAL: &str = ".hikyou-mod-checkout.json";

fn artifact(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".jar") || name.ends_with(".disabled")
}

fn foreign(name: &str) -> bool {
    !artifact(name) && name != ".mod_meta.json"
}

fn entries(dir: &Dir) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for entry in dir.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Mod filenames must be UTF-8")?;
        managed_fs::relative_path(&name)?;
        // Inspect even ignored entries so an unexpected link never becomes an input later.
        if let Some(metadata) = managed_fs::inspect(dir, Path::new(&name))? {
            reject_reparse(&metadata)?;
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

fn copy_file(source: &Dir, destination: &Dir, name: &str) -> Result<(), String> {
    let mut input = managed_fs::read_file(source, Path::new(name))?;
    if input.metadata().map_err(|e| e.to_string())?.len() > MAX_MOD_BYTES {
        return Err("Mod input exceeds the supported size limit".to_string());
    }
    managed_fs::write_with(destination, Path::new(name), false, |output| {
        let count = std::io::copy(&mut input.by_ref().take(MAX_MOD_BYTES + 1), output)
            .map_err(|e| e.to_string())?;
        if count > MAX_MOD_BYTES {
            return Err("Mod input grew beyond the supported size limit".to_string());
        }
        Ok(())
    })
}

fn mods(game: &Path, create: bool) -> Result<Option<Dir>, String> {
    let root = if create {
        managed_fs::root_or_create(game)?
    } else {
        let metadata = match std::fs::symlink_metadata(game) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        reject_reparse(&cap_std::fs::Metadata::from_just_metadata(metadata))?;
        managed_fs::root(game)?
    };
    match managed_fs::inspect(&root, Path::new("mods"))? {
        Some(metadata) => reject_reparse(&metadata)?,
        None if !create => return Ok(None),
        None => {}
    }
    managed_fs::directory(&root, Path::new("mods"), create).map(Some)
}

pub(super) fn source_present(game: &Path) -> Result<bool, String> {
    Ok(mods(game, false)?.is_some())
}

fn reject_reparse(metadata: &cap_std::fs::Metadata) -> Result<(), String> {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("linked/reparse Mod paths are unsupported".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("linked Mod paths are unsupported".into());
    }
    Ok(())
}

fn foreign_files(dir: &Dir) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for name in entries(dir)? {
        if !foreign(&name) {
            continue;
        }
        // Reject directories and hard links as well as path-based links.
        managed_fs::read_file(dir, Path::new(&name))?;
        files.push(name);
    }
    Ok(files)
}

fn transfer_foreign(source: &Dir, destination: &Dir) -> Result<(), String> {
    let names = foreign_files(source)?;
    for name in &names {
        if managed_fs::inspect(destination, Path::new(name))?.is_some() {
            return Err(format!(
                "Foreign Mod-folder file collision; both copies retained: {name}"
            ));
        }
    }
    for name in names {
        rename_exclusive(source, &name, destination)?;
    }
    Ok(())
}

#[cfg(unix)]
fn rename_exclusive(source: &Dir, name: &str, destination: &Dir) -> Result<(), String> {
    rename_entry_exclusive(source, name, destination, name)
}

#[cfg(unix)]
fn rename_entry_exclusive(
    source: &Dir,
    name: &str,
    destination: &Dir,
    new_name: &str,
) -> Result<(), String> {
    rustix::fs::renameat_with(
        source,
        name,
        destination,
        new_name,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|e| format!("Foreign file move failed; recovery data retained: {e}"))
}

#[cfg(windows)]
fn rename_exclusive(source: &Dir, name: &str, destination: &Dir) -> Result<(), String> {
    rename_entry_exclusive(source, name, destination, name)
}

#[cfg(windows)]
fn rename_entry_exclusive(
    source: &Dir,
    name: &str,
    destination: &Dir,
    new_name: &str,
) -> Result<(), String> {
    use std::os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::AsRawHandle,
    };
    use windows::{
        Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, MOVE_FILE_FLAGS, MoveFileExW,
            },
        },
        core::PCWSTR,
    };
    let held_path = |dir: &Dir| -> Result<std::path::PathBuf, String> {
        let mut buffer = vec![0; 512];
        loop {
            let length = unsafe {
                GetFinalPathNameByHandleW(
                    HANDLE(dir.as_raw_handle()),
                    &mut buffer,
                    FILE_NAME_NORMALIZED,
                )
            } as usize;
            if length == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            if length < buffer.len() {
                return Ok(std::path::PathBuf::from(std::ffi::OsString::from_wide(
                    &buffer[..length],
                )));
            }
            if length > 32768 {
                return Err("Mod directory path is too long".into());
            }
            buffer.resize(length + 1, 0);
        }
    };
    // These non-delete-sharing directory handles remain alive through the move.
    let from: Vec<u16> = held_path(source)?
        .join(name)
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let to: Vec<u16> = held_path(destination)?
        .join(new_name)
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .map_err(|e| format!("Foreign file move failed; recovery data retained: {e}"))
}

/// A change detector, not an integrity proof; deep verification lives in the repository.
pub(super) fn fingerprint(game: &Path) -> Result<String, String> {
    let mut hash = Sha256::new();
    if let Some(dir) = mods(game, false)? {
        for name in entries(&dir)? {
            if !artifact(&name) && name != ".mod_meta.json" {
                continue;
            }
            let meta =
                managed_fs::inspect(&dir, Path::new(&name))?.ok_or("Mod input disappeared")?;
            if !meta.is_file() {
                return Err("Mod inputs must be ordinary files".to_string());
            }
            let modified = meta
                .modified()
                .map_err(|e| e.to_string())?
                .into_std()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update(meta.len().to_le_bytes());
            hash.update(modified.to_le_bytes());
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(super) fn stage(paths: &LauncherPaths, game: &Path) -> Result<tempfile::TempDir, String> {
    let _authority = managed_fs::directory(
        &managed_fs::root(&paths.root())?,
        Path::new("state/mod-maintenance/work"),
        true,
    )?;
    let working = tempfile::Builder::new()
        .prefix("prepare-")
        .tempdir_in(paths.state_dir().join("mod-maintenance/work"))
        .map_err(|e| e.to_string())?;
    let destination = mods(working.path(), true)?.ok_or("failed to create Mod workspace")?;
    if let Some(source) = mods(game, false)? {
        for name in entries(&source)? {
            if artifact(&name) || name == ".mod_meta.json" {
                copy_file(&source, &destination, &name)?;
            }
        }
    }
    Ok(working)
}

#[derive(Serialize, Deserialize)]
struct Checkout {
    revision: String,
    previous_head: Option<String>,
    backup: String,
    stage: String,
    committed: bool,
    #[serde(default)]
    original_absent: bool,
}

struct CheckoutStaging {
    root: Dir,
    name: String,
    journaled: bool,
}

impl Drop for CheckoutStaging {
    fn drop(&mut self) {
        if !self.journaled {
            let _ = cleanup(&self.root, &self.name);
        }
    }
}

fn cleanup(root: &Dir, name: &str) -> Result<(), String> {
    if let Some(meta) = managed_fs::inspect(root, Path::new(name))? {
        if !meta.is_dir() {
            return Err("checkout recovery found a non-directory".to_string());
        }
        root.remove_dir_all(name).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn optional_dir(root: &Dir, name: &str) -> Result<Option<Dir>, String> {
    let Some(metadata) = managed_fs::inspect(root, Path::new(name))? else {
        return Ok(None);
    };
    reject_reparse(&metadata)?;
    managed_fs::directory(root, Path::new(name), false).map(Some)
}

// Never recursively delete a directory that may receive late foreign writes.
// A late entry makes remove_dir fail, retaining it and the recovery journal.
fn remove_artifact_directory(root: &Dir, name: &str) -> Result<(), String> {
    let Some(dir) = optional_dir(root, name)? else {
        return Ok(());
    };
    for entry in entries(&dir)? {
        if foreign(&entry) {
            return Err("Foreign files remain in checkout recovery; journal retained".into());
        }
        managed_fs::read_file(&dir, Path::new(&entry))?;
        dir.remove_file(&entry).map_err(|e| e.to_string())?;
    }
    drop(dir);
    root.remove_dir(name)
        .map_err(|e| format!("Checkout cleanup incomplete; journal retained: {e}"))
}

fn cleanup_stage(root: &Dir, name: &str) -> Result<(), String> {
    let Some(stage) = optional_dir(root, name)? else {
        return Ok(());
    };
    remove_artifact_directory(&stage, "mods")?;
    drop(stage);
    root.remove_dir(name)
        .map_err(|e| format!("Checkout staging cleanup incomplete: {e}"))
}

pub(super) fn recover(repo: &ModRepository, id: &str, game: &Path) -> Result<(), String> {
    if !game.try_exists().map_err(|e| e.to_string())? {
        return Ok(());
    }
    let root = managed_fs::root(game)?;
    if managed_fs::inspect(&root, Path::new(JOURNAL))?.is_none() {
        return Ok(());
    }
    let journal: Checkout =
        serde_json::from_reader(managed_fs::read_file(&root, Path::new(JOURNAL))?.take(16 * 1024))
            .map_err(|e| format!("invalid Mod checkout journal: {e}"))?;
    for (name, prefix) in [
        (&journal.backup, ".hikyou-mod-backup-"),
        (&journal.stage, ".hikyou-mod-stage-"),
    ] {
        managed_fs::relative_path(name)?;
        if name.contains('/') || !name.starts_with(prefix) {
            return Err("invalid Mod recovery directory".to_string());
        }
    }
    let head = repo.head(id)?.map(|head| head.id);
    let committed = journal.committed
        || (head.as_deref() == Some(&journal.revision) && head != journal.previous_head);
    let backup = optional_dir(&root, &journal.backup)?;
    let stage = optional_dir(&root, &journal.stage)?;
    let staged_mods = stage
        .as_ref()
        .map(|dir| optional_dir(dir, "mods"))
        .transpose()?
        .flatten();
    let active = optional_dir(&root, "mods")?;
    if committed {
        let active = active.as_ref().ok_or("committed Mod checkout is missing")?;
        if let Some(backup) = &backup {
            transfer_foreign(backup, active)?;
        }
        if let Some(staged) = &staged_mods {
            transfer_foreign(staged, active)?;
        }
    } else if let Some(backup) = &backup {
        if let Some(active) = &active {
            transfer_foreign(active, backup)?;
        }
        if let Some(staged) = &staged_mods {
            transfer_foreign(staged, backup)?;
        }
    } else if let Some(staged) = &staged_mods
        && !foreign_files(staged)?.is_empty()
    {
        let destination = managed_fs::directory(&root, Path::new("mods"), true)?;
        transfer_foreign(staged, &destination)?;
    }
    drop(active);
    drop(staged_mods);
    drop(stage);
    let had_backup = backup.is_some();
    drop(backup);
    if committed {
        remove_artifact_directory(&root, &journal.backup)?;
    } else if had_backup {
        remove_artifact_directory(&root, "mods")?;
        rename_entry_exclusive(&root, &journal.backup, &root, "mods")
            .map_err(|e| format!("Mod checkout recovery failed: {e}"))?;
    } else if journal.original_absent {
        // Preserve newly created foreign files even when no original mods folder existed.
        if let Some(active) = optional_dir(&root, "mods")? {
            for name in entries(&active)? {
                if !foreign(&name) {
                    active.remove_file(name).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    cleanup_stage(&root, &journal.stage)?;
    root.remove_file(JOURNAL).map_err(|e| e.to_string())
}

/// Unknown files survive checkout. Unknown subdirectories require manual review, not deletion.
pub(super) fn checkout(
    repo: &ModRepository,
    revision: &ModRevision,
    generation: &Path,
    game: &Path,
) -> Result<(), String> {
    let root = managed_fs::root_or_create(game)?;
    recover(repo, &revision.profile_id, game)?;
    let stage_name = format!(".hikyou-mod-stage-{}", uuid::Uuid::new_v4());
    let cleanup_root = root.try_clone().map_err(|error| error.to_string())?;
    root.create_dir(&stage_name)
        .map_err(|error| error.to_string())?;
    let mut staging = CheckoutStaging {
        root: cleanup_root,
        name: stage_name.clone(),
        journaled: false,
    };
    let stage = managed_fs::directory(&root, Path::new(&stage_name), false)?;
    let destination = managed_fs::directory(&stage, Path::new("mods"), true)?;
    if let Some(original) = mods(game, false)? {
        for name in entries(&original)? {
            if artifact(&name) || name == ".mod_meta.json" {
                continue;
            }
            let metadata =
                managed_fs::inspect(&original, Path::new(&name))?.ok_or("Mod folder changed")?;
            if !metadata.is_file() {
                return Err("Mod history restore does not replace folders inside mods; move them aside first".to_string());
            }
            managed_fs::read_file(&original, Path::new(&name))?;
        }
    }
    let source = managed_fs::root(generation)?;
    for name in entries(&source)? {
        if artifact(&name) {
            copy_file(&source, &destination, &name)?;
        }
    }
    let metadata: std::collections::BTreeMap<_, _> = revision.mods.iter().map(|entry| (entry.filename.clone(), serde_json::json!({
        "display_name": entry.display_name.clone().unwrap_or_else(|| entry.filename.clone()), "icon_url": entry.icon_url, "project_id": entry.project_id.clone().unwrap_or_default(), "version_number": entry.provider_version_number.clone().or(entry.version.clone()), "version_id": entry.provider_version, "hashes": entry.hashes,
    }))).collect();
    managed_fs::write(
        &destination,
        Path::new(".mod_meta.json"),
        &serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
        false,
    )?;
    drop(destination);
    let mut journal = Checkout {
        revision: revision.id.clone(),
        previous_head: repo.head(&revision.profile_id)?.map(|head| head.id),
        backup: format!(".hikyou-mod-backup-{}", uuid::Uuid::new_v4()),
        stage: stage_name,
        committed: false,
        original_absent: managed_fs::inspect(&root, Path::new("mods"))?.is_none(),
    };
    managed_fs::write(
        &root,
        Path::new(JOURNAL),
        &serde_json::to_vec(&journal).map_err(|e| e.to_string())?,
        false,
    )?;
    staging.journaled = true;
    let result = (|| {
        if managed_fs::inspect(&root, Path::new("mods"))?.is_some() {
            rename_entry_exclusive(&root, "mods", &root, &journal.backup)
                .map_err(|e| format!("Failed to move original Mod folder into backup: {e}"))?;
            let backup = managed_fs::directory(&root, Path::new(&journal.backup), false)?;
            let destination = managed_fs::directory(&stage, Path::new("mods"), false)?;
            transfer_foreign(&backup, &destination)?;
            drop(destination);
            drop(backup);
        }
        rename_entry_exclusive(&stage, "mods", &root, "mods")
            .map_err(|e| format!("Failed to activate staged Mod folder: {e}"))?;
        repo.publish(&revision.profile_id, &revision.id)?;
        journal.committed = true;
        managed_fs::write(
            &root,
            Path::new(JOURNAL),
            &serde_json::to_vec(&journal).map_err(|e| e.to_string())?,
            true,
        )?;
        Ok(())
    })();
    drop(stage);
    let recovery = recover(repo, &revision.profile_id, game);
    match (result, recovery) {
        (Err(error), Err(recovery)) => Err(format!("{error}; recovery pending: {recovery}")),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), result) => result,
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod foreign_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_copy_is_independent_and_change_detection_uses_nanoseconds() {
        let directory = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(directory.path().to_path_buf());
        paths.ensure_dirs().unwrap();
        let game = directory.path().join("game");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/local.jar"), b"original").unwrap();
        let before = fingerprint(&game).unwrap();
        let copy = stage(&paths, &game).unwrap();
        std::fs::write(copy.path().join("mods/local.jar"), b"replacement").unwrap();
        assert_eq!(
            std::fs::read(game.join("mods/local.jar")).unwrap(),
            b"original"
        );
        assert_eq!(fingerprint(&game).unwrap(), before);
        std::fs::write(game.join("mods/local.jar"), b"changed").unwrap();
        assert_ne!(fingerprint(&game).unwrap(), before);
    }

    #[test]
    fn linked_mod_folder_is_not_imported() {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(directory.path().to_path_buf());
        paths.ensure_dirs().unwrap();
        let game = directory.path().join("game");
        std::fs::create_dir(&game).unwrap();
        crate::core::test_support::directory_link(outside.path(), &game.join("mods"));
        assert!(fingerprint(&game).is_err());
        assert!(stage(&paths, &game).is_err());
    }
}
