//! NeoForge Mod ローダーのサポート (1.20.2+)
//!
//! ## SRG jar 生成について
//!
//! NeoForge は `client-{mc}-{timestamp}-srg.jar` を必要とする。
//! このファイルは Maven には存在せず、インストーラーのプロセッサが生成する。
//!
//! 生成手順:
//!   1. インストーラー JAR を ZIP として開き version.json を抽出 → mainClass / libraries
//!   2. バニラ client.jar を temp ディレクトリにコピー
//!   3. インストーラーを `--install-client` で実行（current_dir = temp）
//!      → temp/libraries/net/minecraft/client/.../client-...-srg.jar が生成される
//!   4. 生成されたファイルを paths.libraries() にコピー
//!
//! バニラ jar が既にキャッシュされている場合、インストーラーは
//! ダウンロードをスキップしてプロセッサのみ実行する。

use crate::core::cache;
use crate::core::manifest::{
    ArgumentValue, Arguments, Library, LibraryArtifact, LibraryDownloads, VersionJson,
};
use crate::core::paths::LauncherPaths;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

const NF_MAVEN: &str = "https://maven.neoforged.net/releases";
const CACHE_TYPE: &str = "neoforge_loader";
const CACHE_TTL: i64 = 3600;

// ────────────────────────────────────────────────────────────────────────────
// 公開型
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NeoForgeVersion {
    pub version: String,
    pub stable: bool,
}

// ────────────────────────────────────────────────────────────────────────────
// installer JAR 内部の version.json
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
struct NfVersionJson {
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "inheritsFrom")]
    #[allow(dead_code)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub libraries: Vec<NfLibrary>,
    pub arguments: Option<NfArguments>,
}

#[derive(Debug, Deserialize, Clone)]
struct NfArguments {
    #[serde(default)]
    game: Vec<serde_json::Value>,
    #[serde(default)]
    jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct NfLibrary {
    name: String,
    downloads: Option<NfDownloads>,
}

#[derive(Debug, Deserialize, Clone)]
struct NfDownloads {
    artifact: Option<NfArtifact>,
}

#[derive(Debug, Deserialize, Clone)]
struct NfArtifact {
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

/// MC バージョンに対応する NeoForge バージョン一覧を返す（新しい順、1 時間キャッシュ）。
pub async fn fetch_loader_versions(mc_version: &str) -> Result<Vec<NeoForgeVersion>, String> {
    if let Some(c) = cache::get()
        && let Some(cached) = c.get::<Vec<NeoForgeVersion>>(CACHE_TYPE, mc_version).await
    {
        return Ok(cached);
    }

    let nf_major = mc_to_nf_major(mc_version)?;
    let meta_url = format!("{}/net/neoforged/neoforge/maven-metadata.xml", NF_MAVEN);

    let resp = Client::new()
        .get(&meta_url)
        .send()
        .await
        .map_err(|e| format!("Failed to get NeoForge Maven metadata: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "NeoForge Maven failed to read metadata (HTTP {})",
            resp.status()
        ));
    }

    let xml = resp
        .text()
        .await
        .map_err(|e| format!("failed to read NeoForge Maven metadata: {}", e))?;

    let prefix = format!("{}.", nf_major);
    let mut versions: Vec<NeoForgeVersion> = xml
        .split("<version>")
        .skip(1)
        .filter_map(|chunk| {
            let v = chunk.split("</version>").next()?.trim().to_string();
            if v.starts_with(&prefix) || v == nf_major {
                let stable = !v.contains("beta") && !v.contains("alpha") && !v.contains("rc");
                Some(NeoForgeVersion { version: v, stable })
            } else {
                None
            }
        })
        .collect();

    versions.sort_by(|a, b| semver_cmp_desc(&a.version, &b.version));

    if versions.is_empty() {
        return Err(format!(
            "No NeoForge version compatible with Minecraft {} was found (Minecraft 1.20.2 or newer is required)",
            mc_version
        ));
    }

