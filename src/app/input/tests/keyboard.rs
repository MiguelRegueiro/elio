use super::super::*;
#[cfg(all(unix, not(target_os = "macos")))]
use super::helpers::read_open_capture;
use super::helpers::{
    OpenInSystemCaptureGuard, cleanup_app_temp_root, temp_path, wait_for_background_preview,
    wait_for_directory_load, write_epub_fixture,
};
use crate::config::Action;
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[cfg(all(unix, not(target_os = "macos")))]
struct DefaultOpenWithAppGuard;

#[cfg(all(unix, not(target_os = "macos")))]
impl DefaultOpenWithAppGuard {
    fn install(app: crate::app::state::OpenWithApp) -> Self {
        crate::app::open_with::set_default_open_with_app_for_test(Some(app));
        Self
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for DefaultOpenWithAppGuard {
    fn drop(&mut self) {
        crate::app::open_with::set_default_open_with_app_for_test(None);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct OpenWithAppsFoundGuard;

#[cfg(all(unix, not(target_os = "macos")))]
impl OpenWithAppsFoundGuard {
    fn install(found: bool) -> Self {
        crate::app::open_with::set_open_with_apps_found_for_test(Some(found));
        Self
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for OpenWithAppsFoundGuard {
    fn drop(&mut self) {
        crate::app::open_with::set_open_with_apps_found_for_test(None);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct EditorFallbackAppGuard;

#[cfg(all(unix, not(target_os = "macos")))]
impl EditorFallbackAppGuard {
    fn install(app: crate::app::state::OpenWithApp) -> Self {
        crate::app::open_with::set_editor_fallback_app_for_test(Some(app));
        Self
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for EditorFallbackAppGuard {
    fn drop(&mut self) {
        crate::app::open_with::set_editor_fallback_app_for_test(None);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn fake_default_open_with_app(
    display_name: &str,
    program: &str,
    args: Vec<String>,
    requires_terminal: bool,
) -> crate::app::state::OpenWithApp {
    crate::app::state::OpenWithApp {
        display_name: display_name.to_string(),
        desktop_id: None,
        program: program.to_string(),
        args,
        is_default: true,
        requires_terminal,
    }
}

#[test]
fn pasted_text_is_ignored_in_browser_mode() {
    let root = temp_path("paste-browser-ignored");
    fs::write(root.join("alpha.txt"), "").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    let selected_before = app.selected_entry().map(|entry| entry.path.clone());

    app.handle_event(Event::Paste("jjjj".to_string()))
        .expect("paste event should be accepted");

    assert_eq!(
        app.selected_entry().map(|entry| entry.path.clone()),
        selected_before
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_updates_fuzzy_search_query() {
    let root = temp_path("paste-search");
    fs::write(root.join("alpha.txt"), "").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_fuzzy_finder(SearchScope::Files)
        .expect("failed to open fuzzy finder");

    app.handle_event(Event::Paste("beta".to_string()))
        .expect("paste event should update search");

    assert_eq!(app.search_query(), "beta");
    assert_eq!(app.search_query_cursor(), 4);

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_updates_local_filter() {
    let root = temp_path("paste-local-filter");
    fs::write(root.join("alpha.txt"), "").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_local_filter();

    app.handle_event(Event::Paste("beta".to_string()))
        .expect("paste event should update filter");

    assert_eq!(app.local_filter_query(), "beta");
    assert_eq!(
        app.selected_entry().map(|entry| entry.name.as_str()),
        Some("beta.txt")
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_does_not_reach_local_filter_under_non_text_overlay() {
    let root = temp_path("paste-local-filter-shadowed");
    fs::write(root.join("alpha.txt"), "").expect("failed to write alpha");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_local_filter();
    app.open_copy_overlay();

    app.handle_event(Event::Paste("hidden".to_string()))
        .expect("paste event should be ignored by non-text overlay");

    assert!(app.copy_is_open());
    assert_eq!(app.local_filter_query(), "");

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_updates_create_prompt_without_submitting() {
    let root = temp_path("paste-create");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_create_prompt();

    app.handle_event(Event::Paste("one.txt\ntwo.txt".to_string()))
        .expect("paste event should update create prompt");

    assert!(app.create_is_open());
    assert_eq!(app.create_line_count(), 2);
    assert_eq!(app.create_line(0), "one.txt");
    assert_eq!(app.create_line(1), "two.txt");
    assert_eq!(app.create_cursor_line(), 1);
    assert_eq!(app.create_cursor_col(), 7);

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_updates_rename_prompt_at_cursor() {
    let root = temp_path("paste-rename");
    fs::write(root.join("report.txt"), "draft").expect("failed to write report");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_rename_prompt();

    app.handle_event(Event::Paste("-final".to_string()))
        .expect("paste event should update rename prompt");

    assert_eq!(app.rename_input(), "report-final.txt");
    assert_eq!(app.rename_cursor_col(), "report-final".chars().count());

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_updates_bulk_rename_prompt_at_active_row() {
    let root = temp_path("paste-bulk-rename");
    fs::write(root.join("alpha.txt"), "alpha").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.selected_paths.insert(root.join("alpha.txt"));
    app.navigation.selected_paths.insert(root.join("beta.txt"));
    app.open_bulk_rename_prompt();

    app.handle_event(Event::Paste("renamed-".to_string()))
        .expect("paste event should update bulk rename prompt");

    assert_eq!(app.bulk_rename_new_name(0), "renamed-alpha.txt");
    assert_eq!(app.bulk_rename_new_name(1), "beta.txt");
    assert_eq!(app.bulk_rename_cursor_col(), "renamed-".chars().count());

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_updates_archive_create_name_at_cursor() {
    let root = temp_path("paste-archive-create");
    fs::write(root.join("alpha.txt"), "alpha").expect("failed to write alpha");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_archive_create_prompt();

    app.handle_event(Event::Paste("-backup".to_string()))
        .expect("paste event should update archive create prompt");

    assert_eq!(app.archive_create_input(), "alpha.txt-backup.zip");
    assert_eq!(
        app.archive_create_cursor_col(),
        "alpha.txt-backup".chars().count()
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn pasted_text_updates_archive_password_and_flattens_newlines() {
    let root = temp_path("paste-archive-password");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_archive_password_prompt(
        crate::app::jobs::ArchiveExtractRequest {
            token: 0,
            archives: vec![root.join("archive.zip")],
            password: None,
            batch: crate::app::jobs::ArchiveExtractBatchState::new(1, 0),
        },
        None,
    );

    app.handle_event(Event::Paste("pa\nss".to_string()))
        .expect("paste event should update archive password prompt");

    assert_eq!(app.archive_password_input(), "pa ss");
    assert_eq!(app.archive_password_cursor_col(), "pa ss".chars().count());

    cleanup_app_temp_root(app, root);
}

fn app_with_offscreen_selected_dir(label: &str) -> (PathBuf, PathBuf, App) {
    let root = temp_path(label);
    let offscreen = root.join("offscreen");
    let child = root.join("child");
    fs::create_dir_all(&offscreen).expect("failed to create offscreen dir");
    fs::create_dir_all(&child).expect("failed to create child dir");
    fs::write(child.join("visible.txt"), "visible").expect("failed to write visible file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(offscreen.clone());
    app.set_dir(child).expect("entering child should succeed");
    wait_for_directory_load(&mut app);

    (root, offscreen, app)
}

fn app_with_trash_and_normal_selection(label: &str) -> (PathBuf, App) {
    let root = temp_path(label);
    let trash_root = root.join("trash");
    let trashed = trash_root.join("trashed.txt");
    let normal = root.join("normal.txt");
    fs::create_dir_all(&trash_root).expect("failed to create trash root");
    fs::write(&trashed, "trashed").expect("failed to write trashed file");
    fs::write(&normal, "normal").expect("failed to write normal file");

    let mut app = App::new_at(trash_root).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(trashed);
    app.navigation.selected_paths.insert(normal);

    (root, app)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn app_in_empty_dir_with_offscreen_file(label: &str) -> (PathBuf, PathBuf, App) {
    let root = temp_path(label);
    let empty = root.join("empty");
    let file_path = root.join("note.txt");
    fs::create_dir_all(&empty).expect("failed to create empty dir");
    fs::write(&file_path, "hello").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(file_path.clone());
    app.set_dir(empty)
        .expect("entering empty dir should succeed");
    wait_for_directory_load(&mut app);

    (root, file_path, app)
}

#[test]
fn minus_zooms_grid_when_no_yanked_clipboard() {
    let root = temp_path("grid-minus-zoom-no-yank");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::Grid;
    app.navigation.zoom_level = 1;

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('-'))))
        .expect("minus should zoom out without a yanked clipboard");

    assert_eq!(app.navigation.zoom_level, 0);
    assert_eq!(app.status_message(), "Grid zoom set to 0");

    fs::remove_dir_all(&root).unwrap();
}

#[cfg(unix)]
#[test]
fn minus_links_yanked_paths_in_grid_view() {
    let root = temp_path("grid-minus-link-yank");
    let source_dir = root.join("source");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::create_dir_all(&dest_dir).expect("failed to create dest dir");
    let source = source_dir.join("note.txt");
    fs::write(&source, "note").expect("failed to write source file");

    let mut app = App::new_at(source_dir.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.yank();
    app.navigation.cwd = dest_dir.clone();
    app.navigation.view_mode = ViewMode::Grid;
    app.navigation.zoom_level = 1;

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('-'))))
        .expect("minus should link with a yanked clipboard");

    assert_eq!(fs::read_link(dest_dir.join("note.txt")).unwrap(), source);
    assert_eq!(app.navigation.zoom_level, 1);
    assert_eq!(app.status_message(), "Created symlink \"note.txt\"");

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn f2_renames_outside_trash_but_not_inside_trash() {
    let root = temp_path("f2-rename-not-restore");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("note.txt"), "hello").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::F(2))))
        .expect("F2 should open rename outside trash");
    assert!(app.rename_is_open());
    app.overlays.rename = None;

    app.navigation.in_trash = true;
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::F(2))))
        .expect("F2 should be ignored in trash");
    assert!(!app.rename_is_open());
    assert!(!app.restore_is_open());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn r_renames_outside_trash_and_restores_inside_trash() {
    let root = temp_path("r-rename-restore-context");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("note.txt"), "hello").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('r'))))
        .expect("r should open rename outside trash");
    assert!(app.rename_is_open());
    app.overlays.rename = None;

    app.navigation.in_trash = true;
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('r'))))
        .expect("r should open restore inside trash");
    assert!(!app.rename_is_open());
    assert!(app.restore_is_open());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn shift_slash_opens_and_closes_help_overlay() {
    let root = temp_path("help-shift-slash");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::SHIFT,
    )))
    .expect("shift-slash should open help");
    assert!(app.overlays.help);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('/'),
        KeyModifiers::SHIFT,
    )))
    .expect("shift-slash should close help");
    assert!(!app.overlays.help);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn capital_v_toggles_preview_pane() {
    let root = temp_path("toggle-preview-pane");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    assert!(app.preview_visible());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('V'))))
        .expect("V should hide preview");
    assert!(!app.preview_visible());
    assert_eq!(app.status_message(), "Preview hidden");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('V'))))
        .expect("V should show preview");
    assert!(app.preview_visible());
    assert_eq!(app.status_message(), "Preview shown");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn capital_p_enters_and_escape_keys_exit_fullscreen_preview() {
    let root = temp_path("fullscreen-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    assert!(app.preview_visible());
    assert!(!app.preview_fullscreen());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    assert!(app.preview_visible());
    assert!(app.preview_fullscreen());
    assert_eq!(app.status_message(), "Fullscreen preview");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("Esc should leave fullscreen preview");
    assert!(app.preview_visible());
    assert!(!app.preview_fullscreen());
    assert_eq!(app.status_message(), "Exited fullscreen preview");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview again");
    assert!(app.preview_fullscreen());

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )))
    .expect("Ctrl+C should leave fullscreen preview");
    assert!(app.preview_visible());
    assert!(!app.preview_fullscreen());
    assert_eq!(app.status_message(), "Exited fullscreen preview");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn fullscreen_preview_ignores_hidden_browser_actions_but_keeps_preview_controls() {
    let root = temp_path("fullscreen-preview-input");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    let contents = (0..120)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&alpha, &contents).expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    wait_for_directory_load(&mut app);
    let alpha_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == alpha)
        .expect("alpha should be visible");
    app.select_index(alpha_index);
    app.set_frame_state(FrameState {
        preview_panel: Some(Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 20,
        }),
        preview_rows_visible: 16,
        preview_cols_visible: 58,
        ..FrameState::default()
    });
    app.preview.state.content = crate::preview::PreviewContent::new(
        crate::preview::PreviewKind::Text,
        (0..120)
            .map(|i| ratatui::text::Line::from(format!("line {i}")))
            .collect(),
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    assert!(app.preview_fullscreen());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('s'))))
        .expect("sort should be ignored in fullscreen preview");
    assert_eq!(app.navigation.selected_paths.len(), 0);
    assert!(app.preview_fullscreen());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('?'))))
        .expect("help should open in fullscreen preview");
    assert!(app.overlays.help);
    assert!(app.preview_fullscreen());
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("Esc should close help overlay first");
    assert!(!app.overlays.help);
    assert!(app.preview_fullscreen());

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('J'),
        KeyModifiers::SHIFT,
    )))
    .expect("preview scroll should work in fullscreen preview");
    assert!(app.preview.state.scroll > 0);
    assert_eq!(app.navigation.selected, alpha_index);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('j'))))
        .expect("j should navigate inside fullscreen preview");
    assert!(app.preview_fullscreen());
    assert_eq!(app.navigation.selected, alpha_index + 1);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn fullscreen_preview_preserves_browser_viewport_to_avoid_exit_flicker() {
    let root = temp_path("fullscreen-preview-exit-viewport");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["alpha.txt", "beta.txt", "logo.png"] {
        fs::write(root.join(name), name).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.set_frame_state(FrameState {
        entries_panel: Some(Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 20,
        }),
        metrics: ViewMetrics {
            cols: 1,
            rows_visible: 18,
        },
        ..FrameState::default()
    });
    let logo_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == root.join("logo.png"))
        .expect("logo should be visible");
    app.select_index(logo_index);
    assert_eq!(app.navigation.scroll_row, 0);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.set_frame_state(FrameState {
        preview_panel: Some(Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 20,
        }),
        preview_rows_visible: 18,
        preview_cols_visible: 78,
        ..FrameState::default()
    });

    assert_eq!(
        app.navigation.scroll_row, 0,
        "fullscreen frames should not collapse the browser viewport to the selected row"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("Esc should leave fullscreen preview");
    assert_eq!(app.navigation.scroll_row, 0);

    fs::remove_dir_all(root).ok();
}

