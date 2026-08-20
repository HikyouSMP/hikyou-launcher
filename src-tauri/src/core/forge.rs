//! Forge Mod ローダーのサポート
//!
//! 〜1.20.1 向け。1.20.2 以降は NeoForge を使用。
//!
//! NeoForge と同様にインストーラー JAR を実行してライブラリを生成する。
//!   1. インストーラー JAR を ZIP として開き version.json を抽出
//!   2. インストーラーを `--installClient <dir>` で実行
//!   3. 生成されたライブラリを paths.libraries() にコピー

use crate::core::cache;
use crate::core::manifest::{
    ArgumentValue, Arguments, Library, LibraryArtifact, LibraryDownloads, VersionJson,
};
use crate::core::paths::LauncherPaths;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const CACHE_TYPE: &str = "forge_loader";
const CACHE_TTL: i64 = 3600;

// ────────────────────────────────────────────────────────────────────────────
// 公開型
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ForgeVersion {
    /// フルバージョン文字列 e.g. "1.21.1-52.0.27"
    pub version: String,
    pub stable: bool,
}

// ────────────────────────────────────────────────────────────────────────────
// installer JAR 内部の version.json
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FgVersionJson {
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub libraries: Vec<FgLibrary>,
    pub arguments: Option<FgArguments>,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FgInstallProfile {
    install: Option<FgInstallInfo>,
    #[serde(rename = "versionInfo")]
    version_info: FgVersionJson,
}

#[derive(Debug, Deserialize)]
struct FgInstallInfo {
    path: Option<String>,
    #[serde(rename = "filePath")]
    file_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForgeInstallerKind {
    ModernVersionJson,
    LegacyInstallProfile,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FgArguments {
    #[serde(default)]
    game: Vec<serde_json::Value>,
    #[serde(default)]
    jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FgLibrary {
    name: String,
    url: Option<String>,
    checksums: Option<Vec<String>>,
    downloads: Option<FgDownloads>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FgDownloads {
    artifact: Option<FgArtifact>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FgArtifact {
    path: String,
    url: String,
    #[serde(default)]
    sha1: String,
    #[serde(default)]
    size: u64,
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// MC バージョンに対応する Forge バージョン一覧を返す（新しい順、1 時間キャッシュ）。
pub async fn fetch_loader_versions(mc_version: &str) -> Result<Vec<ForgeVersion>, String> {
    if let Some(c) = cache::get()
        && let Some(cached) = c.get::<Vec<ForgeVersion>>(CACHE_TYPE, mc_version).await
    {
        return Ok(cached);
    }

    let meta_url = format!(
        "{}/net/minecraftforge/forge/maven-metadata.xml",
        FORGE_MAVEN
    );

    let resp = Client::new()
        .get(&meta_url)
        .send()
        .await
        .map_err(|e| format!("Failed to get Forge Maven metadata: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Failed to get Forge Maven metadata (HTTP {})",
            resp.status()
        ));
    }

    let xml = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read Forge Maven metadata: {}", e))?;

    let prefix = format!("{}-", mc_version);
    let mut versions: Vec<ForgeVersion> = xml
        .split("<version>")
        .skip(1)
        .filter_map(|chunk| {
            let v = chunk.split("</version>").next()?.trim().to_string();
            if v.starts_with(&prefix) {
                let stable = !v.contains("beta")
                    && !v.contains("alpha")
                    && !v.contains("rc")
                    && !v.contains("pre");
                Some(ForgeVersion { version: v, stable })
            } else {
                None
            }
        })
        .collect();

    versions.sort_by(|a, b| semver_cmp_desc(&a.version, &b.version));

    if versions.is_empty() {
        return Err(format!("No Forge found for Minecraft {}", mc_version));
    }

    if let Some(c) = cache::get() {
        c.set(CACHE_TYPE, mc_version, &versions, CACHE_TTL)
            .await
            .ok();
    }
    Ok(versions)
}

/// バニラ VersionJson に Forge の情報をマージして返す。
/// 必要に応じてインストーラーを実行しライブラリを生成する。
pub async fn build_forge_version_json(
    vanilla_json: &VersionJson,
    forge_full_version: &str,
    paths: &LauncherPaths,
) -> Result<VersionJson, String> {
    if forge_full_version.is_empty() {
        return Err("Forge version was not specified".to_string());
    }

    // ── 1. version.json 取得 ────────────────────────────────────────────────
    let (fg_json, installer_bytes, installer_kind) =
        fetch_forge_version_json_and_installer(forge_full_version, paths).await?;

    // ── 2. インストーラー実行（未実行の場合のみ） ────────────────────────────
    let safe = forge_full_version.replace(['.', '-'], "_");
    let marker = paths.forge_dir().join(format!("{}.installed", safe));

    if installer_kind == ForgeInstallerKind::LegacyInstallProfile {
        install_legacy_embedded_artifacts(&fg_json, &installer_bytes, paths)?;
        log::info!(
            "Forge {} uses legacy install_profile.json; installer execution is not required",
            forge_full_version
        );
    } else if !marker.exists() {
        run_forge_installer(
            forge_full_version,
            &vanilla_json.id,
            &installer_bytes,
            paths,
        )
        .await?;
        let _ = std::fs::write(&marker, b"ok");
        log::info!("Forge {} installation complete", forge_full_version);
    } else {
        log::info!("Forge {} already installed", forge_full_version);
    }

    // ── 3. vanilla VersionJson にマージ ─────────────────────────────────────
    // inheritsFrom の整合性チェック: Forge の version.json が想定と異なる MC バージョンを
    // 参照している場合は警告を出す（起動は続行する）
    if let Some(ref inherits) = fg_json.inherits_from
        && inherits != &vanilla_json.id
    {
        log::warn!(
            "Forge version.json inheritsFrom ({}) does not match the requested Minecraft version ({}). \
            The selected Forge version may be incompatible.",
            inherits,
            vanilla_json.id
        );
    }

    let mut merged = vanilla_json.clone();
    merged.main_class = fg_json.main_class;

    // Forge ライブラリを先頭に追加（クラスパスの優先順位確保）
    let fg_libs = fg_libs_to_vanilla(fg_json.libraries)?;
    let mut all_libs = fg_libs;
    all_libs.extend(merged.libraries.clone());
    merged.libraries = all_libs;

    let mut seen_paths = std::collections::HashSet::new();
    merged.libraries.retain(|lib| {
        if let Some(dl) = &lib.downloads
            && let Some(art) = &dl.artifact
        {
            return seen_paths.insert(art.path.clone());
        }
        true
    });

    if let Some(fg_args) = fg_json.arguments {
        let va = merged.arguments.get_or_insert(Arguments {
            game: Vec::new(),
            jvm: Vec::new(),
        });
        for v in fg_args.game {
            if let Some(s) = v.as_str() {
                va.game.push(ArgumentValue::Simple(s.to_string()));
            }
        }
        for v in fg_args.jvm {
            if let Some(s) = v.as_str() {
                va.jvm.push(ArgumentValue::Simple(s.to_string()));
            }
        }
    } else if let Some(mc_args) = fg_json.minecraft_arguments {
        // 1.12以前の Forge: Forge の minecraftArguments はバニラ引数を内包しているので
        // バニラと結合せず Forge 側を優先して使用する。
        // 結合すると --gameDir 等が重複して MultipleArgumentsForOptionException が発生する。
        merged.minecraft_arguments = Some(mc_args);
    }

    log::info!(
        "Forge merge complete: MC {} + Forge {}",
        vanilla_json.id,
        forge_full_version
    );
    Ok(merged)
}

// ────────────────────────────────────────────────────────────────────────────
// インストーラー実行
// ────────────────────────────────────────────────────────────────────────────

async fn run_forge_installer(
    forge_version: &str,
    mc_version: &str,
    installer_bytes: &[u8],
    paths: &LauncherPaths,
) -> Result<(), String> {
    let safe = forge_version.replace(['.', '-'], "_");
    let install_dir = paths.forge_install_dir().join(&safe);
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("failed to create install directory: {}", e))?;

    // バニラ jar / version.json を配置
    let mc_ver_dir = install_dir.join("versions").join(mc_version);
    std::fs::create_dir_all(&mc_ver_dir)
        .map_err(|e| format!("failed to create versions directory: {}", e))?;

    let vanilla_jar_src = paths
        .versions()
        .join(mc_version)
        .join(format!("{}.jar", mc_version));
    let vanilla_jar_dst = mc_ver_dir.join(format!("{}.jar", mc_version));
    let vanilla_jsn_src = paths
        .versions()
        .join(mc_version)
        .join(format!("{}.json", mc_version));
    let vanilla_jsn_dst = mc_ver_dir.join(format!("{}.json", mc_version));

    if vanilla_jar_src.exists() && !vanilla_jar_dst.exists() {
        std::fs::copy(&vanilla_jar_src, &vanilla_jar_dst)
            .map_err(|e| format!("failed to copy vanilla jar: {}", e))?;
    }
    if vanilla_jsn_src.exists() && !vanilla_jsn_dst.exists() {
        let _ = std::fs::copy(&vanilla_jsn_src, &vanilla_jsn_dst);
    }

    // launcher_profiles.json を作成（Forge インストーラーが要求する）
    let profiles_json = install_dir.join("launcher_profiles.json");
    if !profiles_json.exists() {
        let fake_profiles = serde_json::json!({
            "profiles": {
                "hikyou": {
                    "name": "Hikyou Launcher",
                    "type": "custom",
                    "gameDir": install_dir.to_string_lossy()
                }
            },
            "selectedProfile": "hikyou",
            "clientToken": "00000000-0000-0000-0000-000000000000",
            "authenticationDatabase": {}
        });
        std::fs::write(&profiles_json, fake_profiles.to_string())
            .map_err(|e| format!("failed to create launcher_profiles.json: {}", e))?;
    }
    let ms_profiles = install_dir.join("launcher_profiles_microsoft_store.json");
    if !ms_profiles.exists() {
        let _ = std::fs::copy(&profiles_json, &ms_profiles);
    }

    // インストーラー JAR を書き出し
    let installer_path = install_dir.join("installer.jar");
    std::fs::write(&installer_path, installer_bytes)
        .map_err(|e| format!("failed to write installer JAR: {}", e))?;

    // Java パスを取得
    let java_path = find_java_for_installer(paths)?;
    log::info!(
        "Running Forge installer: {:?} → {:?}",
        java_path,
        install_dir
    );

    // インストーラー実行
    let install_dir_str = install_dir.to_string_lossy().to_string();
    let mut cmd = tokio::process::Command::new(&java_path);
    cmd.args([
        "-jar",
        installer_path.to_str().unwrap_or(""),
        "--installClient",
        &install_dir_str,
    ])
    .current_dir(&install_dir)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("failed to run installer: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    log::info!(
        "Forge installer exited (exit: {:?})\nstdout: {}",
        output.status.code(),
        &stdout[..stdout.len().min(2000)],
    );

    // 生成されたライブラリを paths.libraries() にコピー
    let gen_libs = install_dir.join("libraries");
    if gen_libs.exists() {
        copy_dir_recursive(&gen_libs, &paths.libraries())
            .unwrap_or_else(|e| log::warn!("Library copy partially failed: {}", e));
        log::info!("Library copy: {:?} → {:?}", gen_libs, paths.libraries());
    }

    // バニラ jar キャッシュ保存
    if !vanilla_jar_src.exists()
        && vanilla_jar_dst.exists()
        && let Some(parent) = vanilla_jar_src.parent()
    {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::copy(&vanilla_jar_dst, &vanilla_jar_src);
    }

    // ライブラリが生成されていれば成功とみなす
    if gen_libs.exists() {
        return Ok(());
    }

    if !output.status.success() {
        let detail = if !stderr.is_empty() {
            stderr[..stderr.len().min(600)].to_string()
        } else {
            stdout[..stdout.len().min(600)].to_string()
        };
        return Err(format!(
            "Forge {} installation failed (exit: {:?})\n{}",
            forge_version,
            output.status.code(),
            detail
        ));
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("failed to create directory {:?}: {}", dst, e))?;
    for entry in
        std::fs::read_dir(src).map_err(|e| format!("failed to read directory {:?}: {}", src, e))?
    {
        let entry = entry.map_err(|e| format!("failed to read directory entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if !dst_path.exists() {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("failed to copy file {:?}: {}", src_path, e))?;
        }
    }
    Ok(())
}

fn find_java_for_installer(paths: &LauncherPaths) -> Result<std::path::PathBuf, String> {
    let java_dir = paths.java_versions();
    for name in &[
        "liberica-nik-21",
        "liberica-nik-17",
        "zulu-21",
        "zulu-17",
        "zulu-8",
    ] {
        let candidate = java_dir.join(name);
        if candidate.exists() {
            let java_bin = if cfg!(target_os = "windows") {
                candidate.join("bin").join("java.exe")
            } else {
                candidate.join("bin").join("java")
            };
            if java_bin.exists() {
                return Ok(java_bin);
            }
        }
    }
    Ok(std::path::PathBuf::from(if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }))
}

// ────────────────────────────────────────────────────────────────────────────
// installer JAR から version.json を抽出（キャッシュ付き）
// ────────────────────────────────────────────────────────────────────────────

async fn fetch_forge_version_json_and_installer(
    forge_version: &str,
    paths: &LauncherPaths,
) -> Result<(FgVersionJson, Vec<u8>, ForgeInstallerKind), String> {
    let cache_dir = paths.forge_dir();
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("failed to create Forge cache: {}", e))?;

    let safe = forge_version.replace(['.', '-'], "_");
    let json_cache = cache_dir.join(format!("{}.json", safe));
    let jar_cache = cache_dir.join(format!("{}-installer.jar", safe));

    // インストーラー JAR: ダウンロードまたはキャッシュ読み込み
    let installer_bytes: Vec<u8> = if jar_cache.exists() {
        log::info!("Using cached Forge installer JAR: {:?}", jar_cache);
        std::fs::read(&jar_cache).map_err(|e| format!("failed to read installer JAR: {}", e))?
    } else {
        let url = format!(
            "{}/net/minecraftforge/forge/{}/forge-{}-installer.jar",
            FORGE_MAVEN, forge_version, forge_version
        );
        log::info!("Downloading Forge installer JAR: {}", url);

        let resp = Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("failed to download Forge installer: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!(
                "Forge {} was not found (HTTP {})",
                forge_version,
                resp.status()
            ));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("failed to read Forge installer: {}", e))?
            .to_vec();

        let _ = std::fs::write(&jar_cache, &bytes);
        bytes
    };

    // version.json: キャッシュまたは JAR から抽出
    let (fg_json, installer_kind): (FgVersionJson, ForgeInstallerKind) =
        if loader_json_is_fresh(&json_cache) {
            if let Ok(content) = std::fs::read_to_string(&json_cache) {
                if let Ok(json) = serde_json::from_str(&content) {
                    log::info!("Using cached Forge version.json");
                    let kind = installer_kind_for_forge_version(forge_version);
                    if kind == ForgeInstallerKind::LegacyInstallProfile
                        && legacy_forge_artifact_missing(&json)
                    {
                        log::info!(
                            "Cached legacy Forge JSON is missing universal artifact; rebuilding"
                        );
                        extract_version_json_from_jar_with_kind(&installer_bytes, &json_cache)?
                    } else {
                        (json, kind)
                    }
                } else {
                    let _ = std::fs::remove_file(&json_cache);
                    extract_version_json_from_jar_with_kind(&installer_bytes, &json_cache)?
                }
            } else {
                extract_version_json_from_jar_with_kind(&installer_bytes, &json_cache)?
            }
        } else {
            extract_version_json_from_jar_with_kind(&installer_bytes, &json_cache)?
        };

    Ok((fg_json, installer_bytes, installer_kind))
}

fn legacy_forge_artifact_missing(json: &FgVersionJson) -> bool {
    json.libraries
        .iter()
        .filter(|lib| lib.name.starts_with("net.minecraftforge:forge:"))
        .any(|lib| {
            lib.downloads
                .as_ref()
                .and_then(|downloads| downloads.artifact.as_ref())
                .map(|artifact| !artifact.path.ends_with("-universal.jar"))
                .unwrap_or(true)
        })
}

fn installer_kind_for_forge_version(forge_version: &str) -> ForgeInstallerKind {
    if matches!(
        forge_version.split('-').next(),
        Some(
            "1.7.10"
                | "1.7.9"
                | "1.7.8"
                | "1.7.7"
                | "1.7.6"
                | "1.7.5"
                | "1.7.4"
                | "1.7.3"
                | "1.7.2"
        )
    ) || forge_version.starts_with("1.6.")
        || forge_version.starts_with("1.5.")
    {
        ForgeInstallerKind::LegacyInstallProfile
    } else {
        ForgeInstallerKind::ModernVersionJson
    }
}

fn loader_json_is_fresh(path: &std::path::Path) -> bool {
    const TTL_SECS: u64 = 30 * 24 * 3600;
    path.metadata()
        .and_then(|m| m.modified())
        .and_then(|t| t.elapsed().map_err(|_| std::io::Error::other("")))
        .map(|age| age.as_secs() < TTL_SECS)
        .unwrap_or(false)
}

fn extract_version_json_from_jar_with_kind(
    jar_bytes: &[u8],
    cache_path: &Path,
) -> Result<(FgVersionJson, ForgeInstallerKind), String> {
    let (text, json, kind) = match read_zip_entry(jar_bytes, "version.json") {
        Ok(text) => {
            let json: FgVersionJson = serde_json::from_str(&text).map_err(|e| {
                format!(
                    "failed to parse Forge version.json: {} (first 200 chars: {})",
                    e,
                    &text[..text.len().min(200)]
                )
            })?;
            (text, json, ForgeInstallerKind::ModernVersionJson)
        }
        Err(version_json_error) => {
            let text = read_zip_entry(jar_bytes, "install_profile.json").map_err(|_| {
                format!(
                    "{}; install_profile.json was also not found",
                    version_json_error
                )
            })?;
            let profile: FgInstallProfile = serde_json::from_str(&text).map_err(|e| {
                format!(
                    "failed to parse Forge install_profile.json: {} (first 200 chars: {})",
                    e,
                    &text[..text.len().min(200)]
                )
            })?;
            let mut version_info = profile.version_info;
            if let Some(install) = profile.install {
                apply_legacy_forge_artifact(&mut version_info, install);
            }
            let normalized =
                serde_json::to_string_pretty(&version_info).unwrap_or_else(|_| text.clone());
            (
                normalized,
                version_info,
                ForgeInstallerKind::LegacyInstallProfile,
            )
        }
    };
    let _ = std::fs::write(cache_path, &text);
    Ok((json, kind))
}

fn apply_legacy_forge_artifact(version_info: &mut FgVersionJson, install: FgInstallInfo) {
    let Some(path_coord) = install.path else {
        return;
    };
    let Some(file_path) = install.file_path else {
        return;
    };
    let Ok(path) = maven_coord_to_path_with_file(&path_coord, &file_path) else {
        return;
    };
    let url = format!("{}/{}", FORGE_MAVEN, path.replace('\\', "/"));
    for lib in &mut version_info.libraries {
        if lib.name == path_coord {
            lib.downloads = Some(FgDownloads {
                artifact: Some(FgArtifact {
                    path,
                    url,
                    sha1: String::new(),
                    size: 0,
                }),
            });
            break;
        }
    }
}

fn install_legacy_embedded_artifacts(
    fg_json: &FgVersionJson,
    installer_bytes: &[u8],
    paths: &LauncherPaths,
) -> Result<(), String> {
    for lib in &fg_json.libraries {
        let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        if !artifact.path.contains("/net/minecraftforge/forge/")
            && !artifact.path.contains("\\net\\minecraftforge\\forge\\")
        {
            continue;
        }
        let dest = paths.libraries().join(&artifact.path);
        if dest.exists() {
            continue;
        }
        let entry_name = std::path::Path::new(&artifact.path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid Forge artifact path: {}", artifact.path))?;
        let bytes = read_zip_entry_bytes(installer_bytes, entry_name)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create Forge library directory: {}", e))?;
        }
        std::fs::write(&dest, bytes)
            .map_err(|e| format!("failed to write embedded Forge library: {}", e))?;
        log::info!("Extracted embedded Forge library: {:?}", dest);
    }
    Ok(())
}

fn read_zip_entry(bytes: &[u8], entry_name: &str) -> Result<String, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to open ZIP archive: {}", e))?;
    let mut file = archive
        .by_name(entry_name)
        .map_err(|_| format!("missing from JAR: {} was not found", entry_name))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("{} failed to read: {}", entry_name, e))?;
    Ok(content)
}

