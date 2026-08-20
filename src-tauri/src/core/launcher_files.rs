use std::path::PathBuf;

#[cfg(target_os = "windows")]
use crate::core::paths::LauncherPaths;

use super::launcher::LaunchRequest;
// ────────────────────────────────────────────────────────────────────────────
// Classpath / Natives
// ────────────────────────────────────────────────────────────────────────────

pub(super) fn build_classpath(req: &LaunchRequest<'_>) -> Result<String, String> {
    let sep = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };
    let mut paths: Vec<String> = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    let mut seen_libraries = std::collections::HashSet::new();

    for lib in &req.version_json.libraries {
        if !lib.is_allowed_on_current_os() {
            continue;
        }
        if let Some(key) = maven_coord_logical_key(&lib.name)
            && !seen_libraries.insert(key.clone())
        {
            log::info!(
                "Skipping duplicate classpath library {} ({})",
                lib.name,
                key
            );
            continue;
        }
        if let Some(downloads) = &lib.downloads {
            if let Some(artifact) = &downloads.artifact {
                let lib_path = req.paths.libraries().join(&artifact.path);
                let path_str = lib_path.to_string_lossy().to_string();
                if lib_path.exists() && seen_paths.insert(path_str.clone()) {
                    paths.push(path_str);
                }
            }
        } else {
            // downloadsフィールドがないライブラリ（旧Forgeなど）
            // Maven座標からパスを推定して存在確認
            if let Some(maven_path) = maven_coord_to_path(&lib.name) {
                let lib_path = req.paths.libraries().join(&maven_path);
                let path_str = lib_path.to_string_lossy().to_string();
                if lib_path.exists() && seen_paths.insert(path_str.clone()) {
                    paths.push(path_str);
                }
            }
        }
    }

    let client_jar = req
        .paths
        .versions()
        .join(req.version_id)
        .join(format!("{}.jar", req.version_id));

    if client_jar.exists() {
        paths.push(client_jar.to_string_lossy().to_string());
    } else {
        return Err(format!(
            "client.jar was not found: {:?}\nDownload the required files before launching.",
            client_jar
        ));
    }

    Ok(paths.join(sep))
}

/// Maven座標 "group.id:artifact:version[:classifier]" → ファイルパス
fn maven_coord_to_path(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).map(|s| format!("-{}", s)).unwrap_or_default();
    Some(format!(
        "{}/{}/{}/{}-{}{}.jar",
        group, artifact, version, artifact, version, classifier
    ))
}

/// Logical classpath identity for Maven coordinates.
///
/// Version is intentionally ignored. Loader profiles are merged before vanilla
/// libraries, so this keeps the loader-selected artifact and prevents older
/// vanilla artifacts from reintroducing duplicate classes, e.g. ASM 9.9 + 9.6.
fn maven_coord_logical_key(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0];
    let artifact = parts[1];
    let classifier = parts.get(3).copied().unwrap_or("");
    Some(format!("{group}:{artifact}:{classifier}"))
}

/// natives JAR (.dll/.so/.dylib) をバージョン専用ディレクトリに展開する
pub(super) fn extract_natives(req: &LaunchRequest<'_>) -> Result<PathBuf, String> {
    let natives_dir = req.paths.meta().join("natives").join(req.version_id);
    std::fs::create_dir_all(&natives_dir)
        .map_err(|e| format!("Failed to create natives directory: {}", e))?;

    for lib in &req.version_json.libraries {
        if !lib.is_allowed_on_current_os() {
            continue;
        }

        let native_artifact = match lib.native_artifact() {
            Some(a) => a,
            None => continue,
        };

        let jar_path = req.paths.libraries().join(&native_artifact.path);
        if !jar_path.exists() {
            log::warn!("natives JAR not found: {:?}", jar_path);
            continue;
        }

        // ZIP として開いて .dll / .so / .dylib を展開
        let file = std::fs::File::open(&jar_path)
            .map_err(|e| format!("Failed to open natives JAR {:?}: {}", jar_path, e))?;

        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to parse natives JAR ZIP: {}", e))?;

        for i in 0..archive.len() {
            let mut zip_file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read ZIP entry: {}", e))?;

            let name = zip_file.name().to_string();

            // META-INF は除外、ネイティブ拡張子のみ展開
            if name.starts_with("META-INF") {
                continue;
            }
            let is_native = name.ends_with(".dll")
                || name.ends_with(".so")
                || name.ends_with(".dylib")
                || name.ends_with(".jnilib");
            if !is_native {
                continue;
            }

            // サブディレクトリがある場合はファイル名だけ取り出す
            let file_name = std::path::Path::new(&name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&name);

            let dest = natives_dir.join(file_name);

            // 既に存在する場合はスキップ
            if dest.exists() {
                continue;
            }

            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut zip_file, &mut buf)
                .map_err(|e| format!("Failed to read ZIP entry {}: {}", name, e))?;

            std::fs::write(&dest, &buf)
                .map_err(|e| format!("Failed to write native {:?}: {}", dest, e))?;

            log::info!("Extracting native: {}", file_name);
        }
    }

    Ok(natives_dir)
}

// ────────────────────────────────────────────────────────────────────────────
// log4j2 設定ダウンロード
// ────────────────────────────────────────────────────────────────────────────

