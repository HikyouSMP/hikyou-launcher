//! JVMの検索・検証・自動ダウンロード
//!
//! 方針: システムJavaは一切使わず、常にランチャー管理下のJDKを使用する。
//!       (Modrinth / Prism Launcher と同じ方針)
//!       これによりユーザー環境のJavaバージョンに依存した問題を排除する。
//!
//! Java 8    → Azul Zulu    (Apple Silicon対応・安定性重視・1.0〜1.16向け)
//! Java 16   → Azul Zulu    (Liberica NIK はJDK16を提供していないため)
//! Java 17   → Liberica NIK (GraalVM CE ベース・BellSoftが品質保証・公式API有)
//! Java 21   → Liberica NIK (同上)
//! Java 25   → Liberica NIK (Minecraft 26.1+ / Java 25 LTS 対応)

use crate::core::paths::LauncherPaths;
use reqwest::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
pub const JAVA_BIN: &str = "java.exe";
#[cfg(not(target_os = "windows"))]
pub const JAVA_BIN: &str = "java";

#[derive(Debug, Clone)]
pub struct JavaInstall {
    pub path: PathBuf,
    pub major_version: u32,
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// 指定バージョンのJavaを返す。管理JDKがなければ自動ダウンロードする。
/// システムJavaは使用しない。
pub async fn get_or_install_java(
    required_major: u32,
    paths: &LauncherPaths,
) -> Result<JavaInstall, String> {
    let dir_name = managed_dir_name(required_major);
    let managed_bin = paths.java_version_dir(&dir_name).join("bin").join(JAVA_BIN);

    // ── 管理JDKが存在するか確認 ──────────────────────────────────────────
    if managed_bin.exists() {
        match probe_java(&managed_bin) {
            Ok(install) => {
                log::info!(
                    "Using managed JDK: {} (Java {})",
                    dir_name,
                    install.major_version
                );
                return Ok(install);
            }
            Err(e) => {
                // 破損している → 削除して再DL
                log::warn!("Managed JDK is corrupted. Re-downloading: {}", e);
                let dir = paths.java_version_dir(&dir_name);
                if dir.exists() {
                    let _ = std::fs::remove_dir_all(&dir);
                }
            }
        }
    }

    // ── 管理JDKが存在しない → 自動ダウンロード ───────────────────────────
    log::info!("Downloading Java {} ({})...", required_major, dir_name);
    auto_install_java(required_major, paths).await
}

pub fn get_max_memory_mb() -> u64 {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};
    let sys = System::new_with_specifics(
        RefreshKind::new().with_memory(MemoryRefreshKind::new().with_ram()),
    );
    sys.total_memory() / 1024 / 1024
}

/// mainClass からJavaバージョンを推定する (javaVersion フィールドがない旧バージョン向け)
pub fn infer_java_from_main_class(main_class: &str) -> u32 {
    // LaunchWrapper は Java 9+ の URLClassLoader 廃止で動かない → Java 8 必須
    if main_class.contains("launchwrapper") {
        return 8;
    }
    // バニラ 1.6〜1.16 は javaVersion フィールドなし → Java 8 で動く
    8
}

// ────────────────────────────────────────────────────────────────────────────
// ディストリビューション選択
// ────────────────────────────────────────────────────────────────────────────

/// 管理ディレクトリ名を返す
/// Java 8/16 → zulu-{N}
/// Java 17/21 → liberica-nik-{N}
fn managed_dir_name(major: u32) -> String {
    if uses_liberica_nik(major) {
        format!("liberica-nik-{}", major)
    } else {
        format!("zulu-{}", major)
    }
}

/// Liberica NIK が対応しているJavaバージョンか
fn uses_liberica_nik(major: u32) -> bool {
    // NIK 25 は bundle-type=standard で Windows/macOS/Linux 全対応
    matches!(major, 17 | 21 | 25)
}

// ────────────────────────────────────────────────────────────────────────────
// インストールディスパッチ
// ────────────────────────────────────────────────────────────────────────────

