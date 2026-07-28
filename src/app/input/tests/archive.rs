use super::super::*;
use super::helpers::{
    cleanup_app_temp_root, temp_path, wait_for_directory_load, write_binary_zip_entries,
    write_encrypted_seven_zip_entries, write_encrypted_zip_entries,
};
use std::{fs, io::Read, thread, time::Duration};

#[test]
fn e_extracts_focused_zip_archive() {
    let root = temp_path("extract-zip-key");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let archive = root.join("sample.zip");
    write_binary_zip_entries(&archive, &[("dir/file.txt", b"hello")]);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should start archive extraction");

    let extracted_file = root.join("sample/dir/file.txt");
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if extracted_file.exists() && app.jobs.archive_extract_progress.is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello");
    assert_eq!(app.status_message(), "Extracted 1 item to \"sample\"");
    assert_eq!(
        app.selected_entry().map(|entry| entry.path.as_path()),
        Some(root.join("sample").as_path())
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn e_extracts_selected_archives_as_one_batch_and_skips_other_files() {
    let root = temp_path("extract-selected-archives");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.zip");
    let beta = root.join("beta.zip");
    let notes = root.join("notes.pdf");
    write_binary_zip_entries(&alpha, &[("file.txt", b"alpha")]);
    write_binary_zip_entries(&beta, &[("file.txt", b"beta")]);
    fs::write(&notes, b"not an archive").expect("failed to write non-archive");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());
    app.navigation.selected_paths.insert(notes);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should start batch archive extraction");
    assert!(
        app.navigation.selected_paths.is_empty(),
        "selected archives should be consumed once extraction starts"
    );

    let alpha_file = root.join("alpha/file.txt");
    let beta_file = root.join("beta/file.txt");
    for _ in 0..300 {
        let _ = app.process_background_jobs();
        if alpha_file.exists() && beta_file.exists() && app.jobs.archive_extract_progress.is_none()
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert_eq!(fs::read_to_string(&alpha_file).unwrap(), "alpha");
    assert_eq!(fs::read_to_string(&beta_file).unwrap(), "beta");
    assert_eq!(
        app.status_message(),
        "Extracted 2 archives, skipped 1 non-archive"
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn e_reports_no_archives_selected_for_non_archive_selection() {
    let root = temp_path("extract-no-selected-archives");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let notes = root.join("notes.pdf");
    fs::write(&notes, b"not an archive").expect("failed to write non-archive");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(notes);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should handle non-archive selection");

    assert_eq!(app.status_message(), "No archives selected");
    assert!(app.jobs.archive_extract_progress.is_none());
    assert_eq!(
        app.navigation.selected_paths.len(),
        1,
        "selection should stay when extraction does not start"
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn e_skips_password_archive_on_cancel_and_continues_batch() {
    let root = temp_path("extract-skip-password-archive");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let secret = root.join("secret.zip");
    let alpha = root.join("alpha.zip");
    let beta = root.join("beta.zip");
    let password = archive_test_password(&root);
    write_encrypted_zip_entries(&secret, &password, &[("file.txt", b"secret")]);
    write_binary_zip_entries(&alpha, &[("file.txt", b"alpha")]);
    write_binary_zip_entries(&beta, &[("file.txt", b"beta")]);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(secret.clone());
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should start batch archive extraction");
    assert!(
        app.navigation.selected_paths.is_empty(),
        "selected archives should be consumed before the password prompt"
    );
    wait_for_archive_password_prompt(&mut app);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("esc should skip password archive");

    let alpha_file = root.join("alpha/file.txt");
    let beta_file = root.join("beta/file.txt");
    for _ in 0..300 {
        let _ = app.process_background_jobs();
        if alpha_file.exists()
            && beta_file.exists()
            && app.jobs.archive_extract_progress.is_none()
            && !app.archive_password_is_open()
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert!(!root.join("secret/file.txt").exists());
    assert_eq!(fs::read_to_string(&alpha_file).unwrap(), "alpha");
    assert_eq!(fs::read_to_string(&beta_file).unwrap(), "beta");
    assert_eq!(app.status_message(), "Extracted 2 archives, 1 skipped");

    cleanup_app_temp_root(app, root);
}

#[test]
fn e_prompts_and_retries_encrypted_seven_zip_archive() {
    let root = temp_path("extract-encrypted-7z-key");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let archive = root.join("sample.7z");
    let password = archive_test_password(&root);
    let wrong_password = format!("{password}-wrong");
    write_encrypted_seven_zip_entries(&archive, &password, &[("dir/file.txt", b"hello")]);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should start archive extraction");
    wait_for_archive_password_prompt(&mut app);

    assert!(app.archive_password_is_open());
    assert_eq!(app.archive_password_error(), None);
    assert!(!root.join("sample").exists());

    type_archive_password(&mut app, &wrong_password);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("enter should submit wrong password");
    wait_for_archive_password_prompt(&mut app);

    assert!(app.archive_password_is_open());
    assert_eq!(app.archive_password_error(), Some("Wrong password"));
    assert_eq!(app.archive_password_input(), "");

    type_archive_password(&mut app, &password);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("enter should submit correct password");

    let extracted_file = root.join("sample/dir/file.txt");
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if extracted_file.exists()
            && app.jobs.archive_extract_progress.is_none()
            && !app.archive_password_is_open()
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello");
    assert_eq!(app.status_message(), "Extracted 1 item to \"sample\"");
    assert_eq!(
        app.selected_entry().map(|entry| entry.path.as_path()),
        Some(root.join("sample").as_path())
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn e_prompts_and_retries_encrypted_rar_archive() {
    let root = temp_path("extract-encrypted-rar-key");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let archive = root.join("sample.rar");
    let password = archive_test_password(&root);
    let wrong_password = format!("{password}-wrong");
    write_encrypted_seven_zip_entries(&archive, &password, &[("dir/file.txt", b"hello")]);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should start archive extraction");
    wait_for_archive_password_prompt(&mut app);

    assert!(app.archive_password_is_open());
    assert_eq!(app.archive_password_error(), None);
    assert!(!root.join("sample").exists());

    type_archive_password(&mut app, &wrong_password);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("enter should submit wrong password");
    wait_for_archive_password_prompt(&mut app);

    assert!(app.archive_password_is_open());
    assert_eq!(app.archive_password_error(), Some("Wrong password"));
    assert_eq!(app.archive_password_input(), "");

    type_archive_password(&mut app, &password);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("enter should submit correct password");

    let extracted_file = root.join("sample/dir/file.txt");
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if extracted_file.exists()
            && app.jobs.archive_extract_progress.is_none()
            && !app.archive_password_is_open()
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello");
    assert_eq!(app.status_message(), "Extracted 1 item to \"sample\"");
    assert_eq!(
        app.selected_entry().map(|entry| entry.path.as_path()),
        Some(root.join("sample").as_path())
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn e_prompts_and_retries_encrypted_zip_archive() {
    let root = temp_path("extract-encrypted-zip-key");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let archive = root.join("sample.zip");
    let password = archive_test_password(&root);
    let wrong_password = format!("{password}-wrong");
    write_encrypted_zip_entries(&archive, &password, &[("dir/file.txt", b"hello")]);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should start archive extraction");
    wait_for_archive_password_prompt(&mut app);

    assert!(app.archive_password_is_open());
    assert_eq!(app.archive_password_error(), None);
    assert!(!root.join("sample").exists());

    type_archive_password(&mut app, &wrong_password);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("enter should submit wrong password");
    wait_for_archive_password_prompt(&mut app);

    assert!(app.archive_password_is_open());
    assert_eq!(app.archive_password_error(), Some("Wrong password"));
    assert_eq!(app.archive_password_input(), "");

    type_archive_password(&mut app, &password);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("enter should submit correct password");

    let extracted_file = root.join("sample/dir/file.txt");
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if extracted_file.exists()
            && app.jobs.archive_extract_progress.is_none()
            && !app.archive_password_is_open()
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello");
    assert_eq!(app.status_message(), "Extracted 1 item to \"sample\"");
    assert_eq!(
        app.selected_entry().map(|entry| entry.path.as_path()),
        Some(root.join("sample").as_path())
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn archive_password_visibility_can_be_toggled() {
    let root = temp_path("archive-password-visibility");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let archive = root.join("sample.zip");
    let password = archive_test_password(&root);
    write_encrypted_zip_entries(&archive, &password, &[("file.txt", b"hello")]);

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should start archive extraction");
    wait_for_archive_password_prompt(&mut app);

    assert!(!app.archive_password_is_visible());
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('v'),
        KeyModifiers::ALT,
    )))
    .expect("visibility binding should be handled");
    assert!(app.archive_password_is_visible());
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('v'),
        KeyModifiers::ALT,
    )))
    .expect("visibility binding should toggle back");
    assert!(!app.archive_password_is_visible());

    cleanup_app_temp_root(app, root);
}

#[test]
fn e_reports_unsupported_archive_format() {
    let root = temp_path("extract-unsupported-key");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("note.txt"), "hello").expect("failed to write file");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('e'))))
        .expect("e should handle unsupported files");

    assert_eq!(
        app.status_message(),
        "Extraction supports ZIP, 7z, RAR, TAR, TAR.GZ, TAR.XZ, TAR.BZ2, and TAR.ZST"
    );
    assert!(app.jobs.archive_extract_progress.is_none());

    cleanup_app_temp_root(app, root);
}

#[test]
fn c_create_archive_clears_selection_when_started() {
    let root = temp_path("create-clears-selection");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");
    fs::write(&beta, "beta").expect("failed to write beta");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    app.navigation.selected_paths.insert(alpha.clone());
    app.navigation.selected_paths.insert(beta.clone());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('C'))))
        .expect("C should open archive creation");
    assert!(app.archive_create_is_open());
    assert_eq!(app.archive_create_input(), "archive.zip");
    assert_eq!(app.archive_create_cursor_col(), "archive".chars().count());
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("enter should start archive creation");

    assert!(
        app.navigation.selected_paths.is_empty(),
        "starting archive creation should clear the consumed selection"
    );

    let archive = root.join("archive.zip");
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if archive.exists() && app.jobs.archive_create_progress.is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert!(archive.exists());
    assert_eq!(app.status_message(), "Created \"archive.zip\"");
    assert_eq!(
        app.selected_entry().map(|entry| entry.path.as_path()),
        Some(archive.as_path())
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn archive_create_password_returns_to_create_overlay_before_creating() {
    let root = temp_path("create-protected-zip");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let alpha = root.join("alpha.txt");
    fs::write(&alpha, "alpha").expect("failed to write alpha");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('C'))))
        .expect("C should open archive creation");
    assert_eq!(app.archive_create_protection_label(), "");
    assert_eq!(app.archive_create_protection_hint(), "Alt+P add password");

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::ALT,
    )))
    .expect("Alt+P should open password prompt");
    assert!(app.archive_password_is_open());
    assert_eq!(app.archive_password_title_prefix(), "New password for");

    let password = archive_test_password(&root);
    type_archive_password(&mut app, &password);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("password Enter should return to create overlay");

    assert!(!app.archive_password_is_open());
    assert!(app.archive_create_is_open());
    assert!(!root.join("alpha.zip").exists());
    assert_eq!(app.archive_create_protection_label(), "Password set");
    assert_eq!(
        app.archive_create_protection_hint(),
        "Alt+P change  Alt+R remove"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("create Enter should start archive creation");

    let archive = root.join("alpha.txt.zip");
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if archive.exists() && app.jobs.archive_create_progress.is_none() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    wait_for_directory_load(&mut app);

    assert_eq!(app.status_message(), "Created protected \"alpha.txt.zip\"");
    {
        let file = fs::File::open(&archive).expect("archive should exist");
        let mut zip = zip::ZipArchive::new(file).expect("created archive should be a ZIP");
        assert!(zip.by_name("alpha.txt").is_err());
        let mut entry = zip
            .by_name_decrypt("alpha.txt", password.as_bytes())
            .expect("password should decrypt archived file");
        let mut contents = String::new();
        entry
            .read_to_string(&mut contents)
            .expect("encrypted entry should be readable");
        assert_eq!(contents, "alpha");
    }

    cleanup_app_temp_root(app, root);
}