#[test]
fn fullscreen_preview_exits_for_selection_actions() {
    let root = temp_path("fullscreen-preview-selection-actions");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "alpha").expect("failed to write file");
    fs::write(root.join("beta.txt"), "beta").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    let selected = app
        .selected_entry()
        .expect("entry should be selected")
        .path
        .clone();

    let selected_index = app.navigation.selected;
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char(' '))))
        .expect("ToggleSelection should stay fullscreen and toggle selection");
    assert!(app.preview_fullscreen());
    assert_eq!(app.navigation.selected, selected_index);
    assert!(app.navigation.selected_paths.contains(&selected));

    app.clear_selection();
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
    )))
    .expect("SelectAll should exit fullscreen and select visible entries");
    assert!(!app.preview_fullscreen());
    assert_eq!(
        app.navigation.selected_paths.len(),
        app.navigation.entries.len()
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn fullscreen_preview_stays_for_same_directory_navigation() {
    let root = temp_path("fullscreen-preview-same-directory-navigation");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["alpha.txt", "beta.txt", "gamma.txt", "omega.txt"] {
        fs::write(root.join(name), name).expect("failed to write file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.view_mode = ViewMode::List;
    app.set_frame_state(FrameState {
        metrics: ViewMetrics {
            cols: 1,
            rows_visible: 2,
        },
        ..FrameState::default()
    });

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::End)))
        .expect("JumpLast should stay fullscreen and select last entry");
    assert!(app.preview_fullscreen());
    assert_eq!(app.navigation.selected, app.navigation.entries.len() - 1);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::PageUp)))
        .expect("PageUp should stay fullscreen and move selection");
    assert!(app.preview_fullscreen());
    assert!(app.navigation.selected < app.navigation.entries.len() - 1);

    app.navigation.view_mode = ViewMode::Grid;
    app.set_frame_state(FrameState {
        metrics: ViewMetrics {
            cols: 2,
            rows_visible: 2,
        },
        ..FrameState::default()
    });
    app.select_index(0);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Right)))
        .expect("grid NavRight should stay fullscreen and move selection");
    assert!(app.preview_fullscreen());
    assert_eq!(app.navigation.selected, 1);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Left)))
        .expect("grid NavLeft should stay fullscreen and move selection");
    assert!(app.preview_fullscreen());
    assert_eq!(app.navigation.selected, 0);

    fs::remove_dir_all(root).ok();
}

