use super::*;
use std::fs;

/// Creates a unique temporary directory under the system temp dir and
/// returns its path.  The caller is responsible for cleanup (or the OS
/// will reclaim it on reboot).  We avoid a `tempfile` dependency by using
/// a pid+nanos unique name — the same scheme used by `rename_into_staging`.
fn make_tmp_dir(tag: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("elio-test-{tag}-{pid}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(target_os = "linux")]
fn path_refs(paths: &[PathBuf]) -> Vec<&Path> {
    paths.iter().map(|path| path.as_path()).collect()
}

#[test]
fn root_permanent_delete_does_not_stage_in_user_data_dir() {
    let data_dir = PathBuf::from("/home/paco/.local/share");
    assert_eq!(staging_root_for(Some(data_dir), 0), None);
}

#[test]
fn normal_permanent_delete_keeps_existing_staging_root() {
    let data_dir = PathBuf::from("/home/paco/.local/share");
    assert_eq!(
        staging_root_for(Some(data_dir.clone()), 1000),
        Some(data_dir.join("elio/cleanup"))
    );
}

#[test]
fn restore_origin_cleanup_collects_only_names_inside_selected_trash() {
    let trash_dir = Path::new("/Users/paco/.Trash");
    let inside = trash_dir.join("inside.txt");
    let outside = PathBuf::from("/Users/paco/Documents/outside.txt");
    let nested = trash_dir.join("folder/nested.txt");
    let mut names = Vec::new();

    collect_deleted_restore_origin(&inside, Some(trash_dir), &mut names);
    collect_deleted_restore_origin(&outside, Some(trash_dir), &mut names);
    collect_deleted_restore_origin(&nested, Some(trash_dir), &mut names);
    collect_deleted_restore_origin(&inside, None, &mut names);

    assert_eq!(names, vec!["inside.txt".to_string()]);
}

fn trash_identity(device: u64, inode: u64) -> TrashEntryIdentity {
    TrashEntryIdentity { device, inode }
}

#[test]
fn trash_name_correlation_preserves_finder_assigned_collision_names() {
    let original = trash_identity(1, 10);
    let collision = trash_identity(1, 11);
    let before = TrashSnapshot::from([(OsString::from("unrelated.txt"), trash_identity(1, 99))]);
    let after = TrashSnapshot::from([
        (OsString::from("unrelated.txt"), trash_identity(1, 99)),
        (OsString::from("foo.txt"), original),
        (OsString::from("foo 11.53.48.txt"), collision),
    ]);

    assert_eq!(
        correlate_trash_name(&before, &after, original),
        Ok(OsString::from("foo.txt"))
    );
    assert_eq!(
        correlate_trash_name(&before, &after, collision),
        Ok(OsString::from("foo 11.53.48.txt"))
    );
}

#[cfg(unix)]
#[test]
fn trash_snapshot_correlation_tracks_identity_across_renames() {
    let root = make_tmp_dir("trash-snapshot-collision");
    let trash_dir = root.join("Trash");
    let source_a = root.join("A/foo.txt");
    let source_b = root.join("B/foo.txt");
    fs::create_dir_all(source_a.parent().unwrap()).unwrap();
    fs::create_dir_all(source_b.parent().unwrap()).unwrap();
    fs::create_dir(&trash_dir).unwrap();
    fs::write(&source_a, b"from A").unwrap();
    fs::write(&source_b, b"from B").unwrap();
    let identity_a = macos_file_identity(&source_a).unwrap();
    let identity_b = macos_file_identity(&source_b).unwrap();
    let before = macos_trash_snapshot(&trash_dir).unwrap();

    fs::rename(&source_a, trash_dir.join("foo.txt")).unwrap();
    fs::rename(&source_b, trash_dir.join("foo 11.53.48.txt")).unwrap();
    let after = macos_trash_snapshot(&trash_dir).unwrap();

    assert_eq!(
        correlate_trash_name(&before, &after, identity_a),
        Ok(OsString::from("foo.txt"))
    );
    assert_eq!(
        correlate_trash_name(&before, &after, identity_b),
        Ok(OsString::from("foo 11.53.48.txt"))
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn trash_name_correlation_rejects_ambiguous_identity_matches() {
    let identity = trash_identity(1, 10);
    let after = TrashSnapshot::from([
        (OsString::from("foo.txt"), identity),
        (OsString::from("foo 11.53.48.txt"), identity),
    ]);

    assert_eq!(
        correlate_trash_name(&TrashSnapshot::new(), &after, identity),
        Err(TrashNameCorrelationError::Ambiguous(2))
    );
}

#[test]
fn trash_name_correlation_never_reuses_preexisting_names() {
    let identity = trash_identity(1, 10);
    let before = TrashSnapshot::from([(OsString::from("foo.txt"), identity)]);
    let after = before.clone();

    assert_eq!(
        correlate_trash_name(&before, &after, identity),
        Err(TrashNameCorrelationError::Missing)
    );
}

#[test]
fn restore_metadata_warning_preserves_completed_trash_count() {
    assert_eq!(
        finish_macos_trash(1, vec!["store is read-only".to_string()]),
        TrashBatchBackendResult::CompletedWithWarning {
            completed: 1,
            warning: "store is read-only".to_string(),
        }
    );
}

#[test]
fn restore_metadata_warning_is_rendered_as_partial_success() {
    let status = format_restore_metadata_warning("Trashed", 1, &["store is read-only".to_string()]);

    assert_eq!(
        status,
        "Trashed 1 item; restore metadata could not be saved: store is read-only"
    );
    assert!(!status.contains("error"));
}

// ── GIO trash backend ─────────────────────────────────────────────────

#[cfg(target_os = "linux")]
#[test]
fn gio_trash_chunks_split_before_budget_is_exceeded() {
    let paths = vec![
        PathBuf::from("/home/user/a.jpg"),
        PathBuf::from("/home/user/b.jpg"),
        PathBuf::from("/home/user/c.jpg"),
    ];
    let refs = path_refs(&paths);
    let budget =
        GIO_TRASH_COMMAND_OVERHEAD + gio_trash_arg_len(&paths[0]) + gio_trash_arg_len(&paths[1])
            - 1;

    let chunks = gio_trash_chunks_with_budget(&refs, budget);

    assert_eq!(chunks, vec![0..1, 1..2, 2..3]);
}

#[cfg(target_os = "linux")]
#[test]
fn gio_trash_chunks_keep_oversized_path_in_its_own_chunk() {
    let paths = vec![PathBuf::from("/home/user/really-long-name.jpg")];
    let refs = path_refs(&paths);

    let chunks = gio_trash_chunks_with_budget(&refs, GIO_TRASH_COMMAND_OVERHEAD + 1);

    assert_eq!(chunks, vec![0..1]);
}

#[cfg(target_os = "linux")]
#[test]
fn gio_first_falls_back_when_gio_is_unavailable() {
    let root = make_tmp_dir("gio-unavailable-fallback");
    let first = root.join("a.jpg");
    let second = root.join("b.jpg");
    fs::write(&first, b"a").unwrap();
    fs::write(&second, b"b").unwrap();
    let paths = vec![first, second];
    let refs = path_refs(&paths);
    let mut gio_calls = 0usize;
    let mut fallback_paths = Vec::new();

    let result = trash_with_gio_runner(
        &refs,
        |_| {
            gio_calls += 1;
            GioTrashCommandResult::Unavailable
        },
        |paths| {
            fallback_paths = paths.iter().map(|path| path.to_path_buf()).collect();
            Ok(())
        },
    );

    assert_eq!(result, TrashBatchBackendResult::Completed);
    assert_eq!(gio_calls, 1);
    assert_eq!(fallback_paths, paths);
    let _ = fs::remove_dir_all(&root);
}

#[cfg(target_os = "linux")]
#[test]
fn gio_first_falls_back_for_remaining_paths_after_partial_failure() {
    let root = make_tmp_dir("gio-partial-fallback");
    let moved = root.join("moved.jpg");
    let remaining = root.join("remaining.jpg");
    fs::write(&moved, b"moved").unwrap();
    fs::write(&remaining, b"remaining").unwrap();
    let paths = vec![moved.clone(), remaining.clone()];
    let refs = path_refs(&paths);
    let mut fallback_paths = Vec::new();

    let result = trash_with_gio_runner(
        &refs,
        |paths| {
            fs::remove_file(paths[0]).unwrap();
            GioTrashCommandResult::Failed("gio failed on this mount".to_string())
        },
        |paths| {
            fallback_paths = paths.iter().map(|path| path.to_path_buf()).collect();
            for path in paths {
                fs::remove_file(*path).unwrap();
            }
            Ok(())
        },
    );

    assert_eq!(result, TrashBatchBackendResult::Completed);
    assert_eq!(fallback_paths, vec![remaining]);
    assert!(!moved.exists());
    assert!(!paths[1].exists());
    let _ = fs::remove_dir_all(&root);
}

#[cfg(target_os = "linux")]
#[test]
fn gio_first_reports_completed_count_when_fallback_fails() {
    let root = make_tmp_dir("gio-fallback-failure");
    let moved = root.join("moved.jpg");
    let remaining = root.join("remaining.jpg");
    fs::write(&moved, b"moved").unwrap();
    fs::write(&remaining, b"remaining").unwrap();
    let paths = vec![moved.clone(), remaining];
    let refs = path_refs(&paths);

    let result = trash_with_gio_runner(
        &refs,
        |paths| {
            fs::remove_file(paths[0]).unwrap();
            GioTrashCommandResult::Failed("gio failed on this mount".to_string())
        },
        |_| Err("fallback failed too".to_string()),
    );

    assert_eq!(
        result,
        TrashBatchBackendResult::Failed {
            completed: 1,
            error: "Could not trash all items: gio failed on this mount; fallback failed: fallback failed too".to_string(),
        }
    );
    let _ = fs::remove_dir_all(&root);
}

/// Simulates the startup logic: delete cleanup_root/{current_pid}/ if it
/// exists (pre-existing = stale), then run the async sweep.  Mirrors what
/// sweep_staging_on_startup does, but against a temp root for testing.
fn startup_sweep(cleanup_root: &Path, current_pid: u32) {
    let current_pid_dir = cleanup_root.join(current_pid.to_string());
    if current_pid_dir.exists() {
        let _ = fs::remove_dir_all(&current_pid_dir);
    }
    sweep_staging_dir(cleanup_root, current_pid);
}

// ── sweep_staging_dir ──────────────────────────────────────────────────
// The cleanup root contains one subdirectory per PID, e.g.:
//   cleanup_root/
//     1234/   ← previous session (should be swept)
//     5678/   ← current session  (must be skipped)

/// Returns the PID of a process that has already exited, guaranteed to be
/// dead (and not yet recycled since we hold the Child handle until after
/// we call this).
#[cfg(unix)]
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

#[cfg(unix)]
#[test]
fn sweep_removes_dead_pid_subdirectory() {
    let cleanup_root = make_tmp_dir("sweep-dead");
    let current_pid = std::process::id();
    let dead = dead_pid();

    let stale_dir = cleanup_root.join(dead.to_string());
    fs::create_dir_all(&stale_dir).unwrap();
    fs::write(stale_dir.join("inner.txt"), b"hello").unwrap();

    sweep_staging_dir(&cleanup_root, current_pid);

    assert!(!stale_dir.exists(), "dead-pid subdir should be removed");
    let _ = fs::remove_dir_all(&cleanup_root);
}

#[cfg(unix)]
#[test]
fn sweep_skips_live_other_pid_subdirectory() {
    // Simulate a concurrently running instance: spawn a child that stays
    // alive while we run the sweep, then kill it.
    let cleanup_root = make_tmp_dir("sweep-live-other");
    let current_pid = std::process::id();

    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let other_pid = child.id();

    let live_dir = cleanup_root.join(other_pid.to_string());
    fs::create_dir_all(&live_dir).unwrap();
    fs::write(live_dir.join("staged.tmp"), b"in-flight").unwrap();

    sweep_staging_dir(&cleanup_root, current_pid);
    child.kill().ok();
    child.wait().ok();

    assert!(
        live_dir.exists(),
        "live other-instance dir must not be swept"
    );
    let _ = fs::remove_dir_all(&cleanup_root);
}

#[test]
fn sweep_skips_current_pid_subdirectory() {
    let cleanup_root = make_tmp_dir("sweep-skip");
    let current_pid = std::process::id();

    // Live session subdir.
    let live_dir = cleanup_root.join(current_pid.to_string());
    fs::create_dir_all(&live_dir).unwrap();
    fs::write(live_dir.join("staged.tmp"), b"live").unwrap();

    sweep_staging_dir(&cleanup_root, current_pid);

    assert!(live_dir.exists(), "current-pid subdir must not be swept");
    let _ = fs::remove_dir_all(&cleanup_root);
}

#[test]
fn startup_sweep_reclaims_stale_dir_with_reused_pid() {
    // Simulate the OS reusing a PID: a previous crashed session left
    // cleanup_root/{current_pid}/ behind, and the new session starts with
    // the same PID.  The startup sweep must remove it.
    let cleanup_root = make_tmp_dir("sweep-pid-reuse");
    let current_pid = std::process::id();

    let stale_same_pid_dir = cleanup_root.join(current_pid.to_string());
    fs::create_dir_all(&stale_same_pid_dir).unwrap();
    fs::write(stale_same_pid_dir.join("leftover.tmp"), b"stale").unwrap();

    startup_sweep(&cleanup_root, current_pid);

    assert!(
        !stale_same_pid_dir.exists(),
        "stale dir with reused PID should be removed at startup"
    );
    let _ = fs::remove_dir_all(&cleanup_root);
}

#[test]
fn sweep_is_no_op_for_empty_cleanup_root() {
    let cleanup_root = make_tmp_dir("sweep-empty");
    sweep_staging_dir(&cleanup_root, std::process::id());
    assert!(cleanup_root.exists());
    let _ = fs::remove_dir_all(&cleanup_root);
}

#[test]
fn sweep_is_no_op_when_cleanup_root_does_not_exist() {
    let parent = make_tmp_dir("sweep-absent-parent");
    let nonexistent = parent.join("no-such-dir");
    // Should not panic.
    sweep_staging_dir(&nonexistent, std::process::id());
    let _ = fs::remove_dir_all(&parent);
}

// ── run_staged_cleanup ─────────────────────────────────────────────────

#[test]
fn staged_cleanup_succeeds_and_returns_no_errors() {
    let staging = make_tmp_dir("cleanup-ok");
    let dir = staging.join("to-delete");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("file.txt"), b"x").unwrap();

    let errors = run_staged_cleanup(vec![("to-delete".to_string(), dir.clone())]);

    assert!(errors.is_empty());
    assert!(!dir.exists());
    let _ = fs::remove_dir_all(&staging);
}

#[test]
fn staged_cleanup_returns_name_on_failure() {
    // Pass a path that does not exist — remove_dir_all returns an error.
    let parent = make_tmp_dir("cleanup-fail-parent");
    let missing = parent.join("ghost-dir");

    let errors = run_staged_cleanup(vec![("ghost-dir".to_string(), missing)]);

    assert_eq!(errors, vec!["ghost-dir"]);
    let _ = fs::remove_dir_all(&parent);
}

#[test]
fn staged_cleanup_reports_only_failed_entries() {
    let staging = make_tmp_dir("cleanup-mixed");
    let good = staging.join("good");
    fs::create_dir_all(&good).unwrap();

    let bad = staging.join("bad-ghost");
    // `bad` deliberately never created

    let errors = run_staged_cleanup(vec![
        ("good".to_string(), good.clone()),
        ("bad-ghost".to_string(), bad),
    ]);

    assert_eq!(errors, vec!["bad-ghost"]);
    assert!(!good.exists());
    let _ = fs::remove_dir_all(&staging);
}
