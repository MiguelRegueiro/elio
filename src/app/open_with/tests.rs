use super::{
    super::{
        App,
        state::{OpenWithApp, PendingTerminalTask},
    },
    overlay::FallbackOpenOutcome,
    path_is_text_like,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::{
    cell::{Cell, RefCell},
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_dir_path(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("elio-open-with-{label}-{unique}"))
}

fn fake_open_with_app(display_name: &str) -> OpenWithApp {
    OpenWithApp {
        display_name: display_name.to_string(),
        desktop_id: None,
        program: "fake".to_string(),
        args: vec!["--arg".to_string()],
        is_default: true,
        requires_terminal: false,
    }
}

fn fake_terminal_app(display_name: &str) -> OpenWithApp {
    OpenWithApp {
        display_name: display_name.to_string(),
        desktop_id: None,
        program: "nvim".to_string(),
        args: vec!["/tmp/file.txt".to_string()],
        is_default: false,
        requires_terminal: true,
    }
}

#[test]
fn zero_discovered_apps_fall_back_to_default_open() {
    let root = temp_dir_path("fallback-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let fallback_called = Cell::new(false);
    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![],
        |_| {
            fallback_called.set(true);
            Ok(FallbackOpenOutcome::DefaultApp)
        },
        |_| unreachable!("launch should not be called when no apps were discovered"),
    );

    assert!(fallback_called.get(), "fallback opener must be called");
    assert!(
        app.overlays.open_with.is_none(),
        "overlay must remain closed"
    );
    assert_eq!(app.status, "No apps found, opened with default");

    fs::remove_dir_all(root).ok();
}

#[test]
fn single_discovered_app_launches_without_opening_overlay() {
    let root = temp_dir_path("single-launch-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let launched = RefCell::new(None::<String>);
    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![fake_open_with_app("Fake App")],
        |_| unreachable!("fallback should not be called when one app was discovered"),
        |app| {
            *launched.borrow_mut() = Some(app.display_name.clone());
            Ok(())
        },
    );

    assert_eq!(launched.into_inner().as_deref(), Some("Fake App"));
    assert!(
        app.overlays.open_with.is_none(),
        "overlay must remain closed"
    );
    assert_eq!(
        app.status, "Opened with Fake App",
        "successful direct launch should name the app"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn single_discovered_app_launch_failure_sets_status_without_overlay() {
    let root = temp_dir_path("single-launch-fail-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![fake_open_with_app("Ghost App")],
        |_| unreachable!("fallback should not be called when one app was discovered"),
        |_| Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing")),
    );

    assert!(
        app.overlays.open_with.is_none(),
        "overlay must remain closed"
    );
    assert_eq!(app.status, "Failed to open with Ghost App: missing");

    fs::remove_dir_all(root).ok();
}

#[test]
fn single_terminal_app_queues_pending_command_without_overlay() {
    let root = temp_dir_path("terminal-single-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![fake_terminal_app("Neovim")],
        |_| unreachable!("fallback should not be called"),
        |_| unreachable!("detached launch should not be called for terminal apps"),
    );

    assert!(
        app.overlays.open_with.is_none(),
        "overlay must remain closed for direct terminal launch"
    );
    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Command {
            program: "nvim".to_string(),
            args: vec!["/tmp/file.txt".to_string()],
        })
    );
    assert!(app.status.is_empty());

    fs::remove_dir_all(root).ok();
}

#[test]
fn env_editor_label_does_not_repeat_terminal_suffix() {
    let root = temp_dir_path("env-editor-label-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![
            fake_terminal_app("Neovim ($VISUAL)"),
            fake_open_with_app("Gedit"),
        ],
        |_| unreachable!(),
        |_| unreachable!(),
    );

    assert_eq!(app.open_with_row_label(0), "Neovim ($VISUAL)");

    fs::remove_dir_all(root).ok();
}

#[test]
fn confirm_terminal_app_from_overlay_queues_pending_command() {
    let root = temp_dir_path("terminal-overlay-root");
    fs::create_dir_all(&root).expect("create temp root");
    fs::write(root.join("file.txt"), "hello").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    // Put two apps in the overlay (terminal + gui) so it opens.
    app.handle_discovered_open_with_apps(
        &root.join("file.txt"),
        vec![fake_terminal_app("Neovim"), fake_open_with_app("Gedit")],
        |_| unreachable!(),
        |_| unreachable!(),
    );
    assert!(app.overlays.open_with.is_some(), "overlay should be open");

    // The first row is the terminal app. Confirm it.
    app.confirm_open_with_index(0)
        .expect("confirm should not error");

    assert!(app.overlays.open_with.is_none(), "overlay must close");
    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Command {
            program: "nvim".to_string(),
            args: vec!["/tmp/file.txt".to_string()],
        })
    );
    assert!(app.status.is_empty());

    fs::remove_dir_all(root).ok();
}

