//! セッションファイルロガー
//!
//! 起動ごとに `logs/session_YYYYMMDD_HHMMSS.log` を作成して
//! INFO 以上のログを書き込む。標準エラーにも同時出力する。
//!
//! Rust の `log` クレートのグローバルロガーとして登録するため、
//! `log::info!()` / `log::warn!()` / `log::error!()` が自動的にファイルに記録される。
//! Tauri の setup クロージャの冒頭で `logger::init()` を呼び出すこと。

use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use crate::core::paths::LauncherPaths;

// ── グローバルロガー ──────────────────────────────────────────────────────────

struct SessionLogger {
    /// None = 未初期化（ファイル作成前）
    file: Mutex<Option<File>>,
}

static LOGGER: SessionLogger = SessionLogger {
    file: Mutex::new(None),
};

static PRIVACY: OnceLock<LogPrivacy> = OnceLock::new();

struct LogPrivacy(Vec<(String, &'static str)>);

impl LogPrivacy {
    fn new(roots: impl IntoIterator<Item = (std::path::PathBuf, &'static str)>) -> Self {
        let mut replacements = Vec::new();
        for (path, label) in roots {
            let value = path.to_string_lossy().to_string();
            if value.is_empty() {
                continue;
            }
            replacements.push((value.replace('\\', "/"), label));
            replacements.push((value.replace('\\', "\\\\"), label));
            replacements.push((value, label));
        }
        replacements.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
        replacements.dedup();
        Self(replacements)
    }

    fn redact(&self, mut message: String) -> String {
        for (path, label) in &self.0 {
            let lower = message.to_ascii_lowercase();
            let pattern = path.to_ascii_lowercase();
            let mut redacted = String::new();
            let mut start = 0;
            for (offset, _) in lower.match_indices(&pattern) {
                redacted.push_str(&message[start..offset]);
                redacted.push_str(label);
                start = offset + path.len();
            }
            if start != 0 {
                redacted.push_str(&message[start..]);
                message = redacted;
            }
        }
        message
    }
}

impl Log for SessionLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let mut line = format!(
            "[{} {:5}] {}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.args()
        );
        if let Some(privacy) = PRIVACY.get() {
            line = privacy.redact(line);
        }

        // 標準エラー出力（開発中はここで確認できる）
        eprint!("{}", line);

        // ファイル出力
        if let Ok(mut guard) = LOGGER.file.lock()
            && let Some(f) = guard.as_mut()
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = LOGGER.file.lock()
            && let Some(f) = guard.as_mut()
        {
            let _ = f.flush();
        }
    }
}

// ── 公開関数 ──────────────────────────────────────────────────────────────────

/// セッションロガーを初期化する。
/// `logs/session_YYYYMMDD_HHMMSS.log` を作成してグローバルロガーとして登録する。
/// すでに別のロガーが登録済みの場合は何もしない（エラーを無視）。
pub fn init(paths: &LauncherPaths) {
    let mut roots = vec![(paths.root(), "<launcher>")];
    if let Some(home) = dirs::home_dir() {
        roots.push((home, "<home>"));
    }
    let _ = PRIVACY.set(LogPrivacy::new(roots));
    let dir = paths.logs_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[logger] Failed to create logs directory: {}", e);
        return;
    }

    let filename = format!(
        "session_{}_{}.log",
        Local::now().format("%Y%m%d_%H%M%S_%3f"),
        std::process::id(),
    );
    let path = dir.join(&filename);

    let mut options = OpenOptions::new();
    options.create_new(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(file) => {
            if let Ok(mut guard) = LOGGER.file.lock() {
                *guard = Some(file);
            }
        }
        Err(e) => {
            eprintln!("[logger] Failed to create log file: {}", e);
            return;
        }
    }

    // グローバルロガーとして登録（すでに登録済みなら無視）
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }

    log::info!(
        "=== Hikyou Launcher {} started ===",
        env!("CARGO_PKG_VERSION")
    );
    log::info!("Log file: {}", filename);
}

#[cfg(test)]
mod tests {
    use super::LogPrivacy;

    #[test]
    fn logs_hide_native_forward_and_debug_escaped_paths() {
        let privacy = LogPrivacy::new([(r"D:\private\player".into(), "<home>")]);
        for message in [
            r"Java: D:\private\player\jdk\bin\java.exe",
            r"Java: D:/PRIVATE/player/jdk/bin/java.exe",
            r"Java: D:\\private\\player\\jdk\\bin\\java.exe",
        ] {
            let redacted = privacy.redact(message.to_string());
            assert!(redacted.contains("<home>"));
            assert!(!redacted.contains("player"));
        }
        assert_eq!(privacy.redact("HTTP 403".to_string()), "HTTP 403");
    }
}
