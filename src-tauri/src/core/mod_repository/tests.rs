use super::*;
use crate::core::test_support::{directory_link, physical_tempdir, zip};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    game: PathBuf,
    repo: ModRepository,
}

impl Fixture {
    fn new() -> Self {
        let temp = physical_tempdir();
        let root = temp.path().join("launcher");
        let game = temp.path().join("game");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        Self {
            repo: ModRepository::new(&root),
            root,
            game,
            _temp: temp,
        }
    }

    fn put(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.game.join("mods").join(name), bytes).unwrap();
    }

    fn capture(&self, parent: Option<&str>) -> ModRevision {
        self.repo
            .capture("smart:latest-plus", &self.game, target(), parent)
            .unwrap()
    }

    fn object(&self, revision: &ModRevision, index: usize) -> PathBuf {
        self.root
            .join(OBJECTS)
            .join(&revision.mods[index].object_id)
    }

    fn generation(&self, revision: &ModRevision) -> PathBuf {
        self.root
            .join(STATE)
            .join("generation-heads")
            .join(format!("{}.json", revision.id))
    }
}

fn target() -> ModTarget {
    ModTarget {
        mc_version: "1.21.1".into(),
        loader: "fabric".into(),
        loader_version: Some("0.16.0".into()),
    }
}

#[cfg(unix)]
#[test]
fn directory_sync_uses_the_held_directory_after_rename() {
    let temp = physical_tempdir();
    let original = temp.path().join("original");
    let moved = temp.path().join("moved");
    std::fs::create_dir(&original).unwrap();
    let dir = open_root(&original, false).unwrap();
    std::fs::rename(&original, &moved).unwrap();
    std::fs::write(&original, b"not a directory").unwrap();
    sync_dir(&dir).unwrap();
    child_dir(&dir, "child", true).unwrap();
    assert!(moved.join("child").is_dir());
    assert_eq!(std::fs::read(&original).unwrap(), b"not a directory");
}

#[test]
fn repository_roots_reject_linked_ancestors_without_creating_outside_files() {
    let temp = physical_tempdir();
    let outside = temp.path().join("outside");
    let alias = temp.path().join("alias");
    std::fs::create_dir(&outside).unwrap();
    directory_link(&outside, &alias);
    assert!(open_root(&alias.join("repository"), true).is_err());
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn intent_changes_are_new_revisions_even_when_jar_bytes_are_identical() {
    let f = Fixture::new();
    f.put("fixture.jar", b"unchanged bytes");
    let legacy = f.capture(None);
    let first = f
        .repo
        .capture_with_intent(
            "smart:latest-plus",
            &f.game,
            target(),
            Some(&legacy.id),
            Some(&"a".repeat(64)),
        )
        .unwrap();
    let second = f
        .repo
        .capture_with_intent(
            "smart:latest-plus",
            &f.game,
            target(),
            Some(&first.id),
            Some(&"b".repeat(64)),
        )
        .unwrap();
    assert_ne!(legacy.id, first.id);
    assert_ne!(first.id, second.id);
    assert_eq!(legacy.mods, second.mods);
    assert_eq!(f.repo.load(&legacy.id).unwrap(), legacy);
}

#[test]
fn deduplicates_bytes_and_same_composition_even_with_parent() {
    let f = Fixture::new();
    f.put("b.jar.disabled", b"same bytes");
    f.put("a.jar", b"same bytes");
    f.put("notes.txt", b"not a mod, leave alone");
    let first = f.capture(None);
    assert_eq!(first.mods.len(), 2);
    assert_eq!(first.mods[0].filename, "a.jar");
    assert!(first.mods[0].enabled);
    assert!(!first.mods[1].enabled);
    assert_eq!(first.mods[0].object_id, first.mods[1].object_id);
    assert_eq!(std::fs::read_dir(f.root.join(OBJECTS)).unwrap().count(), 1);
    assert!(f.repo.head("smart:latest-plus").unwrap().is_none());
    let path = f.repo.materialize(&first).unwrap();
    assert_ne!(path, f.game.join("mods"));
    f.repo.publish("smart:latest-plus", &first.id).unwrap();
    assert_eq!(f.capture(Some(&first.id)), first);
    assert_eq!(f.capture(None), first);
    assert_eq!(
        f.repo.history("smart:latest-plus").unwrap(),
        vec![first.clone()]
    );
    assert_eq!(f.repo.materialize(&first).unwrap(), path);
    assert_eq!(
        std::fs::read(f.game.join("mods/notes.txt")).unwrap(),
        b"not a mod, leave alone"
    );
    assert!(!path.join("notes.txt").exists());
    f.repo.quick_check(&first).unwrap();
    f.repo.verify(&first).unwrap();
    // Source edits cannot mutate the stored object or any prior generation.
    f.put("a.jar", b"new source bytes");
    assert_eq!(std::fs::read(f.object(&first, 0)).unwrap(), b"same bytes");
    assert_eq!(std::fs::read(path.join("a.jar")).unwrap(), b"same bytes");
}

#[test]
fn versions_create_parented_history_and_publish_supports_rollback() {
    let f = Fixture::new();
    f.put("mod.jar", b"v1");
    let first = f.capture(None);
    f.repo.materialize(&first).unwrap();
    f.repo.publish("smart:latest-plus", &first.id).unwrap();
    f.put("mod.jar", b"v2");
    let second = f.capture(Some(&first.id));
    assert_ne!(first.id, second.id);
    assert_eq!(second.parent.as_deref(), Some(first.id.as_str()));
    assert_ne!(first.mods[0].object_id, second.mods[0].object_id);
    assert_eq!(f.repo.head("smart:latest-plus").unwrap().unwrap(), first);
    assert!(f.repo.publish("smart:latest-plus", &second.id).is_err());
    assert_eq!(f.repo.head("smart:latest-plus").unwrap().unwrap(), first);
    f.repo.materialize(&second).unwrap();
    f.repo.publish("smart:latest-plus", &second.id).unwrap();
    assert_eq!(f.repo.head("smart:latest-plus").unwrap().unwrap(), second);
    f.repo.publish("smart:latest-plus", &first.id).unwrap();
    assert_eq!(f.repo.head("smart:latest-plus").unwrap().unwrap(), first);
    let history = f.repo.history("smart:latest-plus").unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.contains(&first) && history.contains(&second));
    assert_eq!(f.repo.load(&second.id).unwrap(), second);
}

