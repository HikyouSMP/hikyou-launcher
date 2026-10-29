use super::*;
use crate::core::mod_repository::ModTarget;
use crate::core::test_support::physical_tempdir;
use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
};

const PROFILE: &str = "smart:latest-plus";

struct Fixture {
    _temp: tempfile::TempDir,
    game: PathBuf,
    source: PathBuf,
    repo: ModRepository,
}

impl Fixture {
    fn new() -> Self {
        let temp = physical_tempdir();
        let game = temp.path().join("game");
        let source = temp.path().join("source");
        let data = temp.path().join("data");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::create_dir_all(source.join("mods")).unwrap();
        fs::create_dir_all(&data).unwrap();
        Self {
            repo: ModRepository::new(&data),
            game,
            source,
            _temp: temp,
        }
    }

    fn revision(&self) -> (ModRevision, PathBuf) {
        fs::write(self.source.join("mods/new.JAR"), b"new mod").unwrap();
        let revision = self
            .repo
            .capture(
                PROFILE,
                &self.source,
                ModTarget {
                    mc_version: "1.21.1".into(),
                    loader: "vanilla".into(),
                    loader_version: None,
                },
                None,
            )
            .unwrap();
        let generation = self.repo.materialize(&revision).unwrap();
        (revision, generation)
    }

