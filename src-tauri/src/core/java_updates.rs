//! Fixed per-major installations, transactional replacement and bounded cleanup.

use super::{
    java::{self, JAVA_BIN, JavaInstall},
    java_catalog::{self, Package},
    java_inventory::{self as inventory, RuntimeRecord, UpdateState},
    java_usage, managed_fs,
    paths::LauncherPaths,
};
use cap_std::fs::{Dir, OpenOptions};
use std::{path::Path, time::Duration};

fn stable_name(major: u32) -> String {
    format!("zulu-{major}")
}
fn backup_name(major: u32) -> String {
    format!(".zulu-{major}.previous")
}

fn coordinator(root: &Dir, major: u32) -> Result<std::fs::File, String> {
    let name = format!(".zulu-{major}.lock");
    managed_fs::inspect(root, Path::new(&name))?;
    root.open_with(name, OpenOptions::new().read(true).write(true).create(true))
        .map(|file| file.into_std())
        .map_err(|e| format!("failed to open Java update lock: {e}"))
}

fn current_layout(state: &UpdateState, major: u32) -> bool {
    state
        .runtime
        .as_ref()
        .is_some_and(|record| record.directory == stable_name(major))
}

pub(super) async fn ensure(major: u32, paths: &LauncherPaths) -> Result<JavaInstall, String> {
    ensure_with_policy(major, paths, false)
        .await
        .map(|outcome| outcome.install)
}

// A usable runtime does not imply that maintenance or its state commit succeeded.
struct UpdateOutcome {
    install: JavaInstall,
    check: Result<(), String>,
}

