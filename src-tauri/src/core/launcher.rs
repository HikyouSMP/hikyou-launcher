//! Minecraft ゲーム起動モジュール
//!
//! バージョン JSON の arguments テンプレートを正しく展開して起動する。
//! 1.13+ の arguments.game / arguments.jvm と、
//! 1.12以前の minecraftArguments 両方に対応。

use crate::auth::StoredAuth;
use crate::core::{java, manifest::VersionJson, paths::LauncherPaths};
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
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
    pub java_overrides: &'a super::java_selection::Overrides,
    /// Stages measured by the command layer before the launcher core runs.
    pub pre_launch_metrics: Vec<(&'static str, u128)>,
    /// Elapsed command-layer time before entering the launcher core.
    pub pre_launch_elapsed_ms: u128,
    pub cancellation: &'a crate::core::running_processes::LaunchCancellation,
    pub mod_revision: Option<&'a str>,
    pub prepared_mods_dir: Option<&'a std::path::Path>,
    pub mod_leases: Vec<std::sync::Arc<std::fs::File>>,
}

/// ゲームを起動する。
/// - ログは "game://log" イベントでフロントエンドに流す
/// - 終了時は "game://exit" イベントを送る
/// - この関数はJavaプロセス起動直後に返る（ゲームはバックグラウンドで動く）
pub async fn launch(req: &LaunchRequest<'_>, app: &AppHandle) -> Result<(), String> {
    let mut observer = match req.mod_revision {
        Some(revision) => match super::mod_health::begin(
            &req.paths.root(),
            req.profile_id
                .ok_or("Mod revision launch requires a profile ref")?,
            revision,
        )
        .await
        {
            Ok(observer) => Some(observer),
            Err(error) => {
                log::warn!("[launch] Outcome recording unavailable: {error}");
                None
            }
        },
        None => None,
    };
    finish_launch(req, app, &mut observer).await
}

/// Continue a revision attempt begun by the coordinator before Mod preparation.
pub async fn launch_with_observer(
    req: &LaunchRequest<'_>,
    app: &AppHandle,
    observer: super::mod_health::Observer,
) -> Result<(), String> {
    if !observer.matches(&req.paths.root(), req.profile_id, req.mod_revision) {
        return Err("Mod launch observer does not match the requested revision/profile".into());
    }
    finish_launch(req, app, &mut Some(observer)).await
}

async fn finish_launch(
    req: &LaunchRequest<'_>,
    app: &AppHandle,
    observer: &mut Option<super::mod_health::Observer>,
) -> Result<(), String> {
    let result = launch_observed(req, app, observer).await;
    if result.is_err()
        && let Some(observer) = observer
    {
        observer
            .preparation_failed(
                result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error == super::running_processes::LAUNCH_CANCELLED_ERROR),
            )
            .await;
    }
    result
}

async fn launch_observed(
    req: &LaunchRequest<'_>,
    app: &AppHandle,
    observer: &mut Option<super::mod_health::Observer>,
) -> Result<(), String> {
    req.cancellation.check()?;
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
    let custom_java =
        super::java_selection::override_for(required_java, req.java_overrides, req.jdk_override);
    let java_install = req
        .cancellation
        .run(super::java_selection::resolve(
            required_java,
            req.paths,
            req.java_overrides,
            req.jdk_override,
        ))
        .await?;
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
    let log4j_arg = req
        .cancellation
        .run(async { Ok(ensure_log4j_config(req).await) })
        .await?;
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

    if req.mod_revision.is_none()
        && let Some(loader) = mod_loader_for_version(req.version_json)
    {
        req.cancellation
            .run(crate::core::mods::sync_auto_mods_for_launch(
                &game_dir,
                req.paths.auto_mods_file(),
                req.version_id,
                loader,
            ))
            .await?;
        let quarantined = req
            .cancellation
            .run(crate::core::mods::quarantine_unloadable_mods(
                &game_dir,
                req.version_id,
                loader,
            ))
            .await?;
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

    // JVM 引数
    let system_total_mb = crate::core::java::get_max_memory_mb();
    let jvm_args = build_jvm_args(req, &vars, java_install.major_version, system_total_mb);
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
    let java_dist = if custom_java.is_some() {
        "Custom Java"
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
        "use_zgc": use_zgc_flag,
        "memory_max_mb": req.memory_max_mb,
        "system_total_mb": system_total_mb,
        "jvm_flags_override": req.jvm_flags_override.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        "jvm_tuning_mode": req.jvm_tuning_mode.unwrap_or("smooth"),
        "jvm_tuning_modules": req.jvm_tuning_modules.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        "jdk_override": custom_java,
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

    req.cancellation.check()?;
    let child = cmd
        .current_dir(&game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start Java process: {}", e))?;
    if let Some(observer) = observer {
        observer.mark_spawned();
    }
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

    let mut leases = req.mod_leases.clone();
    leases.extend(java_install.lease);
    super::game_process::supervise(
        child,
        app,
        req.version_id,
        &profile_id_for_events,
        req.cancellation.clone(),
        leases,
        observer.take(),
    )
    .await?;
    req.cancellation.check()?;

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
