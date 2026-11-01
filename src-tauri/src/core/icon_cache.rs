//! Presentation-only Modrinth icons. Never writes Mod wishes or revisions.
use crate::core::{managed_fs, paths::LauncherPaths};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha1_smol::Sha1;
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    sync::OnceLock,
    time::{Duration, Instant},
};

#[path = "icon_cache_raster.rs"]
mod raster;
const MAX_IMAGE_BYTES: usize = 1024 * 1024;
const MAX_METADATA_BYTES: usize = 64 * 1024;
static FETCH_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

#[derive(Serialize)]
pub struct IconData {
    pub data_url: String,
    pub warning: Option<&'static str>,
}

pub fn validate_icon_url(input: &str) -> Result<url::Url, String> {
    if input.len() > 2048 {
        return Err("icon_untrusted_url".into());
    }
    let url = url::Url::parse(input).map_err(|_| "icon_untrusted_url")?;
    if url.scheme() != "https"
        || url.host_str() != Some("cdn.modrinth.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || !url.path().starts_with("/data/")
    {
        return Err("icon_untrusted_url".into());
    }
    Ok(url)
}

fn client() -> Result<reqwest::Client, String> {
    crate::http::builder()
        .map_err(|_| "icon_network_unavailable")?
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|_| "icon_network_unavailable".into())
}

