use super::super::*;
use super::helpers::{
    OpenInSystemCaptureGuard, read_open_capture, temp_path, wait_for_directory_load,
};
use std::fs;

fn left_click(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn left_release(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn entry_hit(index: usize, row: u16) -> EntryHit {
    EntryHit {
        rect: Rect {
            x: 0,
            y: row,
            width: 20,
            height: 1,
        },
        index,
    }
}

#[test]
fn double_click_opens_clicked_file_not_multi_selection() {
    let root = temp_path("mouse-double-click-file-selection");
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
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(gamma.clone());
    let beta_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == beta)
        .expect("beta should be visible");
    app.set_frame_state(FrameState {
        entry_hits: vec![entry_hit(beta_index, 1)],
        ..FrameState::default()
    });

    app.handle_event(left_click(1, 1))
        .expect("first click should focus clicked file");
    assert!(!capture.exists());

    app.handle_event(left_click(1, 1))
        .expect("second click should open clicked file");

    let opened = read_open_capture(&capture);
    let opened: Vec<_> = opened.lines().map(str::to_owned).collect();
    assert_eq!(opened, vec![beta.display().to_string()]);
    assert_eq!(app.status, "Opened beta.txt");

    fs::remove_dir_all(root).ok();
}

#[test]
fn double_click_suppresses_drag_from_held_second_click() {
    let root = temp_path("mouse-double-click-drag-suppression");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let beta = root.join("beta.txt");
    fs::write(&beta, "beta").expect("failed to write beta");
    let capture = root.join("capture.txt");
    let _capture_guard = OpenInSystemCaptureGuard::install(capture);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    let beta_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == beta)
        .expect("beta should be visible");
    app.set_frame_state(FrameState {
        entry_hits: vec![entry_hit(beta_index, 1)],
        ..FrameState::default()
    });

    app.handle_event(left_click(1, 1)).expect("first click");
    app.handle_event(left_release(1, 1)).expect("first release");
    app.handle_event(left_click(1, 1))
        .expect("second click should open clicked file");

    assert!(app.take_drag_export_paths_at(1, 1).is_empty());

    app.handle_event(left_release(1, 1))
        .expect("second release should clear suppression");
    app.input.last_click = None;
    app.handle_event(left_click(1, 1))
        .expect("next click can start a new drag candidate");

    assert_eq!(app.take_drag_export_paths_at(1, 1), vec![beta]);

    fs::remove_dir_all(root).ok();
}

#[test]
fn double_click_enters_clicked_directory_not_multi_selection() {
    let root = temp_path("mouse-double-click-dir-selection");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let child = root.join("child");
    let selected = root.join("selected.txt");
    fs::create_dir_all(&child).expect("failed to create child dir");
    fs::write(&selected, "selected").expect("failed to write selected file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(selected);
    let child_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == child)
        .expect("child should be visible");
    app.set_frame_state(FrameState {
        entry_hits: vec![entry_hit(child_index, 1)],
        ..FrameState::default()
    });

    app.handle_event(left_click(1, 1))
        .expect("first click should focus clicked directory");
    assert_eq!(app.navigation.cwd, root);

    app.handle_event(left_click(1, 1))
        .expect("second click should enter clicked directory");
    wait_for_directory_load(&mut app);

    assert_eq!(app.navigation.cwd, child);

    fs::remove_dir_all(root).ok();
}

#[test]
fn chooser_double_click_confirms_clicked_file() {
    let root = temp_path("chooser-mouse-double-click-file");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha);
    app.enable_chooser_mode();
    let beta_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == beta)
        .expect("beta should be visible");
    app.set_frame_state(FrameState {
        entry_hits: vec![entry_hit(beta_index, 1)],
        ..FrameState::default()
    });

    app.handle_event(left_click(1, 1))
        .expect("first click should focus clicked file");
    assert_eq!(app.chooser_exit, None);

    app.handle_event(left_click(1, 1))
        .expect("second click should choose clicked file");

    assert!(app.should_quit);
    assert_eq!(
        app.chooser_exit.as_ref(),
        Some(&ChooserExit::Confirmed(vec![beta]))
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn chooser_double_click_enters_clicked_directory() {
    let root = temp_path("chooser-mouse-double-click-directory");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("failed to create child directory");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.enable_chooser_mode();
    let child_index = app
        .navigation
        .entries
        .iter()
        .position(|entry| entry.path == child)
        .expect("child should be visible");
    app.set_frame_state(FrameState {
        entry_hits: vec![entry_hit(child_index, 1)],
        ..FrameState::default()
    });

    app.handle_event(left_click(1, 1))
        .expect("first click should focus clicked directory");
    app.handle_event(left_click(1, 1))
        .expect("second click should enter clicked directory");
    wait_for_directory_load(&mut app);

    assert_eq!(app.navigation.cwd, child);
    assert!(!app.should_quit);
    assert_eq!(app.chooser_exit, None);

    fs::remove_dir_all(root).ok();
}