    if let Some(c) = cache::get() {
        c.set(CACHE_TYPE, mc_version, &versions, CACHE_TTL)
            .await
            .ok();
    }
    Ok(versions)
}

/// バニラ VersionJson に NeoForge の情報をマージして返す。
/// 必要に応じてインストーラーを実行し SRG jar を生成する。
pub async fn build_neoforge_version_json(
    vanilla_json: &VersionJson,
    neoforge_version: &str,
    paths: &LauncherPaths,
) -> Result<VersionJson, String> {
    if neoforge_version.is_empty() {
        return Err("NeoForge version was not specified".to_string());
    }

    // ── 1. version.json 取得（インストーラー JAR から抽出またはキャッシュ） ──
    let (nf_json, installer_bytes) =
        fetch_neoforge_version_json_and_installer(neoforge_version, paths).await?;

    // ── 2. SRG jar 生成（未生成の場合のみ） ─────────────────────────────────
    let srg_marker = paths
        .neoforge_dir()
        .join(format!("{}.srg_installed", neoforge_version));

    // SRG jar の存在も確認（マーカーがあっても実ファイルがない場合は再生成）
    let srg_jar_exists = check_srg_jar_exists(neoforge_version, vanilla_json, paths);

    if !srg_marker.exists() || !srg_jar_exists {
        if !srg_jar_exists {
            log::info!(
                "NeoForge {} SRG jar not found, generating",
                neoforge_version
            );
        }
        generate_srg_jar(neoforge_version, &vanilla_json.id, &installer_bytes, paths).await?;
        let _ = std::fs::write(&srg_marker, b"ok");
        log::info!("NeoForge {} SRG jar generation complete", neoforge_version);
    } else {
        log::info!("NeoForge {} SRG jar already cached", neoforge_version);
    }

    // ── 3. vanilla VersionJson にマージ ─────────────────────────────────────
    let mut merged = vanilla_json.clone();
    merged.main_class = nf_json.main_class;

    // NeoForge ライブラリを先頭に追加（クラスパスの優先順位確保）
    // 空 URL のライブラリ（SRG jar など）はインストーラーが生成済みなので URL は不要
    let nf_libs = nf_libs_to_vanilla(nf_json.libraries)?;
    let mut all_libs = nf_libs;
    all_libs.extend(merged.libraries.clone());
    merged.libraries = all_libs;

    // 同じパスのライブラリが重複するとUnionFileSystemがDuplicate keyエラーになる
    // NeoForgeライブラリを優先して重複を除去
    let mut seen_paths = std::collections::HashSet::new();
    merged.libraries.retain(|lib| {
        if let Some(dl) = &lib.downloads
            && let Some(art) = &dl.artifact
        {
            return seen_paths.insert(art.path.clone());
        }
        true
    });

    if let Some(nf_args) = nf_json.arguments {
        let va = merged.arguments.get_or_insert(Arguments {
            game: Vec::new(),
            jvm: Vec::new(),
        });
        for v in nf_args.game {
            if let Some(s) = v.as_str() {
                va.game.push(ArgumentValue::Simple(s.to_string()));
            }
        }
        for v in nf_args.jvm {
            if let Some(s) = v.as_str() {
                va.jvm.push(ArgumentValue::Simple(s.to_string()));
            }
        }
    }

    log::info!(
        "NeoForge merge complete: MC {} + NeoForge {}",
        vanilla_json.id,
        neoforge_version
    );
    Ok(merged)
}

// ────────────────────────────────────────────────────────────────────────────
// SRG jar 生成: インストーラーを temp ディレクトリで実行
// ────────────────────────────────────────────────────────────────────────────

/// SRG jar が paths.libraries() に存在するかチェック
fn check_srg_jar_exists(
    neoforge_version: &str,
    vanilla_json: &VersionJson,
    paths: &LauncherPaths,
) -> bool {
    let mc_version = &vanilla_json.id;
    let srg_dir = paths
        .libraries()
        .join("net")
        .join("minecraft")
        .join("client");
    if !srg_dir.exists() {
        return false;
    }

    // net/minecraft/client/ 以下に srg.jar が含まれるディレクトリがあるか
    if let Ok(entries) = std::fs::read_dir(&srg_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let n = name.to_string_lossy();
            if n.contains(mc_version) {
                // このディレクトリ内に -srg.jar があるか
                if let Ok(sub) = std::fs::read_dir(entry.path()) {
                    for f in sub.flatten() {
                        let fname = f.file_name();
                        if fname.to_string_lossy().ends_with("-srg.jar") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    let _ = neoforge_version;
    false
}

async fn generate_srg_jar(
    neoforge_version: &str,
    mc_version: &str,
    installer_bytes: &[u8],
    paths: &LauncherPaths,
) -> Result<(), String> {
    // ── インストールディレクトリ (永続化。毎回削除しない) ─────────────────
    // temp を使わず paths.meta() 下に永続化することで:
    //   - インストーラーが途中でダウンロードしたファイルを再利用できる
    //   - 生成された libraries/ が直接使用できる
    let install_dir = paths
        .neoforge_install_dir()
        .join(neoforge_version.replace(['.', '-'], "_").as_str());
    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("failed to create install directory: {}", e))?;

    // ── バニラ jar / version.json を install_dir/versions/<mc>/ に配置 ────
    // インストーラーがここを参照する。事前配置でダウンロードをスキップさせる。
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
        log::info!("Placed vanilla jar in install_dir: {:?}", vanilla_jar_dst);
    }
    if vanilla_jsn_src.exists() && !vanilla_jsn_dst.exists() {
        let _ = std::fs::copy(&vanilla_jsn_src, &vanilla_jsn_dst);
    }

    // ── launcher_profiles.json を作成 ────────────────────────────────────
    // NeoForge インストーラーはターゲットディレクトリに launcher_profiles.json
    // または launcher_profiles_microsoft_store.json が存在することを要求する。
    // サードパーティランチャー (Prism, MultiMC, etc.) は全て偽のファイルを生成して対処。
    // 参考: https://github.com/PrismLauncher/PrismLauncher / MultiMC の実装
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
        log::info!("Created launcher_profiles.json: {:?}", profiles_json);
    }
    // Microsoft Store 版ランチャーも使う場合に備えてシンボリックリンクの代わりにコピー
    let ms_profiles = install_dir.join("launcher_profiles_microsoft_store.json");
    if !ms_profiles.exists() {
        let _ = std::fs::copy(&profiles_json, &ms_profiles);
    }

    // ── インストーラー JAR を install_dir に書き出し ──────────────────────
    let installer_path = install_dir.join("installer.jar");
    std::fs::write(&installer_path, installer_bytes)
        .map_err(|e| format!("failed to write installer JAR: {}", e))?;

    // ── Java パスを取得（Liberica NIK 21 → 17 → Zulu 21 の順）──────────
    let java_path = find_java_for_installer(paths)?;
    log::info!(
        "Running NeoForge installer: {:?} → {:?}",
        java_path,
        install_dir
    );

    // ── インストーラー実行 ────────────────────────────────────────────────
    // "--install-client <install_dir>": ライブラリを install_dir/libraries/ に生成
    let install_dir_str = install_dir.to_string_lossy().to_string();
    let mut cmd = tokio::process::Command::new(&java_path);
    cmd.args([
        "-jar",
        installer_path.to_str().unwrap_or(""),
        "--install-client",
        &install_dir_str,
    ])
    .current_dir(&install_dir)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("failed to run installer: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // installer.jar.log: インストーラーが詳細ログを書き出す場合がある
    let log_file = install_dir.join("installer.jar.log");
    let installer_log = std::fs::read_to_string(&log_file).unwrap_or_default();

    log::info!(
        "NeoForge installer exited (exit: {:?})\nstdout: {}{}",
        output.status.code(),
        &stdout[..stdout.len().min(2000)],
        if installer_log.is_empty() {
            String::new()
        } else {
            format!(
                "\ninstaller.jar.log:\n{}",
                &installer_log[..installer_log.len().min(4000)]
            )
        }
    );

    // ── 生成されたライブラリを paths.libraries() にコピー ─────────────────
    // exit code に関わらずコピー試行（部分成功の可能性があるため）
    let gen_libs = install_dir.join("libraries");
    if gen_libs.exists() {
        copy_dir_recursive(&gen_libs, &paths.libraries())
            .unwrap_or_else(|e| log::warn!("Library copy partially failed: {}", e));
        log::info!("Library copy: {:?} → {:?}", gen_libs, paths.libraries());
    }

    // ── バニラ jar をキャッシュに保存（インストーラーがダウンロードした場合）──
    if !vanilla_jar_src.exists()
        && vanilla_jar_dst.exists()
        && let Some(parent) = vanilla_jar_src.parent()
    {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::copy(&vanilla_jar_dst, &vanilla_jar_src);
    }

    // ── SRG jar 存在確認 (exit code に関わらず確認) ─────────────────────
    // exit code 1 でも SRG jar が生成済みのケースがある（minor warning で終了など）
    let srg_dir = paths
        .libraries()
        .join("net")
        .join("minecraft")
        .join("client");
    if srg_dir.exists() {
        log::info!(
            "NeoForge {} SRG jar verified: {:?}",
            neoforge_version,
            srg_dir
        );
        return Ok(());
    }

    // SRG jar なし → 詳細エラー
    let detail = if !installer_log.is_empty() {
        installer_log[..installer_log.len().min(1000)].to_string()
    } else if !stderr.is_empty() {
        stderr[..stderr.len().min(600)].to_string()
    } else {
        stdout[..stdout.len().min(600)].to_string()
    };
    Err(format!(
        "NeoForge {} failed to generate SRG jar (exit: {:?})\n{}",
        neoforge_version,
        output.status.code(),
        detail
    ))
}

/// ディレクトリを再帰的にコピー（src の中身を dst にマージ）
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
        } else {
            // 既存ファイルは上書きしない（SHA1 が一致している可能性が高い）
            if !dst_path.exists() {
                std::fs::copy(&src_path, &dst_path)
                    .map_err(|e| format!("failed to copy file {:?}: {}", src_path, e))?;
            }
        }
    }
    Ok(())
}

