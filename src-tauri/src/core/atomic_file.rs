use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

/// Commits a complete file replacement without exposing a partially written destination.
pub(crate) fn write(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("destination does not have a parent directory")?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("failed to create destination directory: {e}"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("destination has an invalid file name")?;
    let temporary = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("failed to create temporary file: {e}"))?;
        file.write_all(contents)
            .map_err(|e| format!("failed to write temporary file: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("failed to flush temporary file: {e}"))?;
        drop(file);
        replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(target_os = "windows")]
fn replace(temporary: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACE_FILE_FLAGS,
        ReplaceFileW,
    };
    use windows::core::PCWSTR;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let source = wide(temporary);
    let target = wide(destination);
    unsafe {
        if destination.exists() {
            ReplaceFileW(
                PCWSTR(target.as_ptr()),
                PCWSTR(source.as_ptr()),
                PCWSTR::null(),
                REPLACE_FILE_FLAGS(0),
                None,
                None,
            )
            .or_else(|_| {
                MoveFileExW(
                    PCWSTR(source.as_ptr()),
                    PCWSTR(target.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            })
            .map_err(|e| format!("failed to atomically replace destination: {e}"))
        } else {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|e| format!("failed to atomically commit destination: {e}"))
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn replace(temporary: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(temporary, destination)
        .map_err(|e| format!("failed to atomically replace destination: {e}"))
}

#[cfg(test)]
mod tests {
    use super::write;

    #[test]
    fn replaces_complete_file() {
        let directory =
            std::env::temp_dir().join(format!("hikyou-atomic-{}", uuid::Uuid::new_v4()));
        let path = directory.join("state.dat");
        write(&path, b"first").unwrap();
        write(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
