//! Handle-relative I/O below an explicitly authorized game/world directory.

use cap_std::fs::{Dir, Metadata, OpenOptions};
use std::{
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

pub(super) fn relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.len() > 4096 {
        return Err("empty or excessively long archive path".to_string());
    }
    for part in value.split('/') {
        if !super::paths::is_safe_component(part) || part.chars().any(|c| c.is_ascii_control()) {
            return Err(format!("unsafe relative path: {value:?}"));
        }
        #[cfg(windows)]
        {
            let stem = part.split('.').next().unwrap().to_ascii_uppercase();
            if part.ends_with(['.', ' '])
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || (stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
            {
                return Err(format!("unsupported Windows path: {value:?}"));
            }
        }
    }
    Ok(PathBuf::from(value))
}

pub(super) fn root(path: &Path) -> Result<Dir, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| format!("failed to inspect managed directory: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("managed directory must be a real directory, not a link".to_string());
    }
    Dir::open_ambient_dir(path, cap_std::ambient_authority())
        .map_err(|e| format!("failed to open managed directory: {e}"))
}

pub(super) fn root_or_create(path: &Path) -> Result<Dir, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return root(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("failed to inspect managed root: {e}")),
    }
    for ancestor in path.ancestors().skip(1) {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => {
                return directory(&root(ancestor)?, path.strip_prefix(ancestor).unwrap(), true);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("failed to inspect managed root parent: {e}")),
        }
    }
    Err("managed directory has no existing parent".to_string())
}

fn components(path: &Path) -> Result<Vec<&std::ffi::OsStr>, String> {
    let text = path.to_str().ok_or("managed path is not UTF-8")?;
    relative_path(&text.replace(std::path::MAIN_SEPARATOR, "/"))?;
    path.components()
        .map(|part| match part {
            Component::Normal(name) => Ok(name),
            _ => Err("managed path must be relative".to_string()),
        })
        .collect()
}

pub(super) fn inspect(root: &Dir, path: &Path) -> Result<Option<Metadata>, String> {
    let parts = components(path)?;
    let mut current = root.try_clone().map_err(|e| e.to_string())?;
    for (index, part) in parts.iter().enumerate() {
        let metadata = match current.symlink_metadata(part) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("failed to inspect managed path: {e}")),
        };
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "linked files or directories are not supported here: {}",
                path.display()
            ));
        }
        if index == parts.len() - 1 {
            return Ok(Some(metadata));
        }
        current = current
            .open_dir(part)
            .map_err(|e| format!("unsafe or unavailable managed directory: {e}"))?;
    }
    unreachable!()
}

pub(super) fn directory(root: &Dir, path: &Path, create: bool) -> Result<Dir, String> {
    let mut current = root.try_clone().map_err(|e| e.to_string())?;
    for part in components(path)? {
        if create {
            match current.create_dir(part) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("failed to create managed directory: {e}")),
            }
        }
        let metadata = current
            .symlink_metadata(part)
            .map_err(|e| format!("failed to inspect managed directory: {e}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("linked or non-directory paths are not supported here".to_string());
        }
        current = current
            .open_dir(part)
            .map_err(|e| format!("failed to open confined directory: {e}"))?;
    }
    Ok(current)
}

pub(super) fn read_file(root: &Dir, path: &Path) -> Result<std::fs::File, String> {
    let metadata = inspect(root, path)?.ok_or("managed file does not exist")?;
    if !metadata.is_file() {
        return Err("managed input is not a regular file".to_string());
    }
    let file = root
        .open(path)
        .map_err(|e| format!("failed to open confined file: {e}"))?
        .into_std();
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("managed input changed file type".to_string());
    }
    #[cfg(unix)]
    let links = {
        use std::os::unix::fs::MetadataExt;
        metadata.nlink()
    };
    #[cfg(windows)]
    let links = {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // Inspect the opened file, not a path that could refer to a different file.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
            .map_err(|e| e.to_string())?;
        u64::from(info.nNumberOfLinks)
    };
    if links != 1 {
        return Err("hard-linked managed inputs are not supported here".to_string());
    }
    Ok(file)
}

pub(super) fn write(root: &Dir, path: &Path, bytes: &[u8], replace: bool) -> Result<(), String> {
    write_with(root, path, replace, |file| {
        file.write_all(bytes).map_err(|e| e.to_string())
    })
}

