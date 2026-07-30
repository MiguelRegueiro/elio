use super::*;

use crate::app::overlays::inline_image::{ImageProtocol, TerminalIdentity};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct DiscoveredOpenWithAppsGuard;

impl DiscoveredOpenWithAppsGuard {
    fn install(apps: Vec<crate::app::state::OpenWithApp>) -> Self {
        crate::app::open_with::set_discovered_open_with_apps_for_test(Some(apps));
        Self
    }
}

impl Drop for DiscoveredOpenWithAppsGuard {
    fn drop(&mut self) {
        crate::app::open_with::set_discovered_open_with_apps_for_test(None);
    }
}

fn fake_open_with_app(display_name: &str) -> crate::app::state::OpenWithApp {
    crate::app::state::OpenWithApp {
        display_name: display_name.to_string(),
        desktop_id: None,
        program: "true".to_string(),
        args: vec![],
        is_default: false,
        requires_terminal: false,
    }
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "elio-duplicates-overlay-{label}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ))
}

#[test]
fn opening_duplicate_finder_clears_browser_selection() {
    let root = temp_path("clears-browser-selection");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "same").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "same").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.toggle_selection();
    assert_eq!(app.selection_count(), 1);

    app.open_duplicate_finder();

    assert!(app.duplicates_is_open());
    assert_eq!(app.selection_count(), 0);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_finder_binding_closes_duplicate_overlay() {
    let root = temp_path("binding-closes-overlay");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "same").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "same").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    assert!(app.duplicates_is_open());
    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT))
        .expect("Alt+D should close duplicate finder");

    assert!(!app.duplicates_is_open());
    assert!(!app.trash_is_open());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_finder_help_shortcut_opens_help_overlay_on_top() {
    let root = temp_path("help-on-top");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
        .expect("? should open help over duplicate finder");

    assert!(app.duplicates_is_open());
    assert!(app.overlays.help);

    app.handle_event(crossterm::event::Event::Key(KeyEvent::new(
        KeyCode::Esc,
        KeyModifiers::NONE,
    )))
    .expect("Esc should close help overlay");

    assert!(app.duplicates_is_open());
    assert!(!app.overlays.help);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_scan_stop_keeps_sorted_partial_results_and_unlocks_actions() {
    let root = temp_path("stop-partial-results");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["small-a.txt", "small-b.txt", "large-a.txt", "large-b.txt"] {
        fs::write(root.join(name), "same").expect("failed to write duplicate file");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();
    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group_at(&root, 1, 10, &["small-a.txt", "small-b.txt"]),
        duplicate_group_at(&root, 2, 100, &["large-a.txt", "large-b.txt"]),
    ];
    overlay.stats = crate::fs::duplicates::DuplicateScanStats {
        checked_candidates: 300_549,
        candidate_files: 300_552,
        processed_bytes: 170_000_000_000,
        ..crate::fs::duplicates::DuplicateScanStats::default()
    };
    overlay.selected = 3;
    overlay.scroll = 2;
    overlay.selected_paths.insert(root.join("large-b.txt"));
    overlay.loading = true;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("Esc should stop duplicate scan");

    let overlay = app
        .overlays
        .duplicates
        .as_ref()
        .expect("duplicate overlay should remain open");
    assert!(!overlay.loading);
    assert!(overlay.partial);
    assert_eq!(overlay.selected, 0);
    assert_eq!(overlay.scroll, 0);
    assert_eq!(overlay.groups[0].size, 100);
    assert_eq!(overlay.stats.groups, 2);
    assert_eq!(overlay.stats.duplicate_bytes, 110);
    assert_eq!(app.status_message(), "Duplicate scan stopped");

    app.open_duplicate_delete_permanently_prompt();
    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 1 selected file?");

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_open_with_binding_opens_open_with_overlay_on_top() {
    let root = temp_path("open-with-on-top");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "same").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "same").expect("failed to write beta");
    let _open_with_apps = DiscoveredOpenWithAppsGuard::install(vec![
        fake_open_with_app("Text Editor"),
        fake_open_with_app("Viewer"),
    ]);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();
    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])]
        .into_iter()
        .map(|mut group| {
            for file in &mut group.files {
                file.path = root.join(file.path.file_name().unwrap());
            }
            group
        })
        .collect();
    overlay.loading = false;
    overlay.selected = 1;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT))
        .expect("Open With should open over duplicate finder");

    assert!(app.duplicates_is_open());
    assert!(app.open_with_is_open());
    assert_eq!(app.open_with_row_count(), 2);
    assert_eq!(app.open_with_row_label(0), "Text Editor");
    assert_eq!(app.open_with_row_label(1), "Viewer");

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_row_double_click_reveals_clicked_file() {
    let root = temp_path("double-click-reveals");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "same").expect("failed to write alpha");
    fs::write(root.join("beta.txt"), "same").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();
    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])]
        .into_iter()
        .map(|mut group| {
            for file in &mut group.files {
                file.path = root.join(file.path.file_name().unwrap());
            }
            group
        })
        .collect();
    overlay.loading = false;
    app.input.frame_state.duplicate_panel = Some(ratatui::layout::Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 4,
    });
    app.input.frame_state.duplicate_hits = vec![
        DuplicateHit {
            rect: ratatui::layout::Rect {
                x: 0,
                y: 0,
                width: 40,
                height: 1,
            },
            index: 0,
        },
        DuplicateHit {
            rect: ratatui::layout::Rect {
                x: 0,
                y: 1,
                width: 40,
                height: 1,
            },
            index: 1,
        },
    ];
    let click = crossterm::event::Event::Mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    });

    app.handle_event(click.clone())
        .expect("first click should focus duplicate row");
    assert!(app.duplicates_is_open());
    assert_eq!(app.duplicate_focused_path(), Some(root.join("beta.txt")));

    app.handle_event(click)
        .expect("double click should reveal duplicate row");

    assert!(!app.duplicates_is_open());
    assert_eq!(
        app.selected_entry().map(|entry| entry.path.clone()),
        Some(root.join("beta.txt"))
    );
    assert_eq!(app.status_message(), "Located beta.txt");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_finder_help_overlay_consumes_mouse_wheel() {
    let root = temp_path("help-mouse-wheel");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(
        42,
        10,
        &["alpha.txt", "beta.txt", "gamma.txt"],
    )];
    overlay.loading = false;
    overlay.selected = 1;
    app.overlays.help = true;
    app.overlays.help_scroll = 0;

    app.handle_event(crossterm::event::Event::Mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
    ))
    .expect("mouse wheel should be handled by help overlay");

    assert!(app.duplicates_is_open());
    assert!(app.overlays.help);
    assert_eq!(app.overlays.help_scroll, 2);
    assert_eq!(app.overlays.duplicates.as_ref().unwrap().selected, 1);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_rows_render_display_rank_not_stable_group_id() {
    let root = temp_path("display-rank");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let groups = vec![
        duplicate_group(42, 10, &["big-a.txt", "big-b.txt", "big-c.txt"]),
        duplicate_group(7, 5, &["small-a.txt", "small-b.txt"]),
    ];
    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = groups;
    overlay.loading = false;

    let rows = app.duplicate_rows(10);

    assert_eq!(rows[0].group_rank, 1);
    assert!(rows[0].group_first);
    assert_eq!(rows[1].group_rank, 1);
    assert!(!rows[1].group_first);
    assert_eq!(rows[2].group_rank, 1);
    assert!(!rows[2].group_first);
    assert_eq!(rows[3].group_rank, 2);
    assert!(rows[3].group_first);
    assert_eq!(rows[4].group_rank, 2);
    assert!(!rows[4].group_first);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_rows_can_start_inside_group_without_losing_group_context() {
    let root = temp_path("rows-inside-group");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group(1, 10, &["a.txt", "b.txt", "c.txt"]),
        duplicate_group(2, 20, &["d.txt", "e.txt"]),
    ];
    overlay.loading = false;
    overlay.scroll = 1;
    overlay.selected = 3;

    let rows = app.duplicate_rows(3);

    assert_eq!(
        rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
        vec!["b.txt", "c.txt", "d.txt"]
    );
    assert_eq!(rows[0].index, 1);
    assert_eq!(rows[0].group_rank, 1);
    assert!(!rows[0].group_first);
    assert_eq!(rows[2].group_rank, 2);
    assert!(rows[2].group_first);
    assert!(rows[2].focused);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn selection_summary_tracks_duplicate_focus() {
    let root = temp_path("footer-focus");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
        duplicate_group(7, 5, &["gamma.txt", "delta.txt"]),
    ];
    overlay.loading = false;

    assert_eq!(app.selection_summary(), "1/4  alpha.txt");
    assert_eq!(app.selection_count(), 0);
    app.toggle_duplicate_selection();
    assert_eq!(app.selection_count(), 1);
    app.set_duplicate_selection(2);
    assert_eq!(app.selection_summary(), "3/4  gamma.txt");

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_trash_prompt_opens_on_top_of_duplicate_overlay() {
    let root = temp_path("trash-on-top");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])];
    overlay.loading = false;

    app.open_duplicate_trash_prompt();

    assert!(app.duplicates_is_open());
    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Trash 1 selected file?");
    assert_eq!(app.trash_target_count(), 1);
    assert_eq!(app.trash_target_path_at(0), Some(Path::new("alpha.txt")));

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_trash_prompt_uses_selected_rows_when_selection_exists() {
    let root = temp_path("trash-selected-rows");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
        duplicate_group(7, 20, &["gamma.txt", "delta.txt"]),
    ];
    overlay.loading = false;
    overlay.selected = 0;
    overlay.selected_paths.insert(PathBuf::from("beta.txt"));
    overlay.selected_paths.insert(PathBuf::from("gamma.txt"));

    app.open_duplicate_trash_prompt();

    assert!(app.duplicates_is_open());
    assert!(app.trash_is_open());
    assert_eq!(app.trash_target_count(), 2);
    assert_eq!(app.trash_target_path_at(0), Some(Path::new("beta.txt")));
    assert_eq!(app.trash_target_path_at(1), Some(Path::new("gamma.txt")));

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_trash_binding_permanently_deletes_when_results_are_inside_trash() {
    let root = temp_path("duplicate-trash-binding-inside-trash");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group_at(
        &root,
        42,
        10,
        &["alpha.txt", "beta.txt"],
    )];
    overlay.loading = false;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("trash binding should open permanent delete prompt for trash results");

    assert!(app.duplicates_is_open());
    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 1 selected file?");
    assert!(
        app.overlays
            .trash
            .as_ref()
            .is_some_and(|trash| trash.permanent)
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_trash_binding_blocks_mixed_trash_and_normal_results() {
    let trash_root = temp_path("duplicate-trash-binding-mixed-trash");
    let normal_root = temp_path("duplicate-trash-binding-mixed-normal");
    fs::create_dir_all(&trash_root).expect("failed to create trash temp root");
    fs::create_dir_all(&normal_root).expect("failed to create normal temp root");
    let mut app = App::new_at(trash_root.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group_at(&trash_root, 42, 10, &["alpha.txt", "beta.txt"]),
        duplicate_group_at(&normal_root, 7, 10, &["gamma.txt", "delta.txt"]),
    ];
    overlay.loading = false;
    overlay.selected_paths.insert(trash_root.join("alpha.txt"));
    overlay.selected_paths.insert(normal_root.join("gamma.txt"));

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("mixed trash binding should be handled");

    assert!(app.duplicates_is_open());
    assert!(!app.trash_is_open());
    assert_eq!(
        app.status_message(),
        "Selection mixes trash and normal files"
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(trash_root).expect("failed to remove trash temp root");
    fs::remove_dir_all(normal_root).expect("failed to remove normal temp root");
}

#[test]
fn duplicate_finder_uses_normal_action_bindings_even_when_browser_is_in_trash() {
    let root = temp_path("duplicate-normal-bindings-in-trash");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.navigation.in_trash = true;
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])];
    overlay.loading = false;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))
        .expect("rename binding should work inside Duplicate Finder");

    assert!(app.duplicates_is_open());
    assert!(app.overlays.rename.is_some());

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_permanent_delete_prompt_opens_on_top_after_scan() {
    let root = temp_path("delete-permanent-on-top");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])];
    overlay.loading = false;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT))
        .expect("D should open permanent delete prompt from Duplicate Finder");

    assert!(app.duplicates_is_open());
    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 1 selected file?");
    assert!(
        app.overlays
            .trash
            .as_ref()
            .is_some_and(|trash| trash.permanent)
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_permanent_delete_keeps_finder_open_and_keeps_singleton_remainder() {
    let root = temp_path("delete-permanent-removes-rows");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["alpha.txt", "beta.txt", "gamma.txt", "delta.txt"] {
        fs::write(root.join(name), "same").expect("failed to write duplicate file");
    }
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();
    app.jobs.duplicate_token = app.jobs.duplicate_token.wrapping_add(1);
    app.jobs.scheduler.cancel_duplicate_scan();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group_at(&root, 42, 10, &["alpha.txt", "beta.txt"]),
        duplicate_group_at(&root, 7, 20, &["gamma.txt", "delta.txt"]),
    ];
    overlay.loading = false;
    overlay.selected_paths.insert(root.join("alpha.txt"));

    app.open_duplicate_delete_permanently_prompt();
    app.overlays
        .trash
        .as_mut()
        .expect("trash prompt should be open")
        .confirmed = true;
    app.handle_trash_key(KeyEvent::from(KeyCode::Enter))
        .expect("permanent delete should be submitted");

    assert!(app.duplicates_is_open());
    assert!(!app.trash_is_open());

    for _ in 0..500 {
        let _ = app.process_background_jobs();
        if app.trash_progress().is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert!(app.trash_progress().is_none());

    assert!(app.duplicates_is_open());
    assert!(!root.join("alpha.txt").exists());
    assert_eq!(app.duplicate_file_count(), 3);
    assert_eq!(app.duplicate_focused_path(), Some(root.join("beta.txt")));

    app.close_duplicate_finder();
    app.navigation.directory_runtime.watch = None;
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_permanent_delete_waits_for_scan_completion() {
    let root = temp_path("delete-permanent-loading");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])];
    overlay.loading = true;

    app.open_duplicate_delete_permanently_prompt();

    assert!(app.duplicates_is_open());
    assert!(!app.trash_is_open());
    assert_eq!(
        app.status_message(),
        "Wait for duplicate scan to finish before deleting results"
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_permanent_delete_allows_selected_rows_even_when_they_are_a_whole_group() {
    let root = temp_path("delete-permanent-whole-group");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
        duplicate_group(7, 20, &["gamma.txt", "delta.txt"]),
    ];
    overlay.loading = false;
    overlay.selected_paths.insert(PathBuf::from("alpha.txt"));
    overlay.selected_paths.insert(PathBuf::from("beta.txt"));
    overlay.selected_paths.insert(PathBuf::from("gamma.txt"));

    app.open_duplicate_delete_permanently_prompt();

    assert!(app.duplicates_is_open());
    assert!(app.trash_is_open());
    assert_eq!(app.trash_title(), "Delete permanently 3 files?");
    assert_eq!(app.trash_target_count(), 3);
    assert_eq!(app.trash_target_path_at(0), Some(Path::new("alpha.txt")));
    assert_eq!(app.trash_target_path_at(1), Some(Path::new("beta.txt")));
    assert_eq!(app.trash_target_path_at(2), Some(Path::new("gamma.txt")));

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_select_all_selects_every_duplicate_row() {
    let root = temp_path("select-all");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
        duplicate_group(7, 5, &["gamma.txt", "delta.txt"]),
    ];
    overlay.loading = false;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL))
        .expect("Ctrl+A should select every duplicate row");

    assert_eq!(app.selection_count(), 4);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_batch_append_keeps_existing_focus_stable() {
    let root = temp_path("batch-focus-stable");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    app.apply_duplicate_batch(crate::fs::duplicates::DuplicateScanBatch {
        groups: vec![duplicate_group(1, 10, &["small-a.txt", "small-b.txt"])],
        stats: crate::fs::duplicates::DuplicateScanStats::default(),
    });
    assert_eq!(
        app.duplicate_focused_path(),
        Some(PathBuf::from("small-a.txt"))
    );

    app.apply_duplicate_batch(crate::fs::duplicates::DuplicateScanBatch {
        groups: vec![duplicate_group(2, 10_000, &["large-a.txt", "large-b.txt"])],
        stats: crate::fs::duplicates::DuplicateScanStats::default(),
    });

    assert_eq!(
        app.duplicate_focused_path(),
        Some(PathBuf::from("small-a.txt")),
        "streamed batches should not resort the list and steal preview focus"
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_final_result_resets_focus_to_top_after_reorder() {
    let root = temp_path("final-result-focus-top");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(
        1,
        10,
        &["small-a.txt", "small-b.txt", "small-c.txt"],
    )];
    overlay.selected = 2;
    overlay.scroll = 2;
    overlay.selected_paths.insert(PathBuf::from("small-c.txt"));
    overlay.preview_path = Some(PathBuf::from("small-c.txt"));

    app.apply_duplicate_result(Ok(crate::fs::duplicates::DuplicateScanResult {
        groups: vec![
            duplicate_group(2, 10_000, &["large-a.txt", "large-b.txt"]),
            duplicate_group(1, 10, &["small-a.txt", "small-b.txt", "small-c.txt"]),
        ],
        stats: crate::fs::duplicates::DuplicateScanStats::default(),
    }));

    let overlay = app
        .overlays
        .duplicates
        .as_ref()
        .expect("duplicate overlay should stay open");
    assert_eq!(overlay.selected, 0);
    assert_eq!(overlay.scroll, 0);
    assert_eq!(app.duplicate_scroll_top(), 0);
    assert_eq!(
        app.duplicate_focused_path(),
        Some(PathBuf::from("large-a.txt"))
    );
    assert_eq!(overlay.preview_path, Some(PathBuf::from("large-a.txt")));
    assert!(
        overlay
            .selected_paths
            .contains(&PathBuf::from("small-c.txt"))
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_actions_use_selected_rows_before_focus() {
    let root = temp_path("selected-action-paths");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![
        duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
        duplicate_group(7, 5, &["gamma.txt", "delta.txt"]),
    ];
    overlay.loading = false;
    overlay.selected_paths.insert(PathBuf::from("beta.txt"));
    overlay.selected_paths.insert(PathBuf::from("delta.txt"));

    let paths = app.duplicate_action_paths();

    assert_eq!(
        paths,
        vec![PathBuf::from("beta.txt"), PathBuf::from("delta.txt")]
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_rename_while_loading_patches_paths_without_restarting_scan() {
    let root = temp_path("rename-loading-patches-without-restart");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();
    let old_token = app.jobs.duplicate_token;

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])];
    overlay.loading = true;
    overlay.preview_path = Some(PathBuf::from("alpha.txt"));
    overlay.selected_paths.insert(PathBuf::from("beta.txt"));

    app.apply_duplicate_rename_pairs(vec![
        (
            PathBuf::from("alpha.txt"),
            PathBuf::from("renamed-alpha.txt"),
        ),
        (PathBuf::from("beta.txt"), PathBuf::from("renamed-beta.txt")),
    ]);

    let overlay = app
        .overlays
        .duplicates
        .as_ref()
        .expect("duplicate overlay should stay open");
    assert_eq!(app.jobs.duplicate_token, old_token);
    assert!(overlay.loading);
    assert_eq!(
        overlay.groups[0].files[0].path,
        PathBuf::from("renamed-alpha.txt")
    );
    assert_eq!(overlay.groups[0].files[0].name, "renamed-alpha.txt");
    assert!(
        overlay
            .selected_paths
            .contains(&PathBuf::from("renamed-beta.txt"))
    );
    assert!(!overlay.selected_paths.contains(&PathBuf::from("beta.txt")));

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn hidden_duplicate_preview_layout_keeps_content_target_but_no_image_surface() {
    let root = temp_path("hidden-preview-layout");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.set_terminal_image_protocol_for_tests(
        ImageProtocol::KittyGraphics,
        TerminalIdentity::Other,
    );
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(42, 10, &["alpha.webp", "beta.webp"])];
    overlay.loading = false;
    overlay.preview_visible = true;
    app.preview.visible = false;

    app.input.frame_state.preview_panel = None;
    assert_eq!(
        app.active_preview_entry().map(|entry| entry.name),
        Some("alpha.webp".to_string())
    );
    assert!(!app.preview_surface_visible_for_images());

    app.input.frame_state.preview_panel = Some(ratatui::layout::Rect {
        x: 40,
        y: 0,
        width: 20,
        height: 10,
    });
    assert!(app.preview_surface_visible_for_images());
    assert_eq!(
        app.active_preview_entry().map(|entry| entry.name),
        Some("alpha.webp".to_string())
    );
    app.input.frame_state.preview_content_area = Some(ratatui::layout::Rect {
        x: 40,
        y: 2,
        width: 20,
        height: 8,
    });
    assert!(
        app.active_static_image_overlay_request().is_some(),
        "Duplicate Finder image preview should not depend on browser preview visibility"
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_scroll_clamps_when_visible_rows_increase() {
    let root = temp_path("scroll-clamp");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let names = (0..84)
        .map(|index| format!("file-{index:02}.txt"))
        .collect::<Vec<_>>();
    let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(1, 10, &refs)];
    overlay.loading = false;
    overlay.selected = 83;
    overlay.scroll = 73;

    app.input.frame_state.duplicate_rows_visible = 30;

    assert!(app.sync_duplicate_scroll());
    assert_eq!(app.duplicate_scroll_top(), 54);
    assert_eq!(app.duplicate_rows(30).len(), 30);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_finder_shift_nav_does_not_move_file_focus() {
    let root = temp_path("shift-nav-no-focus-move");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(1, 10, &["alpha.webp", "beta.webp"])];
    overlay.loading = false;
    overlay.selected = 0;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT))
        .expect("Shift+Down should not move Duplicate Finder focus");

    assert_eq!(
        app.overlays
            .duplicates
            .as_ref()
            .expect("duplicate overlay should remain open")
            .selected,
        0
    );

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn duplicate_finder_shift_v_toggles_preview_without_moving_focus() {
    let root = temp_path("shift-v-preview-toggle");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    app.open_duplicate_finder();

    let overlay = app
        .overlays
        .duplicates
        .as_mut()
        .expect("duplicate overlay should be open");
    overlay.groups = vec![duplicate_group(1, 10, &["alpha.webp", "beta.webp"])];
    overlay.loading = false;
    overlay.selected = 0;
    overlay.preview_visible = true;

    app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT))
        .expect("Shift+V should toggle Duplicate Finder preview");

    let overlay = app
        .overlays
        .duplicates
        .as_ref()
        .expect("duplicate overlay should remain open");
    assert_eq!(overlay.selected, 0);
    assert!(!overlay.preview_visible);

    app.close_duplicate_finder();
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

fn duplicate_group_at(
    root: &Path,
    id: u64,
    size: u64,
    names: &[&str],
) -> crate::fs::duplicates::DuplicateGroup {
    crate::fs::duplicates::DuplicateGroup {
        id,
        size,
        files: names
            .iter()
            .map(|name| crate::fs::duplicates::DuplicateFile {
                path: root.join(name),
                name: (*name).to_string(),
                relative: (*name).to_string(),
                size,
                modified: None,
            })
            .collect(),
    }
}

fn duplicate_group(id: u64, size: u64, names: &[&str]) -> crate::fs::duplicates::DuplicateGroup {
    crate::fs::duplicates::DuplicateGroup {
        id,
        size,
        files: names
            .iter()
            .map(|name| crate::fs::duplicates::DuplicateFile {
                path: PathBuf::from(name),
                name: (*name).to_string(),
                relative: (*name).to_string(),
                size,
                modified: None,
            })
            .collect(),
    }
}
