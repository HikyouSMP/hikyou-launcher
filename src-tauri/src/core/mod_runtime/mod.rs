//! A launch reads one completed revision; maintenance builds its successor elsewhere.

mod preparation;
mod record;
mod requests;
#[cfg(test)]
mod tests;
mod workspace;

use super::{
    fabric, managed_fs, manifest,
    mod_intent::{self, Intent},
    mod_repository::{ModRepository, ModRevision, ModTarget},
    mods,
    paths::LauncherPaths,
    profile, running_processes,
};
use cap_std::fs::OpenOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, Weak},
    time::Duration,
};
use tauri::Emitter;
use tokio::sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard};

const CHECK_SECONDS: i64 = 30 * 60;
type LockKey = (PathBuf, String);
type ProfileLocks = Mutex<HashMap<LockKey, Weak<AsyncMutex<()>>>>;
static LOCKS: LazyLock<ProfileLocks> = LazyLock::new(|| Mutex::new(HashMap::new()));
static PREPARATION_REQUESTED: Notify = Notify::const_new();

pub fn wake_preparation() {
    PREPARATION_REQUESTED.notify_one();
}

pub use record::record;
#[cfg(test)]
use record::record_edited;
pub use requests::{
    RequestedState, change_installed_request, recommendation_changes, remove_request,
    request_project, requested_state,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedProfile {
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub revision_id: String,
    #[serde(skip)]
    pub mods_dir: PathBuf,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct MaintenanceState {
    revision_id: String,
    workspace_signature: String,
    recipe: String,
    checked_at: i64,
    #[serde(default)]
    pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_restore: Option<String>,
    #[serde(default)]
    pending_edit: bool,
}

fn pinned(state: &MaintenanceState, head: &ModRevision) -> bool {
    (!state.pending_edit && state.pending_restore.as_deref() == Some(&head.id))
        || (state.pinned && state.revision_id == head.id)
}

pub struct EditGuard {
    _local: OwnedMutexGuard<()>,
    file: Arc<File>,
}

pub fn play_lease(paths: &LauncherPaths, id: &str) -> Result<Arc<File>, String> {
    profile::validate_profile_ref(id)?;
    let dir = state_dir(paths)?;
    let filename = format!("{}.play.lock", key(id));
    managed_fs::inspect(&dir, Path::new(&filename))?;
    let file = dir
        .open_with(
            filename,
            OpenOptions::new().read(true).write(true).create(true),
        )
        .map_err(|e| e.to_string())?
        .into_std();
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| {
        "This profile is already launching or running in another process".to_string()
    })?;
    Ok(Arc::new(file))
}

impl EditGuard {
    pub fn lease(&self) -> Arc<File> {
        self.file.clone()
    }
}

#[cfg(test)]
fn backup_working_tree(paths: &LauncherPaths, id: &str) -> Result<tempfile::TempDir, String> {
    require_existing_source(paths, id)?;
    workspace::stage(paths, &paths.checked_profile_game_dir(id)?)
}

fn require_existing_source(paths: &LauncherPaths, id: &str) -> Result<(), String> {
    if ModRepository::new(&paths.root()).head(id)?.is_some()
        && !workspace::source_present(&paths.checked_profile_game_dir(id)?)?
    {
        return Err("The Mod working folder is missing. Restore a revision from Mod history instead of treating it as an empty configuration.".to_string());
    }
    Ok(())
}

/// File leases vanish when a launcher exits, but its Java process may survive.
pub async fn surviving_game(
    paths: &LauncherPaths,
    id: &str,
    working_tree_only: bool,
) -> Result<bool, String> {
    let game = paths.checked_profile_game_dir(id)?;
    let generations = paths.state_dir().join("mod-revisions/generations");
    tokio::task::spawn_blocking(move || {
        use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};
        let Ok(game) = game.canonicalize() else { return false; };
        let system = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cmd(UpdateKind::Always).with_user(UpdateKind::Always)));
        let current_user = sysinfo::get_current_pid().ok().and_then(|pid| system.process(pid)).and_then(|process| process.user_id());
        system.processes().values().any(|process| {
            if matches!((current_user, process.user_id()), (Some(current), Some(owner)) if current != owner) { return false; }
            let args = process.cmd();
            if args.is_empty() {
                return ["java", "java.exe", "javaw", "javaw.exe"].iter().any(|name| process.name().to_string_lossy().eq_ignore_ascii_case(name));
            }
            let same_game = args.windows(2).any(|pair| pair[0] == "--gameDir" && Path::new(&pair[1]).canonicalize().ok().as_ref() == Some(&game));
            if !same_game { return false; }
            let pinned = args.iter().rev().find_map(|argument| argument.to_str()?.strip_prefix("-Dfabric.modsFolder="));
            let pinned = pinned.and_then(|directory| Path::new(directory).canonicalize().ok());
            let immutable_root = generations.canonicalize().ok();
            !(working_tree_only && matches!((pinned, immutable_root), (Some(path), Some(root)) if path.starts_with(&root)))
        })
    }).await.map_err(|error| error.to_string())
}

