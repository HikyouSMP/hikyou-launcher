use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::core::{atomic_file, profile};

const MAX_SCREENSHOT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotItem {
    pub profile_id: String,
    pub profile_name: String,
    pub filename: String,
    pub captured_at: i64,
    pub size_bytes: u64,
}

pub async fn list_all(root: &Path) -> Result<Vec<ScreenshotItem>, String> {
    let profiles = profile::list_profiles(root).await;
    let mut items = Vec::new();
    for profile in profiles {
        let directory = profile::profile_game_dir_for_ref(root, &profile.id)?.join("screenshots");
        if !directory.exists() {
            continue;
        }
        let mut entries = tokio::fs::read_dir(directory)
            .await
            .map_err(|e| format!("failed to read screenshots: {e}"))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("failed to read screenshot entry: {e}"))?
        {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| format!("failed to inspect screenshot: {e}"))?;
            let filename = entry.file_name().to_string_lossy().to_string();
            if !metadata.is_file()
                || metadata.len() > MAX_SCREENSHOT_BYTES
                || !matches!(
                    extension(&filename).as_deref(),
                    Some("png" | "jpg" | "jpeg")
                )
            {
                continue;
            }
            let captured_at = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or_default();
            items.push(ScreenshotItem {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                filename,
                captured_at,
                size_bytes: metadata.len(),
            });
        }
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.captured_at));
    Ok(items)
}

pub async fn thumbnail(
    root: &Path,
    profile_ref: &str,
    filename: &str,
    max_width: u32,
    max_height: u32,
) -> Result<String, String> {
    let path = screenshot_path(root, profile_ref, filename)?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("failed to inspect screenshot: {e}"))?;
    if metadata.len() > MAX_SCREENSHOT_BYTES {
        return Err("screenshot exceeds 64 MiB".to_string());
    }
    let max_width = max_width.clamp(64, 2560);
    let max_height = max_height.clamp(64, 1440);
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let cache_key = Sha256::digest(format!(
        "{}:{}:{modified}:{max_width}:{max_height}",
        path.to_string_lossy(),
        metadata.len(),
    ));
    let cache_name = cache_key
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let cache_path = root
        .join("caches")
        .join("screenshots")
        .join(format!("{cache_name}.jpg"));
    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        return Ok(image_data_url(&bytes));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("failed to read screenshot: {e}"))?;
    let encoded = tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&bytes)
            .map_err(|e| format!("failed to decode screenshot: {e}"))?;
        let thumbnail = image.thumbnail(max_width, max_height);
        let mut output = Vec::new();
        JpegEncoder::new_with_quality(&mut output, 88)
            .encode_image(&thumbnail)
            .map_err(|e| format!("failed to encode screenshot thumbnail: {e}"))?;
        Ok::<Vec<u8>, String>(output)
    })
    .await
    .map_err(|e| format!("screenshot thumbnail task failed: {e}"))??;
    let cache_copy = encoded.clone();
    tokio::task::spawn_blocking(move || atomic_file::write(&cache_path, &cache_copy))
        .await
        .map_err(|e| format!("screenshot cache task failed: {e}"))??;
    Ok(image_data_url(&encoded))
}

fn image_data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

pub async fn trash(root: &Path, profile_ref: &str, filename: &str) -> Result<(), String> {
    let source = screenshot_path(root, profile_ref, filename)?;
    let game_dir = profile::profile_game_dir_for_ref(root, profile_ref)?;
    let trash = game_dir.join(".hikyou").join("trash").join("screenshots");
    tokio::fs::create_dir_all(&trash)
        .await
        .map_err(|e| format!("failed to create screenshot trash: {e}"))?;
    let destination = trash.join(format!(
        "{}-{filename}",
        chrono::Utc::now().timestamp_millis()
    ));
    tokio::fs::rename(source, destination)
        .await
        .map_err(|e| format!("failed to move screenshot to trash: {e}"))
}

fn screenshot_path(root: &Path, profile_ref: &str, filename: &str) -> Result<PathBuf, String> {
    if filename.is_empty() || filename.contains(['/', '\\', '\0']) {
        return Err("invalid screenshot filename".to_string());
    }
    if !matches!(extension(filename).as_deref(), Some("png" | "jpg" | "jpeg")) {
        return Err("unsupported screenshot format".to_string());
    }
    let directory = profile::profile_game_dir_for_ref(root, profile_ref)?.join("screenshots");
    let path = directory.join(filename);
    if !path.is_file() {
        return Err("screenshot was not found".to_string());
    }
    let directory = directory
        .canonicalize()
        .map_err(|e| format!("failed to resolve screenshot directory: {e}"))?;
    let path = path
        .canonicalize()
        .map_err(|e| format!("failed to resolve screenshot: {e}"))?;
    if !path.starts_with(directory) {
        return Err("screenshot path escaped the profile".to_string());
    }
    Ok(path)
}

fn extension(filename: &str) -> Option<String> {
    filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::thumbnail;
    use std::time::Instant;

    #[tokio::test]
    async fn thumbnail_cache_reuses_derived_image() {
        let root = std::env::temp_dir().join(format!("hikyou-screenshot-{}", uuid::Uuid::new_v4()));
        let directory = root
            .join("smart-profiles")
            .join("latest-plus")
            .join(".minecraft")
            .join("screenshots");
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("sample.png");
        image::RgbImage::from_fn(1280, 720, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8])
        })
        .save(&source)
        .unwrap();

        let cold_start = Instant::now();
        let cold = thumbnail(&root, "smart:latest-plus", "sample.png", 960, 540)
            .await
            .unwrap();
        let cold_elapsed = cold_start.elapsed();
        let warm_start = Instant::now();
        let warm = thumbnail(&root, "smart:latest-plus", "sample.png", 960, 540)
            .await
            .unwrap();
        let warm_elapsed = warm_start.elapsed();

        assert_eq!(cold, warm);
        assert!(cold.starts_with("data:image/jpeg;base64,"));
        assert_eq!(
            std::fs::read_dir(root.join("caches/screenshots"))
                .unwrap()
                .count(),
            1
        );
        assert!(warm_elapsed < cold_elapsed);
        eprintln!("thumbnail cold={cold_elapsed:?}, warm={warm_elapsed:?}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