#[test]
fn failed_publish_keeps_previous_head_and_no_partial_generation_is_visible() {
    let f = Fixture::new();
    f.put("mod.jar", b"good");
    let first = f.capture(None);
    f.repo.materialize(&first).unwrap();
    f.repo.publish("smart:latest-plus", &first.id).unwrap();
    f.put("mod.jar", b"next");
    let second = f.capture(Some(&first.id));
    std::fs::write(f.object(&second, 0), b"broken").unwrap();
    assert!(f.repo.materialize(&second).is_err());
    assert!(!f.generation(&second).exists());
    assert!(f.repo.publish("smart:latest-plus", &second.id).is_err());
    assert_eq!(f.repo.head("smart:latest-plus").unwrap(), Some(first));
    assert!(
        std::fs::read_dir(f.root.join(STATE).join("generations"))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".staging-"))
    );
}

#[test]
fn generation_enumeration_rejects_extra_missing_and_truncated_files() {
    for mode in ["extra", "missing", "truncate", "same-size"] {
        let f = Fixture::new();
        f.put("mod.jar", b"original");
        let revision = f.capture(None);
        let path = f.repo.materialize(&revision).unwrap();
        match mode {
            "extra" => std::fs::write(path.join("other.jar"), b"extra").unwrap(),
            "missing" => std::fs::remove_file(path.join("mod.jar")).unwrap(),
            "truncate" => std::fs::write(path.join("mod.jar"), b"x").unwrap(),
            _ => std::fs::write(path.join("mod.jar"), b"modified").unwrap(),
        }
        assert!(f.repo.quick_check(&revision).is_err(), "accepted {mode}");
        assert!(f.repo.verify(&revision).is_err(), "accepted {mode}");
        assert!(f.repo.publish("smart:latest-plus", &revision.id).is_err());
    }
}

