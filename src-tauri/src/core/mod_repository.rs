//! Immutable mod objects, revision snapshots, and launch-specific generations.
//!
//! Capture never changes the source or the published head. Materialize and verify
//! before publishing. Receipts detect ordinary changes cheaply; they are not an
//! integrity boundary against an attacker with the launcher's own permissions.

use super::{managed_fs, mod_files::MAX_MOD_BYTES};
use cap_std::fs::{Dir, OpenOptions};
use fs2::FileExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_MODS: usize = 4096;
const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FABRIC_JSON: u64 = 1024 * 1024;
const STATE: &str = "state/mod-revisions";
const OBJECTS: &str = "meta/mod-objects";
const MISSING_DIRECTORY: &str = "required mod repository/source directory is missing";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModTarget {
    pub mc_version: String,
    pub loader: String,
    #[serde(default)]
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModEntry {
    pub filename: String,
    pub object_id: String,
    pub size_bytes: u64,
    pub enabled: bool,
    #[serde(default)]
    pub mod_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub provider_version: Option<String>,
    #[serde(default)]
    pub provider_version_number: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRevision {
    pub id: String,
    pub profile_id: String,
    pub parent: Option<String>,
    pub target: ModTarget,
    pub mods: Vec<ModEntry>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModRepository {
    root: PathBuf,
}

struct RepositoryGuard {
    state: Dir,
    objects: Dir,
    // Closing this handle releases the process-wide repository lock.
    _lock: Option<File>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    format: u32,
    revision_id: String,
    files: Vec<FileReceipt>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileReceipt {
    filename: String,
    object_id: String,
    generation: Stamp,
    object: Stamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Stamp {
    size: u64,
    modified: String,
    created: Option<String>,
    identity: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Head {
    profile_id: String,
    revision_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerationHead {
    revision_id: String,
    generation: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SourceMeta {
    #[serde(alias = "displayName")]
    display_name: Option<String>,
    #[serde(alias = "iconUrl")]
    icon_url: Option<String>,
    #[serde(alias = "projectId")]
    project_id: Option<String>,
    #[serde(alias = "versionId")]
    version_id: Option<String>,
    #[serde(alias = "versionNumber")]
    version_number: Option<String>,
    #[serde(alias = "modId")]
    mod_id: Option<String>,
    hashes: Option<BTreeMap<String, String>>,
    // Provider hashes are retained as provenance, never used as object authority.
}

#[derive(Default, Deserialize)]
struct FabricMeta {
    id: Option<String>,
    version: Option<String>,
    name: Option<String>,
}

impl ModRepository {
    pub fn new(root: &Path) -> Self {
        Self { root: root.into() }
    }

    pub fn capture(
        &self,
        profile_id: &str,
        game_dir: &Path,
        target: ModTarget,
        parent: Option<&str>,
    ) -> Result<ModRevision, String> {
        self.capture_with_intent(profile_id, game_dir, target, parent, None)
    }

    pub fn capture_with_intent(
        &self,
        profile_id: &str,
        game_dir: &Path,
        target: ModTarget,
        parent: Option<&str>,
        intent_id: Option<&str>,
    ) -> Result<ModRevision, String> {
        validate_profile(profile_id)?;
        validate_target(&target)?;
        if let Some(id) = intent_id {
            valid_id(id)?;
        }
        let guard = self.open_writer()?;
        let parent_revision = parent.map(|id| load_in(&guard, id)).transpose()?;
        if parent_revision
            .as_ref()
            .is_some_and(|r| r.profile_id != profile_id)
        {
            return Err("mod revision parent belongs to another profile".into());
        }
        let source = child_dir(&open_root(game_dir, false)?, "mods", false)?;
        let names = source_names(&source)?;
        let metadata_stamp = optional_stamp(&source, ".mod_meta.json")?;
        let metadata: BTreeMap<String, SourceMeta> = match inspect(&source, ".mod_meta.json")? {
            Some(_) => read_json(&source, ".mod_meta.json")?,
            None => BTreeMap::new(),
        };
        let mut mods = Vec::with_capacity(names.len());
        let mut captured_stamps = Vec::with_capacity(names.len());
        for name in &names {
            let mut input = read_file(&source, name)?;
            let before = stamp(&input)?;
            captured_stamps.push(before.clone());
            if before.size > MAX_MOD_BYTES {
                return Err(format!("mod exceeds size limit: {name}"));
            }
            let temporary = format!(".capture-{}", uuid::Uuid::new_v4());
            let result = (|| {
                let mut output = create_file(&guard.objects, &temporary)?;
                let (object_id, size_bytes) = copy_hashed(&mut input, &mut output)?;
                output.sync_all().map_err(err)?;
                if size_bytes != before.size || stamp(&input)? != before {
                    return Err(format!("mod changed while capturing: {name}"));
                }
                // Reopen through the source handle to detect replacement during a read.
                if stamp(&read_file(&source, name)?)? != before {
                    return Err(format!("mod was replaced while capturing: {name}"));
                }
                drop(output);
                if inspect(&guard.objects, &object_id)?.is_some() {
                    if validate_bytes(&guard.objects, &object_id, &object_id, size_bytes).is_err() {
                        // Repair only from our freshly verified, independent staging copy.
                        managed_fs::write_with(
                            &guard.objects,
                            Path::new(&object_id),
                            true,
                            |dst| {
                                let mut staged = read_file(&guard.objects, &temporary)?;
                                let before = stamp(&staged)?;
                                let (hash, size) = copy_hashed(&mut staged, dst)?;
                                if hash != object_id
                                    || size != size_bytes
                                    || stamp(&staged)? != before
                                {
                                    return Err("staged mod object changed during recovery".into());
                                }
                                Ok(())
                            },
                        )?;
                        sync_dir(&guard.objects)?;
                        validate_bytes(&guard.objects, &object_id, &object_id, size_bytes)?;
                    }
                } else {
                    guard
                        .objects
                        .rename(&temporary, &guard.objects, &object_id)
                        .map_err(err)?;
                    sync_dir(&guard.objects)?;
                }
                let fabric = fabric_metadata(read_file(&guard.objects, &object_id)?);
                let cached = metadata.get(name).or_else(|| {
                    name.strip_suffix(".disabled")
                        .and_then(|base| metadata.get(base))
                });
                let mut entry = ModEntry {
                    filename: name.clone(),
                    object_id,
                    size_bytes,
                    enabled: !name.to_ascii_lowercase().ends_with(".disabled"),
                    mod_id: fabric.id.or_else(|| cached.and_then(|m| m.mod_id.clone())),
                    version: fabric
                        .version
                        .or_else(|| cached.and_then(|m| m.version_number.clone())),
                    project_id: cached.and_then(|m| m.project_id.clone()),
                    provider_version: cached.and_then(|m| m.version_id.clone()),
                    provider_version_number: cached.and_then(|m| m.version_number.clone()),
                    display_name: cached.and_then(|m| m.display_name.clone()).or(fabric.name),
                    icon_url: cached.and_then(|m| m.icon_url.clone()),
                    hashes: cached.and_then(|m| m.hashes.clone()).unwrap_or_default(),
                };
                sanitize_metadata(&mut entry);
                Ok(entry)
            })();
            let _ = guard.objects.remove_file(&temporary);
            mods.push(result?);
        }
        if source_names(&source)? != names {
            return Err("mod directory changed while capturing".into());
        }
        for (name, before) in names.iter().zip(&captured_stamps) {
            if stamp(&read_file(&source, name)?)? != *before {
                return Err(format!("mod changed before capture completed: {name}"));
            }
        }
        if optional_stamp(&source, ".mod_meta.json")? != metadata_stamp {
            return Err("mod metadata changed while capturing".into());
        }
        // A same-composition capture is a no-op, including when parent was supplied.
        let current = head_in(&guard, profile_id)?;
        for candidate in [current.as_ref(), parent_revision.as_ref()]
            .into_iter()
            .flatten()
        {
            if candidate.target == target
                && candidate.mods == mods
                && candidate.intent_id.as_deref() == intent_id
            {
                return Ok(candidate.clone());
            }
        }
        let mut revision = ModRevision {
            id: String::new(),
            profile_id: profile_id.into(),
            parent: parent.map(str::to_owned),
            target,
            mods,
            created_at: chrono::Utc::now().to_rfc3339(),
            intent_id: intent_id.map(str::to_owned),
        };
        revision.id = revision_id(&revision)?;
        validate_revision(&revision)?;
        let revisions = child_dir(&guard.state, "revisions", false)?;
        let filename = format!("{}.json", revision.id);
        if inspect(&revisions, &filename)?.is_some() {
            return load_in(&guard, &revision.id);
        }
        write_json(&revisions, &filename, &revision)?;
        Ok(revision)
    }

    pub fn load(&self, id: &str) -> Result<ModRevision, String> {
        valid_id(id)?;
        load_in(&self.reader_required()?, id)
    }

    /// Returns an independent copy, never a hard link or the user's active mods.
    pub fn materialize(&self, revision: &ModRevision) -> Result<PathBuf, String> {
        let guard = self.open_writer()?;
        require_saved(&guard, revision)?;
        let generations = child_dir(&guard.state, "generations", false)?;
        if let Some((generation, name)) = generation_location(&guard, revision)?
            && check_generation(&guard, revision, &generation, true).is_ok()
        {
            return Ok(self.generation_path(&name));
        }
        let temporary = format!(".staging-{}", uuid::Uuid::new_v4());
        let generation_name = format!("{}-{}", revision.id, uuid::Uuid::new_v4().simple());
        let result = (|| {
            let staging = child_dir(&generations, &temporary, true)?;
            let mods = child_dir(&staging, "mods", true)?;
            let mut files = Vec::with_capacity(revision.mods.len());
            for entry in &revision.mods {
                let mut input = read_file(&guard.objects, &entry.object_id)?;
                let before = stamp(&input)?;
                let mut output = create_file(&mods, &entry.filename)?;
                let (hash, size) = copy_hashed(&mut input, &mut output)?;
                if hash != entry.object_id || size != entry.size_bytes || stamp(&input)? != before {
                    return Err(format!("mod object is corrupt: {}", entry.object_id));
                }
                output.sync_all().map_err(err)?;
                drop(output);
                // Check the actual staged bytes, not just what was sent to write().
                validate_bytes(&mods, &entry.filename, &entry.object_id, entry.size_bytes)?;
                files.push(FileReceipt {
                    filename: entry.filename.clone(),
                    object_id: entry.object_id.clone(),
                    generation: stamp(&read_file(&mods, &entry.filename)?)?,
                    object: before,
                });
            }
            write_json(
                &staging,
                "receipt.json",
                &Receipt {
                    format: 1,
                    revision_id: revision.id.clone(),
                    files,
                },
            )?;
            sync_dir(&mods)?;
            sync_dir(&staging)?;
            check_generation(&guard, revision, &staging, true)?;
            drop(mods);
            drop(staging);
            generations
                .rename(&temporary, &generations, &generation_name)
                .map_err(err)?;
            sync_dir(&generations)?;
            let committed = child_dir(&generations, &generation_name, false)?;
            check_generation(&guard, revision, &committed, true)?;
            // An atomic locator switch preserves the previous generation during
            // repairs, including for readers/JVMs already using its path.
            let pointers = child_dir(&guard.state, "generation-heads", false)?;
            atomic_head(
                &pointers,
                &format!("{}.json", revision.id),
                &GenerationHead {
                    revision_id: revision.id.clone(),
                    generation: generation_name.clone(),
                },
            )?;
            Ok(self.generation_path(&generation_name))
        })();
        // Abandoned staging is never considered a generation or a history entry.
        // Handle-relative removal cannot traverse a substituted outward link.
        if result.is_err() {
            let _ = generations.remove_dir_all(&temporary);
        }
        result
    }

    /// Publish only an already materialized, deeply validated snapshot.
    pub fn publish(&self, profile_id: &str, id: &str) -> Result<(), String> {
        validate_profile(profile_id)?;
        let guard = self.open_writer()?;
        let revision = load_in(&guard, id)?;
        if revision.profile_id != profile_id {
            return Err("cannot publish another profile's mod revision".into());
        }
        verify_in(&guard, &revision)?;
        let heads = child_dir(&guard.state, "heads", false)?;
        atomic_head(
            &heads,
            &head_filename(profile_id),
            &Head {
                profile_id: profile_id.into(),
                revision_id: id.into(),
            },
        )
    }

    pub fn head(&self, profile_id: &str) -> Result<Option<ModRevision>, String> {
        validate_profile(profile_id)?;
        match self.open_reader()? {
            Some(guard) => head_in(&guard, profile_id),
            None => Ok(None),
        }
    }

    /// Completed, metadata-valid generations, newest first. Pending captures and
    /// abandoned staging are excluded. Call verify for cryptographic validation.
    pub fn history(&self, profile_id: &str) -> Result<Vec<ModRevision>, String> {
        validate_profile(profile_id)?;
        let Some(guard) = self.open_reader()? else {
            return Ok(Vec::new());
        };
        let revisions = child_dir(&guard.state, "revisions", false)?;
        let mut history = Vec::new();
        for entry in revisions.entries().map_err(err)? {
            let name = entry
                .map_err(err)?
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF-8 revision filename")?;
            if name.starts_with(".hikyou-") && name.ends_with(".tmp") {
                inspect(&revisions, &name)?;
                continue;
            }
            let id = name
                .strip_suffix(".json")
                .ok_or("unexpected revision repository entry")?;
            let revision = load_in(&guard, id)?;
            if revision.profile_id == profile_id
                && let Ok(Some((generation, _))) = generation_location(&guard, &revision)
                && check_generation(&guard, &revision, &generation, false).is_ok()
            {
                history.push(revision);
            }
        }
        history.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(history)
    }

    /// No mod-content hashing. Exact file metadata and enumeration must match.
    pub fn quick_check(&self, revision: &ModRevision) -> Result<PathBuf, String> {
        let guard = self.reader_required()?;
        require_saved(&guard, revision)?;
        let (generation, name) =
            generation_location(&guard, revision)?.ok_or("mod revision is not materialized")?;
        check_generation(&guard, revision, &generation, false)?;
        Ok(self.generation_path(&name))
    }

    pub fn verify(&self, revision: &ModRevision) -> Result<(), String> {
        let guard = self.reader_required()?;
        require_saved(&guard, revision)?;
        verify_in(&guard, revision)
    }

    fn generation_path(&self, generation: &str) -> PathBuf {
        self.root
            .join(STATE)
            .join("generations")
            .join(generation)
            .join("mods")
    }

    // Readers do not open, wait on, or take the writer lock. Immutable files and
    // atomic head/locator replacement provide coherent snapshots independently.
    fn open_reader(&self) -> Result<Option<RepositoryGuard>, String> {
        let root = match open_root(&self.root, false) {
            Ok(root) => root,
            Err(e) if e == MISSING_DIRECTORY => return Ok(None),
            Err(e) => return Err(e),
        };
        let state = match descend(&root, STATE, false) {
            Ok(state) => state,
            Err(e) if e == MISSING_DIRECTORY => return Ok(None),
            Err(e) => return Err(e),
        };
        let objects = descend(&root, OBJECTS, false)?;
        Ok(Some(RepositoryGuard {
            state,
            objects,
            _lock: None,
        }))
    }

    fn reader_required(&self) -> Result<RepositoryGuard, String> {
        self.open_reader()?
            .ok_or_else(|| "mod repository has not been initialized".into())
    }

    fn open_writer(&self) -> Result<RepositoryGuard, String> {
        let root = open_root(&self.root, true)?;
        let state = descend(&root, STATE, true)?;
        for name in ["revisions", "generations", "heads", "generation-heads"] {
            child_dir(&state, name, true)?;
        }
        let objects = descend(&root, OBJECTS, true)?;
        if inspect(&state, ".lock")?.is_none() {
            match create_file(&state, ".lock") {
                Ok(file) => file.sync_all().map_err(err)?,
                Err(_) if inspect(&state, ".lock")?.is_some() => {}
                Err(e) => return Err(e),
            }
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        nofollow(&mut options);
        let lock = state.open_with(".lock", &options).map_err(err)?.into_std();
        stamp(&lock)?;
        FileExt::lock_exclusive(&lock).map_err(err)?;
        Ok(RepositoryGuard {
            state,
            objects,
            _lock: Some(lock),
        })
    }
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn validate_profile(id: &str) -> Result<(), String> {
    // Keep exactly the same accepted refs as core::profile::validate_profile_ref.
    if matches!(id, "smart:latest-plus" | "smart:snapshot-plus") {
        return Ok(());
    }
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "invalid mod profile ref".into())
}

fn valid_id(id: &str) -> Result<(), String> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("invalid SHA-256 mod repository ID".into());
    }
    Ok(())
}

fn validate_filename(name: &str) -> Result<(), String> {
    validate_leaf(name)?;
    let lower = name.to_ascii_lowercase();
    if name.starts_with('.') || (!lower.ends_with(".jar") && !lower.ends_with(".disabled")) {
        return Err(format!("unsupported mod filename: {name:?}"));
    }
    Ok(())
}

fn validate_leaf(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 240
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
    {
        return Err(format!("unsafe mod filename: {name:?}"));
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(
            &stem[3..],
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "\u{b9}" | "\u{b2}" | "\u{b3}"
        ))
    {
        return Err(format!("reserved mod filename: {name:?}"));
    }
    Ok(())
}

fn validate_target(target: &ModTarget) -> Result<(), String> {
    for value in [&target.mc_version, &target.loader]
        .into_iter()
        .chain(target.loader_version.as_ref())
    {
        if value.is_empty()
            || value.len() > 256
            || value.chars().any(char::is_control)
            || value.contains(['/', '\\', ':'])
        {
            return Err("invalid mod target".into());
        }
    }
    Ok(())
}

fn revision_id(revision: &ModRevision) -> Result<String, String> {
    let legacy = (
        &revision.profile_id,
        &revision.parent,
        &revision.target,
        &revision.mods,
    );
    // Keep existing revisions readable; new ones also bind the user's intent.
    let bytes = match &revision.intent_id {
        Some(intent) => serde_json::to_vec(&("mod-intent-v1", legacy, intent)),
        None => serde_json::to_vec(&legacy),
    }
    .map_err(err)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_revision(revision: &ModRevision) -> Result<(), String> {
    valid_id(&revision.id)?;
    validate_profile(&revision.profile_id)?;
    validate_target(&revision.target)?;
    if let Some(intent) = &revision.intent_id {
        valid_id(intent)?;
    }
    if let Some(parent) = &revision.parent {
        valid_id(parent)?;
        if parent == &revision.id {
            return Err("self-parented mod revision".into());
        }
    }
    if revision.mods.len() > MAX_MODS {
        return Err("too many mods".into());
    }
    let mut folded = BTreeSet::new();
    let mut previous: Option<&str> = None;
    for entry in &revision.mods {
        validate_filename(&entry.filename)?;
        valid_id(&entry.object_id)?;
        if entry.size_bytes > MAX_MOD_BYTES
            || entry.enabled == entry.filename.to_ascii_lowercase().ends_with(".disabled")
            || previous.is_some_and(|name| name >= entry.filename.as_str())
            || !folded.insert(filename_fold(&entry.filename))
        {
            return Err("invalid or colliding mod composition".into());
        }
        previous = Some(&entry.filename);
        let mut sanitized = entry.clone();
        sanitize_metadata(&mut sanitized);
        if &sanitized != entry {
            return Err("unsafe mod origin/display metadata".into());
        }
    }
    chrono::DateTime::parse_from_rfc3339(&revision.created_at).map_err(err)?;
    if revision_id(revision)? != revision.id {
        return Err("mod revision digest mismatch".into());
    }
    Ok(())
}

fn sanitize_metadata(entry: &mut ModEntry) {
    for field in [
        &mut entry.mod_id,
        &mut entry.version,
        &mut entry.project_id,
        &mut entry.provider_version,
        &mut entry.provider_version_number,
        &mut entry.display_name,
    ] {
        if field.as_ref().is_some_and(|v| {
            v.is_empty()
                || v.len() > 4096
                || v.chars().any(char::is_control)
                || Path::new(v).is_absolute()
                || v.starts_with(['/', '\\'])
                || (v.len() > 1 && v.as_bytes()[1] == b':')
        }) {
            *field = None;
        }
    }
    if entry.icon_url.as_ref().is_some_and(|v| {
        v.len() > 4096
            || v.chars().any(char::is_control)
            || !(v.starts_with("https://") || v.starts_with("http://"))
    }) {
        entry.icon_url = None;
    }
    entry.hashes.retain(|algorithm, value| {
        !algorithm.is_empty()
            && algorithm.len() <= 32
            && algorithm
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            && !value.is_empty()
            && value.len() <= 1024
            && value.bytes().all(|c| c.is_ascii_hexdigit())
    });
}

fn load_in(guard: &RepositoryGuard, id: &str) -> Result<ModRevision, String> {
    valid_id(id)?;
    let revisions = child_dir(&guard.state, "revisions", false)?;
    let revision: ModRevision = read_json(&revisions, &format!("{id}.json"))?;
    validate_revision(&revision)?;
    if revision.id != id {
        return Err("revision filename/ID mismatch".into());
    }
    Ok(revision)
}

fn require_saved(guard: &RepositoryGuard, revision: &ModRevision) -> Result<(), String> {
    if load_in(guard, &revision.id)? != *revision {
        return Err("mod revision differs from its immutable saved snapshot".into());
    }
    Ok(())
}

fn head_filename(profile_id: &str) -> String {
    format!("{:x}.json", Sha256::digest(profile_id.as_bytes()))
}

fn head_in(guard: &RepositoryGuard, profile_id: &str) -> Result<Option<ModRevision>, String> {
    let heads = child_dir(&guard.state, "heads", false)?;
    let filename = head_filename(profile_id);
    if inspect(&heads, &filename)?.is_none() {
        return Ok(None);
    }
    let head: Head = read_atomic_json(&heads, &filename)?;
    if head.profile_id != profile_id {
        return Err("mod head profile mismatch".into());
    }
    let revision = load_in(guard, &head.revision_id)?;
    if revision.profile_id != profile_id {
        return Err("mod head references another profile".into());
    }
    Ok(Some(revision))
}

fn generation_location(
    guard: &RepositoryGuard,
    revision: &ModRevision,
) -> Result<Option<(Dir, String)>, String> {
    let pointers = child_dir(&guard.state, "generation-heads", false)?;
    let filename = format!("{}.json", revision.id);
    if inspect(&pointers, &filename)?.is_none() {
        return Ok(None);
    }
    let pointer: GenerationHead = read_atomic_json(&pointers, &filename)?;
    let suffix = pointer
        .generation
        .strip_prefix(&format!("{}-", revision.id))
        .ok_or("invalid materialized generation locator")?;
    if pointer.revision_id != revision.id
        || suffix.len() != 32
        || !suffix
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("invalid materialized generation locator".into());
    }
    let generations = child_dir(&guard.state, "generations", false)?;
    match inspect(&generations, &pointer.generation)? {
        Some(metadata) if metadata.is_dir() => {}
        _ => return Ok(None),
    }
    Ok(Some((
        child_dir(&generations, &pointer.generation, false)?,
        pointer.generation,
    )))
}

fn verify_in(guard: &RepositoryGuard, revision: &ModRevision) -> Result<(), String> {
    let (generation, _) =
        generation_location(guard, revision)?.ok_or("mod revision is not materialized")?;
    check_generation(guard, revision, &generation, true)
}

fn check_generation(
    guard: &RepositoryGuard,
    revision: &ModRevision,
    generation: &Dir,
    deep: bool,
) -> Result<(), String> {
    if all_names(generation)? != vec!["mods".to_string(), "receipt.json".to_string()] {
        return Err("unexpected generation entries".into());
    }
    let mods = child_dir(generation, "mods", false)?;
    let receipt: Receipt = read_json(generation, "receipt.json")?;
    if receipt.format != 1
        || receipt.revision_id != revision.id
        || receipt.files.len() != revision.mods.len()
        || all_names(&mods)?
            != revision
                .mods
                .iter()
                .map(|m| m.filename.clone())
                .collect::<Vec<_>>()
    {
        return Err("mod generation inventory/receipt mismatch".into());
    }
    for (entry, saved) in revision.mods.iter().zip(receipt.files.iter()) {
        if saved.filename != entry.filename || saved.object_id != entry.object_id {
            return Err("mod receipt composition mismatch".into());
        }
        let generation_file = read_file(&mods, &entry.filename)?;
        let object_file = read_file(&guard.objects, &entry.object_id)?;
        if stamp(&generation_file)? != saved.generation
            || stamp(&object_file)? != saved.object
            || saved.generation.size != entry.size_bytes
            || saved.object.size != entry.size_bytes
        {
            return Err(format!("mod file metadata changed: {}", entry.filename));
        }
        if deep {
            validate_bytes(&mods, &entry.filename, &entry.object_id, entry.size_bytes)?;
            validate_bytes(
                &guard.objects,
                &entry.object_id,
                &entry.object_id,
                entry.size_bytes,
            )?;
        }
    }
    Ok(())
}

fn copy_hashed(input: &mut File, output: &mut impl Write) -> Result<(String, u64), String> {
    let mut hash = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = input.read(&mut buffer).map_err(err)?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or("mod size overflow")?;
        if total > MAX_MOD_BYTES {
            return Err("mod exceeds size limit".into());
        }
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count]).map_err(err)?;
    }
    Ok((format!("{:x}", hash.finalize()), total))
}

fn validate_bytes(dir: &Dir, name: &str, expected: &str, size: u64) -> Result<(), String> {
    let mut file = read_file(dir, name)?;
    let before = stamp(&file)?;
    let (actual, actual_size) = copy_hashed(&mut file, &mut io::sink())?;
    if actual != expected
        || actual_size != size
        || before.size != size
        || stamp(&file)? != before
        || stamp(&read_file(dir, name)?)? != before
    {
        return Err(format!("mod content validation failed: {name}"));
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(dir: &Dir, name: &str) -> Result<T, String> {
    let mut file = read_file(dir, name)?;
    let before = stamp(&file)?;
    if before.size > MAX_JSON_BYTES {
        return Err("repository JSON exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 != before.size
        || stamp(&file)? != before
        || stamp(&read_file(dir, name)?)? != before
    {
        return Err("repository JSON changed while reading".into());
    }
    serde_json::from_slice(&bytes).map_err(err)
}

fn read_atomic_json<T: DeserializeOwned>(dir: &Dir, name: &str) -> Result<T, String> {
    // Read one opened inode/handle. Reopening here would reject a legitimate
    // concurrent atomic replacement instead of returning its prior snapshot.
    let mut last_error = String::new();
    for _ in 0..3 {
        let result = (|| {
            let mut file = read_file(dir, name)?;
            let size = file.metadata().map_err(err)?.len();
            if size > MAX_JSON_BYTES {
                return Err("atomic JSON exceeds size limit".into());
            }
            let mut bytes = Vec::new();
            Read::by_ref(&mut file)
                .take(MAX_JSON_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(err)?;
            if bytes.len() as u64 != size {
                return Err("atomic JSON size changed".into());
            }
            serde_json::from_slice(&bytes).map_err(err)
        })();
        match result {
            Ok(value) => return Ok(value),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn write_json(dir: &Dir, name: &str, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(err)?;
    if bytes.len() as u64 > MAX_JSON_BYTES {
        return Err("repository JSON exceeds size limit".into());
    }
    managed_fs::write(dir, Path::new(name), &bytes, false)?;
    sync_dir(dir)
}

fn all_names(dir: &Dir) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for entry in dir.entries().map_err(err)? {
        let name = entry
            .map_err(err)?
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF-8 mod filename")?;
        inspect(dir, &name)?.ok_or("mod entry disappeared")?;
        names.push(name);
        if names.len() > MAX_MODS + 2 {
            return Err("too many mod directory entries".into());
        }
    }
    names.sort();
    Ok(names)
}

fn source_names(dir: &Dir) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut folded = BTreeSet::new();
    for name in all_names(dir)? {
        let metadata = inspect(dir, &name)?.ok_or("source mod disappeared")?;
        if !metadata.is_file() {
            return Err("mod source contains a non-file entry".into());
        }
        // Opening every entry also rejects hard links, including the metadata cache.
        read_file(dir, &name)?;
        if name == ".mod_meta.json" {
            continue;
        }
        validate_leaf(&name)?;
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".jar") && !lower.ends_with(".disabled") {
            continue;
        }
        validate_filename(&name)?;
        if !folded.insert(filename_fold(&name)) {
            return Err("case-folding mod filename collision".into());
        }
        names.push(name);
        if names.len() > MAX_MODS {
            return Err("too many mods".into());
        }
    }
    Ok(names)
}

fn optional_stamp(dir: &Dir, name: &str) -> Result<Option<Stamp>, String> {
    match inspect(dir, name)? {
        Some(_) => Ok(Some(stamp(&read_file(dir, name)?)?)),
        None => Ok(None),
    }
}

fn filename_fold(name: &str) -> String {
    // Uppercase first unifies final sigma, dotless i, and expanding cases too.
    name.to_uppercase().to_lowercase()
}

fn linked(metadata: &cap_std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn inspect(dir: &Dir, name: &str) -> Result<Option<cap_std::fs::Metadata>, String> {
    let metadata = match dir.symlink_metadata(name) {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    if linked(&metadata) {
        return Err("linked/reparse repository or mod entries are forbidden".into());
    }
    Ok(Some(metadata))
}

fn child_dir(parent: &Dir, name: &str, create: bool) -> Result<Dir, String> {
    if create {
        match parent.create_dir(name) {
            Ok(()) => sync_dir(parent)?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(err(e)),
        }
    }
    if !inspect(parent, name)?.ok_or(MISSING_DIRECTORY)?.is_dir() {
        return Err("mod repository/source path is not a directory".into());
    }
    let dir = parent.open_dir(name).map_err(err)?;
    if linked(&dir.dir_metadata().map_err(err)?) {
        return Err("linked directory is forbidden".into());
    }
    inspect(parent, name)?.ok_or("directory changed while opening")?;
    Ok(dir)
}

fn descend(root: &Dir, relative: &str, create: bool) -> Result<Dir, String> {
    let mut dir = root.try_clone().map_err(err)?;
    for name in relative.split('/') {
        dir = child_dir(&dir, name, create)?;
    }
    Ok(dir)
}

fn open_root(path: &Path, create: bool) -> Result<Dir, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(err)?.join(path)
    };
    let mut anchor = PathBuf::new();
    let mut current: Option<Dir> = None;
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => anchor.push(prefix.as_os_str()),
            Component::RootDir => {
                anchor.push(component.as_os_str());
                current = Some(
                    Dir::open_ambient_dir(&anchor, cap_std::ambient_authority()).map_err(err)?,
                );
            }
            Component::Normal(name) => {
                let name = name.to_str().ok_or("non-UTF-8 managed root")?;
                current = Some(child_dir(
                    current.as_ref().ok_or("unrooted managed path")?,
                    name,
                    create,
                )?);
            }
            _ => return Err("managed root must not contain path navigation".into()),
        }
    }
    current.ok_or_else(|| "managed root is not absolute".into())
}

fn nofollow(options: &mut OpenOptions) {
    #[cfg(windows)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
}

fn read_file(dir: &Dir, name: &str) -> Result<File, String> {
    if !inspect(dir, name)?
        .ok_or("required mod file is missing")?
        .is_file()
    {
        return Err("mod input is not a regular file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    nofollow(&mut options);
    let file = dir.open_with(name, &options).map_err(err)?.into_std();
    stamp(&file)?;
    inspect(dir, name)?.ok_or("mod file disappeared while opening")?;
    Ok(file)
}

fn create_file(dir: &Dir, name: &str) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    nofollow(&mut options);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    dir.open_with(name, &options)
        .map(|file| file.into_std())
        .map_err(err)
}

fn time_key(time: SystemTime) -> String {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}:{:09}", duration.as_secs(), duration.subsec_nanos()),
        Err(error) => format!(
            "-{}:{:09}",
            error.duration().as_secs(),
            error.duration().subsec_nanos()
        ),
    }
}

fn stamp(file: &File) -> Result<Stamp, String> {
    let metadata = file.metadata().map_err(err)?;
    if !metadata.is_file() {
        return Err("mod input changed type".into());
    }
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err("hard-linked mod files are forbidden".into());
        }
        vec![
            metadata.dev(),
            metadata.ino(),
            metadata.ctime() as u64,
            metadata.ctime_nsec() as u64,
        ]
    };
    #[cfg(windows)]
    let identity = {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO, FileBasicInfo,
                GetFileInformationByHandle, GetFileInformationByHandleEx,
            },
        };
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("reparse mod files are forbidden".into());
        }
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
            .map_err(err)?;
        if info.nNumberOfLinks != 1 {
            return Err("hard-linked mod files are forbidden".into());
        }
        let mut basic = FILE_BASIC_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileBasicInfo,
                (&mut basic as *mut FILE_BASIC_INFO).cast(),
                std::mem::size_of::<FILE_BASIC_INFO>() as u32,
            )
        }
        .map_err(err)?;
        vec![
            u64::from(info.dwVolumeSerialNumber),
            u64::from(info.nFileIndexHigh),
            u64::from(info.nFileIndexLow),
            u64::from(metadata.file_attributes()),
            basic.ChangeTime as u64,
        ]
    };
    Ok(Stamp {
        size: metadata.len(),
        modified: time_key(metadata.modified().map_err(err)?),
        created: metadata.created().ok().map(time_key),
        identity,
    })
}