async fn bounded_body(mut response: reqwest::Response, max: usize) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err("icon_http_failed".into());
    }
    if response
        .content_length()
        .is_some_and(|len| len > max as u64)
    {
        return Err("icon_too_large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "icon_network_unavailable")?
    {
        if chunk.len() > max.saturating_sub(bytes.len()) {
            return Err("icon_too_large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Deserialize)]
struct ProjectIcon {
    id: String,
    slug: String,
    icon_url: Option<String>,
}
type MetadataCache = tokio::sync::Mutex<BTreeMap<String, (Instant, String)>>;

async fn project_icon(project: &str) -> Result<String, String> {
    if !crate::core::mod_intent::valid_project(project) {
        return Err("icon_invalid_project".into());
    }
    static CACHE: OnceLock<MetadataCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| tokio::sync::Mutex::new(BTreeMap::new()));
    if let Some((at, url)) = cache.lock().await.get(project)
        && at.elapsed() < Duration::from_secs(3600)
    {
        return Ok(url.clone());
    }
    let mut endpoint = url::Url::parse("https://api.modrinth.com/v2/project/")
        .map_err(|_| "icon_metadata_failed")?;
    endpoint
        .path_segments_mut()
        .map_err(|_| "icon_metadata_failed")?
        .pop_if_empty()
        .push(project);
    let response = client()?
        .get(endpoint)
        .send()
        .await
        .map_err(|_| "icon_network_unavailable")?;
    let bytes = bounded_body(response, MAX_METADATA_BYTES).await?;
    let metadata: ProjectIcon =
        serde_json::from_slice(&bytes).map_err(|_| "icon_metadata_failed")?;
    if metadata.id != project && metadata.slug != project {
        return Err("icon_metadata_failed".into());
    }
    let url = metadata.icon_url.ok_or("icon_missing")?;
    validate_icon_url(&url)?;
    let mut entries = cache.lock().await;
    if entries.len() >= 128 {
        entries.clear();
    }
    entries.insert(project.into(), (Instant::now(), url.clone()));
    Ok(url)
}

fn read_cached(paths: &LauncherPaths, name: &str) -> Result<Option<Vec<u8>>, String> {
    let root = managed_fs::root(&paths.root()).map_err(|_| "icon_cache_read_failed")?;
    let relative = paths
        .icons_dir()
        .strip_prefix(paths.root())
        .map_err(|_| "icon_cache_read_failed")?
        .to_path_buf();
    if managed_fs::inspect(&root, &relative)
        .map_err(|_| "icon_cache_read_failed")?
        .is_none()
    {
        return Ok(None);
    }
    let dir =
        managed_fs::directory(&root, &relative, false).map_err(|_| "icon_cache_read_failed")?;
    if managed_fs::inspect(&dir, Path::new(name))
        .map_err(|_| "icon_cache_read_failed")?
        .is_none()
    {
        return Ok(None);
    }
    let file =
        managed_fs::read_file(&dir, Path::new(name)).map_err(|_| "icon_cache_read_failed")?;
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "icon_cache_read_failed")?;
    raster::mime(&bytes)?;
    Ok(Some(bytes))
}

fn write_cached(paths: &LauncherPaths, name: &str, bytes: &[u8]) -> Result<(), String> {
    let root = managed_fs::root(&paths.root()).map_err(|_| "icon_cache_write_failed")?;
    let relative = paths
        .icons_dir()
        .strip_prefix(paths.root())
        .map_err(|_| "icon_cache_write_failed")?
        .to_path_buf();
    let dir =
        managed_fs::directory(&root, &relative, true).map_err(|_| "icon_cache_write_failed")?;
    managed_fs::write(&dir, Path::new(name), bytes, true)
        .map_err(|_| "icon_cache_write_failed".into())
}

fn downloaded(paths: &LauncherPaths, name: &str, bytes: Vec<u8>) -> Result<IconData, String> {
    let mime = raster::mime(&bytes)?;
    let warning = write_cached(paths, name, &bytes)
        .err()
        .map(|_| "icon_cache_write_failed");
    Ok(IconData {
        data_url: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        warning,
    })
}

pub async fn fetch_icon_data_url(
    input: Option<&str>,
    project: Option<&str>,
    paths: &LauncherPaths,
) -> Result<IconData, String> {
    tokio::time::timeout(Duration::from_secs(12), async {
        let _slot = FETCH_SLOTS
            .acquire()
            .await
            .map_err(|_| "icon_network_unavailable")?;
        let url = match input.filter(|value| !value.is_empty()) {
            Some(input) => validate_icon_url(input)?.to_string(),
            None => project_icon(project.ok_or("icon_missing")?).await?,
        };
        let name = format!("v2-{}.icon", Sha1::from(url.as_bytes()).digest());
        match read_cached(paths, &name)? {
            Some(bytes) => {
                let mime = raster::mime(&bytes)?;
                Ok(IconData {
                    data_url: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
                    warning: None,
                })
            }
            None => {
                let response = client()?
                    .get(&url)
                    .send()
                    .await
                    .map_err(|_| "icon_network_unavailable")?;
                let bytes = bounded_body(response, MAX_IMAGE_BYTES).await?;
                downloaded(paths, &name, bytes)
            }
        }
    })
    .await
    .map_err(|_| "icon_timeout".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png() -> Result<Vec<u8>, String> {
        STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aC1sAAAAASUVORK5CYII=").map_err(|_| "fixture decode failed".into())
    }
    #[test]
    fn only_public_modrinth_cdn_urls_are_accepted() {
        assert!(validate_icon_url("https://cdn.modrinth.com/data/project/icon.png").is_ok());
        for url in [
            "file:///private/icon.png",
            "http://cdn.modrinth.com/data/icon.png",
            "https://cdn.modrinth.com.evil.invalid/data/icon.png",
            "https://127.0.0.1/data/icon.png",
            "https://name:token@cdn.modrinth.com/data/icon.png",
            "https://cdn.modrinth.com:444/data/icon.png",
            "https://cdn.modrinth.com/data/icon.png?token=private",
            "https://cdn.modrinth.com/data/../private/icon.png",
        ] {
            assert_eq!(
                validate_icon_url(url).err().as_deref(),
                Some("icon_untrusted_url")
            );
        }
        assert!(validate_icon_url(&"x".repeat(2049)).is_err());
    }
    #[test]
    fn cache_read_is_bounded_and_write_failure_is_not_hidden()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        let image = png()?;
        assert!(read_cached(&paths, "test.icon")?.is_none());
        let result = downloaded(&paths, "test.icon", image.clone())?;
        assert!(result.warning.is_none());
        assert_eq!(read_cached(&paths, "test.icon")?, Some(image.clone()));
        std::fs::File::create(paths.icons_dir().join("large.icon"))?
            .set_len(MAX_IMAGE_BYTES as u64 + 1)?;
        assert!(read_cached(&paths, "large.icon").is_err());
        std::fs::write(temp.path().join("blocked"), b"not a directory")?;
        let unavailable = LauncherPaths::new(temp.path().join("blocked"));
        let result = downloaded(&unavailable, "test.icon", image)?;
        assert!(result.data_url.starts_with("data:image/png;base64,"));
        assert_eq!(result.warning, Some("icon_cache_write_failed"));
        Ok(())
    }
    #[tokio::test]
    async fn invalid_input_fails_before_network_or_cache_mutation()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        assert!(
            fetch_icon_data_url(Some("https://evil.invalid/icon.png"), None, &paths)
                .await
                .is_err()
        );
        assert!(
            fetch_icon_data_url(None, Some("../private"), &paths)
                .await
                .is_err()
        );
        assert!(!paths.icons_dir().exists());
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn linked_cache_does_not_read_or_replace_outside_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let paths = LauncherPaths::new(temp.path().to_path_buf());
        std::fs::create_dir(temp.path().join("caches"))?;
        std::os::unix::fs::symlink(outside.path(), paths.icons_dir())?;
        std::fs::write(outside.path().join("test.icon"), b"private")?;
        assert!(read_cached(&paths, "test.icon").is_err());
        assert!(write_cached(&paths, "test.icon", &png()?).is_err());
        assert_eq!(std::fs::read(outside.path().join("test.icon"))?, b"private");
        Ok(())
    }
}