async fn auto_install_java(
    major_version: u32,
    paths: &LauncherPaths,
) -> Result<JavaInstall, String> {
    if uses_liberica_nik(major_version) {
        install_liberica_nik(major_version, paths).await
    } else {
        install_zulu(major_version, paths).await
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Azul Zulu インストール (Java 8 / 16 向け)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AzulPackage {
    pub download_url: String,
    pub name: String,
}

async fn install_zulu(major_version: u32, paths: &LauncherPaths) -> Result<JavaInstall, String> {
    let client = Client::new();
    let (os_str, arch_str) = azul_os_arch()?;

    let api_url = format!(
        "https://api.azul.com/metadata/v1/zulu/packages\
         ?java_version={major}\
         &os={os}\
         &arch={arch}\
         &archive_type=zip\
         &javafx_bundled=false\
         &java_package_type=jre\
         &page_size=1",
        major = major_version,
        os = os_str,
        arch = arch_str,
    );

    log::info!("Fetching Zulu {} metadata...", major_version);

    let packages: Vec<AzulPackage> = client
        .get(&api_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Azul API request failed: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse Azul API response: {}", e))?;

    let package = packages.first().ok_or_else(|| {
        format!(
            "No Zulu {} package found (OS: {}, Arch: {})",
            major_version, os_str, arch_str
        )
    })?;

    log::info!("Downloading Zulu {}: {}", major_version, package.name);
    let bytes = download_bytes(&client, &package.download_url).await?;

    let install_base = paths.java_versions();
    std::fs::create_dir_all(&install_base)
        .map_err(|e| format!("Failed to create java_versions directory: {}", e))?;

    let dest_dir = install_base.join(managed_dir_name(major_version));
    extract_and_rename_zip(&bytes, &install_base, &dest_dir)?;

    let java_bin = find_java_bin_in_dir(&dest_dir);

    #[cfg(not(target_os = "windows"))]
    make_executable(&java_bin)?;

    log::info!(
        "Zulu {} installation complete: {:?}",
        major_version,
        java_bin
    );
    probe_java(&java_bin)
}

// ────────────────────────────────────────────────────────────────────────────
// Liberica NIK インストール (Java 17 / 21 向け)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LibericaNikRelease {
    #[serde(rename = "downloadUrl")]
    pub download_url: String,
    #[serde(rename = "packageType")]
    pub package_type: String,
    pub filename: String,
}

async fn install_liberica_nik(
    major_version: u32,
    paths: &LauncherPaths,
) -> Result<JavaInstall, String> {
    let client = Client::new();
    let (os_str, arch_str, pkg_type) = liberica_os_arch()?;

    // BellSoft 公式 REST API
    // https://api.bell-sw.com/v1/nik/releases
    // component-version=liberica@{N}: 対象JDKバージョンを指定
    // bundle-type=core:     NIK 17/21: JDK + native-image のみ (言語プラグインなし)
    // bundle-type=standard: NIK 25+:   "core" が廃止されたため "standard" を使用
    let bundle_type = if major_version >= 25 {
        "standard"
    } else {
        "core"
    };
    let api_url = format!(
        "https://api.bell-sw.com/v1/nik/releases\
         ?version-modifier=latest\
         &component-version=liberica@{major}\
         &os={os}\
         &arch={arch}\
         &bitness=64\
         &bundle-type={bundle}\
         &package-type={pkg}",
        major = major_version,
        os = os_str,
        arch = arch_str,
        bundle = bundle_type,
        pkg = pkg_type,
    );

    log::info!("Fetching Liberica NIK {} metadata...", major_version);

    let resp_bytes = client
        .get(&api_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Liberica NIK API request failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read Liberica NIK API response: {}", e))?;

    let releases: Vec<LibericaNikRelease> = serde_json::from_slice(&resp_bytes).map_err(|e| {
        let body = String::from_utf8_lossy(&resp_bytes);
        let preview = if body.len() > 300 {
            &body[..300]
        } else {
            &body
        };
        format!(
            "failed to parse Liberica NIK API response: {} | body: {}",
            e, preview
        )
    })?;

    let release = releases.first().ok_or_else(|| {
        format!(
            "Liberica NIK {}  packagewas not found (OS: {}, Arch: {})",
            major_version, os_str, arch_str
        )
    })?;

    log::info!(
        "Downloading Liberica NIK {}: {}",
        major_version,
        release.filename
    );
    let bytes = download_bytes(&client, &release.download_url).await?;

    let install_base = paths.java_versions();
    std::fs::create_dir_all(&install_base)
        .map_err(|e| format!("Failed to create java_versions directory: {}", e))?;

    let dest_dir = install_base.join(managed_dir_name(major_version));

    if release.package_type == "zip" {
        extract_and_rename_zip(&bytes, &install_base, &dest_dir)?;
    } else {
        extract_and_rename_targz(&bytes, &install_base, &dest_dir)?;
    }

    let java_bin = find_java_bin_in_dir(&dest_dir);

    #[cfg(not(target_os = "windows"))]
    make_executable(&java_bin)?;

    log::info!(
        "Liberica NIK {} installation complete: {:?}",
        major_version,
        java_bin
    );
    probe_java(&java_bin)
}

// ────────────────────────────────────────────────────────────────────────────
// アーカイブ展開
// ────────────────────────────────────────────────────────────────────────────

/// ZIPを展開し、トップレベルディレクトリを dest_dir にリネームする
fn extract_and_rename_zip(
    bytes: &[u8],
    install_base: &PathBuf,
    dest_dir: &PathBuf,
) -> Result<(), String> {
    if dest_dir.exists() {
        std::fs::remove_dir_all(dest_dir)
            .map_err(|e| format!("failed to remove old JDK: {}", e))?;
    }

    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to parse ZIP: {}", e))?;

    // トップレベルのディレクトリ名を取得
    let top_dir_name = {
        let first = archive
            .by_index(0)
            .map_err(|e| format!("failed to read ZIP entry: {}", e))?;
        first.name().split('/').next().unwrap_or("").to_string()
    };

    archive
        .extract(install_base)
        .map_err(|e| format!("failed to extract ZIP: {}", e))?;

    let extracted = install_base.join(&top_dir_name);
    let jre_root = find_jre_root(&extracted);

    std::fs::rename(&jre_root, dest_dir).map_err(|e| {
        format!(
            "failed to rename JDK directory ({:?} → {:?}): {}",
            jre_root, dest_dir, e
        )
    })?;

    Ok(())
}

/// tar.gz を展開し、トップレベルディレクトリを dest_dir にリネームする
#[cfg(not(target_os = "windows"))]
fn extract_and_rename_targz(
    bytes: &[u8],
    install_base: &PathBuf,
    dest_dir: &PathBuf,
) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    if dest_dir.exists() {
        std::fs::remove_dir_all(dest_dir)
            .map_err(|e| format!("failed to remove old JDK: {}", e))?;
    }

    let gz = GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = Archive::new(gz);
    archive
        .unpack(install_base)
        .map_err(|e| format!("failed to extract tar.gz: {}", e))?;

    let extracted = std::fs::read_dir(install_base)
        .map_err(|e| format!("failed to read directory: {}", e))?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            if !p.is_dir() || p == dest_dir {
                return false;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            name.contains("zulu") || name.contains("bellsoft") || name.contains("graalvm")
        })
        .ok_or("extracted JDK directorywas not found")?;

    let jre_root = find_jre_root(&extracted);

    std::fs::rename(&jre_root, dest_dir).map_err(|e| {
        format!(
            "failed to rename JDK directory ({:?} → {:?}): {}",
            jre_root, dest_dir, e
        )
    })?;

    Ok(())
}

#[cfg(target_os = "windows")]
fn extract_and_rename_targz(
    _bytes: &[u8],
    _install_base: &PathBuf,
    _dest_dir: &PathBuf,
) -> Result<(), String> {
    Err("tar.gz archives are not used on Windows".to_string())
}

/// 展開ディレクトリ内で bin/java が存在するルートを探す
/// 通常: dir/bin/java
/// macOS Liberica NIK: dir/Contents/Home/bin/java
/// macOS Zulu .jre: dir/zulu-N.jre/Contents/Home/bin/java
fn find_jre_root(dir: &PathBuf) -> PathBuf {
    if dir.join("bin").join(JAVA_BIN).exists() {
        return dir.clone();
    }
    let macos_home = dir.join("Contents").join("Home");
    if macos_home.join("bin").join(JAVA_BIN).exists() {
        return macos_home;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.ends_with(".jre") || name.ends_with(".jdk") {
                let candidate = path.join("Contents").join("Home");
                if candidate.join("bin").join(JAVA_BIN).exists() {
                    return candidate;
                }
            }
        }
    }
    dir.clone()
}

