use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::core::profile;

const MAX_CONTENT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Shader,
    ResourcePack,
}

impl ContentKind {
    fn directory(self) -> &'static str {
        match self {
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentItem {
    pub name: String,
    pub size_bytes: u64,
    pub directory: bool,
}

fn content_dir(root: &Path, profile_ref: &str, kind: ContentKind) -> Result<PathBuf, String> {
    profile::validate_profile_ref(profile_ref)?;
    Ok(profile::profile_game_dir_for_ref(root, profile_ref)?.join(kind.directory()))
}

fn safe_name(name: &str) -> Result<&str, String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains(['/', '\\', '\0'])
    {
        return Err("invalid content filename".to_string());
    }
    Ok(trimmed)
}

pub async fn list(
    root: &Path,
    profile_ref: &str,
    kind: ContentKind,
) -> Result<Vec<ContentItem>, String> {
    let dir = content_dir(root, profile_ref, kind)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| format!("failed to read content directory: {e}"))?;
    let mut items = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("failed to read content entry: {e}"))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.ends_with(".disabled") {
            continue;
        }
        let metadata = entry
            .metadata()
            .await
            .map_err(|e| format!("failed to inspect content entry: {e}"))?;
        if metadata.is_file() && !name.to_ascii_lowercase().ends_with(".zip") {
            continue;
        }
        items.push(ContentItem {
            name,
            size_bytes: metadata.len(),
            directory: metadata.is_dir(),
        });
    }
    items.sort_by_key(|item| item.name.to_lowercase());
    Ok(items)
}

pub async fn import(
    root: &Path,
    profile_ref: &str,
    kind: ContentKind,
    filename: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
    let filename = safe_name(filename)?;
    if !filename.to_ascii_lowercase().ends_with(".zip") {
        return Err("content packs must be ZIP files".to_string());
    }
    if bytes.is_empty() || bytes.len() > MAX_CONTENT_BYTES {
        return Err("content pack is empty or exceeds 256 MiB".to_string());
    }
    validate_zip(&bytes)?;
    let dir = content_dir(root, profile_ref, kind)?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("failed to create content directory: {e}"))?;
    let destination = dir.join(filename);
    if destination.exists() {
        return Err("a content pack with this name already exists".to_string());
    }
    let temporary = dir.join(format!(".{filename}.{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|e| format!("failed to stage content pack: {e}"))?;
    tokio::fs::rename(&temporary, &destination)
        .await
        .map_err(|e| format!("failed to commit content pack: {e}"))?;
    Ok(())
}

pub async fn remove(
    root: &Path,
    profile_ref: &str,
    kind: ContentKind,
    name: &str,
) -> Result<(), String> {
    let name = safe_name(name)?;
    let source = content_dir(root, profile_ref, kind)?.join(name);
    if !source.exists() {
        return Err("content pack was not found".to_string());
    }
    let trash = profile::profile_game_dir_for_ref(root, profile_ref)?
        .join(".hikyou")
        .join("trash")
        .join(kind.directory());
    tokio::fs::create_dir_all(&trash)
        .await
        .map_err(|e| format!("failed to create content trash: {e}"))?;
    let destination = trash.join(format!("{}-{name}", chrono::Utc::now().timestamp_millis()));
    tokio::fs::rename(source, destination)
        .await
        .map_err(|e| format!("failed to move content pack to trash: {e}"))
}

fn validate_zip(bytes: &[u8]) -> Result<(), String> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|_| "invalid ZIP archive".to_string())?;
    if archive.len() > 10_000 {
        return Err("content pack contains too many files".to_string());
    }
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| "invalid ZIP entry".to_string())?;
        if file.enclosed_name().is_none() {
            return Err("content pack contains an unsafe path".to_string());
        }
        expanded = expanded.saturating_add(file.size());
        if expanded > 1024 * 1024 * 1024 {
            return Err("content pack expands beyond 1 GiB".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::safe_name;

    #[test]
    fn content_names_cannot_escape_their_directory() {
        assert!(safe_name("pack.zip").is_ok());
        assert!(safe_name("../pack.zip").is_err());
        assert!(safe_name("a/b.zip").is_err());
    }
}