fn state_dir(paths: &LauncherPaths) -> Result<cap_std::fs::Dir, String> {
    managed_fs::directory(
        &managed_fs::root(&paths.root())?,
        Path::new("state/mod-maintenance"),
        true,
    )
}

fn key(profile_id: &str) -> String {
    format!("{:x}", Sha256::digest(profile_id.as_bytes()))
}

fn read_state(paths: &LauncherPaths, id: &str) -> Result<MaintenanceState, String> {
    use std::io::Read;
    let dir = state_dir(paths)?;
    let name = format!("{}.json", key(id));
    let path = Path::new(&name);
    if managed_fs::inspect(&dir, path)?.is_none() {
        return Ok(MaintenanceState::default());
    }
    let mut bytes = Vec::new();
    managed_fs::read_file(&dir, path)?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read Mod maintenance protection state: {e}"))?;
    if bytes.len() > 64 * 1024 {
        return Err("Mod maintenance protection state exceeds its size limit; automatic changes are blocked".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "Cannot decode Mod maintenance protection state; automatic changes are blocked: {e}"
        )
    })
}

fn write_state(paths: &LauncherPaths, id: &str, state: &MaintenanceState) -> Result<(), String> {
    managed_fs::write(
        &state_dir(paths)?,
        Path::new(&format!("{}.json", key(id))),
        &serde_json::to_vec(state).map_err(|e| e.to_string())?,
        true,
    )
}

pub fn status(paths: &LauncherPaths, id: &str) -> Result<Option<serde_json::Value>, String> {
    let Some(head) = ModRepository::new(&paths.root()).head(id)? else {
        return Ok(None);
    };
    let state = read_state(paths, id)?;
    let changed = workspace::fingerprint(&paths.checked_profile_game_dir(id)?)?
        != state.workspace_signature
        || recipe(paths)? != state.recipe;
    let age = (chrono::Utc::now().timestamp() - state.checked_at).max(0);
    let requested = requested_state(paths, id)?;
    Ok(Some(serde_json::json!({
        "mc_version": head.target.mc_version, "loader": head.target.loader,
        "signature": head.id, "revision_id": head.id, "synced_at": head.created_at,
        "age_seconds": age, "folder_changed": changed,
        "fresh": !changed && ready(paths, id)?.is_some(),
        "pinned": pinned(&state, &head), "update_due": !pinned(&state, &head) && age >= CHECK_SECONDS,
        "requested": requested, "launch_outcome": super::mod_health::latest(&paths.root(), id)?,
    })))
}