#[test]
fn store_missing_truncated_and_corrupt_bytes_are_not_reused() {
    for mode in ["missing", "truncate", "same-size"] {
        let f = Fixture::new();
        f.put("mod.jar", b"original");
        let revision = f.capture(None);
        f.repo.materialize(&revision).unwrap();
        match mode {
            "missing" => std::fs::remove_file(f.object(&revision, 0)).unwrap(),
            "truncate" => std::fs::write(f.object(&revision, 0), b"x").unwrap(),
            _ => std::fs::write(f.object(&revision, 0), b"modified").unwrap(),
        }
        assert!(f.repo.quick_check(&revision).is_err());
        assert!(f.repo.verify(&revision).is_err());
        assert!(f.repo.materialize(&revision).is_err());
    }
}

#[test]
fn deep_verification_hashes_even_if_receipt_has_current_metadata() {
    let f = Fixture::new();
    f.put("mod.jar", b"original");
    let revision = f.capture(None);
    let path = f.repo.materialize(&revision).unwrap();
    std::fs::write(path.join("mod.jar"), b"modified").unwrap();
    let receipt_path = path.parent().unwrap().join("receipt.json");
    let mut receipt: Receipt =
        serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
    receipt.files[0].generation = stamp(&File::open(path.join("mod.jar")).unwrap()).unwrap();
    std::fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    // Cheap receipts are not cryptographic authentication.
    f.repo.quick_check(&revision).unwrap();
    assert!(f.repo.verify(&revision).is_err());
    assert!(f.repo.publish("smart:latest-plus", &revision.id).is_err());
}

#[test]
fn abandoned_staging_and_capture_files_never_become_head_or_history() {
    let f = Fixture::new();
    f.put("mod.jar", b"original");
    let first = f.capture(None);
    f.repo.materialize(&first).unwrap();
    f.repo.publish("smart:latest-plus", &first.id).unwrap();
    let abandoned = f
        .root
        .join(STATE)
        .join("generations/.staging-abandoned/mods");
    std::fs::create_dir_all(&abandoned).unwrap();
    std::fs::write(abandoned.join("partial.jar"), b"partial").unwrap();
    std::fs::write(f.root.join(OBJECTS).join(".capture-abandoned"), b"partial").unwrap();
    assert_eq!(
        f.repo.history("smart:latest-plus").unwrap(),
        vec![first.clone()]
    );
    assert_eq!(
        f.repo.head("smart:latest-plus").unwrap(),
        Some(first.clone())
    );
    f.repo.quick_check(&first).unwrap();
    assert_eq!(f.capture(Some(&first.id)), first);
}

#[test]
fn captures_fabric_and_optional_provider_metadata_without_storing_user_paths() {
    let f = Fixture::new();
    let jar = zip(
        &[(
            "fabric.mod.json",
            br#"{"id":"local_mod","version":"2.0","name":"Local Mod"}"#,
        )],
        zip::CompressionMethod::Deflated,
    );
    f.put("local.jar", &jar);
    f.put("old.jar.disabled", &jar);
    f.put(".mod_meta.json", br#"{
        "local.jar":{"display_name":"Cached title","icon_url":"https://example.test/icon.png","project_id":"project","version_id":"provider-build","version_number":"2.0-release","hashes":{"sha512":"untrusted"}},
        "old.jar":{"project_id":"legacy","display_name":"/private/user/path","icon_url":"file:///private/icon.png"}
    }"#);
    let revision = f.capture(None);
    let local = &revision.mods[0];
    assert_eq!(local.mod_id.as_deref(), Some("local_mod"));
    assert_eq!(local.version.as_deref(), Some("2.0"));
    assert_eq!(
        local.provider_version_number.as_deref(),
        Some("2.0-release")
    );
    assert_eq!(local.provider_version.as_deref(), Some("provider-build"));
    assert_eq!(local.project_id.as_deref(), Some("project"));
    assert_eq!(local.display_name.as_deref(), Some("Cached title"));
    assert_eq!(local.hashes.get("sha512").map(String::as_str), None);
    assert_eq!(revision.mods[1].version.as_deref(), Some("2.0"));
    assert_eq!(revision.mods[1].project_id.as_deref(), Some("legacy"));
    let json = serde_json::to_string(&revision).unwrap();
    assert!(json.contains("objectId") && json.contains("providerVersion"));
    assert!(!json.contains("/private") && !json.contains("gameDir"));
}