    fn journal(&self, committed: bool) -> Checkout {
        let journal = Checkout {
            revision: "a".repeat(64),
            previous_head: None,
            backup: ".hikyou-mod-backup-test".into(),
            stage: ".hikyou-mod-stage-test".into(),
            committed,
            original_absent: false,
        };
        fs::create_dir_all(self.game.join(&journal.backup)).unwrap();
        fs::create_dir_all(self.game.join(&journal.stage).join("mods")).unwrap();
        fs::write(
            self.game.join(JOURNAL),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        journal
    }
}

#[test]
fn writable_foreign_handle_follows_the_file_into_active_mods() {
    let f = Fixture::new();
    fs::write(f.game.join("mods/old.jar"), b"old").unwrap();
    fs::write(f.game.join("mods/notes.txt"), b"before").unwrap();
    let mut held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(f.game.join("mods/notes.txt"))
        .unwrap();
    let previous = f
        .repo
        .capture(
            PROFILE,
            &f.game,
            ModTarget {
                mc_version: "1.21.1".into(),
                loader: "vanilla".into(),
                loader_version: None,
            },
            None,
        )
        .unwrap();
    f.repo.materialize(&previous).unwrap();
    f.repo.publish(PROFILE, &previous.id).unwrap();
    let (revision, generation) = f.revision();
    let result = checkout(&f.repo, &revision, &generation, &f.game);
    if let Err(error) = &result {
        assert!(
            cfg!(windows) && error.contains("Access is denied"),
            "unexpected checkout failure: {error}"
        );
        assert!(
            error.contains("Mod folder"),
            "missing failure phase: {error}"
        );
        assert_eq!(f.repo.head(PROFILE).unwrap(), Some(previous.clone()));
        eprintln!("Windows refused parent-directory rename with a live child handle: {error}");
    }
    held.seek(SeekFrom::Start(0)).unwrap();
    held.set_len(0).unwrap();
    held.write_all(b"written after checkout").unwrap();
    held.sync_all().unwrap();
    if result.is_err() {
        let candidates = foreign_locations(&f.game, "notes.txt");
        assert_eq!(candidates.len(), 1);
        assert_eq!(fs::read(&candidates[0]).unwrap(), b"written after checkout");
        drop(held);
        recover(&f.repo, PROFILE, &f.game).unwrap();
        assert_eq!(f.repo.head(PROFILE).unwrap(), Some(previous));
        assert_eq!(
            fs::read(f.game.join("mods/notes.txt")).unwrap(),
            b"written after checkout"
        );
        checkout(&f.repo, &revision, &generation, &f.game).unwrap();
    }
    assert_eq!(
        fs::read(f.game.join("mods/notes.txt")).unwrap(),
        b"written after checkout"
    );
    assert!(f.game.join("mods/new.JAR").is_file());
    assert!(!f.game.join("mods/old.jar").exists());
    assert!(!f.game.join(JOURNAL).exists());
    assert_eq!(f.repo.head(PROFILE).unwrap(), Some(revision));
}

#[test]
fn uncommitted_recovery_returns_foreign_files_from_active_and_stage_to_backup() {
    let f = Fixture::new();
    let journal = f.journal(false);
    fs::write(
        f.game.join(&journal.backup).join("old.jar"),
        b"original mod",
    )
    .unwrap();
    fs::write(
        f.game.join(&journal.backup).join("original.txt"),
        b"original foreign",
    )
    .unwrap();
    fs::write(f.game.join("mods/new.jar"), b"replacement mod").unwrap();
    fs::write(f.game.join("mods/active.txt"), b"active foreign").unwrap();
    fs::write(
        f.game.join(&journal.stage).join("mods/staged.txt"),
        b"staged foreign",
    )
    .unwrap();
    recover(&f.repo, PROFILE, &f.game).unwrap();
    for (name, bytes) in [
        ("old.jar", b"original mod".as_slice()),
        ("original.txt", b"original foreign"),
        ("active.txt", b"active foreign"),
        ("staged.txt", b"staged foreign"),
    ] {
        assert_eq!(fs::read(f.game.join("mods").join(name)).unwrap(), bytes);
    }
    assert!(!f.game.join("mods/new.jar").exists());
    assert!(!f.game.join(&journal.backup).exists());
    assert!(!f.game.join(&journal.stage).exists());
    assert!(!f.game.join(JOURNAL).exists());
}

#[test]
fn crash_before_swap_returns_moved_foreign_inode_from_stage() {
    let f = Fixture::new();
    let journal = f.journal(false);
    fs::remove_dir(f.game.join("mods")).unwrap();
    fs::write(f.game.join(&journal.backup).join("old.jar"), b"old").unwrap();
    let staged_notes = f.game.join(&journal.stage).join("mods/notes.txt");
    fs::write(&staged_notes, b"before").unwrap();
    let mut held = fs::OpenOptions::new()
        .write(true)
        .open(staged_notes)
        .unwrap();
    let result = recover(&f.repo, PROFILE, &f.game);
    if let Err(error) = &result {
        assert!(
            cfg!(windows) && error.contains("Access is denied"),
            "unexpected recovery failure: {error}"
        );
        assert!(error.contains("Mod checkout recovery failed"));
        assert!(f.game.join(JOURNAL).is_file());
        assert!(f.game.join(&journal.backup).is_dir());
        assert_eq!(
            fs::read(f.game.join(&journal.backup).join("old.jar")).unwrap(),
            b"old"
        );
        eprintln!("Windows retained pending rollback until its child handle closes: {error}");
    }
    held.write_all(b"after!").unwrap();
    held.flush().unwrap();
    if result.is_err() {
        let candidates = foreign_locations(&f.game, "notes.txt");
        assert_eq!(candidates.len(), 1);
        assert_eq!(fs::read(&candidates[0]).unwrap(), b"after!");
        drop(held);
        recover(&f.repo, PROFILE, &f.game).unwrap();
    }
    assert_eq!(fs::read(f.game.join("mods/notes.txt")).unwrap(), b"after!");
    assert_eq!(fs::read(f.game.join("mods/old.jar")).unwrap(), b"old");
    assert!(!f.game.join(JOURNAL).exists());
}

fn foreign_locations(game: &Path, name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if game.join("mods").join(name).is_file() {
        paths.push(game.join("mods").join(name));
    }
    for entry in fs::read_dir(game).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        let candidate = if filename.starts_with(".hikyou-mod-backup-") {
            entry.path().join(name)
        } else if filename.starts_with(".hikyou-mod-stage-") {
            entry.path().join("mods").join(name)
        } else {
            continue;
        };
        if candidate.is_file() {
            paths.push(candidate);
        }
    }
    paths
}

#[test]
fn committed_recovery_moves_late_foreign_files_before_deleting_backup() {
    let f = Fixture::new();
    let journal = f.journal(true);
    fs::write(f.game.join(&journal.backup).join("old.jar"), b"old").unwrap();
    fs::write(f.game.join(&journal.backup).join("late.txt"), b"late write").unwrap();
    let mut held = fs::OpenOptions::new()
        .write(true)
        .open(f.game.join(&journal.backup).join("late.txt"))
        .unwrap();
    fs::write(f.game.join("mods/new.jar"), b"new").unwrap();
    recover(&f.repo, PROFILE, &f.game).unwrap();
    held.set_len(0).unwrap();
    held.write_all(b"after!!!!").unwrap();
    held.flush().unwrap();
    assert_eq!(
        fs::read(f.game.join("mods/late.txt")).unwrap(),
        b"after!!!!"
    );
    assert!(f.game.join("mods/new.jar").is_file());
    assert!(!f.game.join(&journal.backup).exists());
    assert!(!f.game.join(JOURNAL).exists());
}

