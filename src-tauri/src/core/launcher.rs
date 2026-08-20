//! Minecraft ゲーム起動モジュール
//!
//! バージョン JSON の arguments テンプレートを正しく展開して起動する。
//! 1.13+ の arguments.game / arguments.jvm と、
//! 1.12以前の minecraftArguments 両方に対応。

use crate::auth::StoredAuth;
use crate::core::{java, manifest::VersionJson, paths::LauncherPaths};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// 起動リクエスト
pub struct LaunchRequest<'a> {
    pub version_id: &'a str,
    /// Profile ref. UUID normal profiles and fixed smart refs both use profile-owned `.minecraft`.
    pub profile_id: Option<&'a str>,
    /// UI state/log event key.
    pub event_profile_id: Option<&'a str>,
    pub version_json: &'a VersionJson,
    pub auth: &'a StoredAuth,
    pub paths: &'a LauncherPaths,
    pub memory_max_mb: u32,
    /// Minecraft ウィンドウ幅 (ピクセル)。None のときはゲームのデフォルトを使用
    pub window_width: Option<u32>,
    /// Minecraft ウィンドウ高さ (ピクセル)。None のときはゲームのデフォルトを使用
    pub window_height: Option<u32>,
    /// 追加JVMフラグ (上書き)。空文字/None は無効
    pub jvm_flags_override: Option<&'a str>,
    /// JVM tuning profile. "smooth" is the stable default; "performance_lab" is opt-in.
    pub jvm_tuning_mode: Option<&'a str>,
    /// Comma-separated Performance Lab modules enabled by the UI.
    pub jvm_tuning_modules: Option<&'a str>,
    /// Java 実行ファイルパスの手動指定。None のときは管理JDKを自動選択
    pub jdk_override: Option<&'a str>,
    /// Stages measured by the command layer before the launcher core runs.
    pub pre_launch_metrics: Vec<(&'static str, u128)>,
    /// Elapsed command-layer time before entering the launcher core.
    pub pre_launch_elapsed_ms: u128,
}

