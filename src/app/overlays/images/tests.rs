use super::*;
use crate::app::overlays::inline_image::{
    ImageProtocol, OverlayPresentState, RenderedImageDimensions, TerminalIdentity,
    TerminalWindowSize,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use ratatui::{buffer::Buffer, layout::Rect};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn temp_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("elio-static-image-{label}-{unique}"))
}

fn configure_terminal_image_support(app: &mut App) {
    let (cells_width, cells_height) = crossterm::terminal::size().unwrap_or((120, 40));
    app.preview.terminal_images.protocol = ImageProtocol::KittyGraphics;
    app.preview.terminal_images.window = Some(TerminalWindowSize {
        cells_width,
        cells_height,
        pixels_width: 1920,
        pixels_height: 1080,
    });
}

fn blank_frame_buffer() -> Buffer {
    Buffer::empty(Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 40,
    })
}

fn write_test_raster_image(path: &Path, format: ImageFormat, width_px: u32, height_px: u32) {
    let mut image = RgbaImage::new(width_px, height_px);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([32, 128, 224, 255]);
    }

    DynamicImage::ImageRgba8(image)
        .save_with_format(path, format)
        .expect("failed to write raster test image");
}

fn set_single_test_entry(app: &mut App, path: &Path) {
    let metadata = fs::metadata(path).expect("file metadata should exist");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("file name should be valid utf-8");
    app.navigation.entries = vec![Entry {
        path: path.to_path_buf(),
        name: name.to_string(),
        name_key: name.to_ascii_lowercase(),
        kind: EntryKind::File,
        symlink: None,
        size: metadata.len(),
        modified: metadata.modified().ok(),
        readonly: false,
    }];
    app.navigation.selected = 0;
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });
    app.input.frame_state.metrics.cols = 1;
    app.input.frame_state.metrics.rows_visible = 6;
}

fn build_selected_static_image_app(label: &str, file_name: &str) -> (App, PathBuf, PathBuf) {
    let root = temp_root(label);
    fs::create_dir_all(&root).expect("failed to create temp root");
    let image_path = root.join(file_name);
    write_test_raster_image(&image_path, ImageFormat::Png, 600, 300);

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.pdf.pdf_tools_available = true;
    set_single_test_entry(&mut app, &image_path);
    app.refresh_preview();

    (app, root, image_path)
}

fn ready_static_image_overlay(app: &mut App) -> StaticImageOverlayRequest {
    app.preview.image.selection_activation_delay = Duration::ZERO;
    app.sync_image_preview_selection_activation();
    app.active_static_image_overlay_request()
        .expect("static image overlay request should exist")
}

fn displayed_sixel_static_image_overlay(
    app: &mut App,
    identity: TerminalIdentity,
) -> StaticImageOverlayRequest {
    let request = ready_static_image_overlay(app);
    app.set_terminal_image_protocol_for_tests(ImageProtocol::Sixel, identity);

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let _ = app.process_background_jobs();
        let _ = app.process_image_preview_timers();
        let output = app
            .present_preview_overlay()
            .expect("initial Sixel image presentation should succeed");
        if !output.is_empty() && app.static_image_overlay_displayed() {
            return request;
        }
        thread::sleep(Duration::from_millis(10));
    }

    panic!("timed out waiting for initial Sixel image presentation");
}