#[test]
fn malformed_and_oversized_embedded_metadata_does_not_prevent_local_import() {
    let f = Fixture::new();
    f.put("broken.jar", b"not a zip; preserve local bytes");
    let huge = vec![b'x'; MAX_FABRIC_JSON as usize + 1];
    f.put(
        "huge.jar",
        &zip(
            &[("fabric.mod.json", &huge)],
            zip::CompressionMethod::Deflated,
        ),
    );
    let revision = f.capture(None);
    assert!(revision.mods.iter().all(|entry| entry.mod_id.is_none()));
    f.repo.materialize(&revision).unwrap();
    f.repo.verify(&revision).unwrap();
}

#[test]
fn missing_source_bad_cache_and_foreign_parent_are_errors() {
    let f = Fixture::new();
    assert!(
        f.repo
            .capture("smart:latest-plus", &f.game.join("missing"), target(), None)
            .is_err()
    );
    std::fs::remove_dir(f.game.join("mods")).unwrap();
    assert!(
        f.repo
            .capture("smart:latest-plus", &f.game, target(), None)
            .is_err()
    );
    std::fs::create_dir(f.game.join("mods")).unwrap();
    f.put(".mod_meta.json", b"{");
    assert!(
        f.repo
            .capture("smart:latest-plus", &f.game, target(), None)
            .is_err()
    );
    std::fs::remove_file(f.game.join("mods/.mod_meta.json")).unwrap();
    let revision = f.capture(None);
    assert!(
        f.repo
            .capture("smart:snapshot-plus", &f.game, target(), Some(&revision.id))
            .is_err()
    );
    f.repo.materialize(&revision).unwrap();
    assert!(f.repo.publish("smart:snapshot-plus", &revision.id).is_err());
}

#[test]
fn malicious_ids_filenames_and_casefold_collisions_are_rejected() {
    let f = Fixture::new();
    for id in [
        "",
        "../outside",
        "/absolute",
        "C:\\escape",
        "AABB",
        &"A".repeat(64),
    ] {
        assert!(f.repo.load(id).is_err());
        assert!(f.repo.publish("smart:latest-plus", id).is_err());
    }
    for filename in [
        "../a.jar",
        "a/b.jar",
        "C:a.jar",
        "CON.jar",
        "com1.jar",
        "LPT\u{b9}.jar",
        "a.jar.",
        "a.jar ",
        "a\n.jar",
        ".jar",
        "a?.jar",
    ] {
        assert!(
            validate_filename(filename).is_err(),
            "accepted {filename:?}"
        );
    }
    for profile in ["../profile", "C:profile", "\nprofile"] {
        assert!(f.repo.capture(profile, &f.game, target(), None).is_err());
    }
    let mut revision = f.capture(None);
    let object_id = format!("{:x}", Sha256::digest(b"x"));
    let base = ModEntry {
        filename: "A.jar".into(),
        object_id,
        size_bytes: 1,
        enabled: true,
        mod_id: None,
        version: None,
        project_id: None,
        provider_version: None,
        provider_version_number: None,
        display_name: None,
        icon_url: None,
        hashes: BTreeMap::new(),
    };
    let mut other = base.clone();
    other.filename = "a.jar".into();
    revision.mods = vec![base, other];
    revision.id = revision_id(&revision).unwrap();
    assert!(validate_revision(&revision).is_err());
    assert!(f.repo.materialize(&revision).is_err());
}

#[test]
fn directory_links_and_hardlinks_are_rejected_without_outside_writes() {
    let f = Fixture::new();
    let outside = f._temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    directory_link(&outside, &f.game.join("mods/linked"));
    assert!(
        f.repo
            .capture("smart:latest-plus", &f.game, target(), None)
            .is_err()
    );
    std::fs::remove_dir(f.game.join("mods/linked")).unwrap_or_else(|_| {
        std::fs::remove_file(f.game.join("mods/linked")).unwrap();
    });
    std::fs::write(outside.join("source.jar"), b"outside").unwrap();
    std::fs::hard_link(outside.join("source.jar"), f.game.join("mods/hard.jar")).unwrap();
    assert!(
        f.repo
            .capture("smart:latest-plus", &f.game, target(), None)
            .is_err()
    );
    std::fs::remove_file(f.game.join("mods/hard.jar")).unwrap();
    std::fs::create_dir_all(f.root.join("meta")).unwrap();
    std::fs::remove_dir(f.root.join(OBJECTS)).unwrap();
    directory_link(&outside, &f.root.join(OBJECTS));
    assert!(f.repo.history("smart:latest-plus").is_err());
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    assert_eq!(
        std::fs::read(outside.join("source.jar")).unwrap(),
        b"outside"
    );
}

