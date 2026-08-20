use std::io::{Cursor, Read};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FabricDependency {
    pub(super) mod_id: String,
    pub(super) predicate: Option<String>,
}

pub(super) struct InstalledModManifest {
    pub(super) filename: String,
    pub(super) id: String,
    pub(super) version: Option<String>,
    pub(super) required_dependencies: Vec<FabricDependency>,
    pub(super) incompatible_dependencies: Vec<FabricDependency>,
}

pub(super) fn mod_jar_is_loadable_for_loader(bytes: &[u8], loader: &str) -> bool {
    if loader != "fabric" && loader != "quilt" {
        return true;
    }
    match fabric_mod_json_has_invalid_wildcard_predicate(bytes) {
        Ok(false) => true,
        Ok(true) => false,
        Err(error) => {
            log::warn!("[mods] Failed to inspect fabric.mod.json: {}", error);
            false
        }
    }
}

pub(super) fn fabric_mod_json_has_invalid_wildcard_predicate(bytes: &[u8]) -> Result<bool, String> {
    let json = read_fabric_mod_json(bytes)?;

    for key in ["depends", "recommends", "suggests", "conflicts", "breaks"] {
        if let Some(value) = json.get(key)
            && json_value_has_invalid_wildcard_predicate(value)
        {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(super) fn fabric_mod_json_required_dependencies(
    bytes: &[u8],
) -> Result<Vec<FabricDependency>, String> {
    let json = read_fabric_mod_json(bytes)?;
    let mut dependencies = Vec::new();
    if let Some(serde_json::Value::Object(depends)) = json.get("depends") {
        for (mod_id, value) in depends {
            if is_external_fabric_dependency(mod_id) {
                dependencies.push(FabricDependency {
                    mod_id: mod_id.to_string(),
                    predicate: fabric_dependency_predicate(value),
                });
            }
        }
    }
    sort_dependencies(&mut dependencies);
    Ok(dependencies)
}

pub(super) fn read_installed_mod_manifest(
    filename: &str,
    bytes: &[u8],
) -> Result<Option<InstalledModManifest>, String> {
    let json = read_fabric_mod_json(bytes)?;
    if json.is_null() {
        return Ok(None);
    }
    let Some(id) = json.get("id").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    Ok(Some(InstalledModManifest {
        filename: filename.to_string(),
        id: id.to_string(),
        version: json
            .get("version")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        required_dependencies: fabric_mod_json_required_dependencies(bytes)?,
        incompatible_dependencies: fabric_mod_json_incompatible_dependencies(bytes)?,
    }))
}

pub(super) fn fabric_mod_json_incompatible_dependencies(
    bytes: &[u8],
) -> Result<Vec<FabricDependency>, String> {
    let json = read_fabric_mod_json(bytes)?;
    let mut dependencies = Vec::new();
    for key in ["breaks", "conflicts"] {
        if let Some(serde_json::Value::Object(entries)) = json.get(key) {
            for (mod_id, value) in entries {
                if is_external_fabric_dependency(mod_id) {
                    dependencies.push(FabricDependency {
                        mod_id: mod_id.to_string(),
                        predicate: fabric_dependency_predicate(value),
                    });
                }
            }
        }
    }
    sort_dependencies(&mut dependencies);
    Ok(dependencies)
}

fn sort_dependencies(dependencies: &mut Vec<FabricDependency>) {
    dependencies.sort_by(|a, b| {
        a.mod_id
            .cmp(&b.mod_id)
            .then_with(|| a.predicate.cmp(&b.predicate))
    });
    dependencies.dedup();
}

fn fabric_dependency_predicate(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.trim().to_string()).filter(|s| !s.is_empty()),
        serde_json::Value::Array(values) => values.iter().find_map(fabric_dependency_predicate),
        serde_json::Value::Object(map) => map.values().find_map(fabric_dependency_predicate),
        _ => None,
    }
}

fn read_fabric_mod_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("failed to open mod JAR: {}", e))?;
    let Ok(mut entry) = archive.by_name("fabric.mod.json") else {
        return Ok(serde_json::Value::Null);
    };

    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|e| format!("failed to read fabric.mod.json: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("failed to parse fabric.mod.json: {}", e))
}

fn is_external_fabric_dependency(mod_id: &str) -> bool {
    !matches!(
        mod_id,
        "java"
            | "minecraft"
            | "fabricloader"
            | "fabric-loader"
            | "quilt_loader"
            | "quilt-loader"
            | "quilted_fabric_api"
    )
}

fn json_value_has_invalid_wildcard_predicate(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => version_predicate_has_invalid_wildcard(text),
        serde_json::Value::Array(values) => {
            values.iter().any(json_value_has_invalid_wildcard_predicate)
        }
        serde_json::Value::Object(map) => {
            map.values().any(json_value_has_invalid_wildcard_predicate)
        }
        _ => false,
    }
}

fn version_predicate_has_invalid_wildcard(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let has_wildcard = lower.contains(".x") || lower.contains(".*");
    has_wildcard
        && [">=", "<=", ">", "<"]
            .iter()
            .any(|operator| lower.contains(operator))
}
