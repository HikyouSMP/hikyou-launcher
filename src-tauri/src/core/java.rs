//! Managed Azul Zulu runtimes. No automatic system-Java or distribution fallback.

pub use super::java_updates::ManualCheckResult;
use super::paths::LauncherPaths;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[cfg(windows)]
pub const JAVA_BIN: &str = "java.exe";
#[cfg(not(windows))]
pub const JAVA_BIN: &str = "java";

#[derive(Debug, Clone)]
pub struct JavaInstall {
    pub path: PathBuf,
    pub major_version: u32,
    pub lease: Option<Arc<std::fs::File>>,
}

pub async fn get_or_install_java(major: u32, paths: &LauncherPaths) -> Result<JavaInstall, String> {
    super::java_updates::ensure(runtime_major(major), paths).await
}

pub(super) fn runtime_major(required: u32) -> u32 {
    if required == 16 { 17 } else { required }
}

pub async fn for_installer(
    paths: &LauncherPaths,
    minecraft_version: &str,
    overrides: &super::java_selection::Overrides,
    legacy: Option<&str>,
) -> Result<JavaInstall, String> {
    let file = paths
        .versions()
        .join(minecraft_version)
        .join(format!("{minecraft_version}.json"));
    let bytes = tokio::fs::read(file)
        .await
        .map_err(|e| format!("failed to read Minecraft Java requirements: {e}"))?;
    #[derive(serde::Deserialize)]
    struct Requirements {
        #[serde(rename = "javaVersion")]
        java_version: Option<RequiredJava>,
    }
    #[derive(serde::Deserialize)]
    struct RequiredJava {
        #[serde(rename = "majorVersion")]
        major_version: u32,
    }
    let version: Requirements = serde_json::from_slice(&bytes)
        .map_err(|e| format!("invalid Minecraft Java requirements: {e}"))?;
    let major = version
        .java_version
        .map(|java| java.major_version)
        .unwrap_or(8);
    super::java_selection::resolve(major, paths, overrides, legacy).await
}

pub fn runtime_status(
    paths: &LauncherPaths,
) -> Result<super::java_inventory::RuntimeStatus, String> {
    super::java_inventory::status(paths)
}

pub async fn check_updates(paths: &LauncherPaths) -> Result<ManualCheckResult, String> {
    super::java_updates::check_installed(paths).await
}

