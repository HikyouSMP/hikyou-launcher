use fastnbt::Value;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::core::{atomic_file, profile};

#[derive(Debug, Default, Serialize, Deserialize)]
struct ServersFile {
    #[serde(default)]
    servers: Vec<ServerRecord>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerRecord {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ip: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(rename = "acceptTextures", default)]
    accept_textures: Option<i8>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub key: String,
    pub name: String,
    pub address: String,
    pub has_icon: bool,
    pub accepts_textures: Option<bool>,
}

fn servers_path(root: &Path, profile_ref: &str) -> Result<PathBuf, String> {
    profile::validate_profile_ref(profile_ref)?;
    Ok(profile::profile_game_dir_for_ref(root, profile_ref)?.join("servers.dat"))
}

pub async fn list(root: &Path, profile_ref: &str) -> Result<Vec<ServerSummary>, String> {
    let file = read_file(&servers_path(root, profile_ref)?).await?;
    Ok(file
        .servers
        .into_iter()
        .map(|server| ServerSummary {
            key: server_key(&server.name, &server.ip),
            name: server.name,
            address: server.ip,
            has_icon: server.icon.as_ref().is_some_and(|value| !value.is_empty()),
            accepts_textures: server.accept_textures.map(|value| value != 0),
        })
        .collect())
}

pub async fn save(
    root: &Path,
    profile_ref: &str,
    key: Option<&str>,
    name: &str,
    address: &str,
) -> Result<(), String> {
    let name = name.trim();
    let address = address.trim();
    if name.is_empty() || name.len() > 128 || address.is_empty() || address.len() > 255 {
        return Err("server name or address is invalid".to_string());
    }
    if address.contains(['\0', '\r', '\n']) {
        return Err("server address contains invalid characters".to_string());
    }
    let path = servers_path(root, profile_ref)?;
    let mut file = read_file(&path).await?;
    match key {
        Some(key) => {
            let server = file
                .servers
                .iter_mut()
                .find(|server| server_key(&server.name, &server.ip) == key)
                .ok_or_else(|| "server entry was not found".to_string())?;
            server.name = name.to_string();
            server.ip = address.to_string();
        }
        None => file.servers.push(ServerRecord {
            name: name.to_string(),
            ip: address.to_string(),
            icon: None,
            accept_textures: None,
            extra: HashMap::new(),
        }),
    }
    write_file(&path, &file).await
}

pub async fn remove(root: &Path, profile_ref: &str, key: &str) -> Result<(), String> {
    let path = servers_path(root, profile_ref)?;
    let mut file = read_file(&path).await?;
    let old_len = file.servers.len();
    file.servers
        .retain(|server| server_key(&server.name, &server.ip) != key);
    if file.servers.len() == old_len {
        return Err("server entry was not found".to_string());
    }
    write_file(&path, &file).await
}

async fn read_file(path: &Path) -> Result<ServersFile, String> {
    if !path.exists() {
        return Ok(ServersFile::default());
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("failed to read servers.dat: {e}"))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("servers.dat is unexpectedly large".to_string());
    }
    fastnbt::from_bytes(&bytes).map_err(|e| format!("failed to parse servers.dat: {e}"))
}

async fn write_file(path: &Path, file: &ServersFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "invalid servers.dat path".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| format!("failed to create game directory: {e}"))?;
    let bytes =
        fastnbt::to_bytes(file).map_err(|e| format!("failed to encode servers.dat: {e}"))?;
    if path.exists() {
        tokio::fs::copy(path, path.with_extension("dat.bak"))
            .await
            .map_err(|e| format!("failed to back up servers.dat: {e}"))?;
    }
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || atomic_file::write(&path, &bytes))
        .await
        .map_err(|e| format!("servers.dat write task failed: {e}"))?
}

fn server_key(name: &str, address: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(name.as_bytes());
    digest.update([0]);
    digest.update(address.as_bytes());
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::{ServerRecord, ServersFile, server_key};
    use fastnbt::Value;
    use std::collections::HashMap;

    #[test]
    fn server_keys_change_with_identity() {
        assert_ne!(server_key("A", "a.example"), server_key("A", "b.example"));
    }

    #[test]
    fn nbt_roundtrip_preserves_fields_hikyou_does_not_own() {
        let file = ServersFile {
            servers: vec![ServerRecord {
                name: "Example".to_string(),
                ip: "example.invalid".to_string(),
                icon: None,
                accept_textures: Some(1),
                extra: HashMap::from([("custom".to_string(), Value::Int(42))]),
            }],
            extra: HashMap::from([("rootCustom".to_string(), Value::String("kept".to_string()))]),
        };
        let encoded = fastnbt::to_bytes(&file).unwrap();
        let decoded: ServersFile = fastnbt::from_bytes(&encoded).unwrap();
        assert_eq!(
            decoded.extra.get("rootCustom"),
            Some(&Value::String("kept".to_string()))
        );
        assert_eq!(
            decoded.servers[0].extra.get("custom"),
            Some(&Value::Int(42))
        );
    }
}