#[test]
fn linked_generation_or_replaced_generation_file_is_rejected() {
    let f = Fixture::new();
    f.put("mod.jar", b"original");
    let revision = f.capture(None);
    let path = f.repo.materialize(&revision).unwrap();
    std::fs::remove_file(path.join("mod.jar")).unwrap();
    std::fs::hard_link(f.object(&revision, 0), path.join("mod.jar")).unwrap();
    assert!(f.repo.quick_check(&revision).is_err());
    assert!(f.repo.verify(&revision).is_err());
    std::fs::remove_file(path.join("mod.jar")).unwrap();
    std::fs::remove_dir(&path).unwrap();
    directory_link(&f.game.join("mods"), &path);
    assert!(f.repo.quick_check(&revision).is_err());
    let repaired = f.repo.materialize(&revision).unwrap();
    assert_ne!(repaired, path);
    f.repo.verify(&revision).unwrap();
    assert_eq!(
        std::fs::read(f.game.join("mods/mod.jar")).unwrap(),
        b"original"
    );
}

#[test]
fn head_persistence_failure_does_not_replace_previous_commit() {
    let f = Fixture::new();
    let dir = open_root(&f.root, true).unwrap();
    let heads = child_dir(&dir, "heads-test", true).unwrap();
    let first = Head {
        profile_id: "smart:latest-plus".into(),
        revision_id: "1".repeat(64),
    };
    atomic_head(&heads, "head.json", &first).unwrap();
    // A directory cannot be atomically replaced by the head file.
    child_dir(&heads, "blocked", true).unwrap();
    let second = Head {
        profile_id: "smart:latest-plus".into(),
        revision_id: "2".repeat(64),
    };
    assert!(atomic_head(&heads, "blocked", &second).is_err());
    let saved: Head = read_json(&heads, "head.json").unwrap();
    assert_eq!(saved.revision_id, first.revision_id);
    atomic_head(&heads, "head.json", &second).unwrap();
    assert_eq!(
        read_json::<Head>(&heads, "head.json").unwrap().revision_id,
        second.revision_id
    );
}

#[test]
fn provider_hashes_are_preserved_with_legacy_default_and_corrupt_cas_can_be_recovered() {
    let f = Fixture::new();
    f.put("mod.jar", b"original");
    f.put(
        ".mod_meta.json",
        br#"{"mod.jar":{"hashes":{"sha512":"Ab12ef","sha1":"0123abcd"}}}"#,
    );
    let first = f.capture(None);
    assert_eq!(first.mods[0].hashes.get("sha512").unwrap(), "Ab12ef");
    let mut legacy = serde_json::to_value(&first.mods[0]).unwrap();
    legacy.as_object_mut().unwrap().remove("hashes");
    assert!(
        serde_json::from_value::<ModEntry>(legacy)
            .unwrap()
            .hashes
            .is_empty()
    );
    std::fs::write(f.object(&first, 0), b"corrupt").unwrap();
    let recovered = f.capture(Some(&first.id));
    assert_eq!(recovered, first);
    assert_eq!(std::fs::read(f.object(&first, 0)).unwrap(), b"original");
    f.repo.materialize(&first).unwrap();
    f.repo.verify(&first).unwrap();
}