pub(super) fn write_with(
    root: &Dir,
    path: &Path,
    replace: bool,
    write_contents: impl FnOnce(&mut std::fs::File) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(metadata) = inspect(root, path)? {
        if !replace {
            return Err("a file with this name already exists".to_string());
        }
        if !metadata.is_file() {
            return Err("destination is not a regular file".to_string());
        }
    }
    let parent = match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => directory(root, parent, true)?,
        None => root.try_clone().map_err(|e| e.to_string())?,
    };
    let name = path.file_name().ok_or("missing destination filename")?;
    let temporary = format!(".hikyou-{}.tmp", uuid::Uuid::new_v4());
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = parent
            .open_with(&temporary, &options)
            .map_err(|e| format!("failed to stage managed file: {e}"))?
            .into_std();
        write_contents(&mut file)?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if replace {
            parent.rename(&temporary, &parent, name)
        } else {
            commit_new(&parent, &temporary, name)
        }
        .map_err(|e| format!("failed to commit managed file: {e}"))
    })();
    let _ = parent.remove_file(&temporary);
    result
}

#[cfg(unix)]
fn commit_new(parent: &Dir, temporary: &str, name: &std::ffi::OsStr) -> io::Result<()> {
    rustix::fs::renameat_with(
        parent,
        temporary,
        parent,
        name,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(Into::into)
}

#[cfg(windows)]
fn commit_new(parent: &Dir, temporary: &str, name: &std::ffi::OsStr) -> io::Result<()> {
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

    // As with cap-std's Windows rename, derive paths from the opened directory.
    // Its non-delete-sharing handle stays alive throughout the operation.
    let mut buffer = vec![0; 256];
    let path = loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(parent.as_raw_handle()),
                &mut buffer,
                FILE_NAME_NORMALIZED,
            )
        } as usize;
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length < buffer.len() {
            break PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]));
        }
        buffer.resize(length + 1, 0);
    };
    let wide = |leaf: &std::ffi::OsStr| -> Vec<u16> {
        path.join(leaf)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect()
    };
    let source = wide(temporary.as_ref());
    let target = wide(name);
    // Omitting REPLACE_EXISTING makes the final operation exclusive, including
    // when another process creates the target after our initial inspection.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xffff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::test_support::directory_link;

    #[test]
    fn boundary_rejects_nested_and_dangling_directory_links() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(game.join("nested")).unwrap();
        std::fs::create_dir(&outside).unwrap();
        directory_link(&outside, &game.join("nested/config"));
        let dir = root(&game).unwrap();
        assert!(write(&dir, Path::new("nested/config/file"), b"outside", true).is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_dir(&outside).unwrap();
        assert!(inspect(&dir, Path::new("nested/config/file")).is_err());
    }

    #[test]
    fn atomic_replacement_does_not_truncate_a_hardlink_peer() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        std::fs::create_dir(&game).unwrap();
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, b"original").unwrap();
        std::fs::hard_link(&outside, game.join("config.txt")).unwrap();
        let dir = root(&game).unwrap();
        assert!(read_file(&dir, Path::new("config.txt")).is_err());
        write(&dir, Path::new("config.txt"), b"replacement", true).unwrap();
        assert_eq!(std::fs::read(&outside).unwrap(), b"original");
        assert_eq!(
            std::fs::read(game.join("config.txt")).unwrap(),
            b"replacement"
        );
    }

    #[test]
    fn exclusive_commit_and_failed_write_keep_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let dir = root(temp.path()).unwrap();
        write(&dir, Path::new("file"), b"original", false).unwrap();
        assert!(write(&dir, Path::new("file"), b"replacement", false).is_err());
        assert!(
            write_with(&dir, Path::new("file"), true, |file| {
                file.write_all(b"partial").unwrap();
                Err("injected I/O failure".to_string())
            })
            .is_err()
        );
        assert_eq!(
            std::fs::read(temp.path().join("file")).unwrap(),
            b"original"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn exclusive_commit_does_not_overwrite_a_concurrently_created_target() {
        let temp = tempfile::tempdir().unwrap();
        let dir = root(temp.path()).unwrap();
        let result = write_with(&dir, Path::new("file"), false, |file| {
            file.write_all(b"staged").unwrap();
            // This destination did not exist when write_with inspected it.
            std::fs::write(temp.path().join("file"), b"concurrent").unwrap();
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(temp.path().join("file")).unwrap(),
            b"concurrent"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn path_policy_rejects_navigation_and_cross_platform_aliases() {
        for name in [
            "a/../file",
            "a/./file",
            "a//file",
            "/file",
            "C:/file",
            "a\\file",
            "file:stream",
            "a/\nfile",
        ] {
            assert!(relative_path(name).is_err(), "accepted {name:?}");
        }
        #[cfg(windows)]
        for name in ["a/.. /file", "a./file", "CON.txt", "a/LPT1"] {
            assert!(relative_path(name).is_err(), "accepted {name:?}");
        }
        assert!(relative_path("設定/normal file.json").is_ok());
    }
}