#[test]
fn fullscreen_preview_exits_for_explicit_overlays_and_terminal_tasks() {
    let root = temp_path("fullscreen-preview-explicit-actions");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('c'))))
        .expect("CopyPath should exit fullscreen and open copy overlay");
    assert!(!app.preview_fullscreen());
    assert!(app.copy_is_open());
    app.overlays.copy = None;

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('z'))))
        .expect("Zoxide should exit fullscreen and queue zoxide picker");
    assert!(!app.preview_fullscreen());
    assert_eq!(app.pending_terminal_task, Some(PendingTerminalTask::Zoxide));
    app.pending_terminal_task = None;

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('y'))))
        .expect("Yank should exit fullscreen and yank focused entry");
    assert!(!app.preview_fullscreen());
    let clipboard = app
        .jobs
        .clipboard
        .as_ref()
        .expect("yank should set clipboard");
    assert_eq!(clipboard.op, ClipOp::Yank);
    assert_eq!(clipboard.paths, vec![file_path]);
    app.jobs.clipboard = None;

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('d'))))
        .expect("Trash should exit fullscreen and open trash prompt");
    assert!(!app.preview_fullscreen());
    assert!(app.trash_is_open());

    fs::remove_dir_all(root).ok();
}
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn fullscreen_preview_allows_file_open_and_exits_first() {
    let root = temp_path("fullscreen-preview-open-file");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    let file_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == file_path)
        .expect("file should be visible");
    app.select_index(file_index);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    assert!(app.preview_fullscreen());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("Enter should open selected file from fullscreen preview");

    assert!(!app.preview_fullscreen());
    let opened = read_open_capture(&capture);
    assert_eq!(opened, file_path.display().to_string());

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn fullscreen_preview_allows_folder_open_and_exits_first() {
    let root = temp_path("fullscreen-preview-open-folder");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let folder = root.join("folder");
    fs::create_dir_all(&folder).expect("failed to create folder");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    let folder_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == folder)
        .expect("folder should be visible");
    app.select_index(folder_index);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen folder preview");
    assert!(app.preview_fullscreen());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('o'))))
        .expect("Open should launch selected folder from fullscreen preview");

    assert!(!app.preview_fullscreen());
    assert_eq!(app.navigation.cwd, root);
    let opened = read_open_capture(&capture);
    assert_eq!(opened, folder.display().to_string());

    fs::remove_dir_all(root).ok();
}

#[test]
fn fullscreen_preview_allows_file_open_with_and_directory_enter() {
    let root = temp_path("fullscreen-preview-open-with");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let folder = root.join("folder");
    let file_path = root.join("note.txt");
    fs::create_dir_all(&folder).expect("failed to create folder");
    fs::write(&file_path, "hello").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    let file_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == file_path)
        .expect("file should be visible");
    app.select_index(file_index);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('O'))))
        .expect("Open With should be allowed for files from fullscreen preview");
    assert!(!app.preview_fullscreen());
    if app.open_with_is_open() {
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
            .expect("Esc should close Open With overlay");
    }
    let folder_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == folder)
        .expect("folder should be visible");
    app.select_index(folder_index);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen folder preview");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('O'))))
        .expect("Open With should be allowed for folders from fullscreen preview");
    assert!(!app.preview_fullscreen());
    if app.open_with_is_open() {
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
            .expect("Esc should close folder Open With overlay");
    }

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('P'))))
        .expect("P should fullscreen folder preview again");
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("Enter should enter folders before leaving fullscreen");
    assert!(app.preview_fullscreen());
    assert_eq!(app.navigation.cwd, root);
    wait_for_directory_load(&mut app);

    assert!(!app.preview_fullscreen());
    assert_eq!(app.navigation.cwd, folder);

    fs::remove_dir_all(root).ok();
}

