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

/// Reuse only a validated, completed native generation; never modify old generations.
pub(super) fn extract_natives(req: &LaunchRequest<'_>) -> Result<PathBuf, String> {
    let inputs = req
        .version_json
        .libraries
        .iter()
        .filter(|lib| lib.is_allowed_on_current_os())
        .filter_map(|lib| lib.native_artifact())
        .take(super::native_cache::MAX_INPUTS + 1)
        .map(|artifact| super::native_cache::NativeInput {
            path: artifact.path.clone(),
            sha1: artifact.sha1.clone(),
            size: artifact.size,
        })
        .collect::<Vec<_>>();
    super::native_cache::prepare(
        &req.paths.libraries(),
        &req.paths.meta().join("native-cache-v1"),
        req.version_id,
        &inputs,
    )
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
        let client = crate::http::client()
            .map_err(|error| log::warn!("log4j HTTPS client initialization failed: {error}"))
            .ok()?;
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