#[test]
fn archive_create_password_can_be_removed_before_creating() {
    let root = temp_path("create-remove-password");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "alpha").expect("failed to write alpha");

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('C'))))
        .expect("C should open archive creation");
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::ALT,
    )))
    .expect("Alt+P should open password prompt");
    type_archive_password(&mut app, &archive_test_password(&root));
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("password Enter should return to create overlay");
    assert_eq!(app.archive_create_protection_label(), "Password set");

    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('r'),
        KeyModifiers::ALT,
    )))
    .expect("Alt+R should remove password");

    assert_eq!(app.archive_create_protection_label(), "");
    assert_eq!(app.archive_create_protection_hint(), "Alt+P add password");
    assert_eq!(app.status_message(), "Archive password removed");

    cleanup_app_temp_root(app, root);
}

#[test]
fn archive_create_unsupported_format_disables_password_prompt() {
    for (label, input) in [
        ("create-tar-no-password", "alpha.tar"),
        ("create-unknown-extension-no-password", "alpha.z"),
    ] {
        let (root, mut app) = archive_create_app(label);
        open_archive_create_with_input(&mut app, input);

        assert_eq!(app.archive_create_protection_label(), "");
        assert_eq!(app.archive_create_protection_hint(), "");
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('p'),
            KeyModifiers::ALT,
        )))
        .expect("Alt+P should be handled");

        assert!(!app.archive_password_is_open());
        assert_eq!(
            app.archive_create_error(),
            Some("Use ZIP or 7Z for passwords")
        );

        cleanup_app_temp_root(app, root);
    }
}