#[test]
fn q_sets_should_quit() {
    let root = temp_path("quit-shortcut");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    assert!(!app.should_quit);
    assert!(app.should_change_directory_on_quit);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('q'))))
        .expect("q should request quit");

    assert!(app.should_quit);
    assert!(app.should_change_directory_on_quit);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_enter_confirms_hovered_entry() {
    let root = temp_path("chooser-enter-hovered");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.enable_chooser_mode();

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should confirm chooser selection");

    assert!(app.should_quit);
    assert!(app.should_change_directory_on_quit);
    assert_eq!(
        app.chooser_exit.as_ref(),
        Some(&ChooserExit::Confirmed(vec![file_path]))
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_enter_confirms_sorted_selection() {
    let root = temp_path("chooser-enter-selection");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    let gamma = root.join("gamma.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");
    fs::write(&gamma, "gamma").expect("failed to write gamma");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(gamma.clone());
    app.navigation.selected_paths.insert(alpha.clone());
    let beta_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == beta)
        .expect("beta should be visible");
    app.select_index(beta_index);
    app.enable_chooser_mode();

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should confirm selected chooser paths");

    assert_eq!(
        app.chooser_exit.as_ref(),
        Some(&ChooserExit::Confirmed(vec![alpha, gamma]))
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_enter_confirms_selection_from_multiple_directories() {
    let root = temp_path("chooser-enter-multi-directory-selection");
    let child = root.join("child");
    let alpha = root.join("alpha.txt");
    let beta = child.join("beta.txt");
    fs::create_dir_all(&child).expect("failed to create child dir");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());
    app.enable_chooser_mode();

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should confirm selected chooser paths");

    assert_eq!(
        app.chooser_exit.as_ref(),
        Some(&ChooserExit::Confirmed(vec![alpha, beta]))
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_quit_cancels_without_cd() {
    let root = temp_path("chooser-cancel");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.enable_chooser_mode();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('q'))))
        .expect("q should cancel chooser mode");

    assert!(app.should_quit);
    assert!(!app.should_change_directory_on_quit);
    assert_eq!(app.chooser_exit.as_ref(), Some(&ChooserExit::Cancelled));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_esc_keeps_normal_selection_clear_behavior() {
    let root = temp_path("chooser-esc-selection");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(file_path);
    app.enable_chooser_mode();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("esc should keep normal selection behavior");

    assert!(!app.should_quit);
    assert!(app.should_change_directory_on_quit);
    assert_eq!(app.chooser_exit, None);
    assert!(app.navigation.selected_paths.is_empty());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_right_enters_directory_instead_of_confirming() {
    let root = temp_path("chooser-right-directory");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("failed to create nested directory");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.view_mode = ViewMode::List;
    app.select_index(0);
    app.enable_chooser_mode();

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Right)))
        .expect("right should enter the selected directory");
    wait_for_directory_load(&mut app);

    assert_eq!(app.navigation.cwd, nested);
    assert!(!app.should_quit);
    assert_eq!(app.chooser_exit, None);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_open_or_enter_action_keeps_normal_behavior() {
    let root = temp_path("chooser-open-or-enter-directory");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("failed to create nested directory");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.view_mode = ViewMode::List;
    app.select_index(0);
    app.enable_chooser_mode();

    app.dispatch_action(Action::OpenOrEnter)
        .expect("open_or_enter should keep normal behavior in chooser mode");
    wait_for_directory_load(&mut app);

    assert_eq!(app.navigation.cwd, nested);
    assert!(!app.should_quit);
    assert_eq!(app.chooser_exit, None);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn modified_plain_shortcut_does_not_trigger_plain_action() {
    let root = temp_path("modified-plain-shortcut");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    assert!(!app.should_quit);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL,
    )))
    .expect("Ctrl-Q should be ignored by the plain q binding");
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::ALT,
    )))
    .expect("Alt-Q should be ignored by the plain q binding");

    assert!(!app.should_quit);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn capital_q_quits_without_changing_directory() {
    let root = temp_path("quit-without-cd-shortcut");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    assert!(!app.should_quit);
    assert!(app.should_change_directory_on_quit);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('Q'))))
        .expect("Q should request quit without cd");

    assert!(app.should_quit);
    assert!(!app.should_change_directory_on_quit);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn capital_d_opens_permanent_delete_prompt_outside_trash() {
    let root = temp_path("delete-permanently-shortcut");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("gone.txt"), "bye").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('D'))))
        .expect("D should open permanent delete prompt");

    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 1 selected file?");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn capital_d_permanent_delete_prompt_uses_selection() {
    let root = temp_path("delete-permanently-selection-shortcut");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha);
    app.navigation.selected_paths.insert(beta);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('D'))))
        .expect("D should open permanent delete prompt");

    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 2 files?");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn trash_prompt_uses_selection_when_selection_is_offscreen() {
    let (root, offscreen, mut app) =
        app_with_offscreen_selected_dir("trash-offscreen-selection-shortcut");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('d'))))
        .expect("d should open trash prompt");

    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Trash 1 selected folder?");
    assert_eq!(app.trash_target_path_at(0), Some(offscreen.as_path()));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn trash_prompt_keeps_normal_selection_non_permanent_from_trash() {
    let root = temp_path("trash-normal-selection-from-trash");
    let trash_root = root.join("trash");
    let normal = root.join("normal.txt");
    fs::create_dir_all(&trash_root).expect("failed to create trash root");
    fs::write(&normal, "normal").expect("failed to write normal file");

    let mut app = App::new_at(trash_root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(normal.clone());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('d'))))
        .expect("d should open trash prompt");

    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Trash 1 selected file?");
    assert_eq!(app.trash_target_path_at(0), Some(normal.as_path()));
    assert_eq!(
        app.trash_target_label_at(0),
        Some(normal.display().to_string())
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn trash_prompt_permanently_deletes_trash_only_selection() {
    let root = temp_path("trash-only-selection-from-trash");
    let trashed = root.join("trashed.txt");
    fs::create_dir_all(&root).expect("failed to create trash root");
    fs::write(&trashed, "trashed").expect("failed to write trashed file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.in_trash = true;
    app.navigation.selected_paths.insert(trashed.clone());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('d'))))
        .expect("d should open permanent delete prompt");

    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 1 selected file?");
    assert_eq!(app.trash_target_path_at(0), Some(trashed.as_path()));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn trash_prompt_refuses_mixed_trash_and_normal_selection() {
    let (root, mut app) = app_with_trash_and_normal_selection("trash-mixed-selection");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('d'))))
        .expect("d should reject mixed selection");

    assert!(!app.trash_is_open());
    assert_eq!(
        app.status_message(),
        "Selection mixes trash and normal files"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn permanent_delete_prompt_accepts_mixed_trash_and_normal_selection() {
    let (root, mut app) = app_with_trash_and_normal_selection("delete-mixed-selection");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('D'))))
        .expect("D should open permanent delete prompt");

    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 2 files?");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn enter_opens_selected_file_with_system_opener() {
    let root = temp_path("enter-opens-file");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should open selected file");

    let opened = read_open_capture(&capture);
    assert_eq!(opened, file_path.display().to_string());

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn newline_key_event_also_opens_selected_file() {
    let root = temp_path("newline-opens-file");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('\n'),
        KeyModifiers::NONE,
    )))
    .expect("newline key event should open selected file");

    let opened = read_open_capture(&capture);
    assert_eq!(opened, file_path.display().to_string());

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn enter_queues_terminal_default_app_for_selected_file() {
    let root = temp_path("enter-terminal-default");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());
    let _default_guard = DefaultOpenWithAppGuard::install(fake_default_open_with_app(
        "Helix",
        "hx",
        vec![file_path.display().to_string()],
        true,
    ));

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should queue terminal default app");

    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Command {
            program: "hx".to_string(),
            args: vec![file_path.display().to_string()],
        })
    );
    assert!(app.status.is_empty());
    assert!(
        !capture.exists(),
        "system opener should not run for terminal default app"
    );

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn enter_opens_selection_when_current_directory_is_empty() {
    let (root, file_path, mut app) =
        app_in_empty_dir_with_offscreen_file("enter-empty-dir-opens-selection");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    assert!(app.selected_entry().is_none());
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should open offscreen selection");

    let opened = read_open_capture(&capture);
    assert_eq!(opened, file_path.display().to_string());

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn enter_queues_terminal_default_app_for_single_offscreen_selection() {
    let (root, file_path, mut app) =
        app_in_empty_dir_with_offscreen_file("enter-offscreen-terminal-default");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());
    let _default_guard = DefaultOpenWithAppGuard::install(fake_default_open_with_app(
        "Helix",
        "hx",
        vec![file_path.display().to_string()],
        true,
    ));

    assert!(app.selected_entry().is_none());
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should queue terminal default app");

    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Command {
            program: "hx".to_string(),
            args: vec![file_path.display().to_string()],
        })
    );
    assert!(app.status.is_empty());
    assert!(
        !capture.exists(),
        "system opener should not run for terminal default app"
    );

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn open_action_keeps_system_opener_for_gui_default_app() {
    let root = temp_path("open-gui-default");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());
    let _default_guard = DefaultOpenWithAppGuard::install(fake_default_open_with_app(
        "Text Editor",
        "gedit",
        vec![file_path.display().to_string()],
        false,
    ));

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('o'))))
        .expect("o should use system opener for GUI defaults");

    let opened = read_open_capture(&capture);
    assert_eq!(opened, file_path.display().to_string());
    assert_eq!(app.pending_terminal_task, None);
    assert_eq!(app.status, "Opened note.txt");

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn open_action_uses_editor_fallback_when_no_desktop_app_exists() {
    let root = temp_path("open-editor-fallback-no-desktop");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("main.rs");
    fs::write(&file_path, "fn main() {}\n").expect("failed to write temp file");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());
    let _found_guard = OpenWithAppsFoundGuard::install(false);
    let _editor_guard = EditorFallbackAppGuard::install(fake_default_open_with_app(
        "Helix ($VISUAL)",
        "hx",
        vec![file_path.display().to_string()],
        true,
    ));

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('o'))))
        .expect("o should use editor fallback when no desktop app exists");

    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Command {
            program: "hx".to_string(),
            args: vec![file_path.display().to_string()],
        })
    );
    assert!(app.status.is_empty());
    assert!(
        !capture.exists(),
        "system opener should not run when editor fallback is available"
    );

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn enter_opens_selected_entries_with_system_opener() {
    let root = temp_path("enter-opens-selection");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    let gamma = root.join("gamma.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");
    fs::write(&gamma, "gamma").expect("failed to write gamma");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(gamma.clone());
    app.navigation.selected_paths.insert(alpha.clone());
    let beta_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == beta)
        .expect("beta should be visible");
    app.select_index(beta_index);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .expect("enter should open selected entries");

    let opened = read_open_capture(&capture);
    let opened: Vec<_> = opened.lines().map(str::to_owned).collect();
    assert_eq!(
        opened,
        vec![alpha.display().to_string(), gamma.display().to_string()]
    );
    assert_eq!(app.status, "Opened 2 items");

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn open_action_opens_selected_entries_with_system_opener() {
    let root = temp_path("open-action-opens-selection");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('o'))))
        .expect("o should open selected entries");

    let opened = read_open_capture(&capture);
    let opened: Vec<_> = opened.lines().map(str::to_owned).collect();
    assert_eq!(
        opened,
        vec![alpha.display().to_string(), beta.display().to_string()]
    );
    assert_eq!(app.status, "Opened 2 items");

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn open_action_opens_selected_entries_from_multiple_directories() {
    let root = temp_path("open-action-opens-cross-directory-selection");
    let child = root.join("child");
    let alpha = root.join("alpha.txt");
    let beta = child.join("beta.txt");
    fs::create_dir_all(&child).expect("failed to create child dir");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());
    app.set_dir(child.clone())
        .expect("entering child should succeed");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('o'))))
        .expect("o should open selected entries");

    let opened = read_open_capture(&capture);
    let opened: Vec<_> = opened.lines().map(str::to_owned).collect();
    assert_eq!(
        opened,
        vec![alpha.display().to_string(), beta.display().to_string()]
    );
    assert_eq!(app.status, "Opened 2 items");

    fs::remove_dir_all(root).ok();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn open_action_keeps_system_opener_for_multiple_selection_with_terminal_default() {
    let root = temp_path("open-selection-terminal-default");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());
    let _default_guard = DefaultOpenWithAppGuard::install(fake_default_open_with_app(
        "Helix",
        "hx",
        vec![alpha.display().to_string()],
        true,
    ));

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('o'))))
        .expect("o should use system opener for multiple selection");

    let opened = read_open_capture(&capture);
    let opened: Vec<_> = opened.lines().map(str::to_owned).collect();
    assert_eq!(
        opened,
        vec![alpha.display().to_string(), beta.display().to_string()]
    );
    assert_eq!(app.pending_terminal_task, None);
    assert_eq!(app.status, "Opened 2 items");

    fs::remove_dir_all(root).ok();
}