#[test]
fn collisions_retain_both_contents_and_journal_for_both_recovery_directions() {
    for committed in [false, true] {
        let f = Fixture::new();
        let journal = f.journal(committed);
        fs::write(
            f.game.join(&journal.backup).join("notes.txt"),
            b"backup notes",
        )
        .unwrap();
        fs::write(f.game.join("mods/notes.txt"), b"active notes").unwrap();
        assert!(recover(&f.repo, PROFILE, &f.game).is_err());
        assert_eq!(
            fs::read(f.game.join(&journal.backup).join("notes.txt")).unwrap(),
            b"backup notes"
        );
        assert_eq!(
            fs::read(f.game.join("mods/notes.txt")).unwrap(),
            b"active notes"
        );
        assert!(f.game.join(JOURNAL).is_file());
        assert!(f.game.join(&journal.stage).is_dir());
    }
}

#[test]
fn exclusive_foreign_move_never_overwrites_a_destination() {
    let temp = physical_tempdir();
    fs::create_dir(temp.path().join("from")).unwrap();
    fs::create_dir(temp.path().join("to")).unwrap();
    fs::write(temp.path().join("from/notes.txt"), b"from").unwrap();
    fs::write(temp.path().join("to/notes.txt"), b"to").unwrap();
    let root = managed_fs::root(temp.path()).unwrap();
    let from = managed_fs::directory(&root, Path::new("from"), false).unwrap();
    let to = managed_fs::directory(&root, Path::new("to"), false).unwrap();
    assert!(rename_exclusive(&from, "notes.txt", &to).is_err());
    assert_eq!(
        fs::read(temp.path().join("from/notes.txt")).unwrap(),
        b"from"
    );
    assert_eq!(fs::read(temp.path().join("to/notes.txt")).unwrap(), b"to");
}

#[test]
fn source_presence_does_not_create_and_rejects_directory_links() {
    let f = Fixture::new();
    assert!(source_present(&f.game).unwrap());
    assert!(!source_present(&f.game.join("missing")).unwrap());
    assert!(!f.game.join("missing").exists());
    fs::remove_dir(f.game.join("mods")).unwrap();
    assert!(!source_present(&f.game).unwrap());
    crate::core::test_support::directory_link(&f.source.join("mods"), &f.game.join("mods"));
    assert!(source_present(&f.game).is_err());
    let linked_game = f._temp.path().join("linked-game");
    crate::core::test_support::directory_link(&f.source, &linked_game);
    assert!(source_present(&linked_game).is_err());
}

#[test]
fn foreign_directory_preflight_failure_leaves_originals_and_no_staging_leak() {
    let f = Fixture::new();
    fs::create_dir(f.game.join("mods/foreign-directory")).unwrap();
    fs::write(f.game.join("mods/foreign-directory/notes.txt"), b"keep").unwrap();
    let (revision, generation) = f.revision();
    assert!(checkout(&f.repo, &revision, &generation, &f.game).is_err());
    assert_eq!(
        fs::read(f.game.join("mods/foreign-directory/notes.txt")).unwrap(),
        b"keep"
    );
    assert!(!f.game.join(JOURNAL).exists());
    assert!(!fs::read_dir(&f.game).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".hikyou-mod-stage-")
    }));
}

#[test]
fn uppercase_artifacts_are_staged_and_provider_number_is_restored() {
    let f = Fixture::new();
    fs::write(f.source.join("mods/new.JAR"), b"mod").unwrap();
    fs::write(f.source.join("mods/old.JAR.DISABLED"), b"disabled").unwrap();
    fs::write(f.source.join("mods/.mod_meta.json"), br#"{"new.JAR":{"version_number":"distribution-label","version_id":"remote-id","hashes":{"sha1":"abcdef"}}}"#).unwrap();
    let paths = LauncherPaths::new(f._temp.path().join("data"));
    let working = stage(&paths, &f.source).unwrap();
    assert!(working.path().join("mods/new.JAR").is_file());
    assert!(working.path().join("mods/old.JAR.DISABLED").is_file());
    let revision = f
        .repo
        .capture(
            PROFILE,
            &f.source,
            ModTarget {
                mc_version: "1.21.1".into(),
                loader: "vanilla".into(),
                loader_version: None,
            },
            None,
        )
        .unwrap();
    let generation = f.repo.materialize(&revision).unwrap();
    checkout(&f.repo, &revision, &generation, &f.game).unwrap();
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(f.game.join("mods/.mod_meta.json")).unwrap()).unwrap();
    assert_eq!(metadata["new.JAR"]["version_number"], "distribution-label");
    assert_eq!(metadata["new.JAR"]["version_id"], "remote-id");
    assert_eq!(metadata["new.JAR"]["hashes"]["sha1"], "abcdef");
}