#[test]
fn archive_create_tar_with_existing_password_shows_actionable_conflict() {
    let (root, mut app) = archive_create_app("create-tar-password-conflict");
    open_archive_create_with_input(&mut app, "alpha.txt.zip");
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::ALT,
    )))
    .expect("Alt+P should open password prompt");
    type_archive_password(&mut app, &archive_test_password(&root));
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("password Enter should return to create overlay");
    set_archive_create_input(&mut app, "alpha.tar");

    assert_eq!(app.archive_create_protection_label(), "Password set");
    assert_eq!(
        app.archive_create_protection_hint(),
        "Switch format or remove"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)))
        .expect("Enter should validate archive creation");
    assert_eq!(app.archive_create_error(), None);
    assert_eq!(app.archive_create_protection_label(), "Password set");
    assert_eq!(
        app.archive_create_protection_hint(),
        "Switch format or remove"
    );

    cleanup_app_temp_root(app, root);
}

#[test]
fn cancel_keys_clear_selection_before_cancelling_archive_creation() {
    for (label, key) in [
        ("esc", KeyEvent::from(KeyCode::Esc)),
        (
            "ctrl-c",
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ),
    ] {
        let root = temp_path(&format!("archive-cancel-selection-first-{label}"));
        fs::create_dir_all(&root).expect("failed to create temp root");
        let alpha = root.join("alpha.txt");
        fs::write(&alpha, "alpha").expect("failed to write alpha");

        let mut app = App::new_at(root.clone()).expect("failed to create app");
        wait_for_directory_load(&mut app);
        app.navigation.selected_paths.insert(alpha);
        app.jobs.archive_create_progress = Some(crate::app::ArchiveCreateProgress {
            completed: 0,
            total: 1,
        });

        app.handle_event(Event::Key(key))
            .expect("cancel key should be handled");

        assert!(app.navigation.selected_paths.is_empty());
        assert!(
            app.jobs.archive_create_progress.is_some(),
            "first cancel key should clear selection instead of cancelling archive creation"
        );

        cleanup_app_temp_root(app, root);
    }
}

