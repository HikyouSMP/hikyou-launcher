use std::{
    cmp::Reverse,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use flate2::read::GzDecoder;
use serde::Serialize;

use crate::{LauncherPaths, core};

const MAX_LOG_READ_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct CrashAnalysisPayload {
    pub profile_id: String,
    pub source: String,
    pub source_path: Option<String>,
    pub lines: Vec<String>,
    pub parsed: core::crash_parser::ParsedCrash,
}

#[derive(Debug, Serialize)]
pub struct LogSourcePayload {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub source_path: String,
    pub modified_ms: u64,
}

#[tauri::command]
pub fn parse_crash_log(log_lines: Vec<String>, lang: String) -> core::crash_parser::ParsedCrash {
    core::crash_parser::parse(&log_lines, &lang)
}

#[tauri::command]
pub async fn get_latest_crash_analysis(
    profile_id: String,
    lang: String,
    since_ms: Option<u64>,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Option<CrashAnalysisPayload>, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    let latest_log = game_dir.join("logs").join("latest.log");
    let crash_artifact = newest_recent_crash_artifact(&game_dir, since_ms)?;

    let (source, source_path) = if let Some((kind, path)) = crash_artifact {
        (kind, path)
    } else if latest_log.is_file() {
        ("latest_log".to_string(), latest_log)
    } else {
        return Ok(None);
    };

    let content = read_log_text(&source_path)?;
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    if lines.len() > 3000 {
        lines = lines.split_off(lines.len() - 3000);
    }

    let parsed = core::crash_parser::parse(&lines, &lang);
    if source == "latest_log" && !is_actionable_latest_log(&parsed) {
        return Ok(None);
    }

    Ok(Some(CrashAnalysisPayload {
        profile_id,
        source,
        source_path: Some(source_path.to_string_lossy().to_string()),
        lines,
        parsed,
    }))
}

fn is_actionable_latest_log(parsed: &core::crash_parser::ParsedCrash) -> bool {
    parsed.is_crash_report
        || parsed.rule_match.is_some()
        || !parsed.exceptions.is_empty()
        || parsed.diagnosis.confidence >= 0.5
}

#[tauri::command]
pub fn list_profile_log_sources(
    profile_id: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<LogSourcePayload>, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    let mut sources = Vec::new();

    collect_log_file(
        &mut sources,
        game_dir.join("logs").join("latest.log"),
        "latest_log",
        "latest.log",
    );
    collect_dir_logs(&mut sources, game_dir.join("logs"), "game_log")?;
    collect_dir_logs(&mut sources, game_dir.join("crash-reports"), "crash_report")?;
    collect_jvm_crash_logs(&mut sources, &game_dir)?;

    sources.sort_by_key(|source| Reverse(source.modified_ms));
    sources.dedup_by(|a, b| a.source_path == b.source_path);
    Ok(sources)
}

#[tauri::command]
pub fn read_profile_log_source(
    profile_id: String,
    source_path: String,
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<Vec<String>, String> {
    let game_dir = paths.checked_profile_game_dir(&profile_id)?;
    let root = game_dir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve profile directory: {}", e))?;
    let requested = PathBuf::from(source_path)
        .canonicalize()
        .map_err(|e| format!("Failed to resolve log file: {}", e))?;
    if !requested.starts_with(&root) || !requested.is_file() {
        return Err("Log file is outside of this profile.".to_string());
    }
    let content = read_log_text(&requested)?;
    Ok(content.lines().map(str::to_string).collect())
}

#[tauri::command]
pub fn ensure_log_inspector_enabled_cmd(
    paths: tauri::State<'_, Arc<LauncherPaths>>,
) -> Result<(), String> {
    ensure_log_inspector_enabled(&paths)?;
    Ok(())
}

fn ensure_log_inspector_enabled(paths: &LauncherPaths) -> Result<(), String> {
    let path = paths.root().join("settings.json");
    let Ok(json) = fs::read_to_string(path) else {
        return Err("Log Inspector is disabled.".to_string());
    };
    let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    let enabled = value
        .pointer("/advanced/enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let inspector_enabled = value
        .pointer("/advanced/logInspectorEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if enabled && inspector_enabled {
        Ok(())
    } else {
        Err("Log Inspector is disabled.".to_string())
    }
}

fn newest_recent_crash_artifact(
    game_dir: &Path,
    since_ms: Option<u64>,
) -> Result<Option<(String, PathBuf)>, String> {
    let dir = game_dir.join("crash-reports");
    let min_ms = since_ms.unwrap_or(0).saturating_sub(5_000);
    let mut newest: Option<(String, PathBuf, u64)> = None;

    if dir.is_dir() {
        for entry in
            fs::read_dir(&dir).map_err(|e| format!("Failed to read crash reports: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to inspect crash report: {}", e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("txt") {
                continue;
            }
            consider_recent_log(&mut newest, "crash_report", path, &entry, min_ms);
        }
    }

    if game_dir.is_dir() {
        for entry in fs::read_dir(game_dir)
            .map_err(|e| format!("Failed to read profile directory: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to inspect profile file: {}", e))?;
            let path = entry.path();
            if !is_hs_err_log(&path) {
                continue;
            }
            consider_recent_log(&mut newest, "jvm_crash", path, &entry, min_ms);
        }
    }

    Ok(newest.map(|(kind, path, _)| (kind, path)))
}

fn consider_recent_log(
    newest: &mut Option<(String, PathBuf, u64)>,
    kind: &str,
    path: PathBuf,
    entry: &fs::DirEntry,
    min_ms: u64,
) {
    let modified = entry
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(system_time_ms)
        .unwrap_or(0);
    if modified < min_ms {
        return;
    }
    if newest
        .as_ref()
        .map(|(_, _, t)| modified > *t)
        .unwrap_or(true)
    {
        *newest = Some((kind.to_string(), path, modified));
    }
}

fn collect_jvm_crash_logs(
    sources: &mut Vec<LogSourcePayload>,
    game_dir: &Path,
) -> Result<(), String> {
    if !game_dir.is_dir() {
        return Ok(());
    }
    for entry in
        fs::read_dir(game_dir).map_err(|e| format!("Failed to read profile directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to inspect profile file: {}", e))?;
        let path = entry.path();
        if !is_hs_err_log(&path) {
            continue;
        }
        let label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("jvm crash")
            .to_string();
        collect_log_file(sources, path, "jvm_crash", &label);
    }
    Ok(())
}

fn is_hs_err_log(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|name| name.starts_with("hs_err_pid") && name.ends_with(".log"))
        .unwrap_or(false)
}

fn collect_dir_logs(
    sources: &mut Vec<LogSourcePayload>,
    dir: PathBuf,
    kind: &str,
) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read logs: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to inspect log: {}", e))?;
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let is_gz_log = ext == "gz"
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| name.ends_with(".log.gz") || name.ends_with(".txt.gz"))
                .unwrap_or(false);
        if !matches!(ext, "log" | "txt") && !is_gz_log {
            continue;
        }
        let label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("log")
            .to_string();
        collect_log_file(sources, path, kind, &label);
    }
    Ok(())
}

fn collect_log_file(sources: &mut Vec<LogSourcePayload>, path: PathBuf, kind: &str, label: &str) {
    if !path.is_file() {
        return;
    }
    let modified_ms = path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(system_time_ms)
        .unwrap_or(0);
    sources.push(LogSourcePayload {
        id: path.to_string_lossy().to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        source_path: path.to_string_lossy().to_string(),
        modified_ms,
    });
}

fn read_log_text(path: &Path) -> Result<String, String> {
    let is_gz = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));

    let file = fs::File::open(path).map_err(|e| format!("Failed to open log file: {}", e))?;
    let mut reader: Box<dyn Read> = if is_gz {
        Box::new(GzDecoder::new(file).take(MAX_LOG_READ_BYTES))
    } else {
        Box::new(file.take(MAX_LOG_READ_BYTES))
    };
    let mut content = String::new();
    reader
        .read_to_string(&mut content)
        .map_err(|e| format!("Failed to read log file: {}", e))?;
    Ok(content)
}

fn system_time_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}
