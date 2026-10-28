//! Read-only inventory and atomic selection metadata; stored paths are relative.

use super::{java::JAVA_BIN, java_catalog::Package, managed_fs, paths::LauncherPaths};
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub(super) const CHECK_INTERVAL: i64 = 24 * 60 * 60;
pub(super) const RETRY_INTERVAL: i64 = 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RuntimeRecord {
    pub directory: String,
    pub package: Package,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct UpdateState {
    pub runtime: Option<RuntimeRecord>,
    pub last_attempt_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub deferred: bool,
    pub cleanup_error: Option<String>,
}

impl UpdateState {
    pub fn next_check_at(&self) -> Option<i64> {
        if self.deferred {
            self.last_attempt_at.map(|time| time.saturating_add(60))
        } else if self.last_error.is_some() {
            self.last_attempt_at
                .map(|time| time.saturating_add(RETRY_INTERVAL))
        } else {
            self.last_checked_at
                .map(|time| time.saturating_add(CHECK_INTERVAL))
        }
    }
    pub fn due(&self, now: i64) -> bool {
        self.next_check_at()
            .is_none_or(|next| next <= now || next > now.saturating_add(CHECK_INTERVAL))
    }
}

fn state_path(major: u32) -> PathBuf {
    PathBuf::from(format!(".zulu-{major}.json"))
}

pub(super) fn load_state(root: &Dir, major: u32) -> Result<UpdateState, String> {
    let path = state_path(major);
    if managed_fs::inspect(root, &path)?.is_none() {
        return Ok(UpdateState::default());
    }
    let bytes = read_small(root, &path)?;
    let state: UpdateState =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid Java update state: {e}"))?;
    if let Some(record) = &state.runtime {
        if !managed_directory(major, &record.directory)
            || record.package.java_version.first() != Some(&major)
        {
            return Err("invalid managed Java selection".to_string());
        }
        managed_fs::relative_path(&record.directory)?;
    }
    Ok(state)
}

pub(super) fn managed_directory(major: u32, name: &str) -> bool {
    name == format!("zulu-{major}")
        || name
            .strip_prefix(&format!("zulu-{major}-"))
            .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok())
}

pub(super) fn save_state(root: &Dir, major: u32, state: &UpdateState) -> Result<(), String> {
    let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
    managed_fs::write(root, &state_path(major), &bytes, true)
}

fn read_small(root: &Dir, path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    managed_fs::read_file(root, path)?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("failed to read Java metadata: {e}"))?;
    if bytes.len() > 65536 {
        return Err("Java metadata exceeds 64 KiB".to_string());
    }
    Ok(bytes)
}

pub(super) fn version_major(version: &str) -> Result<u32, String> {
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts
        .next()
        .ok_or("missing Java version")?
        .parse()
        .map_err(|_| "invalid Java version")?;
    let major = if first == 1 {
        parts
            .next()
            .ok_or("missing legacy Java version")?
            .parse()
            .map_err(|_| "invalid legacy Java version")?
    } else {
        first
    };
    if major == 0 {
        return Err("invalid Java major version".to_string());
    }
    Ok(major)
}

pub(super) struct Release {
    pub version: String,
    pub major: u32,
    pub vendor: String,
    pub architecture: Option<String>,
}

pub(super) fn release(root: &Dir, directory: &str) -> Result<Release, String> {
    managed_fs::relative_path(directory)?;
    let dir = managed_fs::directory(root, Path::new(directory), false)?;
    let bytes = read_small(&dir, Path::new("release"))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Java release metadata is not UTF-8")?;
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name == key).then(|| value.trim_matches('"').to_string())
        })
    };
    let base_version = value("JAVA_VERSION").ok_or("missing JAVA_VERSION in release metadata")?;
    let major = version_major(&base_version)?;
    let version = value("JAVA_RUNTIME_VERSION").unwrap_or(base_version);
    let vendor = value("IMPLEMENTOR").unwrap_or_else(|| "Unknown".to_string());
    let java = managed_fs::inspect(&dir, &Path::new("bin").join(JAVA_BIN))?;
    if !java.is_some_and(|m| m.is_file()) {
        return Err("managed Java binary is missing".to_string());
    }
    Ok(Release {
        version,
        major,
        vendor,
        architecture: value("OS_ARCH"),
    })
}