#[test]
fn archive_create_contents_list_scrolls_with_mouse_wheel() {
    let root = temp_path("archive-create-scroll");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for index in 0..12 {
        let path = root.join(format!("item-{index}.txt"));
        fs::write(&path, "item").expect("failed to write item");
    }

    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    for index in 0..12 {
        app.navigation
            .selected_paths
            .insert(root.join(format!("item-{index}.txt")));
    }

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('C'))))
        .expect("C should open archive creation");
    app.input.frame_state.archive_create_panel = Some(ratatui::layout::Rect::new(0, 0, 40, 12));
    app.input.frame_state.archive_create_list_area = Some(ratatui::layout::Rect::new(1, 4, 38, 8));

    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 2,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }))
    .expect("scroll down should be handled");

    assert_eq!(
        app.overlays
            .archive_create
            .as_ref()
            .expect("archive create overlay should remain open")
            .source_scroll,
        3
    );

    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 2,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }))
    .expect("scroll up should be handled");

    assert_eq!(
        app.overlays
            .archive_create
            .as_ref()
            .expect("archive create overlay should remain open")
            .source_scroll,
        0
    );

    cleanup_app_temp_root(app, root);
}

fn archive_create_app(label: &str) -> (std::path::PathBuf, App) {
    let root = temp_path(label);
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("alpha.txt"), "alpha").expect("failed to write alpha");
    let mut app = App::new_at(root.clone()).expect("failed to create app");
    wait_for_directory_load(&mut app);
    (root, app)
}

fn open_archive_create_with_input(app: &mut App, input: &str) {
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('C'))))
        .expect("C should open archive creation");
    set_archive_create_input(app, input);
}

fn set_archive_create_input(app: &mut App, input: &str) {
    let overlay = app
        .overlays
        .archive_create
        .as_mut()
        .expect("archive create overlay should be open");
    overlay.input = input.to_string();
    overlay.cursor_col = overlay.input.chars().count();
}

fn archive_test_password(root: &std::path::Path) -> String {
    root.file_name()
        .expect("temp root should have a file name")
        .to_string_lossy()
        .into_owned()
}

fn wait_for_archive_password_prompt(app: &mut App) {
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if app.archive_password_is_open() && app.jobs.archive_extract_progress.is_none() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for archive password prompt");
}

fn type_archive_password(app: &mut App, password: &str) {
    for ch in password.chars() {
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char(ch))))
            .expect("password character should be handled");
    }
}