/// インストーラー実行用の Java パスを取得
fn find_java_for_installer(paths: &LauncherPaths) -> Result<std::path::PathBuf, String> {
    // Java 21 (Liberica NIK) → Java 17 → Java 21 (Zulu) の順で探す
    let java_dir = paths.java_versions();
    for name in &["liberica-nik-21", "liberica-nik-17", "zulu-21", "zulu-17"] {
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
    // システム java にフォールバック
    Ok(std::path::PathBuf::from(if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }))
}

// ────────────────────────────────────────────────────────────────────────────
// インストーラー JAR から version.json を抽出（キャッシュ付き）
// ────────────────────────────────────────────────────────────────────────────

async fn fetch_neoforge_version_json_and_installer(
    neoforge_version: &str,
    paths: &LauncherPaths,
) -> Result<(NfVersionJson, Vec<u8>), String> {
    let cache_dir = paths.neoforge_dir();
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Neofailed to create Forge cache: {}", e))?;

    let json_cache = cache_dir.join(format!("{}.json", neoforge_version));
    let jar_cache = cache_dir.join(format!("{}-installer.jar", neoforge_version));

    // ── インストーラー JAR: ダウンロードまたはキャッシュ読み込み ─────────
    let installer_bytes: Vec<u8> = if jar_cache.exists() {
        log::info!("Using cached NeoForge installer JAR: {:?}", jar_cache);
        std::fs::read(&jar_cache).map_err(|e| format!("failed to read installer JAR: {}", e))?
    } else {
        let url = format!(
            "{}/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            NF_MAVEN, neoforge_version, neoforge_version
        );
        log::info!("Downloading NeoForge installer JAR: {}", url);

        let resp = Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Neofailed to download Forge installer: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!(
                "NeoForge {} was not found (HTTP {})",
                neoforge_version,
                resp.status()
            ));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Neofailed to read Forge installer: {}", e))?
            .to_vec();

        let _ = std::fs::write(&jar_cache, &bytes);
        bytes
    };

    // ── version.json: キャッシュまたは JAR から抽出 ──────────────────────
    let nf_json: NfVersionJson = if loader_json_is_fresh(&json_cache) {
        if let Ok(content) = std::fs::read_to_string(&json_cache) {
            if let Ok(json) = serde_json::from_str(&content) {
                log::info!("Using cached NeoForge version.json");
                json
            } else {
                let _ = std::fs::remove_file(&json_cache);
                extract_version_json_from_jar(&installer_bytes, &json_cache)?
            }
        } else {
            extract_version_json_from_jar(&installer_bytes, &json_cache)?
        }
    } else {
        extract_version_json_from_jar(&installer_bytes, &json_cache)?
    };

    Ok((nf_json, installer_bytes))
}

