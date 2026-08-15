use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("elio-{label}-{unique}"))
}

fn cleanup_restore_origins_test_store(path: &Path) {
    fs::remove_file(path).ok();
    if let Ok(lock_path) = restore_origins_lock_path(path) {
        fs::remove_file(lock_path).ok();
    }
}

/// Builds a minimal FreeDesktop trash layout under `root`:
///   root/
///     files/<name>  ← the trashed item (a regular file)
///     info/<name>.trashinfo
///
/// Returns `(trash_files_dir, trash_info_dir, item_path)`.
#[cfg(unix)]
fn make_freedesktop_trash(root: &Path, name: &str, original: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let files_dir = root.join("files");
    let info_dir = root.join("info");
    fs::create_dir_all(&files_dir).expect("failed to create trash files dir");
    fs::create_dir_all(&info_dir).expect("failed to create trash info dir");
    let item_path = files_dir.join(name);
    fs::write(&item_path, b"trashed content").expect("failed to write trashed item");
    let trashinfo = format!(
        "[Trash Info]\nPath={}\nDeletionDate=2024-01-01T00:00:00\n",
        original.to_str().unwrap()
    );
    fs::write(info_dir.join(format!("{name}.trashinfo")), trashinfo)
        .expect("failed to write trashinfo");
    (files_dir, info_dir, item_path)
}

#[test]
#[cfg(unix)]
fn restore_freedesktop_moves_item_to_original_path_and_removes_trashinfo() {
    let root = temp_path("restore-fd-ok");
    let restore_target = temp_path("restore-fd-ok-dest");
    fs::create_dir_all(&root).expect("failed to create trash root");
    fs::create_dir_all(&restore_target).expect("failed to create restore target dir");

    let original = restore_target.join("report.pdf");
    let (_, info_dir, item_path) = make_freedesktop_trash(&root, "report.pdf", &original);

    let result = restore_trash_item(&item_path);
    assert!(result.is_ok(), "restore should succeed: {:?}", result);
    assert!(original.exists(), "file should be at original location");
    assert!(!item_path.exists(), "trashed item should be gone");
    assert!(
        !info_dir.join("report.pdf.trashinfo").exists(),
        "trashinfo should be removed"
    );

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&restore_target).ok();
}

#[test]
#[cfg(unix)]
fn restore_freedesktop_fails_when_destination_already_exists() {
    let root = temp_path("restore-fd-conflict");
    let restore_target = temp_path("restore-fd-conflict-dest");
    fs::create_dir_all(&root).expect("failed to create trash root");
    fs::create_dir_all(&restore_target).expect("failed to create restore target dir");

    let original = restore_target.join("conflict.txt");
    fs::write(&original, b"already here").expect("failed to write blocking file");

    let (_, _, item_path) = make_freedesktop_trash(&root, "conflict.txt", &original);

    let err = restore_trash_item(&item_path).unwrap_err();
    assert!(
        err.to_string().contains("destination already exists"),
        "unexpected error: {err}"
    );

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&restore_target).ok();
}

