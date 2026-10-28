//! Explicit, per-generation Java choices. Browsing reads metadata, never runs a candidate.

use super::{java, paths::LauncherPaths};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

pub type Overrides = BTreeMap<u32, String>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeChoice {
    pub path: PathBuf,
    pub major: u32,
    pub java_version: String,
    pub vendor: String,
}

pub fn override_for<'a>(
    required: u32,
    overrides: &'a Overrides,
    legacy: Option<&'a str>,
) -> Option<&'a str> {
    let path = overrides
        .get(&java::runtime_major(required))
        .map(String::as_str)
        .or(legacy);
    path.filter(|path| !path.trim().is_empty())
}

pub fn inspect(path: &Path, required: u32) -> Result<RuntimeChoice, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute Java executable path".into());
    }
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
    {
        return Err("Choose a local Java runtime, not a network or device path".into());
    }
    let binary = if path.is_dir() {
        path.join("bin").join(java::JAVA_BIN)
    } else {
        path.to_path_buf()
    };
    if binary.file_name().and_then(|name| name.to_str()) != Some(java::JAVA_BIN)
        || !binary.is_file()
    {
        return Err(format!(
            "Choose the runtime's bin/{} executable",
            java::JAVA_BIN
        ));
    }
    let resolved = binary
        .canonicalize()
        .map_err(|e| format!("Cannot resolve Java executable: {e}"))?;
    let home = resolved
        .parent()
        .and_then(Path::parent)
        .ok_or("Java runtime directory is missing")?;
    let mut metadata = Vec::new();
    if !home.join("release").is_file() {
        return Err("Java release metadata is missing beside bin".into());
    }
    std::fs::File::open(home.join("release"))
        .map_err(|_| "Java release metadata is missing beside bin; choose a complete JRE or JDK")?
        .take(65537)
        .read_to_end(&mut metadata)
        .map_err(|e| format!("Cannot read Java release metadata: {e}"))?;
    if metadata.len() > 65536 {
        return Err("Java release metadata is too large".into());
    }
    let metadata = std::str::from_utf8(&metadata).map_err(|_| "Invalid Java release metadata")?;
    let property = |key| {
        metadata.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name == key).then(|| value.trim().trim_matches('"').to_string())
        })
    };
    let version = property("JAVA_VERSION").ok_or("Java release metadata has no version")?;
    if let Some(arch) = property("OS_ARCH") {
        java::validate_architecture(&arch)?;
    }
    let major = super::java_inventory::version_major(&version)?;
    let required = java::runtime_major(required);
    if major != required {
        return Err(format!(
            "Expected Java {required}, but this runtime is Java {major}"
        ));
    }
    Ok(RuntimeChoice {
        path: binary,
        major,
        java_version: property("JAVA_RUNTIME_VERSION").unwrap_or(version),
        vendor: property("IMPLEMENTOR").unwrap_or_else(|| "Unknown".into()),
    })
}

pub fn inspect_external(
    path: &Path,
    required: u32,
    paths: &LauncherPaths,
) -> Result<RuntimeChoice, String> {
    let choice = inspect(path, required)?;
    let managed = paths.java_versions();
    if managed.try_exists().map_err(|e| e.to_string())?
        && choice
            .path
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(managed.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err(
            "For Hikyou-managed runtimes, restore Automatic instead of pinning a managed file"
                .into(),
        );
    }
    Ok(choice)
}

pub async fn resolve(
    required: u32,
    paths: &LauncherPaths,
    overrides: &Overrides,
    legacy: Option<&str>,
) -> Result<java::JavaInstall, String> {
    let Some(path) = override_for(required, overrides, legacy) else {
        return java::get_or_install_java(required, paths).await;
    };
    let choice = inspect_external(Path::new(path), required, paths)?;
    let lease = super::java_usage::for_path(&choice.path, &paths.java_versions()).await?;
    let mut install = java::probe_java(&choice.path, choice.major).await?;
    install.lease = lease;
    Ok(install)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn per_major_choice_precedes_legacy_and_java_16_uses_17() {
        let choices = Overrides::from([(17, "custom-17".into()), (21, " ".into())]);
        assert_eq!(override_for(16, &choices, Some("old")), Some("custom-17"));
        assert_eq!(override_for(21, &choices, Some("old")), None);
        assert_eq!(override_for(25, &choices, Some("old")), Some("old"));
        assert_eq!(
            override_for(8, &Overrides::from([(8, String::new())]), Some("old")),
            None
        );
        assert_eq!(override_for(8, &choices, None), None);
    }
    #[test]
    fn inspection_is_read_only_and_rejects_wrong_major_and_incomplete_runtimes() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let java = bin.join(java::JAVA_BIN);
        std::fs::write(
            &java,
            b"not an executable; must never be run during selection",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("release"),
            "JAVA_VERSION=\"21.0.8\"\nIMPLEMENTOR=\"Fixture\"\n",
        )
        .unwrap();
        assert_eq!(inspect(&java, 21).unwrap().major, 21);
        assert!(inspect(&java, 17).unwrap_err().contains("Expected Java 17"));
        assert!(inspect(Path::new("relative/bin/java"), 21).is_err());
        assert!(inspect(temp.path(), 21).is_ok());
        std::fs::write(
            temp.path().join("release"),
            "JAVA_VERSION=\"21\"\nOS_ARCH=\"wrong-architecture\"\n",
        )
        .unwrap();
        assert!(inspect(&java, 21).unwrap_err().contains("architecture"));
        std::fs::write(temp.path().join("release"), vec![b'A'; 65537]).unwrap();
        assert!(inspect(&java, 21).unwrap_err().contains("too large"));
        std::fs::remove_file(temp.path().join("release")).unwrap();
        assert!(inspect(&java, 21).is_err());
    }
    #[tokio::test]
    #[ignore = "requires HIKYOU_TEST_JAVA and HIKYOU_TEST_JAVA_MAJOR pointing to a trusted local runtime"]
    async fn live_custom_runtime_is_resolved_and_probed_without_installation() {
        let executable = std::env::var("HIKYOU_TEST_JAVA").expect("set HIKYOU_TEST_JAVA");
        let major = std::env::var("HIKYOU_TEST_JAVA_MAJOR")
            .expect("set HIKYOU_TEST_JAVA_MAJOR")
            .parse()
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let overrides = Overrides::from([(major, executable.clone())]);
        let install = resolve(major, &paths, &overrides, None).await.unwrap();
        assert_eq!(install.major_version, major);
        assert_eq!(install.path, Path::new(&executable));
        assert!(
            !paths.java_versions().exists(),
            "custom selection must not download or modify managed runtimes"
        );
    }

    #[test]
    fn managed_files_cannot_be_pinned_against_cleanup_and_auto_updates() {
        let temp = tempfile::tempdir().unwrap();
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let home = paths.java_versions().join("zulu-21");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(home.join("bin").join(java::JAVA_BIN), b"not executable").unwrap();
        std::fs::write(home.join("release"), "JAVA_VERSION=\"21\"\n").unwrap();
        assert!(
            inspect_external(&home, 21, &paths)
                .unwrap_err()
                .contains("Automatic")
        );
    }
}
