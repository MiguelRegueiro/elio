#[cfg(unix)]
use super::super::state::{BulkRenameEditorSession, PendingTerminalTask};
use super::super::{
    App,
    state::{BulkRenameItem, BulkRenameOverlay, DirectoryLoadCompletion, DuplicateFinderOverlay},
};
use super::rename;
#[cfg(unix)]
use std::{
    env,
    ffi::OsString,
    sync::{Mutex, OnceLock},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(unix)]
struct EnvVarGuard {
    key: &'static str,
    original: Option<OsString>,
}

#[cfg(unix)]
impl EnvVarGuard {
    fn set_path(key: &'static str, value: &Path) -> Self {
        let original = env::var_os(key);
        unsafe {
            env::set_var(key, value);
        }
        Self { key, original }
    }
}

#[cfg(unix)]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.original.as_ref() {
            Some(value) => unsafe {
                env::set_var(self.key, value);
            },
            None => unsafe {
                env::remove_var(self.key);
            },
        }
    }
}

/// Drive background jobs until both the trash worker and the subsequent
/// directory reload have both completed.  Checking only `trash_progress`
/// is not enough: a single `process_background_jobs` call can consume
/// the `Trash(done=true)` result *and* the immediately-queued
/// `Directory` reload in the same batch (a tiny directory scan completes
/// before the loop's next `try_recv`).  Driving until `pending_load` is
/// also gone guarantees that `app.status_message()` holds the final
/// status in all cases.
fn wait_for_trash_and_reload(app: &mut App) {
    for _ in 0..500 {
        let _ = app.process_background_jobs();
        if app.trash_progress().is_none() && app.navigation.directory_runtime.pending_load.is_none()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for trash and directory reload to complete");
}

fn wait_for_restore_and_reload(app: &mut App) {
    for _ in 0..500 {
        let _ = app.process_background_jobs();
        if app.restore_progress().is_none()
            && app.navigation.directory_runtime.pending_load.is_none()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for restore and directory reload to complete");
}

fn temp_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("elio-create-{label}-{unique}"))
}

fn take_pending_status(app: &mut App) -> (String, Option<PathBuf>) {
    let load = app
        .navigation
        .directory_runtime
        .pending_load
        .take()
        .expect("expected queued directory load");
    let status = match load.completion {
        DirectoryLoadCompletion::Status(status) => status,
        DirectoryLoadCompletion::Keep => {
            panic!("expected status completion, got keep")
        }
        DirectoryLoadCompletion::Clear => {
            panic!("expected status completion, got clear")
        }
    };
    (status, load.reselect_path)
}

fn encode_trashinfo_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('%', "%25")
        .replace(' ', "%20")
}

fn create_fake_trash_file(label: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = temp_path(label);
    let originals_dir = root.join("originals");
    let trash_files = root.join("Trash/files");
    let trash_info = root.join("Trash/info");
    fs::create_dir_all(&originals_dir).expect("failed to create originals dir");
    fs::create_dir_all(&trash_files).expect("failed to create trash files dir");
    fs::create_dir_all(&trash_info).expect("failed to create trash info dir");

    let original_path = originals_dir.join("restore-target.txt");
    fs::write(&original_path, "restore me").expect("failed to write original file");

    let trashed_path = trash_files.join("restore-target.txt");
    fs::rename(&original_path, &trashed_path).expect("failed to move file into fake trash");
    fs::write(
        trash_info.join("restore-target.txt.trashinfo"),
        format!(
            "[Trash Info]\nPath={}\nDeletionDate=2026-03-21T00:00:00\n",
            encode_trashinfo_path(&original_path)
        ),
    )
    .expect("failed to write trashinfo");

    (root, trash_files, original_path, trashed_path)
}