fn read_zip_entry_bytes(bytes: &[u8], entry_name: &str) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to open ZIP archive: {}", e))?;
    let mut file = archive
        .by_name(entry_name)
        .map_err(|_| format!("missing from JAR: {} was not found", entry_name))?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)
        .map_err(|e| format!("{} failed to read: {}", entry_name, e))?;
    Ok(content)
}

// ────────────────────────────────────────────────────────────────────────────
// 内部ユーティリティ
// ────────────────────────────────────────────────────────────────────────────

fn semver_cmp_desc(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| {
        s.split(['.', '-'])
            .filter_map(|p| p.parse::<u64>().ok())
            .collect::<Vec<_>>()
    };
    let pa = parse(a);
    let pb = parse(b);
    for i in 0..pa.len().max(pb.len()) {
        let va = pa.get(i).copied().unwrap_or(0);
        let vb = pb.get(i).copied().unwrap_or(0);
        if va != vb {
            return vb.cmp(&va);
        }
    }
    std::cmp::Ordering::Equal
}

fn fg_libs_to_vanilla(libs: Vec<FgLibrary>) -> Result<Vec<Library>, String> {
    libs.into_iter()
        .map(|lib| {
            let (path, url, sha1, size) =
                if let Some(dl) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
                    let url = if dl.url.is_empty() {
                        String::new()
                    } else {
                        dl.url.clone()
                    };
                    (dl.path.clone(), url, dl.sha1.clone(), dl.size)
                } else {
                    let p = maven_coord_to_path(&lib.name)?;
                    let base = lib
                        .url
                        .as_deref()
                        .unwrap_or("https://libraries.minecraft.net");
                    let u = format!("{}/{}", base.trim_end_matches('/'), p.replace('\\', "/"));
                    let sha1 = lib
                        .checksums
                        .as_ref()
                        .and_then(|items| items.first())
                        .cloned()
                        .unwrap_or_default();
                    (p, u, sha1, 0)
                };
            Ok(Library {
                name: lib.name,
                downloads: Some(LibraryDownloads {
                    artifact: Some(LibraryArtifact {
                        path,
                        url,
                        sha1,
                        size,
                    }),
                    classifiers: None,
                }),
                rules: None,
                natives: None,
                extract: None,
            })
        })
        .collect()
}