async fn ensure_with_policy(
    major: u32,
    paths: &LauncherPaths,
    manual: bool,
) -> Result<UpdateOutcome, String> {
    if major == 0 {
        return Err("invalid required Java major".to_string());
    }
    let base = paths.java_versions();
    let root = managed_fs::root_or_create(&base)?;
    let mut state = inventory::load_state(&root, major)?;
    let cached = existing(&root, &base, major, &state).await;
    if let Some(cached) = &cached
        && current_layout(&state, major)
        && !manual
        && !state.due(chrono::Utc::now().timestamp())
    {
        return Ok(UpdateOutcome {
            install: cached.clone(),
            check: Ok(()),
        });
    }
    let lock = coordinator(&root, major)?;
    let waiting = std::time::Instant::now();
    loop {
        match fs2::FileExt::try_lock_exclusive(&lock) {
            Ok(()) => break,
            Err(error) if java_usage::contended(&error) => {
                if !manual && let Some(cached) = cached {
                    return Ok(UpdateOutcome {
                        install: cached,
                        check: Ok(()),
                    });
                }
                if waiting.elapsed() > Duration::from_secs(360) {
                    return Err(
                        "another Java installation is still running; try again shortly".to_string(),
                    );
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(format!("failed to lock Java update: {error}")),
        }
    }
    drop(cached);
    state = inventory::load_state(&root, major)?;
    if let Some(idle) = java_usage::idle(&root, &base, major).await? {
        recover_previous(&root, major)?;
        drop(idle);
    }
    let mut cached = existing(&root, &base, major, &state).await;
    if let Some(cached) = &cached
        && current_layout(&state, major)
        && !manual
        && !state.due(chrono::Utc::now().timestamp())
    {
        return Ok(UpdateOutcome {
            install: cached.clone(),
            check: Ok(()),
        });
    }
    // Keep the coordinator lock, but release our own use lease. Other launchers
    // may continue using the current runtime while metadata/downloads are pending.
    if let Some(cached) = &mut cached {
        cached.lease = None;
    }
    state.last_attempt_at = Some(chrono::Utc::now().timestamp());
    state.last_error = Some("Java update was interrupted before completion".to_string());
    state.deferred = false;
    let result = match inventory::save_state(&root, major, &state) {
        Ok(()) => refresh(&root, &base, major, &mut state, cached.as_ref(), manual).await,
        Err(error) => Err(error),
    };
    let mut outcome = match result {
        Ok(Refresh::Installed(install)) => UpdateOutcome {
            install,
            check: Ok(()),
        },
        Ok(Refresh::Cached { deferred }) => {
            state.deferred = deferred;
            let install = cached
                .clone()
                .ok_or("Java is in use; retry after the other Java process exits")?;
            finish_update(&root, major, state, cached, Ok(install))?
        }
        Err(error) => finish_update(&root, major, state, cached, Err(error))?,
    };
    // Publish a protected selection before releasing the coordinator.
    outcome.install.lease = Some(java_usage::shared(&root, major).await?);
    java::probe_java(&outcome.install.path, major).await?;
    Ok(outcome)
}

enum Refresh {
    Installed(JavaInstall),
    Cached { deferred: bool },
}

async fn refresh(
    root: &Dir,
    base: &Path,
    major: u32,
    state: &mut UpdateState,
    cached: Option<&JavaInstall>,
    manual: bool,
) -> Result<Refresh, String> {
    // Do not repeatedly download a replacement while a game is using this major.
    if !manual {
        let Some(idle) = java_usage::idle(root, base, major).await? else {
            return Ok(Refresh::Cached { deferred: true });
        };
        drop(idle);
    }
    let client = java_catalog::client()?;
    let package = java_catalog::latest(&client, major).await?;
    if cached.is_some()
        && let Some(record) = &state.runtime
        && !newer(&package, &record.package)
    {
        if current_layout(state, major) {
            if let Some(_idle) = java_usage::idle(root, base, major).await? {
                record_cleanup(root, major, state);
            }
            return Ok(Refresh::Cached { deferred: false });
        }
        let Some(_idle) = java_usage::idle(root, base, major).await? else {
            return Ok(Refresh::Cached { deferred: true });
        };
        let source = record.directory.clone();
        let package = record.package.clone();
        let install = activate(root, base, major, Path::new(&source), package, state)?;
        record_cleanup(root, major, state);
        return Ok(Refresh::Installed(install));
    }
    let Some(idle) = java_usage::idle(root, base, major).await? else {
        return Ok(Refresh::Cached { deferred: true });
    };
    drop(idle);
    let bytes = package.download(&client).await?;
    let staging = tempfile::Builder::new()
        .prefix(".zulu-")
        .tempdir_in(base)
        .map_err(|e| format!("failed to stage Zulu: {e}"))?;
    let candidate = staging.path().join("runtime");
    #[cfg(windows)]
    super::java_archive::extract_and_rename_zip(&bytes, staging.path(), &candidate)?;
    #[cfg(not(windows))]
    super::java_archive::extract_and_rename_targz(&bytes, staging.path(), &candidate)?;
    drop(bytes);
    {
        let staged_root = managed_fs::root(staging.path())?;
        inventory::zulu_release(&staged_root, "runtime", major)?;
    }
    java::probe_java(&candidate.join("bin").join(JAVA_BIN), major).await?;
    // No network or extraction while holding the exclusive use lease.
    let Some(_idle) = java_usage::idle(root, base, major).await? else {
        return Ok(Refresh::Cached { deferred: true });
    };
    let relative = candidate
        .strip_prefix(base)
        .map_err(|_| "invalid Zulu staging path")?;
    let install = activate(root, base, major, relative, package, state)?;
    drop(staging);
    record_cleanup(root, major, state);
    Ok(Refresh::Installed(install))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualCheckResult {
    pub status: inventory::RuntimeStatus,
    pub checked: usize,
    pub failures: Vec<String>,
}

fn installed_majors(status: &inventory::RuntimeStatus) -> std::collections::BTreeSet<u32> {
    status
        .installations
        .iter()
        .filter(|runtime| !runtime.legacy)
        .map(|runtime| runtime.major)
        .collect()
}

pub async fn check_installed(paths: &LauncherPaths) -> Result<ManualCheckResult, String> {
    let majors = installed_majors(&inventory::status(paths)?);
    let mut failures = Vec::new();
    for major in &majors {
        if let Err(error) = ensure_with_policy(*major, paths, true)
            .await
            .and_then(|outcome| outcome.check)
        {
            failures.push(format!("Java {major}: {error}"));
        }
    }
    let status = inventory::status(paths)?;
    Ok(ManualCheckResult {
        status,
        checked: majors.len(),
        failures,
    })
}

fn finish_update(
    root: &Dir,
    major: u32,
    state: UpdateState,
    cached: Option<JavaInstall>,
    result: Result<JavaInstall, String>,
) -> Result<UpdateOutcome, String> {
    finish_update_with_save(major, state, cached, result, |state| {
        inventory::save_state(root, major, state)
    })
}

fn finish_update_with_save(
    major: u32,
    mut state: UpdateState,
    cached: Option<JavaInstall>,
    result: Result<JavaInstall, String>,
    save: impl FnOnce(&UpdateState) -> Result<(), String>,
) -> Result<UpdateOutcome, String> {
    match &result {
        Ok(_) => {
            if !state.deferred {
                state.last_checked_at = Some(chrono::Utc::now().timestamp());
            }
            state.last_error = None;
        }
        Err(error) => {
            state.last_error = Some(error.clone());
            state.deferred = false;
        }
    }
    if let Err(error) = save(&state) {
        log::warn!("[java] Cannot commit Java {major} update state: {error}");
        let error = match result {
            Ok(_) => error,
            Err(check_error) => format!("{check_error}; failed to save Java update state: {error}"),
        };
        return match cached {
            Some(install) => Ok(UpdateOutcome {
                install,
                check: Err(error),
            }),
            None => Err(error),
        };
    }
    match result {
        Ok(install) => Ok(UpdateOutcome {
            install,
            check: Ok(()),
        }),
        Err(error) => match cached {
            Some(cached) => {
                log::warn!("[java] Update for Java {major} failed; using installed Zulu: {error}");
                Ok(UpdateOutcome {
                    install: cached,
                    check: Err(error),
                })
            }
            None => Err(error),
        },
    }
}

async fn existing(root: &Dir, base: &Path, major: u32, state: &UpdateState) -> Option<JavaInstall> {
    let lease = java_usage::shared(root, major).await.ok()?;
    let directory = state
        .runtime
        .as_ref()
        .map(|r| r.directory.clone())
        .unwrap_or_else(|| stable_name(major));
    inventory::zulu_release(root, &directory, major).ok()?;
    let mut install = java::probe_java(&base.join(directory).join("bin").join(JAVA_BIN), major)
        .await
        .ok()?;
    install.lease = Some(lease);
    Some(install)
}

fn remove_directory(root: &Dir, name: &str) -> Result<(), String> {
    if let Some(metadata) = managed_fs::inspect(root, Path::new(name))? {
        if !metadata.is_dir() {
            return Err("managed Java cleanup target is not a directory".to_string());
        }
        // A validated single name, resolved relative to the opened java_versions
        // directory. cap-std removes nested links themselves, not their targets.
        root.remove_dir_all(name)
            .map_err(|e| format!("failed to remove old Java runtime: {e}"))?;
    }
    Ok(())
}

fn recover_previous(root: &Dir, major: u32) -> Result<(), String> {
    let stable = stable_name(major);
    let backup = backup_name(major);
    let state = inventory::load_state(root, major)?;
    if let Some(record) = state.runtime
        && record.directory != stable
        && managed_fs::inspect(root, Path::new(&record.directory))?.is_none()
        && managed_fs::inspect(root, Path::new(&stable))?.is_some()
    {
        // A migration may have moved its selected UUID directory before the
        // selection file was committed. Restore that original selection first.
        inventory::zulu_release(root, &stable, major)?;
        root.rename(&stable, root, &record.directory)
            .map_err(|e| format!("failed to recover Java layout migration: {e}"))?;
    }
    if managed_fs::inspect(root, Path::new(&stable))?.is_none()
        && managed_fs::inspect(root, Path::new(&backup))?.is_some()
    {
        inventory::zulu_release(root, &backup, major)?;
        root.rename(&backup, root, &stable)
            .map_err(|e| format!("failed to recover previous Java runtime: {e}"))?;
    }
    Ok(())
}

fn activate(
    root: &Dir,
    base: &Path,
    major: u32,
    source: &Path,
    package: Package,
    state: &mut UpdateState,
) -> Result<JavaInstall, String> {
    let stable = stable_name(major);
    let backup = backup_name(major);
    remove_directory(root, &backup)?;
    let previous = managed_fs::inspect(root, Path::new(&stable))?.is_some();
    if previous {
        managed_fs::directory(root, Path::new(&stable), false)?;
        root.rename(&stable, root, &backup)
            .map_err(|e| format!("failed to stage previous Java runtime: {e}"))?;
    }
    if let Err(error) = root.rename(source, root, &stable) {
        if previous {
            root.rename(&backup, root, &stable).map_err(|rollback| {
                format!("Java replacement failed: {error}; rollback failed: {rollback}")
            })?;
        }
        return Err(format!("failed to replace Java runtime: {error}"));
    }
    let next = UpdateState {
        runtime: Some(RuntimeRecord {
            directory: stable.clone(),
            package,
        }),
        last_checked_at: Some(chrono::Utc::now().timestamp()),
        last_error: None,
        deferred: false,
        cleanup_error: None,
        ..state.clone()
    };
    if let Err(error) = inventory::save_state(root, major, &next) {
        root.rename(&stable, root, source).map_err(|rollback| {
            format!("Java selection failed: {error}; rollback failed: {rollback}")
        })?;
        if previous {
            root.rename(&backup, root, &stable).map_err(|rollback| {
                format!("Java selection failed: {error}; recovery failed: {rollback}")
            })?;
        }
        return Err(error);
    }
    *state = next;
    Ok(JavaInstall {
        path: base.join(stable).join("bin").join(JAVA_BIN),
        major_version: major,
        lease: None,
    })
}

fn cleanup_owned(root: &Dir, major: u32) -> Result<(), String> {
    let stable = stable_name(major);
    let mut old = Vec::new();
    for entry in root.entries().map_err(|e| e.to_string())? {
        let name = entry
            .map_err(|e| e.to_string())?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name != stable && inventory::managed_directory(major, &name) {
            old.push(name);
        }
    }
    old.push(backup_name(major));
    for name in old {
        remove_directory(root, &name)?;
    }
    Ok(())
}

fn record_cleanup(root: &Dir, major: u32, state: &mut UpdateState) {
    let error = cleanup_owned(root, major).err();
    if error != state.cleanup_error {
        state.cleanup_error = error;
        if let Err(error) = inventory::save_state(root, major, state) {
            log::warn!("[java] Cannot record Java cleanup status: {error}");
        }
    }
    if let Some(error) = &state.cleanup_error {
        log::warn!("[java] Java {major} cleanup will retry: {error}");
    }
}

pub(super) async fn cleanup(major: u32, paths: &LauncherPaths) -> Result<(), String> {
    let base = paths.java_versions();
    let root = managed_fs::root(&base)?;
    let lock = coordinator(&root, major)?;
    match fs2::FileExt::try_lock_exclusive(&lock) {
        Ok(()) => {}
        Err(error) if java_usage::contended(&error) => return Ok(()),
        Err(error) => return Err(error.to_string()),
    }
    let mut state = inventory::load_state(&root, major)?;
    if !current_layout(&state, major) {
        return Ok(());
    }
    let has_old = managed_fs::inspect(&root, Path::new(&backup_name(major)))?.is_some()
        || root.entries().map_err(|e| e.to_string())?.any(|entry| {
            entry.ok().is_some_and(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name != stable_name(major) && inventory::managed_directory(major, &name)
            })
        });
    if !has_old {
        return Ok(());
    }
    let Some(_idle) = java_usage::idle(&root, &base, major).await? else {
        return Ok(());
    };
    recover_previous(&root, major)?;
    inventory::zulu_release(&root, &stable_name(major), major)?;
    java::probe_java(
        &base.join(stable_name(major)).join("bin").join(JAVA_BIN),
        major,
    )
    .await?;
    record_cleanup(&root, major, &mut state);
    Ok(())
}

fn newer(candidate: &Package, installed: &Package) -> bool {
    let order = (
        &candidate.java_version,
        candidate.openjdk_build_number,
        &candidate.distro_version,
    )
        .cmp(&(
            &installed.java_version,
            installed.openjdk_build_number,
            &installed.distro_version,
        ));
    order.is_gt() || (order.is_eq() && candidate.package_uuid != installed.package_uuid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn manual_check_does_not_install_unrequested_generations() {
        let temp = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let result = check_installed(&paths).await.unwrap();
        assert_eq!(result.checked, 0);
        assert!(result.failures.is_empty());
        assert!(!paths.java_versions().exists());
    }
    #[tokio::test]
    #[ignore = "downloads official Zulu runtimes and verifies 8 GiB heap addressing without starting Minecraft"]
    async fn live_legacy_runtime_architecture_repair_and_heap() {
        let temp = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let root = managed_fs::root_or_create(&paths.java_versions()).unwrap();
        let client = java_catalog::client().unwrap();
        for major in [8, 17] {
            let package = java_catalog::latest(&client, major).await.unwrap();
            let name = stable_name(major);
            let dir = managed_fs::directory(&root, Path::new(&name), true).unwrap();
            let version = if major == 8 { "1.8.0_512" } else { "17.0.21" };
            managed_fs::write(&dir, Path::new("release"), format!(
                "JAVA_VERSION=\"{version}\"\nIMPLEMENTOR=\"Azul Systems, Inc.\"\nOS_ARCH=\"i586\"\n"
            ).as_bytes(), false).unwrap();
            managed_fs::write(
                &dir,
                &Path::new("bin").join(JAVA_BIN),
                b"invalid old runtime",
                false,
            )
            .unwrap();
            // Even a recent successful update record must not preserve a wrong-architecture runtime.
            inventory::save_state(
                &root,
                major,
                &UpdateState {
                    runtime: Some(RuntimeRecord {
                        directory: name,
                        package,
                    }),
                    last_checked_at: Some(chrono::Utc::now().timestamp()),
                    ..UpdateState::default()
                },
            )
            .unwrap();
            drop(dir);
            let installed = ensure(major, &paths).await.unwrap();
            let cached = ensure(major, &paths).await.unwrap();
            assert_eq!(installed.path, cached.path);
            let manual = check_installed(&paths).await.unwrap();
            assert!(manual.failures.is_empty(), "{:?}", manual.failures);
            assert!(
                manual
                    .status
                    .checks
                    .iter()
                    .any(|check| check.major == major && !check.deferred),
                "An already current runtime does not require replacement, even while busy"
            );
            // Simulate a newer published build without relying on old archives staying available.
            let mut state = inventory::load_state(&root, major).unwrap();
            let current_package = state.runtime.as_ref().unwrap().package.clone();
            state.runtime.as_mut().unwrap().package.openjdk_build_number = 0;
            inventory::save_state(&root, major, &state).unwrap();
            let manual = check_installed(&paths).await.unwrap();
            assert!(manual.failures.is_empty(), "{:?}", manual.failures);
            assert!(
                manual
                    .status
                    .checks
                    .iter()
                    .any(|check| check.major == major && check.deferred),
                "Manual checks bypass the interval but must retain active runtime leases"
            );
            let mut state = inventory::load_state(&root, major).unwrap();
            state.runtime.as_mut().unwrap().package = current_package;
            state.deferred = false;
            inventory::save_state(&root, major, &state).unwrap();
            let mut command = tokio::process::Command::new(&installed.path);
            command
                .args(["-Xmx8192M", "-Xms16M", "-version"])
                .kill_on_drop(true);
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            let output = tokio::time::timeout(Duration::from_secs(30), command.output())
                .await
                .unwrap()
                .unwrap();
            assert!(
                output.status.success(),
                "Java {major} heap check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                inventory::status(&paths)
                    .unwrap()
                    .installations
                    .iter()
                    .any(|runtime| runtime.major == major
                        && runtime.selected
                        && runtime.error.is_none())
            );
            eprintln!(
                "Java {major}: wrong-architecture installation replaced; cached reuse and -Xmx8192M succeeded"
            );
        }
    }

    fn package(version: Vec<u32>, build: u32) -> Package {
        Package {
            package_uuid: uuid::Uuid::new_v4(),
            java_version: version,
            distro_version: vec![21, 1, 1],
            openjdk_build_number: build,
            download_url: "https://cdn.azul.com/a".to_string(),
            sha256_hash: "a".repeat(64),
        }
    }
    #[test]
    fn updates_include_build_revisions_but_never_downgrade() {
        let installed = package(vec![21, 0, 10], 7);
        assert!(newer(&package(vec![21, 0, 11], 1), &installed));
        assert!(newer(&package(vec![21, 0, 10], 8), &installed));
        assert!(!newer(&installed.clone(), &installed));
        assert!(newer(&package(vec![21, 0, 10], 7), &installed));
        assert!(!newer(&package(vec![21, 0, 9], 99), &installed));
    }

    #[test]
    fn failed_update_preserves_selection_and_exposes_the_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let root = managed_fs::root(temp.path())?;
        let directory = format!("zulu-21-{}", uuid::Uuid::new_v4());
        let old = JavaInstall {
            path: temp.path().join(&directory).join("bin").join(JAVA_BIN),
            major_version: 21,
            lease: None,
        };
        let state = UpdateState {
            runtime: Some(RuntimeRecord {
                directory: directory.clone(),
                package: package(vec![21, 0, 10], 7),
            }),
            last_attempt_at: Some(200),
            last_checked_at: Some(100),
            last_error: None,
            deferred: false,
            cleanup_error: None,
        };
        let result = finish_update(
            &root,
            21,
            state,
            Some(old.clone()),
            Err("offline".to_string()),
        )?;
        assert_eq!(result.install.path, old.path);
        assert_eq!(result.check, Err("offline".to_string()));
        let saved = inventory::load_state(&root, 21)?;
        assert_eq!(
            saved.runtime.as_ref().map(|record| &record.directory),
            Some(&directory)
        );
        assert_eq!(saved.last_error.as_deref(), Some("offline"));
        assert_eq!(saved.last_checked_at, Some(100));
        assert_eq!(saved.next_check_at(), Some(200 + inventory::RETRY_INTERVAL));
        Ok(())
    }

    #[test]
    fn failed_state_commit_retains_runtime_but_fails_check()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let root = managed_fs::root(temp.path())?;
        root.create_dir(".zulu-21.json")?;
        let old = JavaInstall {
            path: temp.path().join("previous/bin").join(JAVA_BIN),
            major_version: 21,
            lease: None,
        };
        let new = JavaInstall {
            path: temp.path().join("new/bin").join(JAVA_BIN),
            major_version: 21,
            lease: None,
        };
        let result = finish_update(
            &root,
            21,
            UpdateState::default(),
            Some(old.clone()),
            Ok(new),
        )?;
        assert_eq!(result.install.path, old.path);
        assert!(result.check.is_err());
        assert!(root.symlink_metadata(".zulu-21.json")?.is_dir());
        Ok(())
    }

    #[test]
    fn metadata_and_persistence_failures_survive_stale_success_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let root = managed_fs::root(temp.path())?;
        let state = UpdateState {
            last_attempt_at: Some(200),
            last_checked_at: Some(100),
            ..UpdateState::default()
        };
        inventory::save_state(&root, 21, &state)?;
        let cached = JavaInstall {
            path: temp.path().join("zulu-21/bin").join(JAVA_BIN),
            major_version: 21,
            lease: None,
        };
        let outcome = finish_update_with_save(
            21,
            state,
            Some(cached.clone()),
            Err("metadata unavailable".to_string()),
            |attempted| {
                assert_eq!(
                    attempted.last_error.as_deref(),
                    Some("metadata unavailable")
                );
                assert_eq!(attempted.last_checked_at, Some(100));
                Err("injected disk full".to_string())
            },
        )?;
        assert_eq!(outcome.install.path, cached.path);
        assert_eq!(
            outcome.check,
            Err(
                "metadata unavailable; failed to save Java update state: injected disk full"
                    .to_string()
            )
        );
        let persisted = inventory::load_state(&root, 21)?;
        assert!(persisted.last_error.is_none());
        assert_eq!(persisted.last_checked_at, Some(100));
        Ok(())
    }

    #[test]
    fn successful_check_clears_stale_error_and_preserves_deferral_policy() -> Result<(), String> {
        for deferred in [false, true] {
            let install = JavaInstall {
                path: Path::new("zulu-21/bin").join(JAVA_BIN),
                major_version: 21,
                lease: None,
            };
            let state = UpdateState {
                last_checked_at: Some(100),
                last_error: Some("previous failure".to_string()),
                deferred,
                ..UpdateState::default()
            };
            let outcome = finish_update_with_save(
                21,
                state,
                Some(install.clone()),
                Ok(install.clone()),
                |saved| {
                    assert!(saved.last_error.is_none());
                    assert_eq!(saved.deferred, deferred);
                    if deferred {
                        assert_eq!(saved.last_checked_at, Some(100));
                    } else {
                        assert!(saved.last_checked_at.is_some_and(|time| time > 100));
                    }
                    Ok(())
                },
            )?;
            assert_eq!(outcome.install.path, install.path);
            assert_eq!(outcome.check, Ok(()));
        }
        Ok(())
    }

    #[test]
    fn failed_check_without_cached_runtime_remains_an_error() {
        for persistence_fails in [false, true] {
            let result = finish_update_with_save(
                21,
                UpdateState::default(),
                None,
                Err("metadata unavailable".to_string()),
                |_| {
                    if persistence_fails {
                        Err("injected disk full".to_string())
                    } else {
                        Ok(())
                    }
                },
            );
            let expected = if persistence_fails {
                "metadata unavailable; failed to save Java update state: injected disk full"
            } else {
                "metadata unavailable"
            };
            assert_eq!(result.err().as_deref(), Some(expected));
        }
    }

    #[test]
    fn update_lock_is_exclusive_and_released_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let options = &mut OpenOptions::new();
        options.read(true).write(true).create(true);
        let first = root.open_with("lock", options).unwrap().into_std();
        let second = root.open_with("lock", options).unwrap().into_std();
        fs2::FileExt::try_lock_exclusive(&first).unwrap();
        assert_eq!(
            fs2::FileExt::try_lock_exclusive(&second)
                .unwrap_err()
                .raw_os_error(),
            fs2::lock_contended_error().raw_os_error()
        );
        drop(first);
        fs2::FileExt::try_lock_exclusive(&second).unwrap();
    }

    #[tokio::test]
    #[ignore = "downloads an official Zulu runtime into an isolated temporary directory"]
    async fn live_zulu_install_and_cached_reuse() {
        let temp = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let first = ensure(25, &paths).await.unwrap();
        let second = ensure(25, &paths).await.unwrap();
        assert_eq!(first.path, second.path);
        assert_eq!(first.major_version, 25);
        let status = inventory::status(&paths).unwrap();
        assert_eq!(status.installations.len(), 1);
        assert!(status.installations[0].selected);
        assert!(status.checks[0].last_error.is_none());
        assert!(status.checks[0].last_checked_at.is_some());
        // Force the "newer release" branch without depending on an old vendor
        // archive remaining available. Both downloads are the current official build.
        let root = managed_fs::root(&paths.java_versions()).unwrap();
        let mut state = inventory::load_state(&root, 25).unwrap();
        state.last_checked_at = None;
        state.runtime.as_mut().unwrap().package.openjdk_build_number = 0;
        inventory::save_state(&root, 25, &state).unwrap();
        let busy = ensure(25, &paths).await.unwrap();
        assert!(inventory::load_state(&root, 25).unwrap().deferred);
        assert_eq!(first.path, busy.path);
        let original_path = first.path.clone();
        drop((first, second, busy));
        let mut state = inventory::load_state(&root, 25).unwrap();
        state.last_attempt_at = None;
        inventory::save_state(&root, 25, &state).unwrap();
        let updated = ensure(25, &paths).await.unwrap();
        assert_eq!(original_path, updated.path);
        assert_eq!(ensure(25, &paths).await.unwrap().path, original_path);
        let status = inventory::status(&paths).unwrap();
        assert_eq!(status.installations.len(), 1);
        assert!(!status.checks[0].deferred);
        assert!(!base_backup_exists(&root));
        assert_eq!(
            status.installations.iter().filter(|r| r.selected).count(),
            1
        );
    }

    fn base_backup_exists(root: &Dir) -> bool {
        root.symlink_metadata(backup_name(25)).is_ok()
    }

    fn candidate(root: &Dir, directory: &str, marker: &[u8]) {
        let dir = managed_fs::directory(root, Path::new(directory), true).unwrap();
        managed_fs::write(
            &dir,
            Path::new("release"),
            b"JAVA_VERSION=\"21.0.10\"\nIMPLEMENTOR=\"Azul Systems, Inc.\"\n",
            false,
        )
        .unwrap();
        managed_fs::write(&dir, &Path::new("bin").join(JAVA_BIN), b"fixture", false).unwrap();
        managed_fs::write(&dir, Path::new("marker"), marker, false).unwrap();
    }

    #[test]
    fn repeated_replacement_keeps_one_runtime_and_removes_previous_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let mut state = UpdateState::default();
        for index in 0..5 {
            candidate(&root, "stage/runtime", &[index]);
            activate(
                &root,
                temp.path(),
                21,
                Path::new("stage/runtime"),
                package(vec![21, 0, 10], 7),
                &mut state,
            )
            .unwrap();
            record_cleanup(&root, 21, &mut state);
            assert_eq!(
                std::fs::read(temp.path().join("zulu-21/marker")).unwrap(),
                &[index]
            );
            assert!(root.symlink_metadata(backup_name(21)).is_err());
            assert_eq!(state.runtime.as_ref().unwrap().directory, "zulu-21");
        }
    }

    #[test]
    fn failed_metadata_commit_restores_previous_directory_and_candidate() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        candidate(&root, "zulu-21", b"previous");
        candidate(&root, "stage/runtime", b"new");
        root.create_dir(".zulu-21.json").unwrap();
        let mut state = UpdateState::default();
        assert!(
            activate(
                &root,
                temp.path(),
                21,
                Path::new("stage/runtime"),
                package(vec![21, 0, 10], 7),
                &mut state
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(temp.path().join("zulu-21/marker")).unwrap(),
            b"previous"
        );
        assert_eq!(
            std::fs::read(temp.path().join("stage/runtime/marker")).unwrap(),
            b"new"
        );
        assert!(root.symlink_metadata(backup_name(21)).is_err());
        assert!(state.runtime.is_none());
    }

    #[test]
    fn interrupted_swap_recovers_the_previous_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        candidate(&root, &backup_name(21), b"previous");
        recover_previous(&root, 21).unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("zulu-21/marker")).unwrap(),
            b"previous"
        );
        assert!(root.symlink_metadata(backup_name(21)).is_err());
    }

    #[test]
    fn migration_moves_an_existing_generation_to_the_fixed_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let old = format!("zulu-21-{}", uuid::Uuid::new_v4());
        candidate(&root, &old, b"selected generation");
        candidate(&root, "zulu-21", b"older fixed layout");
        let mut state = UpdateState {
            runtime: Some(RuntimeRecord {
                directory: old.clone(),
                package: package(vec![21, 0, 10], 7),
            }),
            ..Default::default()
        };
        let package = state.runtime.as_ref().unwrap().package.clone();
        activate(&root, temp.path(), 21, Path::new(&old), package, &mut state).unwrap();
        record_cleanup(&root, 21, &mut state);
        assert_eq!(
            std::fs::read(temp.path().join("zulu-21/marker")).unwrap(),
            b"selected generation"
        );
        assert!(!temp.path().join(old).exists());
        assert!(root.symlink_metadata(backup_name(21)).is_err());
    }

    #[test]
    fn interrupted_migration_restores_the_selected_generation() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let old = format!("zulu-21-{}", uuid::Uuid::new_v4());
        candidate(&root, &old, b"selected");
        candidate(&root, "zulu-21", b"older");
        let state = UpdateState {
            runtime: Some(RuntimeRecord {
                directory: old.clone(),
                package: package(vec![21, 0, 10], 7),
            }),
            ..Default::default()
        };
        inventory::save_state(&root, 21, &state).unwrap();
        root.rename("zulu-21", &root, backup_name(21)).unwrap();
        root.rename(&old, &root, "zulu-21").unwrap();
        recover_previous(&root, 21).unwrap();
        assert_eq!(
            std::fs::read(temp.path().join(old).join("marker")).unwrap(),
            b"selected"
        );
        assert_eq!(
            std::fs::read(temp.path().join("zulu-21/marker")).unwrap(),
            b"older"
        );
    }

    #[test]
    fn cleanup_removes_only_owned_generations_without_following_nested_links() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("managed");
        let root = managed_fs::root_or_create(&base).unwrap();
        let old = format!("zulu-21-{}", uuid::Uuid::new_v4());
        candidate(&root, &old, b"old");
        for name in ["zulu-21", "zulu-25", "zulu-21-custom", "liberica-nik-21"] {
            root.create_dir(name).unwrap();
        }
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"untouched").unwrap();
        super::super::test_support::directory_link(&outside, &base.join(&old).join("linked"));
        cleanup_owned(&root, 21).unwrap();
        assert!(!base.join(old).exists());
        assert_eq!(
            std::fs::read(outside.join("sentinel")).unwrap(),
            b"untouched"
        );
        for name in ["zulu-21", "zulu-25", "zulu-21-custom", "liberica-nik-21"] {
            assert!(base.join(name).is_dir());
        }
    }

    #[tokio::test]
    async fn busy_runtime_defers_without_a_network_request() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let lease = java_usage::shared(&root, 21).await.unwrap();
        let mut state = UpdateState::default();
        assert!(matches!(
            refresh(&root, temp.path(), 21, &mut state, None, false)
                .await
                .unwrap(),
            Refresh::Cached { deferred: true }
        ));
        drop(lease);
    }
}