#[test]
fn open_with_nav_keys_move_selection() {
    let root = temp_dir_path("nav-keys-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![fake_open_with_app("Gedit"), fake_terminal_app("Neovim")],
        |_| unreachable!(),
        |_| unreachable!(),
    );

    assert_eq!(app.open_with_selected_index(), 0);

    app.handle_open_with_key(KeyEvent::from(KeyCode::Char('j')))
        .expect("nav_down should be handled");
    assert_eq!(app.open_with_selected_index(), 1);

    app.handle_open_with_key(KeyEvent::from(KeyCode::Char('j')))
        .expect("nav_down should clamp");
    assert_eq!(app.open_with_selected_index(), 1);

    app.handle_open_with_key(KeyEvent::from(KeyCode::Char('k')))
        .expect("nav_up should be handled");
    assert_eq!(app.open_with_selected_index(), 0);

    app.handle_open_with_key(KeyEvent::from(KeyCode::Char('k')))
        .expect("nav_up should clamp");
    assert_eq!(app.open_with_selected_index(), 0);

    fs::remove_dir_all(root).ok();
}

#[test]
fn open_with_shortcuts_skip_nav_keys_without_dropping_rows() {
    let root = temp_dir_path("shortcut-precedence-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let apps: Vec<_> = (0..40)
        .map(|index| fake_open_with_app(&format!("App {index}")))
        .collect();

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(&path, apps, |_| unreachable!(), |_| unreachable!());

    assert_eq!(app.open_with_row_count(), 40);
    assert!(
        (0..app.open_with_row_count())
            .all(|index| { !matches!(app.open_with_row_shortcut(index), Some('j' | 'k')) })
    );

    app.handle_open_with_key(KeyEvent::from(KeyCode::Char('j')))
        .expect("nav_down should be handled");

    assert!(app.overlays.open_with.is_some(), "overlay must stay open");
    assert_eq!(app.open_with_selected_index(), 1);

    fs::remove_dir_all(root).ok();
}

#[test]
fn open_with_mouse_wheel_moves_selection() {
    let root = temp_dir_path("mouse-wheel-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![fake_open_with_app("Gedit"), fake_terminal_app("Neovim")],
        |_| unreachable!(),
        |_| unreachable!(),
    );

    app.handle_open_with_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("wheel down should be handled");
    assert_eq!(app.open_with_selected_index(), 1);

    app.handle_open_with_mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("wheel up should be handled");
    assert_eq!(app.open_with_selected_index(), 0);

    fs::remove_dir_all(root).ok();
}

#[test]
fn open_with_open_or_enter_confirms_selected_row() {
    let root = temp_dir_path("nav-confirm-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.txt");
    fs::write(&path, "hello").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![fake_open_with_app("Gedit"), fake_terminal_app("Neovim")],
        |_| unreachable!(),
        |_| unreachable!(),
    );

    app.handle_open_with_key(KeyEvent::from(KeyCode::Char('j')))
        .expect("nav_down should be handled");
    app.handle_open_with_key(KeyEvent::from(KeyCode::Enter))
        .expect("open_or_enter should confirm selection");

    assert!(app.overlays.open_with.is_none(), "overlay must close");
    assert_eq!(
        app.pending_terminal_task,
        Some(PendingTerminalTask::Command {
            program: "nvim".to_string(),
            args: vec!["/tmp/file.txt".to_string()],
        })
    );
    assert!(app.status.is_empty());

    fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "macos")]
#[test]
fn zero_discovered_apps_can_report_text_editor_fallback() {
    let root = temp_dir_path("text-editor-fallback-root");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("file.rs");
    fs::write(&path, "fn main() {}\n").expect("write temp file");

    let mut app = App::new_at(root.clone()).expect("create app");
    app.handle_discovered_open_with_apps(
        &path,
        vec![],
        |_| Ok(FallbackOpenOutcome::TextEditor),
        |_| unreachable!("launch should not be called when no apps were discovered"),
    );

    assert!(app.overlays.open_with.is_none());
    assert_eq!(app.status, "No apps found, opened in text editor");

    fs::remove_dir_all(root).ok();
}

#[test]
fn path_is_text_like_is_true_for_source_files() {
    let root = temp_dir_path("text-like-source");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("main.rs");
    fs::write(&path, "fn main() {}\n").expect("write source file");

    assert!(path_is_text_like(&path));

    fs::remove_dir_all(root).ok();
}

#[test]
fn path_is_text_like_is_false_for_svg_images() {
    let root = temp_dir_path("text-like-svg");
    fs::create_dir_all(&root).expect("create temp root");
    let path = root.join("icon.svg");
    fs::write(
        &path,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"></svg>"#,
    )
    .expect("write svg file");

    assert!(!path_is_text_like(&path));

    fs::remove_dir_all(root).ok();
}
