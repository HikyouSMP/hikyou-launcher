//! Shared use leases cover launch preparation and the lifetime of managed JVMs.

use super::{java_inventory, managed_fs};
use cap_std::fs::{Dir, OpenOptions};
use std::{fs::File, path::Path, sync::Arc, time::Duration};

pub(super) async fn for_path(path: &Path, base: &Path) -> Result<Option<Arc<File>>, String> {
    if !base.try_exists().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let Some(directory) = path
        .strip_prefix(&base)
        .ok()
        .and_then(|p| p.components().next())
        .and_then(|p| p.as_os_str().to_str())
    else {
        return Ok(None);
    };
    let Some(major) = directory
        .strip_prefix("zulu-")
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse::<u32>().ok())
    else {
        return Ok(None);
    };
    if !java_inventory::managed_directory(major, directory) {
        return Ok(None);
    }
    shared(&managed_fs::root(&base)?, major).await.map(Some)
}
fn open(root: &Dir, major: u32) -> Result<File, String> {
    let name = format!(".zulu-{major}.use");
    managed_fs::inspect(root, Path::new(&name))?;
    root.open_with(name, OpenOptions::new().read(true).write(true).create(true))
        .map(|file| file.into_std())
        .map_err(|e| format!("failed to open Java use lease: {e}"))
}

pub(super) async fn shared(root: &Dir, major: u32) -> Result<Arc<File>, String> {
    let file = open(root, major)?;
    let start = std::time::Instant::now();
    loop {
        match fs2::FileExt::try_lock_shared(&file) {
            Ok(()) => return Ok(Arc::new(file)),
            Err(error) if contended(&error) && start.elapsed() < Duration::from_secs(60) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => return Err(format!("Java runtime is temporarily unavailable: {error}")),
        }
    }
}

pub(super) fn contended(error: &std::io::Error) -> bool {
    error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
}

pub(super) async fn idle(root: &Dir, base: &Path, major: u32) -> Result<Option<File>, String> {
    let file = open(root, major)?;
    match fs2::FileExt::try_lock_exclusive(&file) {
        Ok(()) => {}
        Err(error) if contended(&error) => return Ok(None),
        Err(error) => return Err(format!("failed to lock idle Java runtime: {error}")),
    }
    // A game may outlive the launcher that held its lease. Also inspect OS
    // processes; an uninspectable Java process is treated conservatively as busy.
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let busy = tokio::task::spawn_blocking(move || running_process(&base, major))
        .await
        .map_err(|e| format!("failed to inspect Java processes: {e}"))?;
    Ok(if busy { None } else { Some(file) })
}

fn running_process(base: &Path, major: u32) -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessStatus, RefreshKind, System, UpdateKind};
    let system = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_exe(UpdateKind::Always)
                .with_user(UpdateKind::Always),
        ),
    );
    let current_user = sysinfo::get_current_pid()
        .ok()
        .and_then(|pid| system.process(pid))
        .and_then(|p| p.user_id());
    system.processes().values().any(|process| {
        if process.status() == ProcessStatus::Zombie {
            return false;
        }
        let name = process.name().to_string_lossy();
        let java_name = ["java", "java.exe", "javaw", "javaw.exe"]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n));
        let uninspectable_java = java_name
            && match (current_user, process.user_id()) {
                (Some(current), Some(owner)) => current == owner,
                _ => true,
            };
        match process.exe() {
            Some(exe) => match exe.canonicalize() {
                Ok(exe) => executable_uses(&exe, base, major),
                Err(_) => uninspectable_java,
            },
            None => uninspectable_java,
        }
    })
}

fn executable_uses(executable: &Path, base: &Path, major: u32) -> bool {
    executable
        .strip_prefix(base)
        .ok()
        .and_then(|path| path.components().next())
        .and_then(|part| part.as_os_str().to_str())
        .is_some_and(|directory| {
            java_inventory::managed_directory(major, directory)
                || directory == format!(".zulu-{major}.previous")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn shared_use_blocks_replacement_until_the_last_owner_drops() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let first = shared(&root, 21).await.unwrap();
        let second = first.clone();
        let writer = open(&root, 21).unwrap();
        assert!(contended(
            &fs2::FileExt::try_lock_exclusive(&writer).unwrap_err()
        ));
        drop(first);
        assert!(fs2::FileExt::try_lock_exclusive(&writer).is_err());
        drop(second);
        fs2::FileExt::try_lock_exclusive(&writer).unwrap();
    }
    #[test]
    fn process_paths_are_scoped_to_the_managed_major() {
        let base = Path::new("managed");
        assert!(executable_uses(
            Path::new("managed/zulu-21/bin/java"),
            base,
            21
        ));
        assert!(!executable_uses(
            Path::new("managed/zulu-25/bin/java"),
            base,
            21
        ));
        assert!(!executable_uses(
            Path::new("unrelated/zulu-21/bin/java"),
            base,
            21
        ));
    }

    #[test]
    fn managed_process_fixture() {
        if let Some(path) = std::env::var_os("HIKYOU_JAVA_USE_FIXTURE_READY") {
            std::fs::write(path, b"ready").unwrap();
            std::thread::sleep(Duration::from_secs(30));
        }
    }

    #[tokio::test]
    async fn a_managed_process_without_a_launcher_lease_still_blocks_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let executable = temp
            .path()
            .join("zulu-21/bin")
            .join(super::super::java::JAVA_BIN);
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let ready = temp.path().join("ready");
        let mut command = tokio::process::Command::new(&executable);
        command
            .args([
                "--exact",
                "core::java_usage::tests::managed_process_fixture",
                "--test-threads=1",
            ])
            .env("HIKYOU_JAVA_USE_FIXTURE_READY", &ready)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn().unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(idle(&root, temp.path(), 21).await.unwrap().is_none());
        child.kill().await.unwrap();
        assert!(idle(&root, temp.path(), 21).await.unwrap().is_some());
    }
}