#[test]
fn kitty_png_overlay_uses_source_path_for_direct_display() {
    let (mut app, root, image_path) = build_selected_static_image_app("direct-source", "demo.png");
    let request = ready_static_image_overlay(&mut app);
    let key = StaticImageKey::from_request(&request);

    match app.prepared_static_image_for_overlay(&request) {
        StaticImageOverlayPreparation::Ready(prepared) => {
            assert_eq!(prepared.display_path, image_path);
            assert_eq!(
                prepared.dimensions,
                RenderedImageDimensions {
                    width_px: 600,
                    height_px: 300,
                }
            );
            assert!(prepared.inline_payload.is_none());
        }
        StaticImageOverlayPreparation::Pending => {
            panic!("png source path should display directly in kitty")
        }
        StaticImageOverlayPreparation::Failed => {
            panic!("png source path should not fail direct display")
        }
    }

    assert!(app.preview.image.dimensions.contains_key(&key));
    assert!(!app.preview.image.pending_prepares.contains(&key));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn konsole_png_overlay_uses_source_path_for_direct_display() {
    let (mut app, root, image_path) =
        build_selected_static_image_app("konsole-direct-source", "demo.png");
    app.preview.terminal_images.protocol = ImageProtocol::KittyDirectGraphics;
    let request = ready_static_image_overlay(&mut app);
    let key = StaticImageKey::from_request(&request);

    match app.prepared_static_image_for_overlay(&request) {
        StaticImageOverlayPreparation::Ready(prepared) => {
            assert_eq!(prepared.display_path, image_path);
            assert_eq!(
                prepared.dimensions,
                RenderedImageDimensions {
                    width_px: 600,
                    height_px: 300,
                }
            );
            assert!(prepared.inline_payload.is_none());
        }
        StaticImageOverlayPreparation::Pending => {
            panic!("png source path should display directly in Konsole")
        }
        StaticImageOverlayPreparation::Failed => {
            panic!("png source path should not fail direct Konsole display")
        }
    }

    assert!(app.preview.image.dimensions.contains_key(&key));
    assert!(!app.preview.image.pending_prepares.contains(&key));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn cached_rendered_overlay_reuses_cached_path_and_inline_payload() {
    let (mut app, root, image_path) = build_selected_static_image_app("cache-reuse", "demo.png");
    let mut request = ready_static_image_overlay(&mut app);
    request.force_render_to_cache = true;
    request.prepare_inline_payload = true;
    let key = StaticImageKey::from_request(&request);
    let rendered_path = root.join("demo-rendered.png");
    write_test_raster_image(&rendered_path, ImageFormat::Png, 320, 180);
    let payload: Arc<str> = Arc::from("YWJj");

    app.preview.image.dimensions.insert(
        key.clone(),
        RenderedImageDimensions {
            width_px: 320,
            height_px: 180,
        },
    );
    app.remember_rendered_static_image(key.clone(), rendered_path.clone());
    app.remember_static_image_inline_payload(key.clone(), Arc::clone(&payload));

    let prepared = app
        .cached_prepared_static_image_for_overlay(&key, &request)
        .expect("cached rendered overlay should be reused");

    assert_eq!(prepared.display_path, rendered_path);
    assert_eq!(
        prepared.dimensions,
        RenderedImageDimensions {
            width_px: 320,
            height_px: 180,
        }
    );
    assert_eq!(prepared.inline_payload.as_deref(), Some(payload.as_ref()));
    assert_ne!(prepared.display_path, image_path);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn cold_sixel_jpeg_selection_defers_first_keyboard_preview_refresh() {
    let root = temp_root("cold-sixel-jpeg-preview-defer");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.jpg", "c.txt"] {
        let path = root.join(name);
        if name.ends_with(".jpg") {
            write_test_raster_image(&path, ImageFormat::Jpeg, 1600, 900);
        } else {
            fs::write(path, name).expect("failed to write temp file");
        }
    }

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::Sixel;
    app.preview.pdf.pdf_tools_available = true;
    app.navigation.view_mode = ViewMode::List;
    app.set_ffmpeg_available_for_tests(true);
    app.navigation.entries = ["a.txt", "b.jpg", "c.txt"]
        .into_iter()
        .map(|name| {
            let path = root.join(name);
            let metadata = fs::metadata(&path).expect("test file metadata should exist");
            Entry {
                path,
                name: name.to_string(),
                name_key: name.to_ascii_lowercase(),
                kind: EntryKind::File,
                symlink: None,
                size: metadata.len(),
                modified: metadata.modified().ok(),
                readonly: false,
            }
        })
        .collect();
    app.navigation.selected = 0;
    app.refresh_preview();

    let token_before = app.preview.state.token;
    app.move_vertical_keyboard(1);

    assert_eq!(app.navigation.selected, 1);
    assert_eq!(
        app.preview.state.token, token_before,
        "cold sixel jpeg should defer the first keyboard refresh"
    );
    assert!(
        app.preview.state.deferred_refresh_at.is_some(),
        "cold sixel jpeg should schedule a deferred refresh"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn cold_sixel_comic_selection_defers_first_keyboard_preview_refresh() {
    let root = temp_root("cold-sixel-comic-preview-defer");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.cbz", "c.txt"] {
        let path = root.join(name);
        fs::write(path, name).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::Sixel;
    app.preview.pdf.pdf_tools_available = true;
    app.navigation.view_mode = ViewMode::List;
    app.navigation.entries = ["a.txt", "b.cbz", "c.txt"]
        .into_iter()
        .map(|name| {
            let path = root.join(name);
            let metadata = fs::metadata(&path).expect("test file metadata should exist");
            Entry {
                path,
                name: name.to_string(),
                name_key: name.to_ascii_lowercase(),
                kind: EntryKind::File,
                symlink: None,
                size: metadata.len(),
                modified: metadata.modified().ok(),
                readonly: false,
            }
        })
        .collect();
    app.navigation.selected = 0;
    app.refresh_preview();

    let token_before = app.preview.state.token;
    app.move_vertical_keyboard(1);

    assert_eq!(app.navigation.selected, 1);
    assert_eq!(
        app.preview.state.token, token_before,
        "cold sixel comic should defer the first keyboard refresh"
    );
    assert!(
        app.preview.state.deferred_refresh_at.is_some(),
        "cold sixel comic should schedule a deferred refresh"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sixel_preloads_visible_static_images_before_selection_lands_on_them() {
    let root = temp_root("sixel-visible-static-preload");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.jpg", "c.txt"] {
        let path = root.join(name);
        if name.ends_with(".jpg") {
            write_test_raster_image(&path, ImageFormat::Jpeg, 1600, 900);
        } else {
            fs::write(path, name).expect("failed to write temp file");
        }
    }

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::Sixel;
    app.preview.pdf.pdf_tools_available = true;
    app.navigation.view_mode = ViewMode::List;
    app.set_ffmpeg_available_for_tests(true);
    app.navigation.entries = ["a.txt", "b.jpg", "c.txt"]
        .into_iter()
        .map(|name| {
            let path = root.join(name);
            let metadata = fs::metadata(&path).expect("test file metadata should exist");
            Entry {
                path,
                name: name.to_string(),
                name_key: name.to_ascii_lowercase(),
                kind: EntryKind::File,
                symlink: None,
                size: metadata.len(),
                modified: metadata.modified().ok(),
                readonly: false,
            }
        })
        .collect();
    app.navigation.selected = 0;
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });
    app.input.frame_state.metrics.cols = 1;
    app.input.frame_state.metrics.rows_visible = 6;
    app.refresh_preview();
    app.refresh_static_image_preloads();

    let image_entry = &app.navigation.entries[1];
    let request = app
        .static_image_overlay_request_for_entry(image_entry)
        .expect("visible jpeg should have a static image overlay request");
    let key = StaticImageKey::from_request(&request);

    assert!(
        app.preview.image.pending_prepares.contains(&key),
        "visible sixel image should be preloaded even before selection reaches it"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn foot_sixel_limits_nearby_static_image_preloads() {
    let root = temp_root("foot-sixel-preload-limit");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.jpg", "c.jpg", "d.jpg", "e.jpg"] {
        let path = root.join(name);
        if name.ends_with(".jpg") {
            write_test_raster_image(&path, ImageFormat::Jpeg, 1600, 900);
        } else {
            fs::write(path, name).expect("failed to write temp file");
        }
    }

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::Sixel;
    app.preview.terminal_images.identity =
        crate::app::overlays::inline_image::TerminalIdentity::Foot;
    app.preview.pdf.pdf_tools_available = true;
    app.navigation.view_mode = ViewMode::List;
    app.set_ffmpeg_available_for_tests(true);
    app.navigation.entries = ["a.txt", "b.jpg", "c.jpg", "d.jpg", "e.jpg"]
        .into_iter()
        .map(|name| {
            let path = root.join(name);
            let metadata = fs::metadata(&path).expect("test file metadata should exist");
            Entry {
                path,
                name: name.to_string(),
                name_key: name.to_ascii_lowercase(),
                kind: EntryKind::File,
                symlink: None,
                size: metadata.len(),
                modified: metadata.modified().ok(),
                readonly: false,
            }
        })
        .collect();
    app.navigation.selected = 0;
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });
    app.input.frame_state.metrics.cols = 1;
    app.input.frame_state.metrics.rows_visible = 10;
    app.refresh_preview();
    app.refresh_static_image_preloads();

    assert_eq!(
        app.preview.image.pending_prepares.len(),
        STATIC_IMAGE_PRELOAD_LIMIT_SLOW_SIXEL
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn windows_terminal_sixel_limits_nearby_static_image_preloads() {
    let root = temp_root("wt-sixel-preload-limit");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.jpg", "c.jpg", "d.jpg", "e.jpg"] {
        let path = root.join(name);
        if name.ends_with(".jpg") {
            write_test_raster_image(&path, ImageFormat::Jpeg, 1600, 900);
        } else {
            fs::write(path, name).expect("failed to write temp file");
        }
    }

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::Sixel;
    app.preview.terminal_images.identity =
        crate::app::overlays::inline_image::TerminalIdentity::WindowsTerminal;
    app.preview.pdf.pdf_tools_available = true;
    app.navigation.view_mode = ViewMode::List;
    app.set_ffmpeg_available_for_tests(true);
    app.navigation.entries = ["a.txt", "b.jpg", "c.jpg", "d.jpg", "e.jpg"]
        .into_iter()
        .map(|name| {
            let path = root.join(name);
            let metadata = fs::metadata(&path).expect("test file metadata should exist");
            Entry {
                path,
                name: name.to_string(),
                name_key: name.to_ascii_lowercase(),
                kind: EntryKind::File,
                symlink: None,
                size: metadata.len(),
                modified: metadata.modified().ok(),
                readonly: false,
            }
        })
        .collect();
    app.navigation.selected = 0;
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });
    app.input.frame_state.metrics.cols = 1;
    app.input.frame_state.metrics.rows_visible = 10;
    app.refresh_preview();
    app.refresh_static_image_preloads();

    assert_eq!(
        app.preview.image.pending_prepares.len(),
        STATIC_IMAGE_PRELOAD_LIMIT_SLOW_SIXEL
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn repeated_present_static_image_overlay_is_a_noop_when_nothing_changed() {
    let (mut app, root, _image_path) = build_selected_static_image_app("no-op-render", "demo.png");
    app.preview.image.selection_activation_delay = Duration::ZERO;
    app.sync_image_preview_selection_activation();

    let mut first = Vec::new();
    let first_state = app
        .present_static_image_overlay(ImageProtocol::KittyGraphics, &[], false, &mut first)
        .expect("first static image presentation should succeed");
    assert_eq!(first_state, OverlayPresentState::Displayed);
    assert!(!first.is_empty());
    assert!(app.static_image_overlay_displayed());

    let mut second = Vec::new();
    let second_state = app
        .present_static_image_overlay(ImageProtocol::KittyGraphics, &[], false, &mut second)
        .expect("repeat static image presentation should succeed");
    assert_eq!(second_state, OverlayPresentState::Displayed);
    assert!(second.is_empty(), "unchanged image should not redraw");

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn newly_shown_static_image_preview_prefers_image_surface_before_frame_area_exists() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("shown-image-surface-before-area", "demo.png");

    app.toggle_preview_pane();
    app.input.frame_state.preview_content_area = None;
    app.toggle_preview_pane();

    assert!(app.active_static_image_overlay_request().is_none());
    assert!(
        app.preview_will_use_static_image_surface_after_layout(),
        "first visible frame after showing preview should reserve the image surface instead of drawing text fallback into cells that Kitty will use for placeholders"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn kitty_resize_requests_full_screen_clear_for_displayed_overlay() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("kitty-resize-clear", "demo.png");
    let request = ready_static_image_overlay(&mut app);
    app.preview.image.displayed = Some(types::DisplayedStaticImagePreview::from_request(
        &request,
        request.area,
        request.area,
    ));
    app.preview.image.displayed_excluded = vec![Rect {
        x: 4,
        y: 5,
        width: 6,
        height: 3,
    }];

    app.handle_terminal_image_resize();

    assert!(app.take_pending_resize_clear());
    assert!(!app.static_image_overlay_displayed());
    assert!(app.preview.image.displayed_excluded.is_empty());
    assert!(!app.take_pending_resize_clear());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iterm_resize_requests_full_screen_clear_for_displayed_overlay() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("iterm-resize-clear", "demo.png");
    let request = ready_static_image_overlay(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::ItermInline;
    app.preview.image.displayed = Some(types::DisplayedStaticImagePreview::from_request(
        &request,
        request.area,
        request.area,
    ));

    app.handle_terminal_image_resize();

    assert!(app.take_pending_resize_clear());
    assert!(!app.static_image_overlay_displayed());
    assert!(!app.take_pending_resize_clear());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn konsole_resize_does_not_request_full_screen_clear() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("konsole-resize-no-clear", "demo.png");
    let request = ready_static_image_overlay(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::KittyDirectGraphics;
    app.preview.image.displayed = Some(types::DisplayedStaticImagePreview::from_request(
        &request,
        request.area,
        request.area,
    ));

    app.handle_terminal_image_resize();

    assert!(!app.take_pending_resize_clear());
    assert!(app.static_image_overlay_displayed());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sixel_resize_requests_full_screen_clear_for_displayed_overlay() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("sixel-resize-clear", "demo.png");
    let request = ready_static_image_overlay(&mut app);
    app.preview.terminal_images.protocol = ImageProtocol::Sixel;
    app.preview.image.displayed = Some(types::DisplayedStaticImagePreview::from_request(
        &request,
        request.area,
        request.area,
    ));
    app.preview.image.displayed_excluded = vec![Rect {
        x: 2,
        y: 3,
        width: 4,
        height: 2,
    }];

    app.handle_terminal_image_resize();

    assert!(app.take_pending_resize_clear());
    assert!(!app.static_image_overlay_displayed());
    assert!(app.preview.image.displayed_excluded.is_empty());
    assert!(!app.take_pending_resize_clear());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn exclusion_only_updates_redraw_without_clearing_the_existing_image() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("excluded-redraw", "demo.png");
    app.preview.image.selection_activation_delay = Duration::ZERO;
    app.sync_image_preview_selection_activation();

    let mut initial = Vec::new();
    app.present_static_image_overlay(ImageProtocol::KittyGraphics, &[], false, &mut initial)
        .expect("initial static image presentation should succeed");

    let excluded = [Rect {
        x: 4,
        y: 5,
        width: 6,
        height: 3,
    }];
    let mut updated = Vec::new();
    let state = app
        .present_static_image_overlay(ImageProtocol::KittyGraphics, &excluded, false, &mut updated)
        .expect("excluded-only redraw should succeed");
    let output = String::from_utf8(updated).expect("kitty redraw should be utf8");

    assert_eq!(state, OverlayPresentState::Displayed);
    assert!(
        !output.is_empty(),
        "changed exclusions should trigger a redraw"
    );
    assert!(
        !output.contains("\u{1b}_Ga=d,d=A,q=2\u{1b}\\"),
        "exclusion-only redraw should not clear the previous image first"
    );
    assert_eq!(app.preview.image.displayed_excluded, excluded);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn open_with_overlay_updates_kitty_exclusions_and_closing_it_restores_them() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("open-with-exclusions", "demo.png");
    app.preview.image.selection_activation_delay = Duration::ZERO;
    app.sync_image_preview_selection_activation();

    let mut initial = Vec::new();
    app.present_static_image_overlay(ImageProtocol::KittyGraphics, &[], false, &mut initial)
        .expect("initial static image presentation should succeed");
    assert!(app.preview.image.displayed_excluded.is_empty());

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    let popup = Rect {
        x: 4,
        y: 5,
        width: 12,
        height: 4,
    };
    let erase = app.modal_image_post_draw_erase(&[popup], &blank_frame_buffer());
    assert!(
        !erase.is_empty(),
        "opening a transparent popup over a Kitty placeholder image should erase covered cells"
    );
    app.input.frame_state.open_with_panel = Some(popup);

    let with_popup = String::from_utf8(
        app.present_preview_overlay()
            .expect("open-with popup redraw should succeed"),
    )
    .expect("kitty redraw should be valid utf8");
    assert!(
        !with_popup.is_empty(),
        "opening the open-with popup should redraw the kitty image"
    );
    assert_eq!(app.preview.image.displayed_excluded, vec![popup]);

    app.overlays.open_with = None;
    app.input.frame_state.open_with_panel = None;

    let restored = String::from_utf8(
        app.present_preview_overlay()
            .expect("closing the open-with popup should redraw the kitty image"),
    )
    .expect("kitty redraw should be valid utf8");
    assert!(
        !restored.is_empty(),
        "closing the open-with popup should redraw the kitty image"
    );
    assert!(
        app.preview.image.displayed_excluded.is_empty(),
        "closing the popup should remove kitty exclusions"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sixel_popup_skips_post_draw_masking_for_foot_performance() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("sixel-popup-mask", "demo.png");
    let request = displayed_sixel_static_image_overlay(&mut app, TerminalIdentity::Foot);
    assert!(app.static_image_overlay_displayed());

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    let popup = Rect {
        x: request.area.x.saturating_add(1),
        y: request.area.y.saturating_add(1),
        width: request.area.width.saturating_sub(2).max(1),
        height: request.area.height.saturating_sub(2).max(1),
    };
    let erase = app.modal_image_post_draw_erase(&[popup], &blank_frame_buffer());
    assert!(
        erase.is_empty(),
        "Sixel should not use post-draw modal masking because Foot processes those erases slowly"
    );

    let out = app
        .present_preview_overlay()
        .expect("Sixel popup redraw should not fail");
    assert!(
        out.is_empty(),
        "Sixel should not repaint raster image bytes while the popup is open"
    );
    assert!(app.static_image_overlay_displayed());

    app.overlays.open_with = None;
    app.input.frame_state.open_with_panel = None;

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut restored = Vec::new();
    while Instant::now() < deadline {
        let _ = app.process_background_jobs();
        restored = app
            .present_preview_overlay()
            .expect("closing the popup should repaint the Sixel image");
        if !restored.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !restored.is_empty(),
        "closing the popup should repaint the masked Sixel image"
    );
    assert!(app.static_image_overlay_displayed());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn foot_sixel_popup_erases_collision_and_repaints_after_close() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("foot-sixel-popup-collision", "demo.png");
    let request = displayed_sixel_static_image_overlay(&mut app, TerminalIdentity::Foot);
    assert!(app.static_image_overlay_displayed());

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    let popup = Rect {
        x: request.area.x.saturating_add(1),
        y: request.area.y.saturating_add(1),
        width: request.area.width.saturating_sub(2).max(1),
        height: request.area.height.saturating_sub(2).max(1),
    };
    let mask = app.modal_image_post_draw_erase(&[popup], &blank_frame_buffer());
    assert!(
        mask.is_empty(),
        "Foot Sixel should avoid fragile transparent-cell post-draw masking"
    );

    let (collisions, erase) = app.sixel_modal_collision_erase(&[popup]);
    assert_eq!(collisions, vec![popup]);
    assert!(
        !erase.is_empty(),
        "Foot Sixel should erase the popup/image collision behind modal overlays"
    );
    assert!(
        app.static_image_overlay_displayed(),
        "the image remains logically displayed so only the popup intersection is punched out"
    );
    assert!(
        app.present_preview_overlay()
            .expect("Sixel popup redraw should not fail")
            .is_empty(),
        "Foot Sixel should not repaint image bytes over an open popup"
    );

    app.overlays.open_with = None;
    app.input.frame_state.open_with_panel = None;

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut restored = Vec::new();
    while Instant::now() < deadline {
        let _ = app.process_background_jobs();
        let _ = app.process_image_preview_timers();
        restored = app
            .present_preview_overlay()
            .expect("closing the popup should repaint the Sixel image");
        if !restored.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !restored.is_empty(),
        "closing the popup should repaint the erased Foot Sixel collision"
    );
    assert!(app.static_image_overlay_displayed());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn windows_terminal_sixel_popup_erases_collision_and_repaints_after_close() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("wt-sixel-popup-collision", "demo.png");
    let request = displayed_sixel_static_image_overlay(&mut app, TerminalIdentity::WindowsTerminal);
    assert!(app.static_image_overlay_displayed());

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    let popup = Rect {
        x: request.area.x.saturating_add(1),
        y: request.area.y.saturating_add(1),
        width: request.area.width.saturating_sub(2).max(1),
        height: request.area.height.saturating_sub(2).max(1),
    };
    let mask = app.modal_image_post_draw_erase(&[popup], &blank_frame_buffer());
    assert!(
        mask.is_empty(),
        "Windows Terminal Sixel should avoid fragile transparent-cell post-draw masking"
    );

    let (collisions, erase) = app.sixel_modal_collision_erase(&[popup]);
    assert_eq!(collisions, vec![popup]);
    assert!(
        !erase.is_empty(),
        "Windows Terminal Sixel should erase the popup/image collision behind modal overlays"
    );
    assert!(
        app.static_image_overlay_displayed(),
        "the image remains logically displayed so only the popup intersection is punched out"
    );
    assert!(
        app.present_preview_overlay()
            .expect("Windows Terminal Sixel popup redraw should not fail")
            .is_empty(),
        "Windows Terminal Sixel should not repaint image bytes over an open popup"
    );

    app.overlays.open_with = None;
    app.input.frame_state.open_with_panel = None;

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut restored = Vec::new();
    while Instant::now() < deadline {
        let _ = app.process_background_jobs();
        let _ = app.process_image_preview_timers();
        restored = app
            .present_preview_overlay()
            .expect("closing the popup should repaint the Sixel image");
        if !restored.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !restored.is_empty(),
        "closing the popup should repaint the erased Windows Terminal Sixel collision"
    );
    assert!(app.static_image_overlay_displayed());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn foot_sixel_focus_gain_with_popup_does_not_clear_displayed_image_without_resize() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("foot-sixel-focus-popup", "demo.png");
    let request = ready_static_image_overlay(&mut app);
    app.set_terminal_image_protocol_for_tests(ImageProtocol::Sixel, TerminalIdentity::Foot);
    app.handle_terminal_image_focus_gained();
    assert!(!app.take_pending_resize_clear());

    app.preview.image.displayed = Some(types::DisplayedStaticImagePreview::from_request(
        &request,
        request.area,
        request.area,
    ));
    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);

    app.handle_terminal_image_focus_gained();
    assert!(
        !app.take_pending_resize_clear(),
        "focus gained without a terminal-size change must not take the resize-clear path"
    );
    assert!(
        app.static_image_overlay_displayed(),
        "Foot Sixel image state should survive focus refocus while a popup is open"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn foot_sixel_resize_under_popup_repaints_image_behind_modal() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("foot-sixel-resize-popup", "demo.png");
    let request = ready_static_image_overlay(&mut app);
    app.set_terminal_image_protocol_for_tests(ImageProtocol::Sixel, TerminalIdentity::Foot);
    app.preview.image.displayed = Some(types::DisplayedStaticImagePreview::from_request(
        &request,
        request.area,
        request.area,
    ));
    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    let popup = Rect {
        x: request.area.x.saturating_add(1),
        y: request.area.y.saturating_add(1),
        width: request.area.width.saturating_sub(2).max(1),
        height: request.area.height.saturating_sub(2).max(1),
    };

    app.handle_terminal_image_resize();
    app.preview.terminal_images.window = Some(TerminalWindowSize {
        cells_width: 120,
        cells_height: 40,
        pixels_width: 1920,
        pixels_height: 1080,
    });
    assert!(app.take_pending_resize_clear());
    assert!(
        !app.static_image_overlay_displayed(),
        "resize clear removes stale Sixel geometry before redraw"
    );
    assert!(
        app.should_repaint_sixel_under_modal(&[popup]),
        "Foot Sixel should repaint the resized image behind an open popup"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut image_bytes = Vec::new();
    while Instant::now() < deadline {
        let _ = app.process_background_jobs();
        let _ = app.process_image_preview_timers();
        image_bytes = app
            .present_preview_overlay_behind_modal()
            .expect("resize-under-popup repaint should not fail");
        if !image_bytes.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !image_bytes.is_empty(),
        "resize-under-popup should emit fresh Sixel bytes before collision masking"
    );
    assert!(app.static_image_overlay_displayed());

    let (collisions, erase) = app.sixel_modal_collision_erase(&[popup]);
    assert_eq!(collisions, vec![popup]);
    assert!(
        !erase.is_empty(),
        "the freshly repainted image should still be punched out under the popup"
    );
    assert!(
        app.present_preview_overlay()
            .expect("normal popup redraw should not fail")
            .is_empty(),
        "normal Foot Sixel presentation remains blocked while the popup is open"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn open_with_overlay_clears_konsole_image_and_closing_it_redraws_it() {
    let (mut app, root, _image_path) =
        build_selected_static_image_app("konsole-open-with-clear", "demo.png");
    app.preview.terminal_images.protocol = ImageProtocol::KittyDirectGraphics;
    app.preview.image.selection_activation_delay = Duration::ZERO;
    app.sync_image_preview_selection_activation();

    let mut initial = Vec::new();
    app.present_static_image_overlay(ImageProtocol::KittyDirectGraphics, &[], false, &mut initial)
        .expect("initial Konsole image presentation should succeed");
    assert!(app.static_image_overlay_displayed());

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    app.input.frame_state.open_with_panel = Some(Rect {
        x: 4,
        y: 5,
        width: 12,
        height: 4,
    });

    let cleared = String::from_utf8(
        app.present_preview_overlay()
            .expect("opening the open-with overlay should clear the Konsole image"),
    )
    .expect("Konsole clear output should be valid utf8");
    assert!(
        cleared.contains("\u{1b}_Ga=d,d=I,"),
        "opening the popup should send a Konsole delete command"
    );
    assert!(
        !app.static_image_overlay_displayed(),
        "opening the popup should clear the tracked Konsole image"
    );

    app.overlays.open_with = None;
    app.input.frame_state.open_with_panel = None;

    let restored = String::from_utf8(
        app.present_preview_overlay()
            .expect("closing the open-with overlay should redraw the Konsole image"),
    )
    .expect("Konsole redraw output should be valid utf8");
    assert!(
        restored.contains("\u{1b}_Ga=T,"),
        "closing the popup should redraw the Konsole image"
    );
    assert!(
        app.static_image_overlay_displayed(),
        "closing the popup should restore the tracked Konsole image"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}