/// One mutation per profile, including across two launcher processes.
pub async fn edit_guard(paths: &LauncherPaths, id: &str) -> Result<EditGuard, String> {
    profile::validate_profile_ref(id)?;
    let mutex = {
        let mut locks = LOCKS
            .lock()
            .map_err(|_| "mod lock registry is unavailable")?;
        locks.retain(|_, value| value.strong_count() > 0);
        let slot = locks.entry((paths.root(), id.to_string())).or_default();
        match slot.upgrade() {
            Some(lock) => lock,
            None => {
                let lock = Arc::new(AsyncMutex::new(()));
                *slot = Arc::downgrade(&lock);
                lock
            }
        }
    };
    let local = mutex.lock_owned().await;
    let dir = state_dir(paths)?;
    let filename = format!("{}.lock", key(id));
    managed_fs::inspect(&dir, Path::new(&filename))?;
    let file = dir
        .open_with(
            filename,
            OpenOptions::new().read(true).write(true).create(true),
        )
        .map_err(|e| format!("failed to open mod lease: {e}"))?
        .into_std();
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|e| format!("This profile is in use by another game or launcher process: {e}"))?;
    Ok(EditGuard {
        _local: local,
        file: Arc::new(file),
    })
}

fn prepared(repo: &ModRepository, revision: &ModRevision) -> Result<PreparedProfile, String> {
    let mods_dir = repo.quick_check(revision)?;
    Ok(PreparedProfile {
        mc_version: revision.target.mc_version.clone(),
        loader: revision.target.loader.clone(),
        loader_version: revision.target.loader_version.clone(),
        revision_id: revision.id.clone(),
        mods_dir,
    })
}

/// Expiry concerns update discovery, never the usability of a completed revision.
pub fn ready(paths: &LauncherPaths, id: &str) -> Result<Option<PreparedProfile>, String> {
    profile::validate_profile_ref(id)?;
    let repo = ModRepository::new(&paths.root());
    if let Some(head) = repo.head(id)? {
        if let Ok(value) = prepared(&repo, &head) {
            return Ok(Some(value));
        }
        return Err("The prepared mod configuration changed or is damaged. Open Mod history to restore a complete revision, or wait for background preparation.".to_string());
    }
    Ok(None)
}

pub async fn for_launch(paths: &LauncherPaths, id: &str) -> Result<PreparedProfile, String> {
    if let Some(value) = cached_for_launch(paths, id).await? {
        return Ok(value);
    }
    if !profile::is_smart_profile_ref(id) {
        // The launch command holds the edit/use lease for ordinary profiles.
        workspace::recover(
            &ModRepository::new(&paths.root()),
            id,
            &paths.checked_profile_game_dir(id)?,
        )?;
        record(paths, id).await?;
        return ready(paths, id)?
            .ok_or_else(|| "No complete mod configuration is available yet".to_string());
    }
    if let Some(value) = ready(paths, id)? {
        return Ok(value);
    }
    if let Ok(Some(value)) = migrate_smart(paths, id).await {
        return Ok(value);
    }
    // Only first use can need preparation. Future launches never await update checks.
    refresh_smart(paths, id, false).await?;
    ready(paths, id)?.ok_or_else(|| "No complete mod configuration is available yet".to_string())
}

pub async fn cached_for_launch(
    paths: &LauncherPaths,
    id: &str,
) -> Result<Option<PreparedProfile>, String> {
    if profile::is_smart_profile_ref(id) {
        return ready(paths, id);
    }
    let repo = ModRepository::new(&paths.root());
    if let Some(head) = repo.head(id)?
        && head.target.loader == "fabric"
        && head.target == normal_target(paths, id).await?
    {
        return prepared(&repo, &head).map(Some);
    }
    Ok(None)
}

async fn target(paths: &LauncherPaths, id: &str) -> Result<ModTarget, String> {
    let manifest = manifest::refresh_manifest(paths).await?;
    let mc_version = match profile::smart_key_from_ref(id) {
        Some("latest-plus") => manifest.latest.release,
        Some("snapshot-plus") => manifest.latest.snapshot,
        _ => return Err("not a smart profile".to_string()),
    };
    Ok(ModTarget {
        mc_version,
        loader: "fabric".into(),
        loader_version: None,
    })
}