fn loader_json_is_fresh(path: &std::path::Path) -> bool {
    const TTL_SECS: u64 = 30 * 24 * 3600;
    path.metadata()
        .and_then(|m| m.modified())
        .and_then(|t| t.elapsed().map_err(|_| std::io::Error::other("")))
        .map(|age| age.as_secs() < TTL_SECS)
        .unwrap_or(false)
}

fn extract_version_json_from_jar(
    jar_bytes: &[u8],
    cache_path: &Path,
) -> Result<NfVersionJson, String> {
    let text = read_zip_entry(jar_bytes, "version.json")?;
    let json: NfVersionJson = serde_json::from_str(&text).map_err(|e| {
        format!(
            "Neofailed to parse Forge version.json: {} (first 200 chars: {})",
            e,
            &text[..text.len().min(200)]
        )
    })?;
    let _ = std::fs::write(cache_path, &text);
    Ok(json)
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

// ────────────────────────────────────────────────────────────────────────────
// 内部ユーティリティ
// ────────────────────────────────────────────────────────────────────────────

fn mc_to_nf_major(mc_version: &str) -> Result<String, String> {
    let parts: Vec<&str> = mc_version.splitn(3, '.').collect();
    if parts.len() < 2 {
        return Err(format!("invalid Minecraft version: {}", mc_version));
    }
    let minor: u32 = parts[1]
        .parse()
        .map_err(|_| format!("MC failed to parse version data: {}", mc_version))?;
    if minor < 20 {
        return Err("NeoForge only supports Minecraft 1.20.2 or newer".to_string());
    }
    if minor == 20 {
        let patch: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        if patch < 2 {
            return Err("NeoForge only supports Minecraft 1.20.2 or newer".to_string());
        }
    }
    let patch = parts.get(2).copied().unwrap_or("0");
    Ok(format!("{}.{}", minor, patch))
}

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

fn nf_libs_to_vanilla(libs: Vec<NfLibrary>) -> Result<Vec<Library>, String> {
    libs.into_iter()
        .map(|lib| {
            let (path, url, sha1, size) =
                if let Some(dl) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
                    let path = dl.path.clone();
                    // URL が空のライブラリ（SRG jar など）は generate_srg_jar が生成済み
                    // URL を空のまま渡し、downloader がファイル存在チェックでスキップする
                    let url = if dl.url.is_empty() {
                        String::new()
                    } else {
                        dl.url.clone()
                    };
                    (path, url, dl.sha1.clone(), dl.size)
                } else {
                    // downloads フィールドなし: Maven 座標からパスを生成
                    let p = maven_coord_to_path(&lib.name)?;
                    let u = format!("{}/{}", NF_MAVEN, p.replace('\\', "/"));
                    (p, u, String::new(), 0)
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