#[test]
fn c_opens_and_esc_closes_copy_overlay() {
    let root = temp_path("copy-overlay-shortcut");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("report.txt"), "hello").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('c'))))
        .expect("c should open copy overlay");
    assert!(app.copy_is_open());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("esc should close copy overlay");
    assert!(!app.copy_is_open());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn g_opens_goto_overlay_and_goto_shortcuts_keep_g_for_top() {
    let root = temp_path("goto-overlay-shortcut");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.jump_last();
    assert_eq!(
        app.navigation.selected, 2,
        "G behavior should still reach the last item"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('g'))))
        .expect("g should open go-to overlay");
    assert!(app.goto_is_open());
    assert_eq!(
        app.navigation.selected, 2,
        "opening the go-to overlay should not move selection"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('g'))))
        .expect("g inside go-to overlay should jump to top");
    assert!(!app.goto_is_open());
    assert_eq!(
        app.navigation.selected, 0,
        "go-to g shortcut should move to the top item"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('G'))))
        .expect("capital G should still move to the last item");
    assert_eq!(
        app.navigation.selected, 2,
        "capital G should keep the old bottom-jump behavior"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn tab_and_shift_tab_cycle_sidebar_locations_and_skip_section_rows() {
    let root = temp_path("tab-cycles-pinned-places");
    let downloads = root.join("downloads");
    let usb = root.join("usb");
    fs::create_dir_all(&downloads).expect("failed to create downloads dir");
    fs::create_dir_all(&usb).expect("failed to create usb dir");

    let sidebar_rows = || {
        vec![
            SidebarRow::Item(SidebarItem::new(
                SidebarItemKind::Home,
                "Home",
                "H",
                root.clone(),
                root.clone(),
            )),
            SidebarRow::Item(SidebarItem::new(
                SidebarItemKind::Downloads,
                "Downloads",
                "D",
                downloads.clone(),
                downloads.clone(),
            )),
            SidebarRow::Section { title: "Devices" },
            SidebarRow::Item(SidebarItem::new(
                SidebarItemKind::Device { removable: true },
                "USB",
                "U",
                usb.clone(),
                usb.clone(),
            )),
        ]
    };

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.sidebar = sidebar_rows();

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Tab)))
        .expect("tab should cycle sidebar locations");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, downloads);

    app.navigation.sidebar = sidebar_rows();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Tab)))
        .expect("tab should continue into device rows");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, usb);

    app.navigation.sidebar = sidebar_rows();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Tab)))
        .expect("tab should wrap back to the first sidebar location");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, root);

    app.navigation.sidebar = sidebar_rows();
    app.set_dir(usb.clone()).expect("device path should open");
    wait_for_directory_load(&mut app);

    app.navigation.sidebar = sidebar_rows();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::BackTab)))
        .expect("shift-tab should walk sidebar locations in reverse");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, downloads);

    cleanup_app_temp_root(app, root);
}

#[cfg(unix)]
#[test]
fn tab_and_shift_tab_match_symlinked_sidebar_locations_by_identity() {
    use std::os::unix::fs::symlink;

    let root = temp_path("tab-cycles-symlinked-places");
    let target = root.join("target");
    let linked = root.join("linked");
    let next = root.join("next");
    fs::create_dir_all(&target).expect("failed to create target dir");
    fs::create_dir_all(&next).expect("failed to create next dir");
    symlink(&target, &linked).expect("failed to create sidebar symlink");

    let root_identity = root.canonicalize().expect("root should canonicalize");
    let target_identity = target.canonicalize().expect("target should canonicalize");
    let next_identity = next.canonicalize().expect("next should canonicalize");
    let sidebar_rows = || {
        vec![
            SidebarRow::Item(SidebarItem::new(
                SidebarItemKind::Home,
                "Home",
                "H",
                root.clone(),
                root_identity.clone(),
            )),
            SidebarRow::Item(SidebarItem::new(
                SidebarItemKind::Custom,
                "Linked",
                "L",
                linked.clone(),
                target_identity.clone(),
            )),
            SidebarRow::Item(SidebarItem::new(
                SidebarItemKind::Downloads,
                "Next",
                "N",
                next.clone(),
                next_identity.clone(),
            )),
        ]
    };

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.set_dir(linked.clone())
        .expect("symlinked place should open");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, target_identity);

    app.navigation.sidebar = sidebar_rows();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Tab)))
        .expect("tab should advance past the symlinked sidebar location");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, next_identity);

    app.set_dir(linked.clone())
        .expect("symlinked place should reopen");
    wait_for_directory_load(&mut app);
    app.navigation.sidebar = sidebar_rows();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::BackTab)))
        .expect("shift-tab should walk backward from the symlinked sidebar location");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, root_identity);

    app.set_dir(next.clone()).expect("next place should open");
    wait_for_directory_load(&mut app);
    app.navigation.sidebar = sidebar_rows();
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::BackTab)))
        .expect("shift-tab should open the symlinked sidebar location");
    wait_for_directory_load(&mut app);
    assert_eq!(app.navigation.cwd, target_identity);

    cleanup_app_temp_root(app, root);
}