#[test]
fn only_completed_generations_appear_in_history_and_corrupt_generation_is_rebuilt() {
    let f = Fixture::new();
    f.put("mod.jar", b"original");
    let first = f.capture(None);
    assert!(f.repo.history("smart:latest-plus").unwrap().is_empty());
    let old = f.repo.materialize(&first).unwrap();
    assert_eq!(
        f.repo.history("smart:latest-plus").unwrap(),
        vec![first.clone()]
    );
    f.repo.publish("smart:latest-plus", &first.id).unwrap();
    std::fs::write(old.join("mod.jar"), b"corrupt").unwrap();
    assert!(f.repo.history("smart:latest-plus").unwrap().is_empty());
    let repaired = f.repo.materialize(&first).unwrap();
    assert_ne!(old, repaired);
    assert_eq!(std::fs::read(old.join("mod.jar")).unwrap(), b"corrupt");
    assert_eq!(
        std::fs::read(repaired.join("mod.jar")).unwrap(),
        b"original"
    );
    assert_eq!(f.repo.quick_check(&first).unwrap(), repaired);
    f.repo.verify(&first).unwrap();
    assert_eq!(
        f.repo.head("smart:latest-plus").unwrap(),
        Some(first.clone())
    );
    assert_eq!(f.repo.history("smart:latest-plus").unwrap(), vec![first]);
}

#[test]
fn launch_reads_do_not_wait_for_background_writer_lock() {
    use std::{sync::mpsc, thread, time::Duration};
    let f = Fixture::new();
    f.put("mod.jar", b"original");
    let revision = f.capture(None);
    f.repo.materialize(&revision).unwrap();
    f.repo.publish("smart:latest-plus", &revision.id).unwrap();
    let writer_repo = f.repo.clone();
    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let writer = thread::spawn(move || {
        let guard = writer_repo.open_writer().unwrap();
        let mut staging = create_file(&guard.objects, ".capture-concurrency").unwrap();
        staging.write_all(&[0_u8; 64 * 1024]).unwrap();
        locked_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    locked_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let reader_repo = f.repo.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let head = reader_repo.head("smart:latest-plus").unwrap().unwrap();
        assert_eq!(head, revision);
        reader_repo.quick_check(&head).unwrap();
        reader_repo.load(&head.id).unwrap();
        assert_eq!(
            reader_repo.history("smart:latest-plus").unwrap(),
            vec![head]
        );
        done_tx.send(()).unwrap();
    });
    let completed = done_rx.recv_timeout(Duration::from_secs(1));
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    reader.join().unwrap();
    assert!(
        completed.is_ok(),
        "foreground reads waited for a maintenance writer"
    );
}

#[test]
fn accepts_actual_smart_and_uuid_refs_but_not_arbitrary_colon_refs() {
    for profile in [
        "smart:latest-plus",
        "smart:snapshot-plus",
        "6fe27210-c843-4d2b-a840-1b20943e5081",
    ] {
        let f = Fixture::new();
        let revision = f.repo.capture(profile, &f.game, target(), None).unwrap();
        f.repo.materialize(&revision).unwrap();
        f.repo.publish(profile, &revision.id).unwrap();
        assert_eq!(f.repo.head(profile).unwrap(), Some(revision));
    }
    for invalid in [
        "profile",
        "smart:other",
        "smart:../latest-plus",
        "smart:latest-plus/../x",
    ] {
        assert!(validate_profile(invalid).is_err());
    }
}

#[test]
fn repairs_missing_generation_and_missing_or_invalid_receipt_using_new_paths() {
    for mode in [
        "missing-generation",
        "non-directory-generation",
        "missing-receipt",
        "bad-receipt",
    ] {
        let f = Fixture::new();
        f.put("mod.jar", b"original");
        let revision = f.capture(None);
        let old = f.repo.materialize(&revision).unwrap();
        let generation = old.parent().unwrap();
        match mode {
            "missing-generation" => std::fs::remove_dir_all(generation).unwrap(),
            "non-directory-generation" => {
                std::fs::remove_dir_all(generation).unwrap();
                std::fs::write(generation, b"not a directory").unwrap();
            }
            "missing-receipt" => std::fs::remove_file(generation.join("receipt.json")).unwrap(),
            _ => std::fs::write(generation.join("receipt.json"), b"{partial").unwrap(),
        }
        assert!(f.repo.quick_check(&revision).is_err(), "accepted {mode}");
        assert!(f.repo.history("smart:latest-plus").unwrap().is_empty());
        let repaired = f.repo.materialize(&revision).unwrap();
        assert_ne!(repaired, old);
        assert_eq!(
            std::fs::read(repaired.join("mod.jar")).unwrap(),
            b"original"
        );
        assert_eq!(f.repo.quick_check(&revision).unwrap(), repaired);
        f.repo.verify(&revision).unwrap();
    }
}