fn maven_coord_to_path(coord: &str) -> Result<String, String> {
    let parts: Vec<&str> = coord.splitn(4, ':').collect();
    if parts.len() < 3 {
        return Err(format!("invalid Maven coordinates: {}", coord));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let jar = if parts.len() == 4 {
        format!("{}-{}-{}.jar", artifact, version, parts[3])
    } else {
        format!("{}-{}.jar", artifact, version)
    };
    Ok(format!("{}/{}/{}/{}", group, artifact, version, jar))
}

fn maven_coord_to_path_with_file(coord: &str, file_name: &str) -> Result<String, String> {
    let parts: Vec<&str> = coord.splitn(4, ':').collect();
    if parts.len() < 3 || file_name.contains(['/', '\\']) {
        return Err(format!("invalid Maven coordinates: {}", coord));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    Ok(format!("{}/{}/{}/{}", group, artifact, version, file_name))
}

#[cfg(test)]
mod tests {
    use super::{
        ForgeInstallerKind, extract_version_json_from_jar_with_kind,
        installer_kind_for_forge_version,
    };
    use std::io::Write;

    fn zip_with_entry(name: &str, content: &str) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        zip.start_file::<_, ()>(name, zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn extracts_modern_forge_version_json() {
        let bytes = zip_with_entry(
            "version.json",
            r#"{
              "mainClass":"net.minecraftforge.bootstrap.ForgeBootstrap",
              "libraries":[],
              "minecraftArguments":"--username ${auth_player_name}"
            }"#,
        );
        let cache_path = std::env::temp_dir().join("hikyou-modern-forge.json");
        let (json, kind) = extract_version_json_from_jar_with_kind(&bytes, &cache_path).unwrap();
        assert_eq!(kind, ForgeInstallerKind::ModernVersionJson);
        assert_eq!(
            json.main_class,
            "net.minecraftforge.bootstrap.ForgeBootstrap"
        );
        let _ = std::fs::remove_file(cache_path);
    }

    #[test]
    fn extracts_legacy_forge_install_profile_version_info() {
        let bytes = zip_with_entry(
            "install_profile.json",
            r#"{
              "install": {
                "path": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
                "filePath": "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"
              },
              "versionInfo": {
                "id": "1.7.10-Forge10.13.4.1614-1.7.10",
                "inheritsFrom": "1.7.10",
                "mainClass": "net.minecraft.launchwrapper.Launch",
                "minecraftArguments": "--username ${auth_player_name} --tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
                "libraries": [
                  { "name": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10" }
                ]
              }
            }"#,
        );
        let cache_path = std::env::temp_dir().join("hikyou-legacy-forge.json");
        let (json, kind) = extract_version_json_from_jar_with_kind(&bytes, &cache_path).unwrap();
        assert_eq!(kind, ForgeInstallerKind::LegacyInstallProfile);
        assert_eq!(json.inherits_from.as_deref(), Some("1.7.10"));
        assert_eq!(json.main_class, "net.minecraft.launchwrapper.Launch");
        assert!(
            json.minecraft_arguments
                .as_deref()
                .unwrap_or_default()
                .contains("FMLTweaker")
        );
        let forge_artifact = json.libraries[0]
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
            .unwrap();
        assert!(forge_artifact.path.ends_with(
            "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"
        ));
        let _ = std::fs::remove_file(cache_path);
    }

    #[test]
    fn treats_legacy_forge_versions_as_no_installer_cli() {
        assert_eq!(
            installer_kind_for_forge_version("1.7.10-10.13.4.1614-1.7.10"),
            ForgeInstallerKind::LegacyInstallProfile
        );
        assert_eq!(
            installer_kind_for_forge_version("1.12.2-14.23.5.2860"),
            ForgeInstallerKind::ModernVersionJson
        );
    }
}