#[test]
fn repeated_down_arrow_is_throttled_without_starving_hold_repeat() {
    let root = temp_path("down-arrow-debounce");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.select_index(0);

    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .expect("first down arrow should be handled");

    let throttled_at = app
        .input
        .last_navigation_key
        .expect("accepted navigation key should be tracked")
        .1;
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .expect("second down arrow should be handled");

    assert_eq!(app.navigation.selected, 1);
    assert_eq!(
        app.input
            .last_navigation_key
            .expect("throttled navigation key should keep prior timestamp")
            .1,
        throttled_at
    );

    app.input.last_navigation_key = Some((
        NavigationRepeatKey::Down,
        Instant::now() - KEY_REPEAT_NAV_INTERVAL - Duration::from_millis(1),
    ));
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .expect("third down arrow should be handled");

    assert_eq!(app.navigation.selected, 2);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn high_frequency_alt_right_scrolls_preview_instead_of_history() {
    let root = temp_path("preview-horizontal-alt-right");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("long.rs");
    fs::write(
        &file_path,
        "fn main() { let preview_line = \"this line is intentionally long for horizontal preview scrolling\"; }\n",
    )
    .expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.input.wheel_profile = WheelProfile::HighFrequency;
    app.input.last_wheel_target = Some(WheelTarget::Entries);
    app.select_index(0);
    app.input.last_selection_change_at =
        Instant::now() - PREVIEW_AUTO_FOCUS_DELAY - Duration::from_millis(1);
    app.set_frame_state(FrameState {
        preview_panel: Some(Rect {
            x: 21,
            y: 0,
            width: 20,
            height: 8,
        }),
        preview_rows_visible: 6,
        preview_cols_visible: 12,
        ..FrameState::default()
    });

    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)))
        .expect("alt-right should be handled");

    assert!(app.preview.state.horizontal_scroll > 0);
    assert_eq!(app.navigation.selected, 0);
    assert_eq!(app.input.last_wheel_target, Some(WheelTarget::Preview));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn high_frequency_down_arrow_keeps_browser_navigation() {
    let root = temp_path("high-frequency-down-keeps-browser");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.input.wheel_profile = WheelProfile::HighFrequency;
    app.select_index(0);
    app.input.last_wheel_target = Some(WheelTarget::Preview);
    app.input.last_selection_change_at =
        Instant::now() - PREVIEW_AUTO_FOCUS_DELAY - Duration::from_millis(1);
    app.set_frame_state(FrameState {
        preview_panel: Some(Rect {
            x: 21,
            y: 0,
            width: 20,
            height: 8,
        }),
        preview_rows_visible: 4,
        preview_cols_visible: 20,
        ..FrameState::default()
    });

    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .expect("down arrow should be handled");

    assert_eq!(app.navigation.selected, 1);
    assert_eq!(app.preview.state.scroll, 0);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn high_frequency_right_arrow_in_list_view_still_enters_directory() {
    let root = temp_path("high-frequency-right-enters");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("failed to create child dir");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.input.wheel_profile = WheelProfile::HighFrequency;
    app.select_index(0);
    app.input.last_wheel_target = Some(WheelTarget::Preview);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::NONE,
    )))
    .expect("right arrow should be handled");
    wait_for_directory_load(&mut app);

    assert_eq!(app.navigation.cwd, child);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn rapid_audio_navigation_defers_second_cold_heavy_preview_refresh() {
    let root = temp_path("rapid-audio-navigation-preview-defer");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.mp3", "b.mp3", "c.mp3"] {
        fs::write(root.join(name), name).expect("failed to write temp audio file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.set_media_ffprobe_available_for_tests(false);
    app.set_media_ffmpeg_available_for_tests(false);
    app.input.last_selection_change_at =
        Instant::now() - WHEEL_SCROLL_BURST_WINDOW - Duration::from_millis(1);

    let initial_token = app.preview.state.token;
    app.move_vertical(1);

    // Cold heavy audio is always deferred regardless of burst window state.
    assert_eq!(app.navigation.selected, 1);
    assert_eq!(app.preview.state.token, initial_token);
    assert!(app.preview.state.deferred_refresh_at.is_some());

    app.move_vertical(1);

    assert_eq!(app.navigation.selected, 2);
    assert_eq!(app.preview.state.token, initial_token);
    assert!(app.preview.state.deferred_refresh_at.is_some());

    thread::sleep(HIGH_FREQUENCY_PREVIEW_REFRESH_DELAY + Duration::from_millis(20));
    assert!(app.process_preview_refresh_timers());
    assert!(app.preview.state.token > initial_token);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn rapid_key_navigation_defers_preview_for_non_heavy_files() {
    let root = temp_path("rapid-key-nav-preview-defer");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.select_index(0);

    // First move: last_key_nav_at is in the past → Immediate preview.
    let token_before = app.preview.state.token;
    app.move_vertical_keyboard(1);
    assert_eq!(app.navigation.selected, 1);
    assert!(
        app.preview.state.token > token_before,
        "first move should trigger an immediate preview refresh"
    );
    assert!(
        app.preview.state.deferred_refresh_at.is_none(),
        "first move should not leave a deferred timer"
    );

    // Second move within KEY_NAV_RAPID_THRESHOLD → Deferred preview.
    let token_before = app.preview.state.token;
    app.move_vertical_keyboard(1);
    assert_eq!(app.navigation.selected, 2);
    assert_eq!(
        app.preview.state.token, token_before,
        "second rapid move should not immediately refresh preview"
    );
    assert!(
        app.preview.state.deferred_refresh_at.is_some(),
        "second rapid move should schedule a deferred refresh"
    );

    // After the deferred delay the preview fires.
    thread::sleep(HIGH_FREQUENCY_PREVIEW_REFRESH_DELAY + Duration::from_millis(20));
    assert!(app.process_preview_refresh_timers());
    assert!(
        app.preview.state.token > token_before,
        "deferred preview should fire after pause"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn rapid_key_navigation_clears_directory_totals_until_deferred_refresh_runs() {
    let root = temp_path("rapid-key-nav-directory-stats");
    for dir in ["a-dir", "b-dir"] {
        let path = root.join(dir);
        fs::create_dir_all(&path).expect("failed to create temp dir");
        fs::write(path.join("file.txt"), vec![b'x'; 100]).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.select_index(0);
    wait_for_background_preview(&mut app);
    for _ in 0..100 {
        let _ = app.process_directory_stats_timer();
        let _ = app.process_background_jobs();
        if matches!(
            app.preview.state.directory_stats,
            Some(PreviewDirectoryStatsState::Complete { .. })
        ) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(
        app.preview.state.directory_stats,
        Some(PreviewDirectoryStatsState::Complete { .. })
    ));

    app.input.last_key_nav_at = Instant::now();
    let token_before = app.preview.state.token;
    app.move_vertical_keyboard(1);

    assert_eq!(app.navigation.selected, 1);
    assert_eq!(app.preview.state.token, token_before);
    assert!(app.preview.state.deferred_refresh_at.is_some());
    assert!(app.preview.state.directory_stats.is_none());

    thread::sleep(HIGH_FREQUENCY_PREVIEW_REFRESH_DELAY + Duration::from_millis(20));
    assert!(app.process_preview_refresh_timers());
    for _ in 0..100 {
        let _ = app.process_directory_stats_timer();
        let _ = app.process_background_jobs();
        if app.preview_header_detail_for_width(8, 80).as_deref()
            == Some(&format!("1 item • {}", crate::app::format_size(100)))
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        app.preview_header_detail_for_width(8, 80).as_deref(),
        Some(format!("1 item • {}", crate::app::format_size(100)).as_str())
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn high_frequency_alt_right_does_not_trigger_history_navigation() {
    let root = temp_path("high-frequency-alt-right-no-history");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("failed to create child dir");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    app.input.wheel_profile = WheelProfile::HighFrequency;
    app.select_index(0);
    app.open_selected()
        .expect("opening selected directory should succeed");
    wait_for_directory_load(&mut app);
    app.go_back().expect("go back should succeed");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)))
        .expect("alt-right should be handled");

    assert_eq!(app.navigation.cwd, root);
    assert_eq!(
        app.selected_entry().map(|entry| entry.path.as_path()),
        Some(child.as_path())
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

// --- keybindings integration tests ---

/// Parses a [keys] override, derives an action via KeyBindings::action_for,
/// dispatches it through App::dispatch_action, and checks the resulting state.
/// This covers the full config-to-runtime pipeline:
///   TOML string → KeyBindings → action_for → dispatch_action → app state
///
/// Note: App::handle_event reads from the process-wide config singleton, so we
/// cannot inject per-test overrides there.  The dispatch_action path (which
/// handle_event delegates to for every configurable key) is tested here
/// directly with a parsed KeyBindings, giving equivalent coverage.
#[test]
fn rebound_yank_key_dispatches_yank_action() {
    use crate::config::KeyBindings;

    // Parse a config that rebinds yank from "y" to "Y".
    let kb = KeyBindings::from_toml_str("[keys]\nyank = \"Y\"");
    assert_eq!(
        kb.action_for('Y'),
        Some(Action::Yank),
        "new key should map to Yank"
    );
    assert_eq!(
        kb.action_for('y'),
        None,
        "old key should no longer map to Yank"
    );

    // Create an app with a file so yank has something to act on.
    let root = temp_path("rebind-yank-e2e");
    fs::write(root.join("file.txt"), "hello").expect("failed to write file");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.select_index(0);

    assert!(app.jobs.clipboard.is_none(), "clipboard should start empty");

    // Dispatch the action the rebound key would trigger.
    let action = kb.action_for('Y').expect("Y should be bound");
    app.dispatch_action(action)
        .expect("dispatch should succeed");

    assert!(
        app.jobs.clipboard.is_some(),
        "yank should have populated the clipboard"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn rebound_quit_key_sets_should_quit() {
    use crate::config::KeyBindings;

    let kb = KeyBindings::from_toml_str("[keys]\nquit = \"u\"");
    assert_eq!(kb.action_for('u'), Some(Action::Quit));
    assert_eq!(kb.action_for('q'), None);

    let root = temp_path("rebind-quit-e2e");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    assert!(!app.should_quit);

    app.dispatch_action(kb.action_for('u').unwrap())
        .expect("dispatch should succeed");
    assert!(app.should_quit);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn rebound_open_or_enter_key_opens_selected_file() {
    use crate::config::KeyBindings;

    let kb = KeyBindings::from_toml_str(
        r#"[keys]
nav_right = "right"
open_or_enter = ["enter", "l"]"#,
    );
    assert_eq!(kb.action_for('l'), Some(Action::OpenOrEnter));

    let root = temp_path("rebind-open-or-enter-file");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file_path = root.join("note.txt");
    fs::write(&file_path, "hello").expect("failed to write temp file");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture.clone());

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.dispatch_action(kb.action_for('l').unwrap())
        .expect("open_or_enter should open selected file");

    let opened = read_open_capture(&capture);
    assert_eq!(opened, file_path.display().to_string());

    fs::remove_dir_all(root).ok();
}

#[test]
fn zoxide_action_queues_pending_terminal_task() {
    let root = temp_path("zoxide-action");
    let mut app = App::new_at(root.clone()).expect("failed to create app");

    app.dispatch_action(Action::Zoxide)
        .expect("dispatch should succeed");

    assert_eq!(app.pending_terminal_task, Some(PendingTerminalTask::Zoxide));
    assert!(app.status.is_empty());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn shell_action_queues_shell_in_current_directory() {
    let root = temp_path("shell-action");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");

    app.dispatch_action(Action::Shell)
        .expect("dispatch should succeed");

    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Shell { cwd: root.clone() })
    );
    assert!(app.status.is_empty());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn rebound_shell_key_queues_shell_action() {
    use crate::config::KeyBindings;

    let kb = KeyBindings::from_toml_str("[keys]\nshell = \"S\"");
    assert_eq!(kb.action_for('S'), Some(Action::Shell));
    assert_eq!(kb.action_for('!'), None);

    let root = temp_path("rebind-shell-e2e");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");

    app.dispatch_action(kb.action_for('S').unwrap())
        .expect("dispatch should succeed");

    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Shell { cwd: root.clone() })
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn zoxide_selection_opens_directory() {
    let root = temp_path("zoxide-selection");
    let target = root.join("target");
    fs::create_dir_all(&target).expect("failed to create target");
    let mut app = App::new_at(root.clone()).expect("failed to create app");

    app.open_zoxide_selection(target.clone());
    wait_for_directory_load(&mut app);

    assert_eq!(app.navigation.cwd, target);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn missing_zoxide_selection_reports_error() {
    let root = temp_path("zoxide-missing-selection");
    let missing = root.join("missing");
    let mut app = App::new_at(root.clone()).expect("failed to create app");

    app.open_zoxide_selection(missing);

    assert!(app.status_message().starts_with("Cannot open "));
    assert_eq!(app.navigation.cwd, root);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn capital_o_opens_open_with_overlay_for_selected_file() {
    let root = temp_path("open-with-overlay-file");
    fs::write(root.join("document.txt"), "hello").expect("failed to write temp file");
    let _capture_guard = OpenInSystemCaptureGuard::install(root.join("open-capture.txt"));

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.select_index(0);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('O'))))
        .expect("O should be handled");

    let overlay_opened = app.overlays.open_with.is_some();

    // The file was either opened directly (single handler, status cleared),
    // the overlay was shown (multiple handlers), or no handlers were found
    // and the system default was used.  All three are valid outcomes.
    let no_apps = app.status == "No apps found, opened with default";
    assert!(
        overlay_opened || no_apps || app.status.is_empty(),
        "O on a file should open overlay, report no apps, or auto-launch; got status: {:?}",
        app.status
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn capital_o_on_directory_uses_open_with_instead_of_file_only_error() {
    let root = temp_path("open-with-overlay-dir");
    let child = root.join("subdir");
    fs::create_dir_all(&child).expect("failed to create child dir");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.select_index(0);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('O'))))
        .expect("O on a directory should not fail");

    assert_ne!(
        app.status, "Open With is for files",
        "directories should use Open With instead of the old file-only error"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn esc_closes_open_with_overlay() {
    let root = temp_path("open-with-overlay-esc");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.inject_open_with_for_test("Fake App", "/usr/bin/true", vec![], false);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("Esc should close the overlay");
    assert!(app.overlays.open_with.is_none());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn open_with_shortcut_confirms_row_and_closes_overlay() {
    let root = temp_path("open-with-overlay-confirm");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.inject_open_with_for_test("Fake App", "/usr/bin/true", vec![], false);

    let shortcut = app
        .open_with_row_shortcut(0)
        .expect("first row should have a shortcut");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char(shortcut))))
        .expect("shortcut should confirm the row");

    assert!(
        app.overlays.open_with.is_none(),
        "overlay should close after confirming"
    );
    // Confirming a row launches the app — status is cleared on success or shows
    // a failure message if the process could not be spawned.
    assert!(
        app.status.is_empty() || app.status.starts_with("Failed to open with"),
        "unexpected status after confirm: {:?}",
        app.status
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn ctrl_c_closes_open_with_overlay() {
    let root = temp_path("open-with-overlay-ctrl-c");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.inject_open_with_for_test("Fake App", "/usr/bin/true", vec![], false);

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )))
    .expect("Ctrl-C should close the overlay");
    assert!(app.overlays.open_with.is_none());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

// ── open-with launch behavior ─────────────────────────────────────────────────

/// Creates a shell script in `dir` that touches `sentinel` when run,
/// makes it executable, and returns its path.
#[cfg(unix)]
fn write_sentinel_script(dir: &std::path::Path, sentinel: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("fake-app.sh");
    fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\n", sentinel.display()),
    )
    .expect("write sentinel script");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod sentinel script");
    script
}

#[cfg(unix)]
#[test]
fn detached_open_command_executes_program() {
    let dir = temp_path("detached-open-cmd");
    let sentinel = dir.join("ran");
    let script = write_sentinel_script(&dir, &sentinel);

    crate::fs::detached_open_command(script.to_str().unwrap(), &[])
        .expect("detached_open_command should succeed");

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1000);
    while !sentinel.exists() && std::time::Instant::now() < deadline {
        thread::sleep(std::time::Duration::from_millis(10));
    }

    let ran = sentinel.exists(); // capture before cleanup
    fs::remove_dir_all(&dir).ok();
    assert!(ran, "script must have run");
}

#[cfg(unix)]
#[test]
fn confirm_open_with_launches_program_and_closes_overlay() {
    let dir = temp_path("open-with-launch");
    let sentinel = dir.join("launched");
    let script = write_sentinel_script(&dir, &sentinel);

    let root = temp_path("open-with-launch-root");
    fs::write(root.join("file.txt"), "hello").expect("write file");
    let mut app = App::new_at(root.clone()).expect("create app");
    wait_for_directory_load(&mut app);

    app.inject_open_with_for_test("Fake App", script.to_str().unwrap(), vec![], false);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('1'))))
        .expect("shortcut should fire");

    assert!(
        app.overlays.open_with.is_none(),
        "overlay must close after launch"
    );
    assert!(
        app.status.is_empty(),
        "status should be empty after successful launch; got: {:?}",
        app.status
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1000);
    while !sentinel.exists() && std::time::Instant::now() < deadline {
        thread::sleep(std::time::Duration::from_millis(10));
    }

    let launched = sentinel.exists(); // capture before cleanup
    fs::remove_dir_all(&dir).ok();
    fs::remove_dir_all(&root).ok();
    assert!(launched, "fake app must have been executed");
}

#[test]
fn confirm_open_with_launch_failure_sets_status() {
    let root = temp_path("open-with-fail");
    fs::write(root.join("file.txt"), "hello").expect("write file");
    let mut app = App::new_at(root.clone()).expect("create app");
    wait_for_directory_load(&mut app);

    // Point at a program that does not exist — spawn will fail.
    app.inject_open_with_for_test(
        "Ghost App",
        "/this/program/absolutely/does/not/exist",
        vec![],
        false,
    );
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('1'))))
        .expect("shortcut should fire");

    assert!(
        app.overlays.open_with.is_none(),
        "overlay must close even on failure"
    );
    assert!(
        app.status.starts_with("Failed to open with Ghost App: "),
        "spawn error should be visible: {}",
        app.status
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn shift_arrow_keys_scroll_text_preview_vertically() {
    let root = temp_path("shift-arrow-scroll-text-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let long_file = root.join("long.txt");
    let contents = (0..120)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&long_file, &contents).expect("failed to write long file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    let long_index = app
        .navigation
        .entries
        .iter()
        .position(|e| e.path == long_file)
        .expect("long.txt should be in entries");
    app.select_index(long_index);
    app.set_frame_state(FrameState {
        preview_panel: Some(Rect {
            x: 21,
            y: 0,
            width: 40,
            height: 20,
        }),
        preview_rows_visible: 16,
        preview_cols_visible: 38,
        ..FrameState::default()
    });
    wait_for_background_preview(&mut app);

    assert_eq!(app.preview.state.scroll, 0, "preview should start at top");

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::SHIFT,
    )))
    .expect("Shift+Down should be handled");
    let after_down = app.preview.state.scroll;
    assert!(
        after_down > 0,
        "Shift+Down should scroll the text preview down, got {after_down}"
    );

    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)))
        .expect("Shift+Up should be handled");
    assert!(
        app.preview.state.scroll < after_down,
        "Shift+Up should scroll the text preview back up, got {} (was {after_down})",
        app.preview.state.scroll
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn shift_j_k_step_epub_sections_on_paged_preview() {
    let root = temp_path("shift-jk-step-epub-sections");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let archive = root.join("story.epub");
    write_epub_fixture(
        &archive,
        &[
            ("Opening", "<p>Opening chapter text.</p>"),
            ("Middle", "<p>Middle chapter text.</p>"),
            ("Closing", "<p>Closing chapter text.</p>"),
        ],
    );

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    let archive_index = app
        .navigation
        .entries
        .iter()
        .position(|e| e.path == archive)
        .expect("story.epub should be in entries");
    app.select_index(archive_index);
    wait_for_background_preview(&mut app);
    app.set_frame_state(FrameState {
        preview_panel: Some(Rect {
            x: 21,
            y: 0,
            width: 40,
            height: 20,
        }),
        preview_rows_visible: 16,
        preview_cols_visible: 38,
        ..FrameState::default()
    });

    assert_eq!(
        app.preview.state.content.ebook_section_index,
        Some(0),
        "EPUB preview should open on the first section"
    );

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('J'),
        KeyModifiers::SHIFT,
    )))
    .expect("Shift+J should be handled");
    assert_eq!(
        app.preview.state.content.ebook_section_index,
        Some(1),
        "Shift+J should step EPUB to the next section"
    );

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('K'),
        KeyModifiers::SHIFT,
    )))
    .expect("Shift+K should be handled");
    assert_eq!(
        app.preview.state.content.ebook_section_index,
        Some(0),
        "Shift+K should step EPUB back to the previous section"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn shift_j_k_scroll_text_preview_vertically() {
    let root = temp_path("shift-jk-scroll-text-preview");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let long_file = root.join("long.txt");
    let contents = (0..120)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&long_file, &contents).expect("failed to write long file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.view_mode = ViewMode::List;
    let long_index = app
        .navigation
        .entries
        .iter()
        .position(|e| e.path == long_file)
        .expect("long.txt should be in entries");
    app.select_index(long_index);
    app.set_frame_state(FrameState {
        preview_panel: Some(Rect {
            x: 21,
            y: 0,
            width: 40,
            height: 20,
        }),
        preview_rows_visible: 16,
        preview_cols_visible: 38,
        ..FrameState::default()
    });
    wait_for_background_preview(&mut app);

    let selected_before = app.navigation.selected;

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('J'),
        KeyModifiers::SHIFT,
    )))
    .expect("Shift+J should be handled");
    let after_down = app.preview.state.scroll;
    assert!(
        after_down > 0,
        "Shift+J should scroll the text preview down, got {after_down}"
    );
    assert_eq!(
        app.navigation.selected, selected_before,
        "Shift+J must not move the file selection"
    );

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('K'),
        KeyModifiers::SHIFT,
    )))
    .expect("Shift+K should be handled");
    assert!(
        app.preview.state.scroll < after_down,
        "Shift+K should scroll the text preview back up, got {} (was {after_down})",
        app.preview.state.scroll
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}