fn sync_dir(dir: &Dir) -> Result<(), String> {
    #[cfg(unix)]
    {
        // cap-std may hold an O_PATH descriptor, which cannot be fsynced.
        // Reopen the held directory itself, without resolving an ambient path.
        let fd = rustix::fs::openat(
            dir,
            ".",
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(err)?;
        File::from(fd).sync_all().map_err(err)?;
    }
    #[cfg(windows)]
    let _ = dir; // Windows cannot FlushFileBuffers a read-only directory handle.
    Ok(())
}

fn atomic_head(dir: &Dir, name: &str, head: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(head).map_err(err)?;
    if inspect(dir, name)?.is_some() {
        read_file(dir, name)?;
    }
    #[cfg(windows)]
    {
        // cap-std holds this directory without delete sharing. Derive the path
        // from that handle, not the original public pathname, while it is held.
        let path = held_directory_path(dir)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&path).map_err(err)?;
        temporary.write_all(&bytes).map_err(err)?;
        temporary.as_file().sync_all().map_err(err)?;
        temporary.persist(path.join(name)).map_err(err)?;
        Ok(())
    }
    #[cfg(unix)]
    {
        // Handle-relative rename is atomic and never follows the destination.
        managed_fs::write(dir, Path::new(name), &bytes, true)?;
        sync_dir(dir)
    }
}