pub(super) fn zulu_release(root: &Dir, directory: &str, major: u32) -> Result<Release, String> {
    let release = release(root, directory)?;
    if release.major != major || !release.vendor.to_ascii_lowercase().contains("azul") {
        return Err("runtime is not the expected Azul Zulu Java version".to_string());
    }
    if let Some(arch) = &release.architecture {
        super::java::validate_architecture(arch)?;
    }
    Ok(release)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedRuntime {
    pub executable: PathBuf,
    pub directory: String,
    pub major: u32,
    pub java_version: String,
    pub vendor: String,
    pub selected: bool,
    pub legacy: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub major: u32,
    pub checking: bool,
    pub deferred: bool,
    pub cleanup_error: Option<String>,
    pub last_checked_at: Option<i64>,
    pub last_attempt_at: Option<i64>,
    pub next_check_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub installations: Vec<ManagedRuntime>,
    pub checks: Vec<UpdateCheck>,
}

pub(super) fn status(paths: &LauncherPaths) -> Result<RuntimeStatus, String> {
    let path = paths.java_versions();
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(RuntimeStatus::default());
    }
    let root = managed_fs::root(&path)?;
    let mut status = RuntimeStatus::default();
    let mut majors = std::collections::BTreeSet::new();
    for entry in root.entries().map_err(|e| e.to_string())? {
        let name = entry
            .map_err(|e| e.to_string())?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if let Some(major) = name
            .strip_prefix(".zulu-")
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<u32>().ok())
        {
            majors.insert(major);
        }
        let previous_provider = name.starts_with("liberica-nik-");
        let Some(major) = name
            .strip_prefix("zulu-")
            .or_else(|| name.strip_prefix("liberica-nik-"))
            .and_then(|s| s.split('-').next())
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        majors.insert(major);
        let legacy = previous_provider || major == 16 || !managed_directory(major, &name);
        let state = load_state(&root, major)?;
        let info = release(&root, &name);
        let (version, vendor, error) = match info {
            Ok(info) => {
                let error = info
                    .architecture
                    .as_deref()
                    .and_then(|arch| super::java::validate_architecture(arch).err());
                (info.version, info.vendor, error)
            }
            Err(error) => ("Unknown".to_string(), "Unknown".to_string(), Some(error)),
        };
        status.installations.push(ManagedRuntime {
            executable: paths.java_versions().join(&name).join("bin").join(JAVA_BIN),
            selected: !legacy
                && error.is_none()
                && vendor.to_ascii_lowercase().contains("azul")
                && (state.runtime.as_ref().is_some_and(|r| r.directory == name)
                    || (state.runtime.is_none() && name == format!("zulu-{major}"))),
            directory: name,
            major,
            java_version: version,
            vendor,
            legacy,
            error,
        });
    }
    for major in majors {
        let state = load_state(&root, major)?;
        status.checks.push(UpdateCheck {
            major,
            checking: checking(&root, major)?,
            deferred: state.deferred,
            last_checked_at: state.last_checked_at,
            last_attempt_at: state.last_attempt_at,
            next_check_at: state.next_check_at(),
            cleanup_error: state.cleanup_error,
            last_error: state.last_error,
        });
    }
    status.installations.sort_by(|a, b| {
        b.major
            .cmp(&a.major)
            .then(b.selected.cmp(&a.selected))
            .then(a.directory.cmp(&b.directory))
    });
    Ok(status)
}

