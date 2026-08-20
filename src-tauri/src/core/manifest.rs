//! Mojang バージョンマニフェストの取得とキャッシュ管理

use crate::core::paths::LauncherPaths;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;

const MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

// ────────────────────────────────────────────────────────────────────────────
// 型定義
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub url: String,
    pub sha1: String,
    pub time: Option<String>,
    #[serde(rename = "releaseTime")]
    pub release_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

/// 特定バージョンのJSONから必要な情報
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
    pub libraries: Vec<Library>,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndex,
    pub assets: String,
    pub downloads: VersionDownloads,
    /// 1.13+ の新形式引数
    pub arguments: Option<Arguments>,
    /// 1.12以前の旧形式引数 (スペース区切りの文字列)
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    /// log4j2 設定 (1.7〜1.18.0 に存在、log4shell 対策用)
    pub logging: Option<LoggingConfig>,
}

/// バージョン JSON の logging フィールド
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingConfig {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingClient {
    /// JVM 引数テンプレート。例: "-Dlog4j.configurationFile=${path}"
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingFile {
    pub id: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

/// バージョン JSON の arguments フィールド
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgumentValue>,
    #[serde(default)]
    pub jvm: Vec<ArgumentValue>,
}

/// 引数要素: 単純な文字列か、ルール付き条件引数
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ArgumentValue {
    Simple(String),
    Conditional(ConditionalArgument),
}

/// ルール付き条件引数 (デモモード、カスタム解像度など)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConditionalArgument {
    pub rules: Vec<LibraryRule>,
    pub value: StringOrVec,
}

/// 条件引数の値: 単一または複数文字列
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaVersion {
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<LibraryRule>>,
    /// natives classifierのOS別マッピング
    /// 例: {"windows": "natives-windows", "osx": "natives-osx"}
    pub natives: Option<std::collections::HashMap<String, String>>,
    /// extractフィールド (除外パターンなど、基本無視でOK)
    pub extract: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryRule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsRule {
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryDownloads {
    pub artifact: Option<LibraryArtifact>,
    /// ネイティブライブラリのclassifier別ダウンロード情報
    /// 例: {"natives-windows": {...}, "natives-linux": {...}}
    pub classifiers: Option<std::collections::HashMap<String, LibraryArtifact>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryArtifact {
    pub path: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetIndex {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionDownloads {
    pub client: DownloadEntry,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadEntry {
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

impl Library {
    pub fn is_allowed_on_current_os(&self) -> bool {
        let rules = match &self.rules {
            None => return true,
            Some(r) => r,
        };

        let current_os = match std::env::consts::OS {
            "windows" => "windows",
            "macos" => "osx",
            "linux" => "linux",
            _ => return true,
        };

        let has_allow_rule = rules.iter().any(|r| matches!(r.action, RuleAction::Allow));
        let mut allowed = !has_allow_rule;

        for rule in rules {
            let os_matches = rule
                .os
                .as_ref()
                .and_then(|os| os.name.as_deref())
                .map(|name| name == current_os)
                .unwrap_or(true);

            match rule.action {
                RuleAction::Allow => {
                    if os_matches {
                        allowed = true;
                    }
                }
                RuleAction::Disallow => {
                    if os_matches {
                        allowed = false;
                    }
                }
            }
        }

        allowed
    }

    /// 現在の OS 向け natives classifier 名を返す
    /// 例: Windows なら "natives-windows"
    pub fn native_classifier(&self) -> Option<&str> {
        let natives = self.natives.as_ref()?;
        let os_key = match std::env::consts::OS {
            "windows" => "windows",
            "macos" => "osx",
            "linux" => "linux",
            _ => return None,
        };
        natives.get(os_key).map(|s| s.as_str())
    }

    /// 現在の OS 向け natives JAR のダウンロード情報を返す
    pub fn native_artifact(&self) -> Option<&LibraryArtifact> {
        let classifier = self.native_classifier()?;
        self.downloads
            .as_ref()?
            .classifiers
            .as_ref()?
            .get(classifier)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 公開関数
// ────────────────────────────────────────────────────────────────────────────

pub async fn fetch_manifest(paths: &LauncherPaths) -> Result<VersionManifest, String> {
    let cache_path = paths.versions().join("version_manifest_v2.json");

    if cache_path.exists() {
        let content = fs::read_to_string(&cache_path)
            .map_err(|e| format!("failed to read manifest cache: {}", e))?;
        if let Ok(manifest) = serde_json::from_str(&content) {
            return Ok(manifest);
        }
    }

    let manifest = fetch_manifest_from_api().await?;
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("failed to serialize: {}", e))?;
    fs::write(&cache_path, json).map_err(|e| format!("failed to write cache: {}", e))?;

    Ok(manifest)
}

pub async fn refresh_manifest(paths: &LauncherPaths) -> Result<VersionManifest, String> {
    let manifest = fetch_manifest_from_api().await?;

    let cache_path = paths.versions().join("version_manifest_v2.json");
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("failed to serialize: {}", e))?;
    fs::write(&cache_path, json).map_err(|e| format!("failed to write cache: {}", e))?;

    log::info!("Version manifest updated");
    Ok(manifest)
}

pub async fn fetch_version_json(
    version_id: &str,
    url: &str,
    paths: &LauncherPaths,
) -> Result<VersionJson, String> {
    let cache_path = paths.versions().join(format!("{}.json", version_id));

    if cache_path.exists() {
        let content = fs::read_to_string(&cache_path)
            .map_err(|e| format!("failed to read version JSON cache: {}", e))?;
        if let Ok(version_json) = serde_json::from_str::<VersionJson>(&content) {
            // arguments も minecraftArguments もない = 旧フォーマットでキャッシュされた古いファイル
            // → 削除して再取得する
            if version_json.arguments.is_none() && version_json.minecraft_arguments.is_none() {
                log::warn!(
                    "Version {} cache is in old format. Fetching again.",
                    version_id
                );
                let _ = fs::remove_file(&cache_path);
            } else {
                return Ok(version_json);
            }
        } else {
            // デfailed to serialize = 破損ファイル → 削除して再取得
            let _ = fs::remove_file(&cache_path);
        }
    }

    let client = Client::new();
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("version JSON request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("failed to fetch version JSON: {}", res.status()));
    }

    let version_json: VersionJson = res
        .json()
        .await
        .map_err(|e| format!("failed to parse version JSON: {}", e))?;

    let json = serde_json::to_string_pretty(&version_json)
        .map_err(|e| format!("failed to serialize: {}", e))?;
    fs::write(&cache_path, json).map_err(|e| format!("failed to write cache: {}", e))?;

    log::info!("Fetched JSON for version {}", version_id);
    Ok(version_json)
}

async fn fetch_manifest_from_api() -> Result<VersionManifest, String> {
    let client = Client::new();
    let res = client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|e| format!("manifest request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("failed to fetch manifest: {}", res.status()));
    }

    res.json()
        .await
        .map_err(|e| format!("failed to parse manifest: {}", e))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetIndexFile {
    pub objects: std::collections::HashMap<String, AssetObject>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}
