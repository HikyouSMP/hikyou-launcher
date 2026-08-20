//! ファイルダウンロードとSHA1検証
//!
//! Modrinthの fetch.rs を参考にした設計

use crate::core::manifest::{Library, VersionJson};
use crate::core::paths::LauncherPaths;
use futures::StreamExt;
use reqwest::Client;
use serde::Serialize;
use sha1_smol::Sha1;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

// ────────────────────────────────────────────────────────────────────────────
// 共有 Client
// ────────────────────────────────────────────────────────────────────────────

pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .tcp_keepalive(Some(std::time::Duration::from_secs(10)))
        .user_agent(format!(
            "HikyouLauncher/{} (hikyou-launcher)",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .expect("failed to initialize HTTP client")
});

const FETCH_ATTEMPTS: usize = 2;

// ────────────────────────────────────────────────────────────────────────────
// 公開型
// ────────────────────────────────────────────────────────────────────────────

/// フロントエンドへの進捗イベント ("download://progress")
#[derive(Debug, Serialize, Clone)]
pub struct DownloadProgress {
    /// 完了ファイル数
    pub completed: usize,
    /// 合計ファイル数
    pub total: usize,
    pub current_file: String,
    pub bytes_downloaded: u64,
    pub bytes_total: u64,
    /// フェーズ: "libraries" | "assets" | "java"
    pub phase: String,
}