#[test]
#[cfg(unix)]
fn restore_freedesktop_fails_when_trashinfo_is_missing() {
    let root = temp_path("restore-fd-no-info");
    let files_dir = root.join("files");
    let info_dir = root.join("info");
    fs::create_dir_all(&files_dir).expect("failed to create files dir");
    fs::create_dir_all(&info_dir).expect("failed to create info dir");

    let item_path = files_dir.join("orphan.txt");
    fs::write(&item_path, b"no metadata").expect("failed to write orphan item");
    // Deliberately do NOT write a .trashinfo file.

    let err = restore_trash_item(&item_path).unwrap_err();
    assert!(
        err.to_string().contains("orphan.txt.trashinfo"),
        "error should mention the missing trashinfo, got: {err}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn restore_fails_for_path_outside_any_known_trash_layout() {
    let tmp = temp_path("restore-unsupported");
    fs::create_dir_all(&tmp).expect("failed to create temp dir");
    let fake_item = tmp.join("item.txt");
    fs::write(&fake_item, b"content").expect("failed to write item");

    #[cfg(not(target_os = "macos"))]
    {
        let err = restore_trash_item(&fake_item).unwrap_err();
        assert!(
            err.to_string().contains("not supported"),
            "unexpected error: {err}"
        );
    }

    fs::remove_dir_all(&tmp).ok();
}

/// Regression test for false-positive FreeDesktop detection.
///
/// On macOS, `~/.Trash/foo` computes `~/info` as the candidate info dir.
/// If the user happens to have a `~/info` directory, the old code would
/// take the FreeDesktop path and then fail looking for a `.trashinfo` file
/// instead of falling through to the Finder backend.
///
/// The fix requires the entry's immediate parent to be named `files` before
/// treating the layout as FreeDesktop, so a `~/.Trash`-style path is never
/// misidentified even when a coincidental `info/` exists nearby.
#[test]
#[cfg(not(target_os = "macos"))]
fn restore_does_not_misdetect_freedesktop_when_info_dir_exists_at_wrong_level() {
    let root = temp_path("restore-false-positive");
    let trash_dir = root.join("Trash");
    let decoy_info = root.join("info");
    fs::create_dir_all(&trash_dir).expect("failed to create trash dir");
    fs::create_dir_all(&decoy_info).expect("failed to create decoy info dir");

    let item_path = trash_dir.join("foo.txt");
    fs::write(&item_path, b"content").expect("failed to write item");

    let err = restore_trash_item(&item_path).unwrap_err();
    assert!(
        err.to_string().contains("not supported"),
        "should bail as unsupported, not attempt FreeDesktop restore: {err}"
    );

    fs::remove_dir_all(&root).ok();
}

// ── macOS DS_Store restore helpers ────────────────────────────────────────

#[test]
#[cfg(target_os = "macos")]
fn decode_utf16be_decodes_ascii_string() {
    let bytes = b"\x00H\x00i";
    assert_eq!(decode_utf16be(bytes), Some("Hi".to_string()));
}

#[test]
#[cfg(target_os = "macos")]
fn decode_utf16be_decodes_non_ascii() {
    let bytes = b"\x00\xe9";
    assert_eq!(decode_utf16be(bytes), Some("é".to_string()));
}

#[test]
#[cfg(target_os = "macos")]
fn decode_utf16be_rejects_odd_byte_count() {
    assert_eq!(decode_utf16be(b"\x00H\x00"), None);
}

#[test]
#[cfg(target_os = "macos")]
fn decode_utf16be_empty_slice_gives_empty_string() {
    assert_eq!(decode_utf16be(b""), Some(String::new()));
}

// ── remove_from_origins_map ───────────────────────────────────────────────

#[test]
fn remove_from_origins_map_removes_exact_match() {
    let mut map = std::collections::HashMap::from([
        (
            "report.pdf".to_string(),
            "/home/user/report.pdf".to_string(),
        ),
        ("notes.txt".to_string(), "/home/user/notes.txt".to_string()),
    ]);
    let changed = remove_from_origins_map(&mut map, &["report.pdf"]);
    assert!(changed);
    assert!(
        !map.contains_key("report.pdf"),
        "target entry should be removed"
    );
    assert!(
        map.contains_key("notes.txt"),
        "unrelated entry should be untouched"
    );
}

#[test]
fn remove_from_origins_map_removes_only_exact_timestamped_collision_key() {
    let mut map = std::collections::HashMap::from([
        (
            "report.pdf".to_string(),
            "/Users/paco/A/report.pdf".to_string(),
        ),
        (
            "report 11.53.48.pdf".to_string(),
            "/Users/paco/B/report.pdf".to_string(),
        ),
    ]);

    let changed = remove_from_origins_map(&mut map, &["report 11.53.48.pdf"]);

    assert!(changed);
    assert_eq!(
        map.get("report.pdf").map(String::as_str),
        Some("/Users/paco/A/report.pdf")
    );
    assert!(!map.contains_key("report 11.53.48.pdf"));
}

#[test]
fn remove_from_origins_map_never_infers_a_base_name() {
    let mut map = std::collections::HashMap::from([(
        "report.pdf".to_string(),
        "/Users/paco/A/report.pdf".to_string(),
    )]);

    let changed = remove_from_origins_map(&mut map, &["report 2.pdf"]);

    assert!(!changed);
    assert_eq!(map.len(), 1);
}

#[test]
fn remove_from_origins_map_returns_false_when_key_not_found() {
    let mut map = std::collections::HashMap::from([(
        "other.txt".to_string(),
        "/home/user/other.txt".to_string(),
    )]);
    let changed = remove_from_origins_map(&mut map, &["missing.txt"]);
    assert!(
        !changed,
        "no match should return false and leave map untouched"
    );
    assert_eq!(map.len(), 1);
}

#[test]
fn remove_from_origins_map_removes_multiple_names() {
    let mut map = std::collections::HashMap::from([
        ("a.txt".to_string(), "/home/user/a.txt".to_string()),
        ("b.txt".to_string(), "/home/user/b.txt".to_string()),
        ("c.txt".to_string(), "/home/user/c.txt".to_string()),
    ]);
    let changed = remove_from_origins_map(&mut map, &["a.txt", "c.txt"]);
    assert!(changed);
    assert!(!map.contains_key("a.txt"));
    assert!(map.contains_key("b.txt"), "untargeted entry must survive");
    assert!(!map.contains_key("c.txt"));
}

#[test]
fn remove_from_origins_map_no_op_on_empty_map() {
    let mut map = std::collections::HashMap::new();
    let changed = remove_from_origins_map(&mut map, &["foo.txt"]);
    assert!(!changed);
}

#[test]
fn restore_origin_lookup_uses_exact_finder_assigned_names() {
    let map = std::collections::HashMap::from([
        ("foo.txt".to_string(), "/Users/paco/A/foo.txt".to_string()),
        (
            "foo 11.53.48.txt".to_string(),
            "/Users/paco/B/foo.txt".to_string(),
        ),
    ]);

    assert_eq!(
        restore_origin_from_map(&map, "foo.txt"),
        Some(PathBuf::from("/Users/paco/A/foo.txt"))
    );
    assert_eq!(
        restore_origin_from_map(&map, "foo 11.53.48.txt"),
        Some(PathBuf::from("/Users/paco/B/foo.txt"))
    );
    assert_eq!(restore_origin_from_map(&map, "foo 2.txt"), None);
}

#[test]
fn checked_origin_save_preserves_distinct_collision_mappings_and_existing_entries() {
    let path = temp_path("origins-save-collisions");
    fs::write(&path, br#"{"existing.txt":"/Users/paco/existing.txt"}"#)
        .expect("failed to write existing origins store");
    let items = vec![
        (
            "foo.txt".to_string(),
            PathBuf::from("/Users/paco/A/foo.txt"),
        ),
        (
            "foo 11.53.48.txt".to_string(),
            PathBuf::from("/Users/paco/B/foo.txt"),
        ),
    ];

    let rejected = save_restore_origins_at_path_checked(&path, &items)
        .expect("distinct Trash names should be persisted");
    assert!(rejected.is_empty());

    let map: std::collections::HashMap<String, String> =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        map.get("existing.txt").map(String::as_str),
        Some("/Users/paco/existing.txt")
    );
    assert_eq!(
        map.get("foo.txt").map(String::as_str),
        Some("/Users/paco/A/foo.txt")
    );
    assert_eq!(
        map.get("foo 11.53.48.txt").map(String::as_str),
        Some("/Users/paco/B/foo.txt")
    );
    cleanup_restore_origins_test_store(&path);
}

#[cfg(unix)]
#[test]
fn checked_origin_save_keeps_valid_mappings_when_an_origin_is_not_utf8() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let path = temp_path("origins-save-non-utf8");
    fs::write(&path, br#"{"existing.txt":"/Users/paco/existing.txt"}"#)
        .expect("failed to write existing origins store");
    let invalid = PathBuf::from(OsString::from_vec(b"/Users/paco/bad-\xff.txt".to_vec()));
    let items = vec![
        (
            "valid.txt".to_string(),
            PathBuf::from("/Users/paco/valid.txt"),
        ),
        ("invalid.txt".to_string(), invalid.clone()),
    ];

    let rejected = save_restore_origins_at_path_checked(&path, &items)
        .expect("representable mappings should still be saved");

    assert_eq!(rejected, vec![invalid]);
    let map: std::collections::HashMap<String, String> =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        map.get("existing.txt").map(String::as_str),
        Some("/Users/paco/existing.txt")
    );
    assert_eq!(
        map.get("valid.txt").map(String::as_str),
        Some("/Users/paco/valid.txt")
    );
    assert!(!map.contains_key("invalid.txt"));
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn concurrent_origin_saves_preserve_both_updates() {
    use std::{
        sync::mpsc::{self, RecvTimeoutError},
        thread,
        time::Duration,
    };

    let path = temp_path("origins-concurrent-save-save");
    let (first_entered_tx, first_entered_rx) = mpsc::sync_channel(0);
    let (release_first_tx, release_first_rx) = mpsc::sync_channel(0);
    let first_path = path.clone();
    let first = thread::spawn(move || {
        let items = vec![("first.txt".to_string(), PathBuf::from("/A/first.txt"))];
        save_restore_origins_at_path_with(&first_path, &items, |path, json| {
            first_entered_tx.send(()).unwrap();
            release_first_rx.recv().unwrap();
            write_restore_origins_atomically(path, json)
        })
    });
    first_entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first save did not enter persistence");

    let (second_entered_tx, second_entered_rx) = mpsc::sync_channel(0);
    let second_path = path.clone();
    let second = thread::spawn(move || {
        let items = vec![("second.txt".to_string(), PathBuf::from("/B/second.txt"))];
        save_restore_origins_at_path_with(&second_path, &items, |path, json| {
            second_entered_tx.send(()).unwrap();
            write_restore_origins_atomically(path, json)
        })
    });

    assert!(matches!(
        second_entered_rx.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Timeout)
    ));
    release_first_tx.send(()).unwrap();
    assert!(first.join().unwrap().unwrap().is_empty());
    second_entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("second save did not resume after the first transaction");
    assert!(second.join().unwrap().unwrap().is_empty());

    let map: std::collections::HashMap<String, String> =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        map.get("first.txt").map(String::as_str),
        Some("/A/first.txt")
    );
    assert_eq!(
        map.get("second.txt").map(String::as_str),
        Some("/B/second.txt")
    );
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn concurrent_origin_save_and_remove_preserve_unrelated_updates() {
    use std::{
        sync::mpsc::{self, RecvTimeoutError},
        thread,
        time::Duration,
    };

    let path = temp_path("origins-concurrent-save-remove");
    fs::write(
        &path,
        br#"{"remove.txt":"/old/remove.txt","keep.txt":"/old/keep.txt"}"#,
    )
    .unwrap();
    let (save_entered_tx, save_entered_rx) = mpsc::sync_channel(0);
    let (release_save_tx, release_save_rx) = mpsc::sync_channel(0);
    let save_path = path.clone();
    let save = thread::spawn(move || {
        let items = vec![("new.txt".to_string(), PathBuf::from("/new/new.txt"))];
        save_restore_origins_at_path_with(&save_path, &items, |path, json| {
            save_entered_tx.send(()).unwrap();
            release_save_rx.recv().unwrap();
            write_restore_origins_atomically(path, json)
        })
    });
    save_entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("save did not enter persistence");

    let (remove_entered_tx, remove_entered_rx) = mpsc::sync_channel(0);
    let remove_path = path.clone();
    let remove = thread::spawn(move || {
        remove_restore_origins_at_path_with(&remove_path, &["remove.txt"], |path, json| {
            remove_entered_tx.send(()).unwrap();
            write_restore_origins_atomically(path, json)
        })
    });

    assert!(matches!(
        remove_entered_rx.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Timeout)
    ));
    release_save_tx.send(()).unwrap();
    assert!(save.join().unwrap().unwrap().is_empty());
    remove_entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("remove did not resume after the save transaction");
    remove.join().unwrap().unwrap();

    let map: std::collections::HashMap<String, String> =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(!map.contains_key("remove.txt"));
    assert_eq!(
        map.get("keep.txt").map(String::as_str),
        Some("/old/keep.txt")
    );
    assert_eq!(map.get("new.txt").map(String::as_str), Some("/new/new.txt"));
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn restore_origins_sidecar_lock_serializes_independent_file_handles() {
    use std::{
        sync::mpsc::{self, RecvTimeoutError},
        thread,
        time::Duration,
    };

    let path = temp_path("origins-sidecar-lock");
    let first = RestoreOriginsFileLock::acquire(&path).unwrap();
    let (acquired_tx, acquired_rx) = mpsc::sync_channel(0);
    let second_path = path.clone();
    let second = thread::spawn(move || {
        let _second = RestoreOriginsFileLock::acquire(&second_path).unwrap();
        acquired_tx.send(()).unwrap();
    });

    assert!(matches!(
        acquired_rx.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Timeout)
    ));
    drop(first);
    acquired_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("second file handle did not acquire the released sidecar lock");
    second.join().unwrap();
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn checked_origin_save_reports_persistence_failure_without_touching_existing_mappings() {
    let path = temp_path("origins-save-failure");
    let existing = br#"{"existing.txt":"/Users/paco/existing.txt"}"#;
    fs::write(&path, existing).expect("failed to write existing origins store");
    let items = vec![(
        "foo.txt".to_string(),
        PathBuf::from("/Users/paco/A/foo.txt"),
    )];

    let error = save_restore_origins_at_path_with(&path, &items, |_, _| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "store is read-only",
        ))
    })
    .unwrap_err();

    assert!(error.to_string().contains("cannot write"));
    assert_eq!(fs::read(&path).unwrap(), existing);
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn checked_origin_removal_accepts_missing_store() {
    let path = temp_path("origins-missing");
    assert!(remove_restore_origins_at_path_checked(&path, &["foo.txt"]).is_ok());
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn checked_origin_removal_accepts_missing_key_without_rewriting() {
    let path = temp_path("origins-missing-key");
    let contents = br#"{"other.txt":"/Users/paco/other.txt"}"#;
    fs::write(&path, contents).expect("failed to write origins store");

    remove_restore_origins_at_path_checked(&path, &["foo.txt"])
        .expect("missing key should be a successful no-op");

    assert_eq!(fs::read(&path).unwrap(), contents);
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn checked_origin_removal_persists_matching_mutation() {
    let path = temp_path("origins-persist");
    fs::write(
        &path,
        br#"{"report.pdf":"/Users/paco/report.pdf","notes.txt":"/Users/paco/notes.txt"}"#,
    )
    .expect("failed to write origins store");

    remove_restore_origins_at_path_checked(&path, &["report.pdf"])
        .expect("matching key should be removed");

    let map: std::collections::HashMap<String, String> =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(!map.contains_key("report.pdf"));
    assert_eq!(
        map.get("notes.txt").map(String::as_str),
        Some("/Users/paco/notes.txt")
    );
    cleanup_restore_origins_test_store(&path);
}

#[test]
fn checked_origin_removal_rejects_malformed_store() {
    let path = temp_path("origins-malformed");
    fs::write(&path, b"{").expect("failed to write malformed origins store");

    let error = remove_restore_origins_at_path_checked(&path, &["report.pdf"]).unwrap_err();

    assert!(error.to_string().contains("cannot parse"));
    assert_eq!(fs::read(&path).unwrap(), b"{");
    cleanup_restore_origins_test_store(&path);
}

#[cfg(unix)]
#[test]
fn checked_origin_removal_reports_read_failure() {
    let path = temp_path("origins-read-error");
    fs::create_dir(&path).expect("failed to create origins directory");

    let error = remove_restore_origins_at_path_checked(&path, &["report.pdf"]).unwrap_err();

    assert!(error.to_string().contains("cannot read"));
    fs::remove_dir(&path).ok();
    cleanup_restore_origins_test_store(&path);
}

#[cfg(unix)]
#[test]
fn checked_origin_removal_reports_write_failure() {
    use std::os::unix::fs::PermissionsExt;

    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let root = temp_path("origins-write-error");
    fs::create_dir(&root).expect("failed to create origins directory");
    let path = root.join("trash-origins.json");
    fs::write(&path, br#"{"report.pdf":"/Users/paco/report.pdf"}"#)
        .expect("failed to write origins store");
    fs::write(restore_origins_lock_path(&path).unwrap(), b"")
        .expect("failed to create restore-origins lock");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500))
        .expect("failed to make origins directory read-only");

    let error = remove_restore_origins_at_path_checked(&path, &["report.pdf"]).unwrap_err();

    assert!(error.to_string().contains("cannot write"));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).ok();
    fs::remove_dir_all(root).ok();
}