fn checking(root: &Dir, major: u32) -> Result<bool, String> {
    let path = PathBuf::from(format!(".zulu-{major}.lock"));
    if managed_fs::inspect(root, &path)?.is_none() {
        return Ok(false);
    }
    let file = managed_fs::read_file(root, &path)?;
    match fs2::FileExt::try_lock_shared(&file) {
        Ok(()) => Ok(false),
        Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
            Ok(true)
        }
        Err(error) => Err(format!("failed to inspect Java update lock: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrong_architecture_is_not_selected_or_accepted_as_cached_zulu() {
        let temp = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let root = managed_fs::root_or_create(&paths.java_versions()).unwrap();
        let dir = managed_fs::directory(&root, Path::new("zulu-8"), true).unwrap();
        managed_fs::write(
            &dir,
            Path::new("release"),
            b"JAVA_VERSION=\"1.8.0_512\"\nIMPLEMENTOR=\"Azul Systems, Inc.\"\nOS_ARCH=\"i586\"\n",
            false,
        )
        .unwrap();
        managed_fs::write(
            &dir,
            &Path::new("bin").join(JAVA_BIN),
            b"not executed",
            false,
        )
        .unwrap();
        assert!(
            zulu_release(&root, "zulu-8", 8)
                .err()
                .unwrap()
                .contains("architecture")
        );
        let runtime = status(&paths).unwrap().installations.remove(0);
        assert!(!runtime.selected);
        assert_eq!(runtime.java_version, "1.8.0_512");
        assert!(runtime.error.unwrap().contains("architecture"));
    }

    #[test]
    fn inventory_lists_full_versions_without_executing_runtimes() {
        let temp = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let root = managed_fs::root_or_create(&paths.java_versions()).unwrap();
        for (directory, version, vendor) in [
            ("zulu-8", "1.8.0_504-b08", "Azul Systems, Inc."),
            ("zulu-16", "16.0.2+7", "Azul Systems, Inc."),
            ("liberica-nik-21", "21.0.8+9", "BellSoft"),
        ] {
            let dir = managed_fs::directory(&root, Path::new(directory), true).unwrap();
            let metadata = format!("JAVA_VERSION=\"{version}\"\nIMPLEMENTOR=\"{vendor}\"\n");
            managed_fs::write(&dir, Path::new("release"), metadata.as_bytes(), false).unwrap();
            // Deliberately not executable: inventory must only inspect metadata.
            managed_fs::write(
                &dir,
                &Path::new("bin").join(JAVA_BIN),
                b"not a program",
                false,
            )
            .unwrap();
        }
        let status = status(&paths).unwrap();
        assert_eq!(status.installations.len(), 3);
        let java8 = status.installations.iter().find(|r| r.major == 8).unwrap();
        assert_eq!(java8.java_version, "1.8.0_504-b08");
        assert!(java8.selected);
        for runtime in status.installations.iter().filter(|r| r.major != 8) {
            assert!(!runtime.selected);
            assert!(runtime.legacy);
        }
    }

    #[test]
    fn status_distinguishes_active_updates_from_interrupted_attempts() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        assert!(!checking(&root, 21).unwrap());
        let lock = root
            .open_with(
                ".zulu-21.lock",
                cap_std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true),
            )
            .unwrap()
            .into_std();
        assert!(!checking(&root, 21).unwrap());
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        assert!(checking(&root, 21).unwrap());
        drop(lock);
        assert!(!checking(&root, 21).unwrap());
    }
    #[test]
    fn versions_and_check_intervals_are_strict_and_clock_safe() {
        assert_eq!(version_major("1.8.0_504-b08").unwrap(), 8);
        assert_eq!(version_major("25.0.4.1+8-LTS").unwrap(), 25);
        for version in ["garbage", "0", "1.x"] {
            assert!(version_major(version).is_err());
        }
        let mut state = UpdateState {
            last_checked_at: Some(100),
            ..Default::default()
        };
        assert!(!state.due(101));
        assert!(state.due(100 + CHECK_INTERVAL));
        assert!(state.due(0));
        state.last_attempt_at = Some(200);
        state.last_error = Some("offline".to_string());
        assert!(!state.due(201));
        assert!(state.due(200 + RETRY_INTERVAL));
    }
    #[test]
    fn selection_metadata_cannot_escape_or_choose_another_major() {
        let temp = tempfile::tempdir().unwrap();
        let root = managed_fs::root(temp.path()).unwrap();
        let bad = br#"{"runtime":{"directory":"../outside","package":{"package_uuid":"00000000-0000-0000-0000-000000000000","java_version":[21],"distro_version":[21],"openjdk_build_number":1,"download_url":"https://cdn.azul.com/a","sha256_hash":""}},"last_attempt_at":null,"last_checked_at":null,"last_error":null}"#;
        managed_fs::write(&root, &state_path(21), bad, false).unwrap();
        assert!(load_state(&root, 21).is_err());
        let state = UpdateState {
            last_error: Some("offline".to_string()),
            ..Default::default()
        };
        save_state(&root, 8, &state).unwrap();
        assert_eq!(
            load_state(&root, 8).unwrap().last_error.as_deref(),
            Some("offline")
        );
    }
}