#[test]
fn confirm_create_creates_files_and_folders_and_reselects_last_created_path() {
    let root = temp_path("create-success");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_create_prompt();
    let overlay = app
        .overlays
        .create
        .as_mut()
        .expect("create overlay should be open");
    overlay.lines = vec!["notes.txt".to_string(), "/docs/".to_string()];
    overlay.line_errors = vec![None; overlay.lines.len()];

    app.confirm_create().expect("create should succeed");

    assert!(app.overlays.create.is_none());
    assert!(root.join("notes.txt").is_file());
    assert!(root.join("docs").is_dir());

    let (status, reselect_path) = take_pending_status(&mut app);
    assert_eq!(status, "Created 1 file and 1 folder");
    assert_eq!(reselect_path, Some(root.join("docs")));

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_create_reports_duplicate_names_after_dir_marker_normalization() {
    let root = temp_path("create-duplicates");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_create_prompt();
    let overlay = app
        .overlays
        .create
        .as_mut()
        .expect("create overlay should be open");
    overlay.lines = vec!["logs/".to_string(), "/logs".to_string()];
    overlay.line_errors = vec![None; overlay.lines.len()];

    app.confirm_create()
        .expect("create validation should succeed");

    let overlay = app
        .overlays
        .create
        .as_ref()
        .expect("create overlay should stay open");
    assert_eq!(overlay.cursor_line, 1);
    assert_eq!(
        overlay.line_errors[1].as_deref(),
        Some("\"logs\" appears more than once")
    );
    assert!(!root.join("logs").exists());
    assert!(app.navigation.directory_runtime.pending_load.is_none());

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_rename_renames_selected_entry_and_queues_reselect() {
    let root = temp_path("rename-success");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("report.txt"), "draft").expect("failed to write source file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_rename_prompt();
    let overlay = app
        .overlays
        .rename
        .as_mut()
        .expect("rename overlay should be open");
    assert_eq!(overlay.original_name, "report.txt");
    assert_eq!(overlay.cursor_col, 6);
    overlay.input = "summary.txt".to_string();

    app.confirm_rename().expect("rename should succeed");

    assert!(app.overlays.rename.is_none());
    assert!(!root.join("report.txt").exists());
    assert!(root.join("summary.txt").is_file());

    let (status, reselect_path) = take_pending_status(&mut app);
    assert_eq!(status, "Renamed \"report.txt\" → \"summary.txt\"");
    assert_eq!(reselect_path, Some(root.join("summary.txt")));

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn cursor_before_extension_skips_hidden_file_prefix_dot() {
    assert_eq!(rename::cursor_before_extension(".env"), 4);
    assert_eq!(rename::cursor_before_extension("report.txt"), 6);
    assert_eq!(rename::cursor_before_extension("archive.tar.gz"), 11);
}

#[test]
fn confirm_bulk_rename_renames_changed_entries_and_skips_unchanged_rows() {
    let root = temp_path("bulk-rename-success");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "alpha").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(root.join("alpha.txt"));
    app.navigation.selected_paths.insert(root.join("beta.txt"));
    app.open_bulk_rename_prompt();

    let overlay = app
        .overlays
        .bulk_rename
        .as_mut()
        .expect("bulk rename overlay should be open");
    assert_eq!(overlay.new_names, vec!["alpha.txt", "beta.txt"]);
    overlay.new_names[0] = "gamma.txt".to_string();

    app.confirm_bulk_rename()
        .expect("bulk rename should succeed");

    assert!(app.overlays.bulk_rename.is_none());
    assert!(root.join("gamma.txt").is_file());
    assert!(root.join("beta.txt").is_file());
    assert!(!root.join("alpha.txt").exists());
    assert!(app.navigation.selected_paths.is_empty());

    let (status, reselect_path) = take_pending_status(&mut app);
    assert_eq!(status, "Renamed \"alpha.txt\" → \"gamma.txt\"");
    assert_eq!(reselect_path, Some(root.join("gamma.txt")));

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_bulk_rename_reports_duplicate_destination_names() {
    let root = temp_path("bulk-rename-duplicates");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "alpha").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(root.join("alpha.txt"));
    app.navigation.selected_paths.insert(root.join("beta.txt"));
    app.open_bulk_rename_prompt();

    let overlay = app
        .overlays
        .bulk_rename
        .as_mut()
        .expect("bulk rename overlay should be open");
    overlay.new_names = vec!["shared.txt".to_string(), "shared.txt".to_string()];

    app.confirm_bulk_rename()
        .expect("bulk rename validation should succeed");

    let overlay = app
        .overlays
        .bulk_rename
        .as_ref()
        .expect("bulk rename overlay should stay open");
    assert_eq!(overlay.cursor_line, 1);
    assert_eq!(
        overlay.line_errors[1].as_deref(),
        Some("\"shared.txt\" appears more than once")
    );
    assert!(root.join("alpha.txt").is_file());
    assert!(root.join("beta.txt").is_file());
    assert!(app.navigation.directory_runtime.pending_load.is_none());

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_bulk_rename_apply_failure_keeps_review_context() {
    let root = temp_path("bulk-rename-apply-failure");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let missing = root.join("missing.txt");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(missing.clone());
    app.overlays.bulk_rename = Some(BulkRenameOverlay {
        items: vec![BulkRenameItem {
            path: missing.clone(),
            original_name: "missing.txt".to_string(),
            is_dir: false,
        }],
        new_names: vec!["renamed.txt".to_string()],
        root: None,
        cursor_line: 0,
        cursor_col: 0,
        preferred_col: 0,
        line_errors: vec![None],
    });

    app.confirm_bulk_rename()
        .expect("apply failure should be reported as status");

    assert!(app.overlays.bulk_rename.is_some());
    assert!(app.navigation.selected_paths.contains(&missing));
    assert!(
        app.status_message()
            .starts_with("Could not rename \"missing.txt\":")
    );
    assert!(app.navigation.directory_runtime.pending_load.is_none());

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn bulk_rename_uses_selection_from_multiple_directories() {
    let root = temp_path("bulk-rename-cross-directory-selection");
    let child = root.join("child");
    let alpha = root.join("alpha.txt");
    let beta = child.join("beta.txt");
    fs::create_dir_all(&child).expect("failed to create child dir");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());
    app.open_bulk_rename_prompt();

    let overlay = app
        .overlays
        .bulk_rename
        .as_mut()
        .expect("bulk rename overlay should be open");
    assert_eq!(overlay.new_names, vec!["alpha.txt", "beta.txt"]);
    overlay.new_names = vec![
        "alpha-renamed.txt".to_string(),
        "beta-renamed.txt".to_string(),
    ];

    app.confirm_bulk_rename()
        .expect("bulk rename should succeed");

    assert!(!alpha.exists());
    assert!(!beta.exists());
    assert!(root.join("alpha-renamed.txt").is_file());
    assert!(child.join("beta-renamed.txt").is_file());
    assert!(app.navigation.selected_paths.is_empty());

    let (status, reselect_path) = take_pending_status(&mut app);
    assert_eq!(status, "Renamed 2 items");
    assert_eq!(reselect_path, Some(child.join("beta-renamed.txt")));

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn bulk_rename_allows_same_new_name_in_different_directories() {
    let root = temp_path("bulk-rename-same-name-different-dirs");
    let left = root.join("left");
    let right = root.join("right");
    let left_file = left.join("old.txt");
    let right_file = right.join("old.txt");
    fs::create_dir_all(&left).expect("failed to create left dir");
    fs::create_dir_all(&right).expect("failed to create right dir");
    fs::write(&left_file, "left").expect("failed to write left file");
    fs::write(&right_file, "right").expect("failed to write right file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(left_file);
    app.navigation.selected_paths.insert(right_file);
    app.open_bulk_rename_prompt();

    let overlay = app
        .overlays
        .bulk_rename
        .as_mut()
        .expect("bulk rename overlay should be open");
    overlay.new_names = vec!["new.txt".to_string(), "new.txt".to_string()];

    app.confirm_bulk_rename()
        .expect("bulk rename should succeed");

    assert!(app.overlays.bulk_rename.is_none());
    assert!(left.join("new.txt").is_file());
    assert!(right.join("new.txt").is_file());

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn editor_bulk_rename_file_preserves_selection_order() {
    let _env_guard = env_lock()
        .lock()
        .expect("failed to lock process environment for test");
    let _visual_guard = EnvVarGuard::set_path("VISUAL", Path::new("true"));

    let root = temp_path("editor-bulk-rename-order");
    let nested = root.join("ui-workspace");
    let angular = nested.join("angular.json");
    let analysis = root.join("analysis.ipynb");
    let analytics = root.join("analytics.sqlite3");
    fs::create_dir_all(&nested).expect("failed to create nested dir");
    fs::write(&angular, "{}").expect("failed to write angular.json");
    fs::write(&analysis, "analysis").expect("failed to write analysis");
    fs::write(&analytics, "analytics").expect("failed to write analytics");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(angular.clone());
    app.navigation.selected_paths.insert(analysis.clone());
    app.navigation.selected_paths.insert(analytics.clone());

    app.open_editor_bulk_rename()
        .expect("editor bulk rename should open");

    let task = app
        .pending_terminal_task
        .take()
        .expect("expected pending editor task");
    let PendingTerminalTask::EditorBulkRename { session, .. } = task else {
        panic!("expected editor bulk rename task");
    };
    let edited = fs::read_to_string(&session.temp_path).expect("failed to read temp rename file");
    assert_eq!(
        edited,
        "ui-workspace/angular.json\nanalysis.ipynb\nanalytics.sqlite3\n"
    );
    let _ = fs::remove_file(&session.temp_path);

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn invalid_editor_bulk_rename_aborts_without_review_overlay() {
    let root = temp_path("editor-bulk-rename-invalid");
    let file = root.join("alpha.txt");
    fs::create_dir_all(&root).expect("failed to create root dir");
    fs::write(&file, "alpha").expect("failed to write alpha");

    let temp_file = root.join("rename.txt");
    fs::write(&temp_file, "../outside.txt\n").expect("failed to write edited rename file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(file.clone());
    let session = BulkRenameEditorSession {
        root: root.clone(),
        temp_path: temp_file.clone(),
        items: vec![BulkRenameItem {
            path: file.clone(),
            original_name: "alpha.txt".to_string(),
            is_dir: false,
        }],
    };
    let status = std::os::unix::process::ExitStatusExt::from_raw(0);

    app.finish_editor_bulk_rename(session, Ok(status))
        .expect("editor rename finish should succeed");

    assert!(app.overlays.bulk_rename.is_none());
    assert!(app.overlays.editor_rename_confirm.is_none());
    assert!(app.navigation.selected_paths.contains(&file));
    assert_eq!(
        app.status_message(),
        "Editor rename aborted: line 1: Path cannot contain . or .."
    );
    assert!(file.exists());
    assert!(!temp_file.exists());

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn finish_editor_bulk_rename_opens_confirmation_with_relative_paths() {
    let root = temp_path("editor-bulk-rename-review");
    let left = root.join("left");
    let right = root.join("right");
    let alpha = left.join("alpha.txt");
    let beta = right.join("beta.txt");
    fs::create_dir_all(&left).expect("failed to create left dir");
    fs::create_dir_all(&right).expect("failed to create right dir");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");

    let temp_file = root.join("rename.txt");
    fs::write(
        &temp_file,
        "left/renamed-alpha.txt\nright/renamed-beta.txt\n",
    )
    .expect("failed to write edited rename file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    let session = BulkRenameEditorSession {
        root: root.clone(),
        temp_path: temp_file.clone(),
        items: vec![
            BulkRenameItem {
                path: alpha.clone(),
                original_name: "alpha.txt".to_string(),
                is_dir: false,
            },
            BulkRenameItem {
                path: beta.clone(),
                original_name: "beta.txt".to_string(),
                is_dir: false,
            },
        ],
    };
    let status = std::os::unix::process::ExitStatusExt::from_raw(0);

    app.finish_editor_bulk_rename(session, Ok(status))
        .expect("editor rename finish should succeed");

    let overlay = app
        .overlays
        .editor_rename_confirm
        .as_ref()
        .expect("editor rename confirmation should open");
    assert_eq!(overlay.root, root);
    assert_eq!(
        overlay.new_names,
        vec!["left/renamed-alpha.txt", "right/renamed-beta.txt"]
    );
    assert_eq!(app.editor_rename_confirm_title(), "Confirm 2 renames?");
    assert!(app.editor_rename_confirmed());
    assert_eq!(app.status_message(), "");
    assert!(!temp_file.exists());

    app.overlays.duplicates = Some(DuplicateFinderOverlay {
        cwd: root.clone(),
        groups: vec![crate::fs::duplicates::DuplicateGroup {
            id: 1,
            size: 5,
            files: vec![
                crate::fs::duplicates::DuplicateFile {
                    path: alpha.clone(),
                    name: "alpha.txt".to_string(),
                    relative: "left/alpha.txt".to_string(),
                    size: 5,
                    modified: None,
                },
                crate::fs::duplicates::DuplicateFile {
                    path: beta.clone(),
                    name: "beta.txt".to_string(),
                    relative: "right/beta.txt".to_string(),
                    size: 5,
                    modified: None,
                },
            ],
        }],
        stats: crate::fs::duplicates::DuplicateScanStats::default(),
        selected: 0,
        scroll: 0,
        selected_paths: [beta.clone()].into_iter().collect(),
        loading: false,
        partial: false,
        error: None,
        preview_visible: true,
        preview_path: Some(alpha.clone()),
    });

    app.confirm_editor_rename()
        .expect("confirmation should rename files");
    assert!(left.join("renamed-alpha.txt").is_file());
    assert!(right.join("renamed-beta.txt").is_file());
    assert!(!alpha.exists());
    assert!(!beta.exists());

    let duplicate_overlay = app
        .overlays
        .duplicates
        .as_ref()
        .expect("duplicate overlay should stay open");
    assert_eq!(
        duplicate_overlay.groups[0].files[0].path,
        left.join("renamed-alpha.txt")
    );
    assert_eq!(
        duplicate_overlay.groups[0].files[0].name,
        "renamed-alpha.txt"
    );
    assert_eq!(
        duplicate_overlay.groups[0].files[0].relative,
        "left/renamed-alpha.txt"
    );
    assert!(
        duplicate_overlay
            .selected_paths
            .contains(&right.join("renamed-beta.txt"))
    );
    assert!(!duplicate_overlay.selected_paths.contains(&beta));
    assert_eq!(
        duplicate_overlay.preview_path,
        Some(left.join("renamed-alpha.txt"))
    );

    let (status, reselect_path) = take_pending_status(&mut app);
    assert_eq!(status, "Renamed 2 items");
    assert_eq!(reselect_path, Some(right.join("renamed-beta.txt")));

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn bulk_rename_refuses_selection_containing_trash_item() {
    let _env_guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = temp_path("bulk-rename-refuses-trash-selection");
    let data_home = root.join("data");
    let trash_files = data_home.join("Trash/files");
    let normal_dir = root.join("normal");
    let normal = normal_dir.join("normal.txt");
    let trashed = trash_files.join("trashed.txt");
    fs::create_dir_all(&trash_files).expect("failed to create trash files dir");
    fs::create_dir_all(&normal_dir).expect("failed to create normal dir");
    fs::write(&normal, "normal").expect("failed to write normal file");
    fs::write(&trashed, "trashed").expect("failed to write trashed file");

    let _xdg_data_home = EnvVarGuard::set_path("XDG_DATA_HOME", &data_home);

    let mut app = App::new_at(normal_dir.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(normal);
    app.navigation.selected_paths.insert(trashed);
    app.open_bulk_rename_prompt();

    assert!(!app.bulk_rename_is_open());
    assert_eq!(app.status_message(), "Cannot rename items from Trash");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn bulk_rename_selected_parent_of_current_directory_reloads_parent() {
    let root = temp_path("bulk-rename-parent-selection");
    let parent = root.join("parent");
    let child_file = parent.join("child.txt");
    fs::create_dir_all(&parent).expect("failed to create parent dir");
    fs::write(&child_file, "child").expect("failed to write child file");

    let mut app = App::new_at(parent.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(parent.clone());
    app.open_bulk_rename_prompt();

    assert!(app.bulk_rename_is_open());
    assert_eq!(app.bulk_rename_item_count(), 1);

    let overlay = app
        .overlays
        .bulk_rename
        .as_mut()
        .expect("bulk rename overlay should be open");
    overlay.new_names[0] = "renamed".to_string();

    app.confirm_bulk_rename()
        .expect("bulk rename should succeed");

    let load = app
        .navigation
        .directory_runtime
        .pending_load
        .as_ref()
        .expect("rename should queue a directory reload");
    assert_eq!(load.target_cwd, root);
    assert_eq!(load.reselect_path, Some(root.join("renamed")));
    assert!(!parent.exists());
    assert!(root.join("renamed/child.txt").is_file());

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_trash_permanently_deletes_selected_items_inside_trash() {
    let root = temp_path("trash-permanent");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("gone.txt"), "bye").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(root.join("gone.txt"));
    app.open_trash_prompt();

    assert_eq!(app.trash_title(), "Delete permanently 1 selected file?");
    app.confirm_trash().expect("trash should succeed");

    assert!(app.overlays.trash.is_none());
    assert!(app.navigation.selected_paths.is_empty());

    // Deletion is async — wait for the background worker *and* the
    // subsequent directory reload to both finish.
    wait_for_trash_and_reload(&mut app);

    assert!(!root.join("gone.txt").exists());
    // Status is set by apply_directory_snapshot once the reload completes.
    assert_eq!(app.status_message(), "Permanently deleted \"gone.txt\"");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_delete_permanently_removes_selected_items_outside_trash() {
    let root = temp_path("delete-permanent-outside-trash");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("gone.txt"), "bye").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(root.join("gone.txt"));
    app.open_delete_permanently_prompt();

    assert_eq!(app.trash_title(), "Delete permanently 1 selected file?");
    app.confirm_trash().expect("delete should succeed");

    assert!(app.overlays.trash.is_none());
    assert!(app.navigation.selected_paths.is_empty());

    wait_for_trash_and_reload(&mut app);

    assert!(!root.join("gone.txt").exists());
    assert_eq!(app.status_message(), "Permanently deleted \"gone.txt\"");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_delete_selected_parent_of_current_directory_moves_to_parent() {
    let root = temp_path("delete-parent-of-current");
    let parent = root.join("parent");
    let child = parent.join("child");
    fs::create_dir_all(&child).expect("failed to create child dir");
    fs::write(child.join("file.txt"), "child").expect("failed to write child file");

    let mut app = App::new_at(child.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(parent.clone());
    app.open_delete_permanently_prompt();

    assert_eq!(app.trash_title(), "Delete permanently 1 selected folder?");
    app.confirm_trash().expect("delete should succeed");

    assert!(app.overlays.trash.is_none());
    assert!(app.navigation.selected_paths.is_empty());

    wait_for_trash_and_reload(&mut app);

    assert_eq!(app.navigation.cwd, root);
    assert!(!parent.exists());
    assert_eq!(app.status_message(), "Permanently deleted \"parent\"");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn after_delete_cursor_moves_to_next_surviving_entry() {
    // Deleting a middle entry should leave the cursor on what was the
    // entry immediately below it (now occupying the same visual row).
    let root = temp_path("cursor-next-after-delete");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "a").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "b").expect("failed to write beta");
    fs::write(root.join("gamma.txt"), "c").expect("failed to write gamma");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    // entries are sorted by name: alpha=0, beta=1, gamma=2
    app.navigation.in_trash = true;
    app.navigation.selected = 1; // cursor on beta.txt
    app.remember_current_directory_view(); // simulate a rendered frame committing the position
    app.open_trash_prompt();
    app.confirm_trash().expect("trash should succeed");

    wait_for_trash_and_reload(&mut app);

    assert!(!root.join("beta.txt").exists());
    assert_eq!(
        app.selected_entry().map(|e| e.name.as_str()),
        Some("gamma.txt"),
        "cursor should land on gamma.txt (next surviving entry)"
    );

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn after_delete_cursor_falls_back_to_previous_entry_when_last_is_deleted() {
    // Deleting the last entry should leave the cursor on the entry above it.
    let root = temp_path("cursor-prev-after-delete");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "a").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "b").expect("failed to write beta");
    fs::write(root.join("gamma.txt"), "c").expect("failed to write gamma");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    // entries are sorted by name: alpha=0, beta=1, gamma=2
    app.navigation.in_trash = true;
    app.navigation.selected = 2; // cursor on gamma.txt
    app.remember_current_directory_view(); // simulate a rendered frame committing the position
    app.open_trash_prompt();
    app.confirm_trash().expect("trash should succeed");

    wait_for_trash_and_reload(&mut app);

    assert!(!root.join("gamma.txt").exists());
    assert_eq!(
        app.selected_entry().map(|e| e.name.as_str()),
        Some("beta.txt"),
        "cursor should fall back to beta.txt (last surviving entry before cursor)"
    );

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn cancelled_delete_does_not_move_cursor_away_from_surviving_entry() {
    // When permanent delete is cancelled before any item is removed, the
    // cursor must not jump away — the targeted entry is still present.
    let root = temp_path("cursor-cancel-delete");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "a").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "b").expect("failed to write beta");
    fs::write(root.join("gamma.txt"), "c").expect("failed to write gamma");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.navigation.selected = 1; // cursor on beta.txt
    app.remember_current_directory_view(); // simulate a rendered frame committing the position
    app.open_trash_prompt();
    app.confirm_trash().expect("trash should succeed");

    // Cancel before the worker starts processing.
    app.jobs.scheduler.cancel_trash(app.jobs.trash_token);

    wait_for_trash_and_reload(&mut app);

    // beta.txt may or may not have been deleted depending on race, but the
    // cursor must not have jumped to an entry other than what was at index 1.
    // If the file still exists, the cursor must be on it (not on gamma.txt).
    if root.join("beta.txt").exists() {
        assert_eq!(
            app.selected_entry().map(|e| e.name.as_str()),
            Some("beta.txt"),
            "cursor must stay on beta.txt when cancel won the race"
        );
    }
    // If the cancel lost the race and beta.txt was deleted, the cursor
    // should have moved to gamma.txt (completed == total == 1).
    // Either outcome is valid; the key invariant is that we never land
    // on a position whose entry no longer exists.
    assert!(
        app.selected_entry().is_none()
            || root
                .join(app.selected_entry().unwrap().name.as_str())
                .exists(),
        "cursor must point to a surviving entry"
    );

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_trash_batch_trashes_multiple_files_and_reports_count() {
    let root = temp_path("trash-batch-multi");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "a").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "b").expect("failed to write beta");
    fs::write(root.join("gamma.txt"), "c").expect("failed to write gamma");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    // in_trash = false → non-permanent batch trash
    app.navigation.selected_paths.insert(root.join("alpha.txt"));
    app.navigation.selected_paths.insert(root.join("beta.txt"));
    app.navigation.selected_paths.insert(root.join("gamma.txt"));
    app.open_trash_prompt();

    assert_eq!(app.trash_title(), "Trash 3 files?");
    app.confirm_trash().expect("trash should succeed");

    assert!(app.overlays.trash.is_none());
    assert!(app.navigation.selected_paths.is_empty());

    wait_for_trash_and_reload(&mut app);

    assert!(!root.join("alpha.txt").exists());
    assert!(!root.join("beta.txt").exists());
    assert!(!root.join("gamma.txt").exists());
    assert_eq!(app.status_message(), "Trashed 3 items");

    // Purge the items we just trashed from the OS trash so the test
    // leaves no permanent side-effects.
    // trash::os_limited is only available on non-macOS Unix (freedesktop).
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use trash::os_limited::{list, purge_all};
        if let Ok(items) = list() {
            let ours: Vec<_> = items
                .into_iter()
                .filter(|item| item.original_parent == root)
                .collect();
            let _ = purge_all(ours);
        }
    }

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_trash_batch_single_file_shows_quoted_name() {
    let root = temp_path("trash-batch-single");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("notes.txt"), "hello").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    // in_trash = false → non-permanent batch trash
    app.navigation.selected_paths.insert(root.join("notes.txt"));
    app.open_trash_prompt();

    assert_eq!(app.trash_title(), "Trash 1 selected file?");
    app.confirm_trash().expect("trash should succeed");

    assert!(app.overlays.trash.is_none());
    assert!(app.navigation.selected_paths.is_empty());

    wait_for_trash_and_reload(&mut app);

    assert!(!root.join("notes.txt").exists());
    assert_eq!(app.status_message(), "Trashed \"notes.txt\"");

    // Purge from OS trash to avoid side-effects.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use trash::os_limited::{list, purge_all};
        if let Ok(items) = list() {
            let ours: Vec<_> = items
                .into_iter()
                .filter(|item| item.original_parent == root)
                .collect();
            let _ = purge_all(ours);
        }
    }

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn esc_during_batched_trash_keeps_chip_visible_until_done() {
    // Non-permanent (batched) trash is a single atomic OS call that may
    // already be in flight when the user presses Esc.  The chip must
    // remain visible until the worker sends done=true so the user can
    // see the operation is still running, not silently cancelled.
    let root = temp_path("trash-cancel-batched");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("canary.txt"), "x").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation
        .selected_paths
        .insert(root.join("canary.txt"));
    app.open_trash_prompt();
    app.confirm_trash().expect("trash should succeed");

    // Chip is showing immediately after submit.
    assert!(
        app.trash_progress().is_some(),
        "chip should be visible after submit"
    );

    // Simulate Esc: cancel_trash is called but chip must NOT be cleared.
    app.jobs.scheduler.cancel_trash(app.jobs.trash_token);
    // trash_progress is still Some — chip stays visible.
    assert!(
        app.trash_progress().is_some(),
        "chip must remain visible after Esc for batched trash"
    );

    // Wait for the worker to finish (cancelled before start or completed).
    wait_for_trash_and_reload(&mut app);

    // Chip is gone once done=true is processed.
    assert!(
        app.trash_progress().is_none(),
        "chip should be gone after completion"
    );

    // Status is either "Trash cancelled" (cancel won the race) or "Trashed
    // \"canary.txt\"" (batch was already in flight).  Either is correct.
    let status = app.status_message();
    let valid = status.starts_with("Trash cancelled")
        || status.starts_with("Trashed")
        || status.starts_with("Nothing was deleted");
    assert!(valid, "unexpected status: {status:?}");

    // Purge from OS trash if the file actually got trashed.
    #[cfg(all(unix, not(target_os = "macos")))]
    if !root.join("canary.txt").exists() {
        use trash::os_limited::{list, purge_all};
        if let Ok(items) = list() {
            let ours: Vec<_> = items
                .into_iter()
                .filter(|item| item.original_parent == root)
                .collect();
            let _ = purge_all(ours);
        }
    }

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn esc_during_permanent_delete_clears_chip_immediately() {
    // Permanent delete can be interrupted between items, so pressing Esc
    // should clear the chip right away (not wait for done=true).
    let root = temp_path("trash-cancel-permanent");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("gone.txt"), "x").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(root.join("gone.txt"));
    app.open_trash_prompt();
    app.confirm_trash().expect("trash should succeed");

    assert!(
        app.trash_progress().is_some(),
        "chip should be visible after submit"
    );

    // Simulate Esc for permanent delete: chip clears immediately.
    let token = app.jobs.trash_token;
    app.jobs.scheduler.cancel_trash(token);
    app.jobs.trash_progress = None;

    assert!(
        app.trash_progress().is_none(),
        "chip should clear immediately for permanent delete"
    );

    // Drive to completion so background thread shuts down cleanly.
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        std::thread::sleep(Duration::from_millis(10));
    }

    app.navigation.directory_runtime.watch = None;
    drop(app);
    // root may or may not still contain gone.txt depending on the race.
    let _ = fs::remove_dir_all(root);
}

#[test]
fn confirm_restore_restores_file_from_trashinfo_and_queues_reload() {
    let (root, trash_files, original_path, trashed_path) = create_fake_trash_file("restore");

    let mut app = App::new_at(trash_files.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.open_restore_prompt();

    assert_eq!(app.restore_title(), "Restore 1 selected file?");
    app.confirm_restore().expect("restore should succeed");

    assert!(app.overlays.restore.is_none());
    assert!(app.navigation.selected_paths.is_empty());

    // Restore is now async — wait for the background worker and
    // subsequent directory reload to both complete.
    wait_for_restore_and_reload(&mut app);

    assert!(original_path.is_file());
    assert!(!trashed_path.exists());
    assert_eq!(app.status_message(), "Restored \"restore-target.txt\"");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn confirm_restore_bulk_restores_multiple_files_and_reports_count() {
    let root = temp_path("restore-bulk");
    let originals_dir = root.join("originals");
    let trash_files = root.join("Trash/files");
    let trash_info = root.join("Trash/info");
    fs::create_dir_all(&originals_dir).expect("failed to create originals dir");
    fs::create_dir_all(&trash_files).expect("failed to create trash files dir");
    fs::create_dir_all(&trash_info).expect("failed to create trash info dir");

    // Create two fake trashed files.
    for name in ["alpha.txt", "beta.txt"] {
        let original = originals_dir.join(name);
        let trashed = trash_files.join(name);
        fs::write(&original, name).expect("failed to write original");
        fs::rename(&original, &trashed).expect("failed to move to fake trash");
        fs::write(
            trash_info.join(format!("{name}.trashinfo")),
            format!(
                "[Trash Info]\nPath={}\nDeletionDate=2026-03-23T00:00:00\n",
                encode_trashinfo_path(&original)
            ),
        )
        .expect("failed to write trashinfo");
    }

    let mut app = App::new_at(trash_files.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.navigation
        .selected_paths
        .insert(trash_files.join("alpha.txt"));
    app.navigation
        .selected_paths
        .insert(trash_files.join("beta.txt"));
    app.open_restore_prompt();

    assert_eq!(app.restore_title(), "Restore 2 files?");
    app.confirm_restore().expect("restore should succeed");

    assert!(app.overlays.restore.is_none());
    assert!(app.navigation.selected_paths.is_empty());

    wait_for_restore_and_reload(&mut app);

    assert!(originals_dir.join("alpha.txt").is_file());
    assert!(originals_dir.join("beta.txt").is_file());
    assert!(!trash_files.join("alpha.txt").exists());
    assert!(!trash_files.join("beta.txt").exists());
    assert_eq!(app.status_message(), "Restored 2 items");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn restore_refuses_normal_selection_from_trash() {
    let root = temp_path("restore-normal-selection");
    let trash_files = root.join("Trash/files");
    let normal = root.join("normal.txt");
    fs::create_dir_all(&trash_files).expect("failed to create trash files dir");
    fs::write(&normal, "normal").expect("failed to write normal file");

    let mut app = App::new_at(trash_files.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(normal);
    app.open_restore_prompt();

    assert!(!app.restore_is_open());
    assert_eq!(app.status_message(), "Cannot restore normal files");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn restore_refuses_mixed_trash_and_normal_selection() {
    let root = temp_path("restore-mixed-selection");
    let trash_files = root.join("Trash/files");
    let trashed = trash_files.join("trashed.txt");
    let normal = root.join("normal.txt");
    fs::create_dir_all(&trash_files).expect("failed to create trash files dir");
    fs::write(&trashed, "trashed").expect("failed to write trashed file");
    fs::write(&normal, "normal").expect("failed to write normal file");

    let mut app = App::new_at(trash_files.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(trashed);
    app.navigation.selected_paths.insert(normal);
    app.open_restore_prompt();

    assert!(!app.restore_is_open());
    assert_eq!(
        app.status_message(),
        "Selection mixes trash and normal files"
    );

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn restore_allows_selected_parent_of_current_directory() {
    let root = temp_path("restore-parent-selection");
    let trash_files = root.join("Trash/files");
    let parent = trash_files.join("parent");
    let child_file = parent.join("child.txt");
    fs::create_dir_all(&parent).expect("failed to create parent dir");
    fs::write(&child_file, "child").expect("failed to write child file");

    let mut app = App::new_at(parent.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(parent);
    app.open_restore_prompt();

    assert!(app.restore_is_open());
    assert_eq!(app.restore_title(), "Restore 1 selected folder?");
    assert_eq!(
        app.restore_target_path_at(0),
        Some(trash_files.join("parent").as_path())
    );

    app.navigation.directory_runtime.watch = None;
    drop(app);
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn esc_during_restore_clears_chip_immediately() {
    // Restore is per-item (like permanent delete), so pressing Esc should
    // clear the chip right away rather than waiting for done=true.
    let (root, trash_files, _original_path, _trashed_path) =
        create_fake_trash_file("restore-cancel");

    let mut app = App::new_at(trash_files.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.open_restore_prompt();
    app.confirm_restore().expect("restore should succeed");

    assert!(
        app.restore_progress().is_some(),
        "chip should be visible after submit"
    );

    // Simulate Esc: chip clears immediately for per-item operations.
    let token = app.jobs.restore_token;
    app.jobs.scheduler.cancel_restore(token);
    app.jobs.restore_progress = None;

    assert!(
        app.restore_progress().is_none(),
        "chip should clear immediately after Esc for restore"
    );

    // Drive to completion so the background thread shuts down cleanly.
    // The done=true result still arrives and is ignored (token matches
    // but restore_progress is already None), and restore_source_cwd is
    // taken and a directory reload is queued.
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if app.jobs.restore_source_cwd.is_none()
            && app.navigation.directory_runtime.pending_load.is_none()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Status is either "Restore cancelled" (cancel won the race) or
    // "Restored \"restore-target.txt\"" (restore finished before cancel).
    let status = app.status_message();
    let valid = status.starts_with("Restore cancelled")
        || status.starts_with("Restored")
        || status.starts_with("Nothing was restored");
    assert!(valid, "unexpected status: {status:?}");

    app.navigation.directory_runtime.watch = None;
    drop(app);
    // root may or may not still contain the original file depending on
    // the race; ignore removal errors.
    let _ = fs::remove_dir_all(root);
}

#[test]
fn confirm_restore_while_in_progress_shows_status_and_dismisses_overlay() {
    // If the user opens and confirms a second restore while one is already
    // running, confirm_restore should surface a status message and close
    // the overlay without submitting a duplicate job.
    let (root, trash_files, _original_path, _trashed_path) =
        create_fake_trash_file("restore-in-progress");

    let mut app = App::new_at(trash_files.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.open_restore_prompt();
    app.confirm_restore().expect("first restore should succeed");

    // A second restore is attempted while the first is still in flight.
    app.open_restore_prompt();
    assert!(app.overlays.restore.is_some(), "overlay should open");
    app.confirm_restore()
        .expect("second confirm should not error");

    assert!(
        app.overlays.restore.is_none(),
        "overlay should be dismissed by the in-progress guard"
    );
    assert_eq!(
        app.status, "Restore in progress — press Esc to cancel",
        "in-progress message should be shown"
    );

    // Clean up the background worker.
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if app.restore_progress().is_none()
            && app.navigation.directory_runtime.pending_load.is_none()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    app.navigation.directory_runtime.watch = None;
    drop(app);
    let _ = fs::remove_dir_all(root);
}