/// ゲームを起動する。
/// - ログは "game://log" イベントでフロントエンドに流す
/// - 終了時は "game://exit" イベントを送る
/// - この関数はJavaプロセス起動直後に返る（ゲームはバックグラウンドで動く）
pub async fn launch(req: &LaunchRequest<'_>, app: &AppHandle) -> Result<(), String> {
    let launch_timer = Instant::now();
    let mut stage_timer = Instant::now();
    let mut launch_metrics: Vec<(&str, u128)> = req.pre_launch_metrics.clone();
    // ── Java を探す ──────────────────────────────────────────────────────
    // javaVersion フィールドがある (1.17+) → そのまま使う
    // ない (〜1.16)     → mainClass でバージョンを推定する
    let required_java = match req
        .version_json
        .java_version
        .as_ref()
        .map(|j| j.major_version)
    {
        Some(v) => v,
        None => java::infer_java_from_main_class(&req.version_json.main_class),
    };
    let java_install = match req.jdk_override.filter(|s| !s.is_empty()) {
        Some(custom_path) => {
            log::info!("Using custom JDK: {}", custom_path);
            let mut p = PathBuf::from(custom_path);
            // ディレクトリが指定された場合は bin/java[.exe] を付加
            if p.is_dir() {
                p = p.join("bin").join(java::JAVA_BIN);
            } else if !p.exists() {
                // 拡張子なしのパス (Windows) → .exe を試す
                #[cfg(target_os = "windows")]
                {
                    let with_exe = p.with_extension("exe");
                    if with_exe.exists() {
                        p = with_exe;
                    }
                }
            }
            let java_install = java::JavaInstall {
                path: p,
                major_version: required_java,
            };
            log::info!(
                "JDK override: input={:?}, resolved={:?}, exists={}",
                custom_path,
                java_install.path,
                java_install.path.exists()
            );
            if !java_install.path.exists() {
                return Err(format!(
                    "Specified JDK was not found: {:?}\nIf a directory was selected, Hikyou automatically appends bin/java.exe.",
                    java_install.path
                ));
            }
            java_install
        }
        None => java::get_or_install_java(required_java, req.paths).await?,
    };
    log::info!(
        "Using Java: {:?} (v{})",
        java_install.path,
        java_install.major_version
    );
    log::info!(
        "[launch] Java runtime ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("java_runtime", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    // ── log4j2 設定をダウンロード (1.7〜1.18.0 のlog4shell対策) ─────────
    let log4j_arg = ensure_log4j_config(req).await;
    log::info!(
        "[launch] Log4j config ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("log4j_config", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    // ── Natives を展開 ────────────────────────────────────────────────────
    let natives_dir = extract_natives(req)?;
    log::info!(
        "[launch] Natives ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("natives", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    // ── Classpath を組み立てる ────────────────────────────────────────────
    let classpath = build_classpath(req)?;
    log::info!(
        "[launch] Classpath ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("classpath", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    // ── gameDir を確保 ────────────────────────────────────────────────────
    let game_dir = match req.profile_id {
        Some(pid) => req.paths.profile_game_dir_for_ref(pid)?,
        None => req.paths.profile_game_dir(req.version_id),
    };
    std::fs::create_dir_all(&game_dir)
        .map_err(|e| format!("Failed to create game directory: {}", e))?;
    log::info!(
        "[launch] Game directory ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("game_directory", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    if let Some(loader) = mod_loader_for_version(req.version_json) {
        crate::core::mods::sync_auto_mods_for_launch(
            &game_dir,
            req.paths.auto_mods_file(),
            req.version_id,
            loader,
        )
        .await?;
        let quarantined = crate::core::mods::quarantine_unloadable_mods(&game_dir, loader).await?;
        for filename in quarantined {
            log::warn!(
                "Disabled incompatible mod before launch: {} ({})",
                filename,
                loader
            );
        }
        crate::core::mods::mark_auto_mod_sync_current(
            &game_dir,
            req.paths.auto_mods_file(),
            req.version_id,
            loader,
        )
        .await;
    }
    log::info!(
        "[launch] Auto mods ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("auto_mods", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    // ── テンプレート変数マップを構築 ──────────────────────────────────────
    // Mojangのテンプレート変数をすべて定義する
    let vars: HashMap<&str, String> = [
        // ── 基本情報 ────────────────────────────────────────────────────────
        (
            "auth_player_name",
            req.auth
                .username
                .clone()
                .unwrap_or_else(|| "Player".to_string()),
        ),
        ("version_name", req.version_id.to_string()),
        ("game_directory", game_dir.to_string_lossy().to_string()),
        (
            "assets_root",
            req.paths.assets().to_string_lossy().to_string(),
        ),
        ("assets_index_name", req.version_json.assets.clone()),
        ("auth_uuid", req.auth.uuid.clone().unwrap_or_default()),
        ("user_type", "msa".to_string()),
        ("version_type", "release".to_string()),
        // ── JVM引数用 ────────────────────────────────────────────────────────
        (
            "natives_directory",
            natives_dir.to_string_lossy().to_string(),
        ),
        ("launcher_name", "hikyou-launcher".to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
        ("classpath", classpath.clone()),
        // ── 旧バージョン互換 (1.7.x 以前) ───────────────────────────────────
        // --userProperties: 空のJSONオブジェクトを渡す (1.7.x が GSON でパースする)
        ("user_properties", "{}".to_string()),
        // --profileName: ランチャーのプロファイル名
        ("profile_name", "hikyou-launcher".to_string()),
        // 旧形式ユーザーID (UUID と同じで OK)
        ("auth_player_id", req.auth.uuid.clone().unwrap_or_default()),
        // game_assets は 1.7.2 以前が使うローカルアセットパス
        // 仮想アセットディレクトリ (resources フォルダ) を使う古いバージョン向け
        (
            "game_assets",
            req.paths
                .assets()
                .join("virtual")
                .join("legacy")
                .to_string_lossy()
                .to_string(),
        ),
        // ── NeoForge / Forge 固有のテンプレート変数 ─────────────────────────
        // ${library_directory}: NeoForge が -Dlibraries.dir や -p の引数で使用する
        (
            "library_directory",
            req.paths.libraries().to_string_lossy().to_string(),
        ),
        // ${classpath_separator}: OS ごとのクラスパス区切り文字
        (
            "classpath_separator",
            if cfg!(target_os = "windows") {
                ";".to_string()
            } else {
                ":".to_string()
            },
        ),
        // ${version_type}: "release" / "snapshot" 等
        ("version_type", "release".to_string()),
    ]
    .into_iter()
    .collect();

    // ── 引数を組み立てる ──────────────────────────────────────────────────
    // JVM/classpath arguments never need authentication material. Keep game
    // arguments separate: they may contain the access token and are zeroized
    // immediately after Java has been spawned.
    let mut launcher_args: Vec<String> = Vec::new();

    // Liberica NIK (GraalVM ベース) かどうかを Java パスで判定
    let is_liberica_nik = java_install.path.to_string_lossy().contains("liberica-nik");

    // JVM 引数
    let system_total_mb = crate::core::java::get_max_memory_mb();
    let jvm_args = build_jvm_args(
        req,
        &vars,
        is_liberica_nik,
        java_install.major_version,
        system_total_mb,
    );
    launcher_args.extend(jvm_args);

    // log4j2 設定引数 (-Dlog4j.configurationFile=...)
    if let Some(arg) = log4j_arg {
        launcher_args.push(arg);
    }

    // classpath と mainClass
    launcher_args.push("-cp".to_string());
    launcher_args.push(classpath);
    launcher_args.push(req.version_json.main_class.clone());

    // ゲーム引数
    let game_args = build_game_args(req, &vars);
    log::info!(
        "[launch] Arguments ready in {} ms",
        stage_timer.elapsed().as_millis()
    );
    launch_metrics.push(("arguments", stage_timer.elapsed().as_millis()));
    stage_timer = Instant::now();

    log::info!(
        "Launch command: {:?} ({} args)",
        java_install.path,
        launcher_args.len() + game_args.len()
    );

    // ── デバッグ情報イベント ──────────────────────────────────────────────
    let java_dist = if is_liberica_nik {
        "Liberica NIK (GraalVM)"
    } else {
        "Azul Zulu"
    };
    let jvm_debug_args = launcher_args
        .iter()
        .filter(|a| {
            a.starts_with("-XX")
                || a.starts_with("-X")
                || a.starts_with("-Dgraal")
                || a.starts_with("-Djdk.graal")
        })
        .cloned()
        .collect::<Vec<_>>();
    let use_zgc_flag = jvm_debug_args.iter().any(|a| a == "-XX:+UseZGC");
    let _ = app.emit("debug://java-info", serde_json::json!({
        "profile_id":   req.profile_id.unwrap_or(req.version_id),
        "java_path":    java_install.path.to_string_lossy(),
        "java_version": java_install.major_version,
        "java_dist":    java_dist,
        "is_liberica_nik": is_liberica_nik,
        "use_zgc": use_zgc_flag,
        "memory_max_mb": req.memory_max_mb,
        "system_total_mb": system_total_mb,
        "jvm_flags_override": req.jvm_flags_override.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        "jvm_tuning_mode": req.jvm_tuning_mode.unwrap_or("smooth"),
        "jvm_tuning_modules": req.jvm_tuning_modules.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        "jdk_override": req.jdk_override.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        "jvm_args": jvm_debug_args,
    }));

    // ── プロセス起動 ──────────────────────────────────────────────────────
    let mut cmd = Command::new(&java_install.path);

    // Windows でコマンドライン長が上限に近い場合は @argfile を使用。
    // Java 9+ は argfile をサポートする。Forge は大量のライブラリで
    // クラスパスが長くなり CreateProcess の 32767 文字制限を超えることがある。
    #[cfg(target_os = "windows")]
    {
        let total_len: usize = launcher_args
            .iter()
            .map(|arg| arg.len() + 3)
            .chain(game_args.iter().map(|arg| arg.len() + 3))
            .sum();
        if java_install.major_version >= 9 && total_len > 6000 {
            // Never persist game arguments in the argfile: they carry the
            // Minecraft access token. Java accepts direct program arguments
            // after the main class supplied by the argfile.
            match write_java_argfile(&launcher_args, req.version_id, req.paths) {
                Ok(argfile_path) => {
                    let path_str = argfile_path.to_string_lossy().replace('\\', "/");
                    log::info!(
                        "Using @argfile (estimated command line length {} chars): {:?}",
                        total_len,
                        argfile_path
                    );
                    cmd.arg(format!("@{}", path_str));
                    cmd.args(game_args.iter().map(|arg| arg.as_str()));
                }
                Err(e) => {
                    log::warn!(
                        "argfile creation failed (launching with direct args): {}",
                        e
                    );
                    cmd.args(&launcher_args);
                    cmd.args(game_args.iter().map(|arg| arg.as_str()));
                }
            }
        } else {
            cmd.args(&launcher_args);
            cmd.args(game_args.iter().map(|arg| arg.as_str()));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        cmd.args(&launcher_args);
        cmd.args(game_args.iter().map(|arg| arg.as_str()));
    }

    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    // profile_id をキャプチャ（イベントに含める）
    let profile_id_for_events = req
        .event_profile_id
        .or(req.profile_id)
        .map(|s| s.to_string())
        .unwrap_or_else(|| req.version_id.to_string());
    if let Err(e) = app.emit(
        "game://launching",
        serde_json::json!({
            "version_id": req.version_id,
            "profile_id": &profile_id_for_events,
        }),
    ) {
        log::warn!("[launch] Failed to emit game://launching event: {}", e);
    }

    let mut child = cmd
        .current_dir(&game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start Java process: {}", e))?;
    // Command owns an OS-argument copy. It is no longer needed after spawn;
    // drop it and the zeroizing game-argument buffers at this boundary.
    drop(cmd);
    drop(game_args);
    log::info!(
        "[launch] Java spawn returned in {} ms",
        stage_timer.elapsed().as_millis()
    );
    let java_spawn_ms = stage_timer.elapsed().as_millis();
    launch_metrics.push(("java_spawn", java_spawn_ms));
    let total_pre_spawn_ms = req.pre_launch_elapsed_ms + launch_timer.elapsed().as_millis();
    log::info!(
        "[launch] Java process prepared in {} ms; total pre-spawn time {} ms",
        total_pre_spawn_ms.saturating_sub(java_spawn_ms),
        total_pre_spawn_ms
    );
    let _ = app.emit(
        "debug://launch-metrics",
        serde_json::json!({
            "profile_id": &profile_id_for_events,
            "version_id": req.version_id,
            "total_pre_spawn_ms": total_pre_spawn_ms,
            "java_spawn_ms": java_spawn_ms,
            "stages": launch_metrics
                .iter()
                .map(|(name, ms)| serde_json::json!({ "name": name, "ms": ms }))
                .collect::<Vec<_>>(),
        }),
    );

    if let Some(pid) = child.id() {
        crate::core::running_processes::register(&profile_id_for_events, pid);
    }

    // ── stdout/stderr をイベントで流す ────────────────────────────────────
    let app_stdout = app.clone();
    let pid_stdout = profile_id_for_events.clone();
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = app_stdout.emit(
                    "game://log",
                    serde_json::json!({"profile_id": pid_stdout, "line": line}),
                );
            }
        });
    }

    let app_stderr = app.clone();
    let pid_stderr = profile_id_for_events.clone();
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = app_stderr.emit(
                    "game://log",
                    serde_json::json!({"profile_id": pid_stderr, "line": line}),
                );
            }
        });
    }

    // ── 終了を待ちながら game://exit を送る ───────────────────────────────
    let app_exit = app.clone();
    let version_id = req.version_id.to_string();
    let pid_exit = profile_id_for_events.clone();
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => {
                crate::core::running_processes::unregister(&pid_exit);
                log::info!("Minecraft exited: {:?}", status.code());
                let _ = app_exit.emit(
                    "game://exit",
                    serde_json::json!({
                        "version_id": version_id,
                        "profile_id": pid_exit,
                        "exit_code": status.code()
                    }),
                );
            }
            Err(e) => {
                crate::core::running_processes::unregister(&pid_exit);
                log::error!("Process wait failed: {}", e);
                let _ = app_exit.emit(
                    "game://exit",
                    serde_json::json!({
                        "version_id": version_id,
                        "profile_id": pid_exit,
                        "exit_code": null
                    }),
                );
            }
        }
    });

    Ok(())
}

fn mod_loader_for_version(version_json: &VersionJson) -> Option<&'static str> {
    let id = version_json.id.to_ascii_lowercase();
    let main_class = version_json.main_class.to_ascii_lowercase();

    if id.contains("neoforge") || main_class.contains("neoforge") {
        Some("neoforge")
    } else if id.contains("forge") || main_class.contains("forge") {
        Some("forge")
    } else if id.contains("quilt") || main_class.contains("quilt") {
        Some("quilt")
    } else if id.contains("fabric")
        || main_class.contains("fabricmc")
        || main_class.contains("knot")
    {
        Some("fabric")
    } else {
        None
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 引数組み立て
// ────────────────────────────────────────────────────────────────────────────

use super::launcher_args::{build_game_args, build_jvm_args};
// ────────────────────────────────────────────────────────────────────────────
// Classpath / Natives / Launch Files
// ────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
use super::launcher_files::write_java_argfile;
use super::launcher_files::{build_classpath, ensure_log4j_config, extract_natives};