fn find_java_bin_in_dir(dest_dir: &Path) -> PathBuf {
    dest_dir.join("bin").join(JAVA_BIN)
}

// ────────────────────────────────────────────────────────────────────────────
// ダウンロードユーティリティ
// ────────────────────────────────────────────────────────────────────────────

async fn download_bytes(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let res = client
        .get(url)
        .header("User-Agent", "hikyou-launcher")
        .send()
        .await
        .map_err(|e| format!("download request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("download failed {} ({})", url, res.status()));
    }

    res.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("failed to read download response: {}", e))
}

#[cfg(not(target_os = "windows"))]
fn make_executable(path: &PathBuf) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|e| format!("failed to read permissions: {}", e))?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .map_err(|e| format!("failed to set executable permission: {}", e))
}

// ────────────────────────────────────────────────────────────────────────────
// OS / アーキテクチャ文字列
// ────────────────────────────────────────────────────────────────────────────

fn azul_os_arch() -> Result<(&'static str, &'static str), String> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        other => return Err(format!("unsupported OS: {}", other)),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => return Err(format!("unsupported architecture: {}", other)),
    };
    Ok((os, arch))
}

/// Liberica NIK API 用の (os, arch, package_type) を返す
fn liberica_os_arch() -> Result<(&'static str, &'static str, &'static str), String> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        other => return Err(format!("unsupported OS: {}", other)),
    };
    // Liberica NIK API の arch パラメータ: x86_64 → "x86", aarch64 → "arm"
    // （API は "aarch64" を受け付けない: {"errorCode":400,"parameterName":"arch","parameterValue":"aarch64"}）
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86",
        "aarch64" => "arm",
        other => return Err(format!("unsupported architecture: {}", other)),
    };
    let pkg = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    };
    Ok((os, arch, pkg))
}

// ────────────────────────────────────────────────────────────────────────────
// Java バージョン検証
// ────────────────────────────────────────────────────────────────────────────

fn probe_java(path: &PathBuf) -> Result<JavaInstall, String> {
    if !path.exists() {
        return Err(format!("java binary does not exist: {:?}", path));
    }

    let mut cmd = std::process::Command::new(path);
    cmd.arg("-version");

    // Windows ではコンソールウィンドウを非表示にする
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to execute java: {}", e))?;

    let version_str = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);

    let major = parse_java_major_version(&version_str)?;
    Ok(JavaInstall {
        path: path.clone(),
        major_version: major,
    })
}

fn parse_java_major_version(output: &str) -> Result<u32, String> {
    let line = output
        .lines()
        .find(|l| l.contains("version"))
        .ok_or("version information was not found")?;

    let version_str = line
        .split('"')
        .nth(1)
        .ok_or("failed to parse version string")?;

    let first = version_str.split('.').next().unwrap_or("0");
    let major: u32 = if first == "1" {
        version_str
            .split('.')
            .nth(1)
            .unwrap_or("8")
            .parse()
            .unwrap_or(8)
    } else {
        first.parse().unwrap_or(8)
    };
    Ok(major)
}