fn recipe(paths: &LauncherPaths) -> Result<String, String> {
    let mods = mods::load_auto_mods(&paths.auto_mods_file())?;
    let bytes =
        serde_json::to_vec(&(1u32, env!("CARGO_PKG_VERSION"), mods)).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

async fn seal(
    paths: &LauncherPaths,
    id: &str,
    game_dir: PathBuf,
    target: ModTarget,
    parent: Option<String>,
) -> Result<ModRevision, String> {
    let root = paths.root();
    let id = id.to_string();
    tokio::task::spawn_blocking(move || {
        let repo = ModRepository::new(&root);
        let revision = repo.capture(&id, &game_dir, target, parent.as_deref())?;
        repo.materialize(&revision)?;
        Ok(revision)
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn seal_requested(
    paths: &LauncherPaths,
    id: &str,
    game_dir: PathBuf,
    target: ModTarget,
    parent: Option<String>,
    intent: Intent,
    edges: Vec<mod_intent::DependencyEdge>,
) -> Result<ModRevision, String> {
    let root = paths.root();
    let id = id.to_string();
    let intent_id = intent.fingerprint()?;
    tokio::task::spawn_blocking(move || {
        let repo = ModRepository::new(&root);
        let revision =
            repo.capture_with_intent(&id, &game_dir, target, parent.as_deref(), Some(&intent_id))?;
        mod_intent::verify_selected(&intent, &revision)?;
        let paths = LauncherPaths::new(root);
        requests::record_resolution(&paths, &revision, intent, edges)?;
        repo.materialize(&revision)?;
        Ok(revision)
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn validate_inputs(
    paths: &LauncherPaths,
    game_dir: &Path,
    target: &ModTarget,
    online: bool,
) -> Result<(), String> {
    if !super::paths::is_safe_component(&target.mc_version)
        || target
            .loader_version
            .as_deref()
            .is_some_and(|version| !super::paths::is_safe_component(version))
    {
        return Err("unsafe Minecraft or loader version".to_string());
    }
    if !mods::supports_dependency_validation(&target.loader) {
        return mods::validate_mod_set(game_dir, &target.mc_version, &target.loader).await;
    }
    let java_major = if online {
        let manifest = manifest::fetch_manifest(paths).await?;
        let entry = manifest
            .versions
            .iter()
            .find(|entry| entry.id == target.mc_version)
            .ok_or("Minecraft version is absent from the manifest")?;
        let json = manifest::fetch_version_json(&entry.id, &entry.url, paths).await?;
        if let Some(loader) = target.loader_version.as_deref() {
            fabric::build_fabric_version_json(&json, loader, paths).await?;
        }
        Some(
            json.java_version
                .as_ref()
                .map(|java| java.major_version)
                .unwrap_or_else(|| super::java::infer_java_from_main_class(&json.main_class)),
        )
    } else {
        use std::io::Read;
        managed_fs::root(&paths.versions())
            .ok()
            .and_then(|dir| {
                managed_fs::read_file(&dir, Path::new(&format!("{}.json", target.mc_version))).ok()
            })
            .and_then(|file| {
                serde_json::from_reader::<_, serde_json::Value>(file.take(16 * 1024 * 1024)).ok()
            })
            .and_then(|json| json.get("javaVersion")?.get("majorVersion")?.as_u64())
            .and_then(|major| u32::try_from(major).ok())
    };
    let result = mods::validate_mod_set_with_runtime(
        game_dir,
        &target.mc_version,
        &target.loader,
        target.loader_version.as_deref(),
        java_major.map(super::java::runtime_major),
    )
    .await;
    if !online
        && java_major.is_none()
        && result
            .as_ref()
            .is_err_and(|error| error == mods::MISSING_JAVA_RUNTIME_ERROR)
    {
        // First import can precede the first Minecraft download. Fetch runtime
        // metadata once, never downgrade a missing Java version to "compatible".
        let manifest = manifest::fetch_manifest(paths).await?;
        let entry = manifest
            .versions
            .iter()
            .find(|entry| entry.id == target.mc_version)
            .ok_or("Minecraft version is absent from the manifest")?;
        let json = manifest::fetch_version_json(&entry.id, &entry.url, paths).await?;
        let major = json
            .java_version
            .as_ref()
            .map(|java| java.major_version)
            .unwrap_or_else(|| super::java::infer_java_from_main_class(&json.main_class));
        return mods::validate_mod_set_with_runtime(
            game_dir,
            &target.mc_version,
            &target.loader,
            target.loader_version.as_deref(),
            Some(super::java::runtime_major(major)),
        )
        .await;
    }
    result
}

async fn preparation_java(paths: &LauncherPaths, target: &ModTarget) -> Result<u32, String> {
    let manifest = manifest::fetch_manifest(paths).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|entry| entry.id == target.mc_version)
        .ok_or("Minecraft version is absent from the manifest")?;
    let json = manifest::fetch_version_json(&entry.id, &entry.url, paths).await?;
    Ok(super::java::runtime_major(
        json.java_version
            .as_ref()
            .map(|java| java.major_version)
            .unwrap_or_else(|| super::java::infer_java_from_main_class(&json.main_class)),
    ))
}

async fn migrate_smart(paths: &LauncherPaths, id: &str) -> Result<Option<PreparedProfile>, String> {
    let _guard = edit_guard(paths, id).await?;
    let repo = ModRepository::new(&paths.root());
    if let Some(head) = repo.head(id)? {
        return prepared(&repo, &head).map(Some);
    }
    let Some(resolved) = profile::list_profiles(&paths.root())
        .await
        .into_iter()
        .find(|profile| profile.id == id)
        .and_then(|profile| profile.resolved)
    else {
        return Ok(None);
    };
    let target = ModTarget {
        mc_version: resolved.mc_version,
        loader: resolved.loader,
        loader_version: resolved.loader_version,
    };
    if !matches!(target.loader.as_str(), "fabric" | "vanilla") {
        return Ok(None);
    }
    let game = paths.checked_profile_game_dir(id)?;
    let working = workspace::stage(paths, &game)?;
    validate_inputs(paths, working.path(), &target, false).await?;
    let revision = seal(paths, id, working.path().to_path_buf(), target, None).await?;
    let intent = Intent::from_revision(&revision);
    if mod_intent::load(&paths.root(), id)?.is_none() {
        mod_intent::save(&paths.root(), &intent)?;
    }
    requests::record_resolution(paths, &revision, intent, Vec::new())?;
    repo.publish(id, &revision.id)?;
    write_state(
        paths,
        id,
        &MaintenanceState {
            revision_id: revision.id.clone(),
            workspace_signature: workspace::fingerprint(&game)?,
            recipe: recipe(paths)?,
            checked_at: 0,
            pinned: false,
            pending_restore: None,
            pending_edit: false,
        },
    )?;
    prepared(&repo, &revision).map(Some)
}

pub async fn refresh_smart(
    paths: &LauncherPaths,
    id: &str,
    force: bool,
) -> Result<ModRevision, String> {
    let guard = Arc::new(edit_guard(paths, id).await?);
    let result = refresh_locked(paths, id, force, guard.clone()).await;
    if let Err(error) = &result
        && let Some(intent) = mod_intent::load(&paths.root(), id)?
    {
        let busy = !profile::is_smart_profile_ref(id) && running_processes::is_active(id);
        mod_intent::set_preparation(
            &paths.root(),
            id,
            &intent.fingerprint()?,
            if busy { "queued" } else { "blocked" },
            if busy {
                None
            } else {
                Some(requests::failure_reason(error))
            },
        )?;
    }
    result
}

async fn refresh_locked(
    paths: &LauncherPaths,
    id: &str,
    force: bool,
    guard: Arc<EditGuard>,
) -> Result<ModRevision, String> {
    if !profile::is_smart_profile_ref(id) && running_processes::is_active(id) {
        return Err("Profile is in use".to_string());
    }
    let repo = ModRepository::new(&paths.root());
    let game_dir = paths.checked_profile_game_dir(id)?;
    let mut state = read_state(paths, id)?;
    workspace::recover(&repo, id, &game_dir)?;
    let head = repo.head(id)?;
    record::recover(paths, id, head.as_ref(), &game_dir, &mut state)?;
    if let Some(ref head) = head
        && pinned(&state, head)
        && repo.quick_check(head).is_ok()
    {
        let restored = mod_intent::for_revision(&paths.root(), head)?
            .map(|resolved| resolved.intent)
            .unwrap_or_else(|| Intent::from_revision(head));
        mod_intent::save(&paths.root(), &restored)?;
        mod_intent::set_preparation(&paths.root(), id, &restored.fingerprint()?, "ready", None)?;
        return Ok(head.clone());
    }
    require_existing_source(paths, id)?;
    let signature = workspace::fingerprint(&game_dir)?;
    let recipe = recipe(paths)?;
    let baseline_target = if let Some(head) = &head {
        head.target.clone()
    } else if profile::is_smart_profile_ref(id) {
        target(paths, id).await?
    } else {
        normal_target(paths, id).await?
    };
    let mut intent = requests::current(paths, id, head.as_ref(), &baseline_target)?;
    let intent_id = intent.fingerprint()?;
    if let Some(ref head) = head
        && state.revision_id == head.id
        && state.workspace_signature == signature
        && state.recipe == recipe
        && mod_intent::for_revision(&paths.root(), head)?
            .is_some_and(|resolved| resolved.intent.fingerprint().ok().as_ref() == Some(&intent_id))
        && !force
        && (0..CHECK_SECONDS).contains(&(chrono::Utc::now().timestamp() - state.checked_at))
        && repo.quick_check(head).is_ok()
    {
        return Ok(head.clone());
    }
    mod_intent::set_preparation(&paths.root(), id, &intent_id, "preparing", None)?;
    let root = paths.root();
    let source = game_dir.clone();
    let profile_id = id.to_string();
    let parent = head.as_ref().map(|head| head.id.clone());
    let (working, baseline) = tokio::task::spawn_blocking(move || {
        let paths = LauncherPaths::new(root.clone());
        let working = workspace::stage(&paths, &source)?;
        let baseline = ModRepository::new(&root).capture(
            &profile_id,
            working.path(),
            baseline_target,
            parent.as_deref(),
        )?;
        Ok::<_, String>((working, baseline))
    })
    .await
    .map_err(|error| error.to_string())??;
    requests::import_external(paths, &mut intent, head.as_ref(), &baseline, working.path())?;
    let aliases = tokio::time::timeout(
        Duration::from_secs(60),
        requests::normalize_projects(paths, &mut intent, head.as_ref()),
    )
    .await
    .map_err(|_| "mod_resolution_limit: project identity lookup wall time".to_string())??;
    let intent_id = intent.fingerprint()?;
    mod_intent::set_preparation(&paths.root(), id, &intent_id, "preparing", None)?;
    drop(working);
    let revision =
        preparation::resolve(paths, id, &game_dir, head.as_ref(), &intent, &aliases).await?;
    if workspace::fingerprint(&game_dir)? != signature
        || self::recipe(paths)? != recipe
        || mod_intent::load(&paths.root(), id)?
            .ok_or("Mod request disappeared")?
            .fingerprint()?
            != intent_id
    {
        return Err("Mod inputs changed while preparing the configuration; the previous revision is retained".to_string());
    }
    let root = paths.root();
    let committed = revision.clone();
    let destination = game_dir.clone();
    if surviving_game(paths, id, true).await? {
        return Err("A running game still uses the editable Mod folder; the prepared successor is retained without checkout".to_string());
    }
    let checkout_guard = guard.clone();
    tokio::task::spawn_blocking(move || {
        let _guard = checkout_guard;
        let repo = ModRepository::new(&root);
        let generation = repo.quick_check(&committed)?;
        workspace::checkout(&repo, &committed, &generation, &destination)
    })
    .await
    .map_err(|e| e.to_string())??;
    write_state(
        paths,
        id,
        &MaintenanceState {
            revision_id: revision.id.clone(),
            workspace_signature: workspace::fingerprint(&game_dir)?,
            recipe,
            checked_at: chrono::Utc::now().timestamp(),
            pinned: false,
            pending_restore: None,
            pending_edit: false,
        },
    )?;
    log::info!(
        "[mods] Published complete revision {} for {id}",
        revision.id
    );
    mod_intent::set_preparation(&paths.root(), id, &intent_id, "ready", None)?;
    Ok(revision)
}

async fn normal_target(paths: &LauncherPaths, id: &str) -> Result<ModTarget, String> {
    let profile = profile::list_profiles(&paths.root())
        .await
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("profile not found")?;
    Ok(ModTarget {
        mc_version: profile.mc_version,
        loader: profile.loader,
        loader_version: profile.loader_version,
    })
}

pub async fn restore(
    paths: &LauncherPaths,
    id: &str,
    revision_id: &str,
) -> Result<ModRevision, String> {
    profile::validate_profile_ref(id)?;
    let _play = play_lease(paths, id)?;
    let guard = Arc::new(edit_guard(paths, id).await?);
    if running_processes::is_active(id) || surviving_game(paths, id, false).await? {
        return Err("Stop the game before restoring mods".to_string());
    }
    let repo = ModRepository::new(&paths.root());
    let revision = repo.load(revision_id)?;
    if revision.profile_id != id {
        return Err("revision belongs to another profile".to_string());
    }
    let restored_intent = mod_intent::for_revision(&paths.root(), &revision)?
        .map(|resolved| resolved.intent)
        .unwrap_or_else(|| Intent::from_revision(&revision));
    mod_intent::verify_selected(&restored_intent, &revision)?;
    if mod_intent::for_revision(&paths.root(), &revision)?.is_none() {
        requests::record_resolution(paths, &revision, restored_intent.clone(), Vec::new())?;
    }
    if !profile::is_smart_profile_ref(id) {
        let current = profile::list_profiles(&paths.root())
            .await
            .into_iter()
            .find(|p| p.id == id)
            .ok_or("profile not found")?;
        if current.mc_version != revision.target.mc_version
            || current.loader != revision.target.loader
            || current.loader_version != revision.target.loader_version
        {
            return Err("This revision targets another Minecraft or loader version; restore it in a matching profile".to_string());
        }
    }
    let root = paths.root();
    let game_dir = paths.checked_profile_game_dir(id)?;
    let restore_revision = revision.clone();
    let generation = tokio::task::spawn_blocking(move || {
        let repo = ModRepository::new(&root);
        let mods_dir = repo.materialize(&restore_revision)?;
        Ok::<_, String>(mods_dir)
    })
    .await
    .map_err(|e| e.to_string())??;
    validate_inputs(
        paths,
        generation.parent().ok_or("invalid Mod generation")?,
        &revision.target,
        false,
    )
    .await?;
    let root = paths.root();
    let restored = revision.clone();
    let previous_state = read_state(paths, id)?;
    let mut intent = previous_state.clone();
    intent.pending_restore = Some(revision.id.clone());
    intent.pending_edit = false;
    write_state(paths, id, &intent)?;
    let checkout_guard = guard.clone();
    let checkout_result = tokio::task::spawn_blocking(move || {
        let _guard = checkout_guard;
        workspace::checkout(
            &ModRepository::new(&root),
            &restored,
            &generation,
            &game_dir,
        )
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Err(error) = checkout_result {
        if repo.head(id)?.is_none_or(|head| head.id != revision.id) {
            write_state(paths, id, &previous_state)?;
        }
        return Err(error);
    }
    mod_intent::save(&paths.root(), &restored_intent)?;
    mod_intent::set_preparation(
        &paths.root(),
        id,
        &restored_intent.fingerprint()?,
        "ready",
        None,
    )?;
    write_state(
        paths,
        id,
        &MaintenanceState {
            revision_id: revision.id.clone(),
            workspace_signature: workspace::fingerprint(&paths.checked_profile_game_dir(id)?)?,
            recipe: recipe(paths)?,
            checked_at: chrono::Utc::now().timestamp(),
            pinned: profile::is_smart_profile_ref(id),
            pending_restore: None,
            pending_edit: false,
        },
    )?;
    Ok(revision)
}

pub async fn resume(paths: &LauncherPaths, id: &str) -> Result<ModRevision, String> {
    if !profile::is_smart_profile_ref(id) {
        return Err("not a smart profile".to_string());
    }
    {
        let _guard = edit_guard(paths, id).await?;
        let mut state = read_state(paths, id)?;
        state.pinned = false;
        state.pending_restore = None;
        state.pending_edit = false;
        write_state(paths, id, &state)?;
    }
    refresh_smart(paths, id, true).await
}

pub async fn maintain(paths: Arc<LauncherPaths>, app: tauri::AppHandle) {
    let mut last_attempt: HashMap<String, (i64, String)> = HashMap::new();
    let mut startup = true;
    loop {
        if let Err(error) = recommendation_changes(&paths, None).await {
            log::warn!("[mods] Recommendation propagation is blocked: {error}");
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        }
        let settings = match super::settings_store::read(&paths.root()) {
            Ok(settings) => settings,
            Err(error) => {
                log::warn!("[mods] Maintenance blocked by unreadable settings: {error}");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        };
        for profile in profile::list_profiles(&paths.root()).await {
            let enabled = match profile.smart_key.as_deref() {
                Some("latest-plus") => settings
                    .pointer("/game/latestReleasePlus")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
                Some("snapshot-plus") => settings
                    .pointer("/game/snapshotPlus")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                _ => true,
            };
            if !enabled {
                continue;
            }
            let result = async {
                let now = chrono::Utc::now().timestamp();
                let desired = mod_intent::load(&paths.root(), &profile.id)?.map(|intent| intent.fingerprint()).transpose()?.unwrap_or_default();
                let workspace = workspace::fingerprint(&paths.checked_profile_game_dir(&profile.id)?)?;
                let signature = format!("{}:{desired}:{workspace}", recipe(&paths)?);
                if last_attempt.get(&profile.id).is_some_and(|(then, previous)| previous == &signature && now - then < 5 * 60) { return Ok(()); }
                last_attempt.insert(profile.id.clone(), (now, signature));
                if !profile::is_smart_profile_ref(&profile.id) {
                    if running_processes::is_active(&profile.id) { return Ok(()); }
                } else if ModRepository::new(&paths.root()).head(&profile.id)?.is_none() {
                    let _ = migrate_smart(&paths, &profile.id).await;
                }
                if startup && let Some(head) = ModRepository::new(&paths.root()).head(&profile.id)? {
                    let repo = ModRepository::new(&paths.root());
                    if let Err(error) = tokio::task::spawn_blocking(move || repo.verify(&head)).await.map_err(|e| e.to_string())? {
                        log::warn!("[mods] Stored revision needs repair: {error}");
                    }
                }
                let revision = refresh_smart(&paths, &profile.id, startup).await?;
                let _ = app.emit("mods://prepared", serde_json::json!({ "profileId": profile.id, "resolved": { "mcVersion": revision.target.mc_version, "loader": revision.target.loader, "loaderVersion": revision.target.loader_version, "resolvedAt": revision.created_at } }));
                Ok::<(), String>(())
            }.await;
            if let Err(error) = result {
                log::warn!(
                    "[mods] Background preparation for {} failed; any completed revision remains unchanged: {error}",
                    profile.id
                );
                let _ = app.emit(
                    "mods://requests-changed",
                    serde_json::json!({ "profileId": profile.id }),
                );
            }
        }
        startup = false;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(60)) => {},
            _ = PREPARATION_REQUESTED.notified() => {},
        }
    }
}
