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
use std::sync::Mutex;

use crate::core::paths::LauncherPaths;

// ── グローバルロガー ──────────────────────────────────────────────────────────

struct SessionLogger {
    /// None = 未初期化（ファイル作成前）
    file: Mutex<Option<File>>,
}

// SAFETY: File は Send、Mutex は Sync を提供する
unsafe impl Sync for SessionLogger {}

static LOGGER: SessionLogger = SessionLogger {
    file: Mutex::new(None),
};

impl Log for SessionLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let line = format!(
            "[{} {:5}] {}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.args()
        );

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
    let dir = paths.logs_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[logger] Failed to create logs directory: {}", e);
        return;
    }

    let filename = Local::now().format("session_%Y%m%d_%H%M%S.log").to_string();
    let path = dir.join(&filename);

    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => {
            if let Ok(mut guard) = LOGGER.file.lock() {
                *guard = Some(file);
            }
        }
        Err(e) => {
            eprintln!("[logger] Failed to create log file {:?}: {}", path, e);
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
    log::info!("Log file: {:?}", path);
}
