//! Bounded provider observations. They never own selection or installation.
use super::{mod_intent::valid_project, modrinth_provider::ModrinthVersion};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;

#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub icon_url: Option<String>,
}

fn client() -> Result<reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = crate::http::builder()?
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("HikyouLauncher/1.0")
        .build()
        .map_err(|_| "mod_catalog_client_failed".to_string())?;
    let _ = CLIENT.set(client.clone());
    Ok(client)
}

async fn body<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    limit: usize,
) -> Result<T, String> {
    let mut response = request
        .send()
        .await
        .map_err(|_| "mod_catalog_unavailable")?
        .error_for_status()
        .map_err(|_| "mod_catalog_http_failed")?;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("mod_catalog_too_large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "mod_catalog_unavailable")?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("mod_catalog_too_large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "mod_catalog_invalid".into())
}

pub async fn project(id: &str) -> Result<Project, String> {
    if !valid_project(id) {
        return Err("mod_catalog_invalid_identity".into());
    }
    let value: Project = body(
        client()?.get(format!("https://api.modrinth.com/v2/project/{id}")),
        256 * 1024,
    )
    .await?;
    if !valid_project(&value.id)
        || (value.id != id && value.slug != id)
        || value.title.trim().is_empty()
        || value.title.len() > 4096
    {
        return Err("mod_catalog_identity_mismatch".into());
    }
    Ok(value)
}

pub(super) async fn versions(
    id: &str,
    mc: Option<&str>,
    loader: &str,
) -> Result<Vec<ModrinthVersion>, String> {
    if !valid_project(id) || !["fabric", "quilt", "forge", "neoforge"].contains(&loader) {
        return Err("mod_catalog_invalid_target".into());
    }
    let mut query = vec![
        (
            "loaders",
            serde_json::to_string(&[loader]).map_err(|e| e.to_string())?,
        ),
        ("include_changelog", "false".into()),
    ];
    if let Some(mc) = mc {
        if !super::paths::is_safe_component(mc) {
            return Err("mod_catalog_invalid_target".into());
        }
        query.push((
            "game_versions",
            serde_json::to_string(&[mc]).map_err(|e| e.to_string())?,
        ));
    }
    let result: Vec<ModrinthVersion> = body(
        client()?
            .get(format!("https://api.modrinth.com/v2/project/{id}/version"))
            .query(&query),
        8 * 1024 * 1024,
    )
    .await?;
    if result.len() > 4096
        || result.iter().any(|v| {
            v.project_id.as_deref() != Some(id)
                || !v.loaders.iter().any(|l| l == loader)
                || mc.is_some_and(|mc| !v.game_versions.iter().any(|v| v == mc))
        })
    {
        return Err("mod_catalog_version_mismatch".into());
    }
    Ok(result)
}