#[derive(Debug, Clone)]
struct DownloadTask {
    url: String,
    dest: PathBuf,
    sha1: String,
    display_name: String,
    size: u64,
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

/// client.jar + 全ライブラリをダウンロードする。
/// 進捗は "download://progress" (phase: "libraries") で通知。
pub async fn download_version_files(
    version_json: &VersionJson,
    paths: &LauncherPaths,
    app: &AppHandle,
    max_concurrent: usize,
) -> Result<(), String> {
    let mut tasks: Vec<DownloadTask> = Vec::new();

    // client.jar
    let client_jar_dir = paths.versions().join(&version_json.id);
    fs::create_dir_all(&client_jar_dir)
        .map_err(|e| format!("failed to create version directory: {}", e))?;

    tasks.push(DownloadTask {
        url: version_json.downloads.client.url.clone(),
        dest: client_jar_dir.join(format!("{}.jar", version_json.id)),
        sha1: version_json.downloads.client.sha1.clone(),
        display_name: format!("{}.jar", version_json.id),
        size: version_json.downloads.client.size,
    });

    // ライブラリ (通常 artifact)
    for lib in &version_json.libraries {
        // 通常ライブラリ
        if let Some(task) = library_to_task(lib, paths) {
            tasks.push(task);
        }
        // natives JAR (1.12以前)
        if let Some(native_artifact) = lib.native_artifact()
            && lib.is_allowed_on_current_os()
        {
            tasks.push(DownloadTask {
                url: native_artifact.url.clone(),
                dest: paths.libraries().join(&native_artifact.path),
                sha1: native_artifact.sha1.clone(),
                display_name: format!("{} (natives)", lib.name),
                size: native_artifact.size,
            });
        }
    }

    run_downloads(tasks, app, "libraries", max_concurrent).await
}

// ────────────────────────────────────────────────────────────────────────────
// 内部関数
// ────────────────────────────────────────────────────────────────────────────

async fn run_downloads(
    tasks: Vec<DownloadTask>,
    app: &AppHandle,
    phase: &'static str,
    max_concurrent: usize,
) -> Result<(), String> {
    let total = tasks.len();
    if tasks.iter().all(download_task_is_present) {
        log::info!(
            "[{}] Download check skipped; all {} files are present",
            phase,
            total
        );
        return Ok(());
    }

    let semaphore = Arc::new(Semaphore::new(max_concurrent.max(1)));
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app = Arc::new(app.clone());

    let mut handles = Vec::new();

    for task in tasks {
        let sem = Arc::clone(&semaphore);
        let completed = Arc::clone(&completed);
        let app = Arc::clone(&app);

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
            download_with_retry(&task, &app, phase, total, &completed).await
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.map_err(|e| format!("task failed: {}", e))??;
    }

    log::info!("[{}] All files downloaded ({} files)", phase, total);
    Ok(())
}

fn download_task_is_present(task: &DownloadTask) -> bool {
    task.dest
        .metadata()
        .map(|metadata| {
            if !metadata.is_file() {
                return false;
            }
            if task.size > 0 {
                metadata.len() == task.size
            } else {
                metadata.len() > 0
            }
        })
        .unwrap_or(false)
}

async fn download_with_retry(
    task: &DownloadTask,
    app: &AppHandle,
    phase: &'static str,
    total: usize,
    completed: &std::sync::atomic::AtomicUsize,
) -> Result<(), String> {
    if task.dest.exists() {
        // sha1 が空 (Fabric ライブラリ等) はサイズ確認のみ
        let already_ok = if task.sha1.is_empty() {
            task.dest.metadata().map(|m| m.len() > 0).unwrap_or(false)
        } else {
            verify_sha1_async(&task.dest, &task.sha1).await?
        };
        if already_ok {
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = app.emit(
                "download://progress",
                DownloadProgress {
                    completed: done,
                    total,
                    current_file: task.display_name.clone(),
                    bytes_downloaded: task.size,
                    bytes_total: task.size,
                    phase: phase.to_string(),
                },
            );
            return Ok(());
        }
    }

    for attempt in 1..=(FETCH_ATTEMPTS + 1) {
        match download_file_streaming(task, app, phase, total, completed).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt <= FETCH_ATTEMPTS => {
                log::warn!(
                    "Retry {}/{}: {} - {}",
                    attempt,
                    FETCH_ATTEMPTS,
                    task.display_name,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

async fn download_file_streaming(
    task: &DownloadTask,
    app: &AppHandle,
    phase: &'static str,
    total: usize,
    completed: &std::sync::atomic::AtomicUsize,
) -> Result<(), String> {
    if let Some(parent) = task.dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directory {:?}: {}", parent, e))?;
    }

    let res = HTTP_CLIENT
        .get(&task.url)
        .send()
        .await
        .map_err(|e| format!("request failed {}: {}", task.display_name, e))?;

    if !res.status().is_success() {
        return Err(format!(
            "HTTP request failed {} ({})",
            task.display_name,
            res.status()
        ));
    }

    let content_length = res.content_length().unwrap_or(task.size);
    let mut stream = res.bytes_stream();
    let mut bytes_buf: Vec<u8> = Vec::with_capacity(content_length as usize);
    let mut bytes_downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("failed to read stream: {}", e))?;
        bytes_downloaded += chunk.len() as u64;
        bytes_buf.extend_from_slice(&chunk);

        let _ = app.emit(
            "download://progress",
            DownloadProgress {
                completed: completed.load(std::sync::atomic::Ordering::Relaxed),
                total,
                current_file: task.display_name.clone(),
                bytes_downloaded,
                bytes_total: content_length,
                phase: phase.to_string(),
            },
        );
    }

    let sha1_expected = task.sha1.clone();
    let hash = {
        let data = bytes_buf.clone();
        tokio::task::spawn_blocking(move || format!("{}", Sha1::from(data).digest()))
            .await
            .map_err(|e| format!("failed to compute SHA1: {}", e))?
    };

    // sha1 が空 (Fabric ライブラリ等) はスキップ
    if !sha1_expected.is_empty() && hash != sha1_expected {
        return Err(format!(
            "SHA1 mismatch {} (expected: {}, actual: {})",
            task.display_name, sha1_expected, hash
        ));
    }

    fs::write(&task.dest, &bytes_buf)
        .map_err(|e| format!("failed to write {:?}: {}", task.dest, e))?;

    let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let _ = app.emit(
        "download://progress",
        DownloadProgress {
            completed: done,
            total,
            current_file: task.display_name.clone(),
            bytes_downloaded,
            bytes_total: content_length,
            phase: phase.to_string(),
        },
    );

    Ok(())
}

fn library_to_task(lib: &Library, paths: &LauncherPaths) -> Option<DownloadTask> {
    if !lib.is_allowed_on_current_os() {
        return None;
    }
    let artifact = lib.downloads.as_ref()?.artifact.as_ref()?;
    Some(DownloadTask {
        url: artifact.url.clone(),
        dest: paths.libraries().join(&artifact.path),
        sha1: artifact.sha1.clone(),
        display_name: lib.name.clone(),
        size: artifact.size,
    })
}

async fn verify_sha1_async(path: &Path, expected: &str) -> Result<bool, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read file {:?}: {}", path, e))?;
    let expected = expected.to_string();
    let result =
        tokio::task::spawn_blocking(move || format!("{}", Sha1::from(bytes).digest()) == expected)
            .await
            .map_err(|e| format!("failed to verify SHA1: {}", e))?;
    Ok(result)
}