#[cfg(windows)]
fn held_directory_path(dir: &Dir) -> Result<PathBuf, String> {
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW},
    };
    let mut buffer = vec![0; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(dir.as_raw_handle()),
                &mut buffer,
                FILE_NAME_NORMALIZED,
            )
        } as usize;
        if length == 0 {
            return Err(err(io::Error::last_os_error()));
        }
        if length < buffer.len() {
            return Ok(PathBuf::from(std::ffi::OsString::from_wide(
                &buffer[..length],
            )));
        }
        if length > 32768 {
            return Err("repository directory path is too long".into());
        }
        buffer.resize(length + 1, 0);
    }
}

fn fabric_metadata(mut file: File) -> FabricMeta {
    // Build a bounded, single-entry archive before invoking the ZIP decoder.
    // This prevents ZIP64/fallback EOCD records from causing unbounded allocation.
    let result = (|| -> Option<FabricMeta> {
        let size = file.metadata().ok()?.len();
        let tail_size = size.min(65_557) as usize;
        file.seek(SeekFrom::End(-(tail_size as i64))).ok()?;
        let mut tail = vec![0; tail_size];
        file.read_exact(&mut tail).ok()?;
        let position = tail.windows(4).rposition(|w| w == b"PK\x05\x06")?;
        let end = tail.get(position..)?;
        if end.len() < 22 {
            return None;
        }
        let u16_at = |i| u16::from_le_bytes([end[i], end[i + 1]]);
        let u32_at = |i| u32::from_le_bytes([end[i], end[i + 1], end[i + 2], end[i + 3]]);
        let count = u16_at(10);
        let central_size = u64::from(u32_at(12));
        let central_offset = u64::from(u32_at(16));
        if u16_at(4) != 0
            || u16_at(6) != 0
            || u16_at(8) != count
            || count > 4096
            || central_size > 8 * 1024 * 1024
            || central_offset.checked_add(central_size)? > size - tail_size as u64 + position as u64
            || end.len() != 22 + usize::from(u16_at(20))
            || (position >= 20 && &tail[position - 20..position - 16] == b"PK\x06\x07")
        {
            return None;
        }
        file.seek(SeekFrom::Start(central_offset)).ok()?;
        let mut central = vec![0; central_size as usize];
        file.read_exact(&mut central).ok()?;
        let mut offset = 0_usize;
        let mut found: Option<Vec<u8>> = None;
        for _ in 0..count {
            let header = central.get(offset..offset.checked_add(46)?)?;
            if &header[..4] != b"PK\x01\x02" {
                return None;
            }
            let name_len = usize::from(le16(header, 28)?);
            let extra_len = usize::from(le16(header, 30)?);
            let comment_len = usize::from(le16(header, 32)?);
            let length = 46_usize
                .checked_add(name_len)?
                .checked_add(extra_len)?
                .checked_add(comment_len)?;
            let entry = central.get(offset..offset.checked_add(length)?)?;
            if entry.get(46..46 + name_len)? == b"fabric.mod.json" {
                if found.is_some() {
                    return None;
                }
                found = Some(entry.to_vec());
            }
            offset = offset.checked_add(length)?;
        }
        if offset != central.len() {
            return None;
        }
        let mut header = found?;
        let compressed_size = u64::from(le32(&header, 20)?);
        if compressed_size > MAX_FABRIC_JSON * 2
            || u64::from(le32(&header, 24)?) > MAX_FABRIC_JSON
            || le16(&header, 8)? & 0x41 != 0
            || le16(&header, 34)? != 0
        {
            return None;
        }
        let local_offset = u64::from(le32(&header, 42)?);
        file.seek(SeekFrom::Start(local_offset)).ok()?;
        let mut local = vec![0; 30];
        file.read_exact(&mut local).ok()?;
        if &local[..4] != b"PK\x03\x04"
            || le16(&local, 6)? != le16(&header, 8)?
            || le16(&local, 8)? != le16(&header, 10)?
        {
            return None;
        }
        let name_len = usize::from(le16(&local, 26)?);
        let extra_len = usize::from(le16(&local, 28)?);
        let length = 30_usize
            .checked_add(name_len)?
            .checked_add(extra_len)?
            .checked_add(compressed_size as usize)?;
        if local_offset.checked_add(length as u64)? > central_offset {
            return None;
        }
        local.resize(length, 0);
        file.read_exact(&mut local[30..]).ok()?;
        if local.get(30..30 + name_len)? != b"fabric.mod.json" {
            return None;
        }
        // Strip untrusted extra fields and comments from the central record.
        let central_name_len = usize::from(le16(&header, 28)?);
        header.truncate(46 + central_name_len);
        header[30..34].fill(0);
        header[42..46].fill(0);
        let start = local.len() as u32;
        let central_len = header.len() as u32;
        local.extend_from_slice(&header);
        local.extend_from_slice(b"PK\x05\x06\0\0\0\0\x01\0\x01\0");
        local.extend_from_slice(&central_len.to_le_bytes());
        local.extend_from_slice(&start.to_le_bytes());
        local.extend_from_slice(&[0, 0]);
        let mut archive = zip::ZipArchive::new(io::Cursor::new(local)).ok()?;
        let mut entry = archive.by_name("fabric.mod.json").ok()?;
        if entry.size() > MAX_FABRIC_JSON || entry.is_dir() {
            return None;
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut entry)
            .take(MAX_FABRIC_JSON + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 > MAX_FABRIC_JSON {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    })();
    result.unwrap_or_default()
}

fn le16(bytes: &[u8], index: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(index..index.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn le32(bytes: &[u8], index: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(index..index.checked_add(4)?)?.try_into().ok()?,
    ))
}

#[cfg(test)]
#[path = "mod_repository/tests.rs"]
mod tests;