pub async fn maintain(paths: Arc<LauncherPaths>) {
    // The first launch also checks on demand; keep setup responsive.
    tokio::time::sleep(Duration::from_secs(10)).await;
    loop {
        match runtime_status(&paths) {
            Ok(status) => {
                let majors: std::collections::BTreeSet<_> = status
                    .installations
                    .iter()
                    .map(|runtime| runtime_major(runtime.major))
                    .collect();
                for major in majors {
                    if let Err(error) = get_or_install_java(major, &paths).await {
                        log::warn!("[java] Maintenance for Java {major} failed: {error}");
                    }
                    if let Err(error) = super::java_updates::cleanup(major, &paths).await {
                        log::warn!("[java] Cleanup for Java {major} deferred: {error}");
                    }
                }
            }
            Err(error) => log::warn!("[java] Cannot inspect managed runtimes: {error}"),
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn read_version_output(reader: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;
    const LIMIT: u64 = 64 * 1024;
    let mut bytes = Vec::new();
    reader
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Java version output is too large".into());
    }
    Ok(bytes)
}

pub(super) async fn probe_java(path: &Path, required_major: u32) -> Result<JavaInstall, String> {
    let mut command = tokio::process::Command::new(path);
    command
        .arg("-XshowSettings:properties")
        .arg("-version")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let text = tokio::time::timeout(Duration::from_secs(15), async {
        let mut child = command
            .spawn()
            .map_err(|e| format!("failed to execute Java: {e}"))?;
        let stdout = child.stdout.take().ok_or("Java stdout is unavailable")?;
        let stderr = child.stderr.take().ok_or("Java stderr is unavailable")?;
        let (out, err) =
            tokio::try_join!(read_version_output(stdout), read_version_output(stderr))?;
        if !child.wait().await.map_err(|e| e.to_string())?.success() {
            return Err("Java version check failed".to_string());
        }
        Ok(String::from_utf8_lossy(&err).to_string() + &String::from_utf8_lossy(&out))
    })
    .await
    .map_err(|_| "Java version check timed out")??;
    validate_version_output(&text, required_major)?;
    Ok(JavaInstall {
        path: path.to_path_buf(),
        major_version: required_major,
        lease: None,
    })
}

pub(super) fn validate_architecture(arch: &str) -> Result<(), String> {
    let normalized = match arch {
        "amd64" | "x86_64" | "x64" => "x86_64",
        "arm64" | "aarch64" => "aarch64",
        other => other,
    };
    if normalized != std::env::consts::ARCH {
        return Err(format!(
            "Java architecture {arch} does not match this launcher ({}); use Automatic to install the correct 64-bit Azul Zulu runtime",
            std::env::consts::ARCH
        ));
    }
    Ok(())
}

fn validate_version_output(text: &str, required_major: u32) -> Result<(), String> {
    let property = |key| {
        text.lines().find_map(|line| {
            let (name, value) = line.trim().split_once('=')?;
            (name.trim() == key).then(|| value.trim())
        })
    };
    if property("sun.arch.data.model") != Some("64") {
        return Err("Java must report a 64-bit data model; use Automatic to install the correct Azul Zulu runtime".into());
    }
    validate_architecture(property("os.arch").ok_or("Java did not report its architecture")?)?;
    let version = text
        .lines()
        .find_map(|line| {
            line.contains("version")
                .then(|| line.split('"').nth(1))
                .flatten()
        })
        .ok_or("Java did not report a version")?;
    let major = super::java_inventory::version_major(version)?;
    if major != required_major {
        return Err(format!(
            "expected Java {required_major}, received Java {major}"
        ));
    }
    Ok(())
}

pub fn get_max_memory_mb() -> u64 {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};
    let sys = System::new_with_specifics(
        RefreshKind::new().with_memory(MemoryRefreshKind::new().with_ram()),
    );
    sys.total_memory() / 1024 / 1024
}

pub fn infer_java_from_main_class(_main_class: &str) -> u32 {
    8
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn version_probe_output_is_bounded() {
        assert_eq!(
            read_version_output(&b"openjdk version\"21\""[..])
                .await
                .unwrap()
                .len(),
            19
        );
        assert!(
            read_version_output(&vec![b'A'; 65537][..])
                .await
                .unwrap_err()
                .contains("too large")
        );
        assert_eq!(
            read_version_output(&vec![b'A'; 65536][..])
                .await
                .unwrap()
                .len(),
            65536
        );
    }
    #[test]
    fn java_16_requirements_use_17_without_changing_other_generations() {
        assert_eq!(runtime_major(16), 17);
        for major in [8, 17, 21, 25] {
            assert_eq!(runtime_major(major), major);
        }
    }

    #[test]
    fn version_probe_rejects_32_bit_wrong_architecture_and_missing_properties() {
        for major in [8, 17, 21, 25] {
            let version = if major == 8 {
                "1.8.0_512".into()
            } else {
                format!("{major}.0.1")
            };
            let output = format!(
                "Property settings:\n    os.arch = {}\n    sun.arch.data.model = 64\nopenjdk version \"{version}\"\n",
                std::env::consts::ARCH
            );
            validate_version_output(&output, major).unwrap();
            assert!(validate_version_output(&output.replace("= 64", "= 32"), major).is_err());
            assert!(
                validate_version_output(&output.replace(std::env::consts::ARCH, "i386"), major)
                    .is_err()
            );
            assert!(validate_version_output(&output, major + 1).is_err());
        }
        assert!(validate_version_output("openjdk version \"17.0.21\"", 17).is_err());
    }
}