/// バージョン JSON に logging フィールドがある場合、log4j2 設定 XML をダウンロードして
/// `-Dlog4j.configurationFile=<path>` 引数を返す。
///
/// これは 1.7〜1.18.0 の log4shell 脆弱性 (CVE-2021-44228) 対策として
/// Mojang が提供するカスタム log4j2 設定。1.18.1 以降はゲーム内で修正済みのため不要。
pub(super) async fn ensure_log4j_config(req: &LaunchRequest<'_>) -> Option<String> {
    let client_logging = req.version_json.logging.as_ref()?.client.as_ref()?;
    let dir = req.paths.log_configs_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| log::warn!("Failed to create log_configs directory: {}", e))
        .ok()?;
    let dest = dir.join(&client_logging.file.id);
    if !dest.exists() {
        let client = reqwest::Client::new();
        let bytes = client
            .get(&client_logging.file.url)
            .send()
            .await
            .map_err(|e| log::warn!("log4j config download failed: {}", e))
            .ok()?
            .bytes()
            .await
            .map_err(|e| log::warn!("log4j config response read failed: {}", e))
            .ok()?;
        std::fs::write(&dest, &bytes)
            .map_err(|e| log::warn!("log4j config write failed: {}", e))
            .ok()?;
        log::info!("log4j2 config downloaded: {}", client_logging.file.id);
    }
    let path_str = file_uri(&dest);
    Some(client_logging.argument.replace("${path}", &path_str))
}

fn file_uri(path: &std::path::Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.starts_with('/') {
        format!("file://{}", normalized)
    } else {
        format!("file:///{}", normalized)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Java argfile (Windows 長コマンドライン対策)
// ────────────────────────────────────────────────────────────────────────────

/// JVM 引数を argfile に書き出す。
/// Java の `@argfile` 構文を使い、CreateProcess の 32767 文字制限を回避する。
/// バックスラッシュはフォワードスラッシュに変換してエスケープ問題を防ぐ。
#[cfg(target_os = "windows")]
pub(super) fn write_java_argfile(
    args: &[String],
    version_id: &str,
    paths: &LauncherPaths,
) -> Result<std::path::PathBuf, String> {
    let safe_id = version_id.replace(['.', '-', ' '], "_");
    let dir = paths.meta().join("argfiles");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create argfiles directory: {}", e))?;
    let argfile_path = dir.join(format!("{}.args", safe_id));

    purge_legacy_secret_argfiles(&dir, &argfile_path);

    let content: String = args
        .iter()
        .map(|a| {
            // バックスラッシュ → フォワードスラッシュ（Java は両対応）
            let normalized = a.replace('\\', "/");
            if normalized.contains(' ') {
                format!("\"{}\"", normalized.replace('"', "\\\""))
            } else {
                normalized
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    std::fs::write(&argfile_path, content)
        .map_err(|e| format!("Failed to write argfile {:?}: {}", argfile_path, e))?;
    Ok(argfile_path)
}

/// Remove only legacy launcher argfiles that contain Minecraft session arguments.
///
/// Older Hikyou versions placed game arguments in `@argfile`, which persisted an
/// access token under the launcher-managed `meta/argfiles` directory. Current
/// argfiles contain JVM/launcher arguments only. We deliberately inspect the
/// content instead of deleting every argfile so this maintenance step is narrow
/// and does not disturb ordinary launch state.
#[cfg(target_os = "windows")]
fn purge_legacy_secret_argfiles(dir: &std::path::Path, current_path: &std::path::Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            log::warn!("Failed to inspect legacy argfiles: {}", error);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path == current_path
            || path.extension().and_then(std::ffi::OsStr::to_str) != Some("args")
        {
            continue;
        }

        let contents = match std::fs::read(&path) {
            Ok(contents) => contents,
            Err(error) => {
                log::warn!("Failed to inspect legacy argfile {:?}: {}", path, error);
                continue;
            }
        };

        if contains_legacy_secret_args(&contents) {
            match std::fs::remove_file(&path) {
                Ok(()) => log::info!(
                    "Removed legacy argfile containing session arguments: {:?}",
                    path
                ),
                Err(error) => log::warn!(
                    "Failed to remove legacy secret argfile {:?}: {}",
                    path,
                    error
                ),
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn contains_legacy_secret_args(contents: &[u8]) -> bool {
    contents
        .windows(b"--accessToken".len())
        .any(|window| window == b"--accessToken")
        || contents
            .windows(b"--session token:".len())
            .any(|window| window == b"--session token:")
}

#[cfg(test)]
mod tests {
    use super::maven_coord_logical_key;

    #[cfg(target_os = "windows")]
    use super::contains_legacy_secret_args;

    #[test]
    fn maven_logical_key_ignores_version() {
        assert_eq!(
            maven_coord_logical_key("org.ow2.asm:asm:9.9"),
            maven_coord_logical_key("org.ow2.asm:asm:9.6")
        );
    }

    #[test]
    fn maven_logical_key_preserves_classifier() {
        assert_ne!(
            maven_coord_logical_key("org.lwjgl:lwjgl:3.3.3"),
            maven_coord_logical_key("org.lwjgl:lwjgl:3.3.3:natives-windows")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn identifies_only_legacy_argfiles_with_session_arguments() {
        assert!(contains_legacy_secret_args(b"--accessToken\nsecret"));
        assert!(contains_legacy_secret_args(b"--session token:secret"));
        assert!(!contains_legacy_secret_args(b"-cp\nC:/minecraft/libraries"));
    }
}
