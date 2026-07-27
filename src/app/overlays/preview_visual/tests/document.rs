use super::*;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{buffer::Buffer, text::Line};

fn configure_iterm_image_support(app: &mut App) {
    let (cells_width, cells_height) = crossterm::terminal::size().unwrap_or((120, 40));
    app.preview.terminal_images.protocol = ImageProtocol::ItermInline;
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

fn build_displayed_iterm_inline_image_app(label: &str) -> (App, PathBuf) {
    let root = temp_root(label);
    fs::create_dir_all(&root).expect("failed to create temp root");
    let page = root.join("page.png");
    write_test_raster_image(&page, ImageFormat::Png, 640, 360);
    let page_metadata = fs::metadata(&page).expect("page metadata should exist");

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_iterm_image_support(&mut app);
    app.navigation.entries = vec![Entry {
        path: page.clone(),
        name: "page.png".to_string(),
        name_key: "page.png".to_string(),
        kind: EntryKind::File,
        symlink: None,
        size: page_metadata.len(),
        modified: page_metadata.modified().ok(),
        readonly: false,
    }];
    app.navigation.selected = 0;
    app.preview.image.selection_activation_delay = Duration::ZERO;
    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 12,
    });
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 15,
        width: 48,
        height: 8,
    });
    app.preview.state.content = PreviewContent::new(PreviewKind::Comic, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::Inline,
            path: page,
            size: page_metadata.len(),
            modified: page_metadata.modified().ok(),
        });

    app.refresh_static_image_preloads();
    wait_for_displayed_preview_overlay(&mut app);
    assert!(app.static_image_overlay_displayed());
    (app, root)
}

#[test]
fn page_image_visual_uses_full_preview_height() {
    let root = temp_root("full-height");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.state.content = PreviewContent::new(PreviewKind::Archive, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::FullHeight,
            path: root.join("page.jpg"),
            size: 11 * 1024,
            modified: None,
        });

    assert_eq!(
        app.preview_visual_rows(Rect {
            x: 0,
            y: 0,
            width: 48,
            height: 20,
        }),
        Some(20)
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn failed_full_height_page_image_falls_back_to_text_layout() {
    let root = temp_root("failed-full-height");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.state.content = PreviewContent::new(PreviewKind::Comic, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::FullHeight,
            path: root.join("page.jpg"),
            size: 11 * 1024,
            modified: None,
        });
    let area = Rect {
        x: 0,
        y: 0,
        width: 48,
        height: 20,
    };
    let request = StaticImageOverlayRequest {
        path: root.join("page.jpg"),
        size: 11 * 1024,
        modified: None,
        area,
        target_width_px: image_target_width_px(area, app.cached_terminal_window()),
        target_height_px: image_target_height_px(area, app.cached_terminal_window()),
        mode: StaticImageOverlayMode::Inline,
        force_render_to_cache: false,
        prepare_inline_payload: false,
    };
    app.preview
        .image
        .failed_images
        .insert(StaticImageKey::from_request(&request));

    assert_eq!(app.preview_visual_rows(area), None);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn inline_page_image_leaves_room_for_summary_text() {
    let root = temp_root("inline-page");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.set_ffmpeg_available_for_tests(true);
    app.preview.state.content = PreviewContent::new(PreviewKind::Comic, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::Inline,
            path: root.join("page.jpg"),
            size: 11 * 1024,
            modified: None,
        });

    assert_eq!(
        app.preview_visual_rows(Rect {
            x: 0,
            y: 0,
            width: 48,
            height: 20,
        }),
        Some(14)
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn inline_cover_uses_more_of_the_preview_panel_height() {
    let root = temp_root("inline-cover");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.set_ffmpeg_available_for_tests(true);
    app.preview.state.content = PreviewContent::new(PreviewKind::Video, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::Cover,
            layout: PreviewVisualLayout::Inline,
            path: root.join("cover.png"),
            size: 11 * 1024,
            modified: None,
        });

    assert_eq!(
        app.preview_visual_rows(Rect {
            x: 0,
            y: 0,
            width: 48,
            height: 20,
        }),
        Some(10)
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn non_video_inline_cover_keeps_default_compact_height() {
    let root = temp_root("inline-cover-document");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.state.content = PreviewContent::new(PreviewKind::Document, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::Cover,
            layout: PreviewVisualLayout::Inline,
            path: root.join("cover.png"),
            size: 11 * 1024,
            modified: None,
        });

    assert_eq!(
        app.preview_visual_rows(Rect {
            x: 0,
            y: 0,
            width: 48,
            height: 20,
        }),
        Some(6)
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn document_page_image_prepares_in_background_before_display() {
    let root = temp_root("document-jpeg-background");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let page = root.join("page.jpg");
    write_test_raster_image(&page, ImageFormat::Jpeg, 1600, 900);
    let page_size = fs::metadata(&page)
        .expect("page image metadata should exist")
        .len();

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.set_ffmpeg_available_for_tests(true);
    app.navigation.entries.clear();
    app.navigation.selected = 0;
    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });
    app.preview.state.content = PreviewContent::new(PreviewKind::Document, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::FullHeight,
            path: page,
            size: page_size,
            modified: None,
        });
    let request = app
        .active_preview_visual_overlay_request()
        .expect("document page image request should be available");
    let key = StaticImageKey::from_request(&request);

    app.refresh_static_image_preloads();
    assert!(app.preview.image.pending_prepares.contains(&key));
    app.present_preview_overlay()
        .expect("presenting a document page overlay should not fail");
    assert!(!app.static_image_overlay_displayed());
    wait_for_displayed_preview_overlay(&mut app);

    assert!(app.static_image_overlay_displayed());
    assert!(app.preview.image.dimensions.contains_key(&key));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn large_inline_cover_uses_more_height_without_hiding_details() {
    let root = temp_root("large-inline-cover-document");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.preview.state.content = PreviewContent::new(PreviewKind::Document, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::Cover,
            layout: PreviewVisualLayout::LargeInline,
            path: root.join("cover.png"),
            size: 11 * 1024,
            modified: None,
        });

    assert_eq!(
        app.preview_visual_rows(Rect {
            x: 0,
            y: 0,
            width: 48,
            height: 20,
        }),
        Some(10)
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iterm_inline_page_image_clear_area_stays_inside_media_area() {
    let root = temp_root("iterm-inline-clear-area");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let page = root.join("page.png");
    write_test_raster_image(&page, ImageFormat::Png, 900, 1400);
    let page_size = fs::metadata(&page)
        .expect("page image metadata should exist")
        .len();
    let next_page = root.join("next-page.png");
    write_test_raster_image(&next_page, ImageFormat::Png, 1200, 800);
    let next_page_size = fs::metadata(&next_page)
        .expect("next page image metadata should exist")
        .len();

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_iterm_image_support(&mut app);
    app.navigation.entries.clear();
    app.navigation.selected = 0;
    app.input.frame_state.preview_panel = Some(Rect {
        x: 1,
        y: 1,
        width: 50,
        height: 24,
    });
    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 12,
    });
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 15,
        width: 48,
        height: 8,
    });
    app.preview.state.content = PreviewContent::new(PreviewKind::Comic, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::Inline,
            path: page,
            size: page_size,
            modified: None,
        });

    app.refresh_static_image_preloads();
    wait_for_displayed_preview_overlay(&mut app);

    assert_eq!(
        app.displayed_static_image_clear_area(),
        Some(Rect {
            x: 2,
            y: 3,
            width: 48,
            height: 12,
        })
    );

    app.preview.state.content = PreviewContent::new(PreviewKind::Comic, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::Inline,
            path: next_page,
            size: next_page_size,
            modified: None,
        });
    assert!(!app.displayed_static_image_matches_active());
    app.queue_forced_iterm_preview_erase();
    let erase =
        String::from_utf8(app.iterm_pre_draw_erase()).expect("iTerm erase should be valid utf8");
    assert!(erase.contains("\x1b[4;3H"));
    assert!(!erase.contains("\x1b[16;3H"));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn document_overlay_keeps_previous_page_visible_while_next_page_waits() {
    let root = temp_root("document-overlay-pending");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let first = root.join("001.png");
    let second = root.join("002.jpg");
    write_test_raster_image(&first, ImageFormat::Png, 600, 300);
    write_test_raster_image(&second, ImageFormat::Jpeg, 1600, 900);
    let first_size = fs::metadata(&first)
        .expect("first image metadata should exist")
        .len();

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_terminal_image_support(&mut app);
    app.set_ffmpeg_available_for_tests(true);
    app.navigation.entries.clear();
    app.navigation.selected = 0;
    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });
    app.preview.state.content = PreviewContent::new(PreviewKind::Document, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::FullHeight,
            path: first,
            size: first_size,
            modified: None,
        });

    app.refresh_static_image_preloads();
    wait_for_displayed_preview_overlay(&mut app);
    assert!(app.static_image_overlay_displayed());

    app.preview.state.content = PreviewContent::new(PreviewKind::Document, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::FullHeight,
            path: second,
            size: 20 * 1024 * 1024,
            modified: None,
        });
    let next_request = app
        .active_preview_visual_overlay_request()
        .expect("next document preview request should be available");
    let next_key = StaticImageKey::from_request(&next_request);
    app.refresh_static_image_preloads();
    app.input.last_selection_change_at = Instant::now() - std::time::Duration::from_secs(1);
    app.sync_image_preview_selection_activation();

    app.present_preview_overlay()
        .expect("pending document page transition should not fail");
    assert!(app.static_image_overlay_displayed());
    assert!(app.preview.image.pending_prepares.contains(&next_key));
    assert!(!app.displayed_static_image_matches_active());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iterm_popup_masks_displayed_image_until_close() {
    let root = temp_root("iterm-popup-deferred-erase");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let page = root.join("page.png");
    write_test_raster_image(&page, ImageFormat::Png, 600, 300);
    let page_size = fs::metadata(&page)
        .expect("page image metadata should exist")
        .len();

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_iterm_image_support(&mut app);
    app.navigation.entries.clear();
    app.navigation.selected = 0;
    app.input.frame_state.preview_panel = Some(Rect {
        x: 1,
        y: 1,
        width: 50,
        height: 24,
    });
    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 12,
    });
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 15,
        width: 48,
        height: 8,
    });
    app.preview.state.content = PreviewContent::new(PreviewKind::Comic, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::Inline,
            path: page,
            size: page_size,
            modified: None,
        });

    app.refresh_static_image_preloads();
    wait_for_displayed_preview_overlay(&mut app);
    assert!(app.static_image_overlay_displayed());

    let image_area = app
        .displayed_static_image_clear_area()
        .expect("displayed image should have a clear area");
    let popup = Rect {
        x: image_area.x.saturating_add(1),
        y: image_area.y.saturating_add(1),
        width: image_area.width.saturating_sub(2).max(1),
        height: image_area.height.saturating_sub(2).max(1),
    };
    app.overlays.help = true;
    let frame_buffer = blank_frame_buffer();
    let erase = app.modal_image_post_draw_erase(&[popup], &frame_buffer);
    assert!(
        !erase.is_empty(),
        "opening a transparent popup over an iTerm image should erase the covered image cells"
    );
    let out = app
        .present_preview_overlay()
        .expect("iTerm popup redraw should not fail");

    assert!(out.is_empty());
    assert!(app.static_image_overlay_displayed());
    assert!(app.iterm_pre_draw_erase().is_empty());

    app.overlays.help = false;
    let restore = String::from_utf8(
        app.present_preview_overlay()
            .expect("closing the popup should redraw the image"),
    )
    .expect("restored iTerm image output should be valid utf8");

    assert!(restore.contains("\x1b]1337;File=inline=1;"));
    assert!(app.static_image_overlay_displayed());
    assert!(
        app.present_preview_overlay()
            .expect("steady-state redraw should succeed")
            .is_empty()
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn closing_open_with_popup_restores_iterm_inline_image() {
    let (mut app, root) = build_displayed_iterm_inline_image_app("iterm-open-with-popup");

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    let popup = app
        .displayed_static_image_clear_area()
        .expect("displayed image should have a clear area");
    let frame_buffer = blank_frame_buffer();
    let erase = app.modal_image_post_draw_erase(&[popup], &frame_buffer);
    assert!(
        !erase.is_empty(),
        "opening the open-with popup should erase the covered iTerm image cells"
    );

    let out = app
        .present_preview_overlay()
        .expect("iTerm open-with popup redraw should not fail");
    assert!(out.is_empty());
    assert!(app.static_image_overlay_displayed());
    assert!(app.iterm_pre_draw_erase().is_empty());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("Esc should close the open-with overlay");
    let restore = String::from_utf8(
        app.present_preview_overlay()
            .expect("closing the open-with popup should redraw the image"),
    )
    .expect("restored iTerm image output should be valid utf8");

    assert!(restore.contains("\x1b]1337;File=inline=1;"));
    assert!(app.static_image_overlay_displayed());
    assert!(
        app.present_preview_overlay()
            .expect("steady-state redraw should succeed")
            .is_empty()
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iterm_modal_layout_change_repaints_image_behind_popup() {
    let (mut app, root) = build_displayed_iterm_inline_image_app("iterm-modal-layout-repaint");

    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 36,
        height: 10,
    });
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 13,
        width: 36,
        height: 8,
    });
    assert!(
        !app.displayed_static_image_matches_active(),
        "resized preview layout should no longer match the displayed iTerm image"
    );

    let next_request = app
        .active_static_image_overlay_request()
        .expect("resized static preview should request a new image target");
    let next_key = StaticImageKey::from_request(&next_request);
    app.refresh_static_image_preloads();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.preview.image.pending_prepares.contains(&next_key) && Instant::now() < deadline {
        let _ = app.process_background_jobs();
        let _ = app.process_image_preview_timers();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !app.preview.image.pending_prepares.contains(&next_key),
        "resized iTerm image target should be prepared before the modal repaint"
    );

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    let popup = app
        .displayed_static_image_clear_area()
        .expect("displayed image should have a clear area");
    assert!(app.should_repaint_iterm_inline_under_modal(&[popup]));

    let blocked = app
        .present_preview_overlay()
        .expect("normal iTerm modal presentation should not fail");
    assert!(
        blocked.is_empty(),
        "normal iTerm presentation must stay blocked while a modal is open"
    );

    let repaint = String::from_utf8(
        app.present_preview_overlay_behind_modal()
            .expect("controlled iTerm modal repaint should not fail"),
    )
    .expect("controlled iTerm repaint output should be valid utf8");
    assert!(repaint.contains("\x1b]1337;File=inline=1;"));
    assert!(app.static_image_overlay_displayed());
    assert!(app.displayed_static_image_matches_active());

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Esc)))
        .expect("Esc should close the open-with overlay");
    let restore = String::from_utf8(
        app.present_preview_overlay()
            .expect("closing the popup should repaint the covered iTerm image"),
    )
    .expect("restored iTerm image output should be valid utf8");
    assert!(restore.contains("\x1b]1337;File=inline=1;"));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iterm_modal_repaints_image_when_preview_pane_reappears() {
    let (mut app, root) = build_displayed_iterm_inline_image_app("iterm-modal-preview-reappears");

    app.inject_open_with_for_test("Preview", "/usr/bin/true", vec![], false);
    app.handle_terminal_image_resize();
    app.expire_terminal_image_resize_settle_for_tests();
    configure_iterm_image_support(&mut app);
    assert!(
        app.take_pending_resize_clear(),
        "iTerm resize should clear stale image geometry before redraw"
    );
    assert!(
        !app.static_image_overlay_displayed(),
        "resize clear should drop the stale logical image target"
    );

    app.input.frame_state.preview_media_area = None;
    app.input.frame_state.preview_content_area = None;
    assert!(
        app.active_static_image_overlay_request().is_none(),
        "hidden preview pane should remove the active static image target"
    );
    assert!(
        app.present_preview_overlay()
            .expect("hidden pane with popup should not repaint directly")
            .is_empty()
    );

    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 12,
    });
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 15,
        width: 48,
        height: 8,
    });
    let popup = Rect {
        x: 4,
        y: 5,
        width: 24,
        height: 8,
    };

    assert!(app.active_static_image_overlay_request().is_some());
    assert!(
        !app.displayed_static_image_matches_active(),
        "restored pane should need a fresh iTerm image placement"
    );
    assert!(app.should_repaint_iterm_inline_under_modal(&[popup]));
    let repaint = String::from_utf8(
        app.present_preview_overlay_behind_modal()
            .expect("controlled iTerm modal repaint should not fail"),
    )
    .expect("controlled iTerm repaint output should be valid utf8");

    assert!(repaint.contains("\x1b]1337;File=inline=1;"));
    assert!(app.static_image_overlay_displayed());
    assert!(app.displayed_static_image_matches_active());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn leaving_fullscreen_preview_queues_image_geometry_clear() {
    let (mut app, root) = build_displayed_iterm_inline_image_app("fullscreen-preview-exit-clear");
    assert!(app.static_image_overlay_displayed());

    app.preview.fullscreen = true;
    assert!(
        app.exit_fullscreen_preview(),
        "fullscreen exit should report a mode transition"
    );
    assert!(
        app.take_pending_resize_clear(),
        "image geometry clear should be queued before rendering normal panes"
    );
    assert!(
        !app.static_image_overlay_displayed(),
        "geometry clear should drop stale fullscreen image target"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn returning_from_terminal_task_replaces_logically_displayed_image() {
    let (mut app, root) = build_displayed_iterm_inline_image_app("terminal-task-image-restore");
    assert!(app.static_image_overlay_displayed());
    assert!(app.displayed_static_image_matches_active());
    assert!(
        app.present_preview_overlay()
            .expect("steady-state image presentation should not fail")
            .is_empty(),
        "a matching logical image would otherwise skip re-placement"
    );

    app.invalidate_terminal_image_overlay_after_terminal_task();
    assert!(
        app.take_pending_resize_clear(),
        "returning from a terminal task should force a full image repaint"
    );
    assert!(
        !app.static_image_overlay_displayed(),
        "terminal-task invalidation should drop stale logical image state"
    );

    let restored = String::from_utf8(
        app.present_preview_overlay()
            .expect("re-presenting after terminal task should not fail"),
    )
    .expect("restored iTerm image output should be valid utf8");
    assert!(restored.contains("\x1b]1337;File=inline=1;"));
    assert!(app.static_image_overlay_displayed());
    assert!(app.displayed_static_image_matches_active());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn resize_settle_holds_image_placement_until_window_expires() {
    let (mut app, root) = build_displayed_iterm_inline_image_app("resize-settle-hold");

    app.handle_terminal_image_resize();
    configure_iterm_image_support(&mut app);
    app.defer_terminal_image_resize_settle_for_tests();
    assert!(
        !app.process_terminal_image_resize_settle_timer(),
        "settle timer should still be pending immediately after a resize"
    );
    assert!(app.pending_terminal_image_resize_settle_timer().is_some());
    assert!(app.take_pending_resize_clear());

    let held = app
        .present_preview_overlay()
        .expect("present during resize settle should not fail");
    assert!(
        held.is_empty(),
        "no image payload should be transmitted while a resize burst is settling"
    );
    assert!(!app.static_image_overlay_displayed());

    app.expire_terminal_image_resize_settle_for_tests();
    let placed = String::from_utf8(
        app.present_preview_overlay()
            .expect("present after settle should not fail"),
    )
    .expect("placement output should be valid utf8");
    assert!(placed.contains("\x1b]1337;File=inline=1;"));
    assert!(app.static_image_overlay_displayed());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn resize_settle_does_not_arm_without_image_protocol() {
    let root = temp_root("resize-settle-no-protocol");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let mut app = App::new_at(root.clone()).expect("app should initialize");

    app.handle_terminal_image_resize();
    assert!(app.pending_terminal_image_resize_settle_timer().is_none());

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn iterm_pre_draw_erase_detects_cover_layout_change_before_frame_update() {
    let root = temp_root("iterm-cover-layout-change");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let cover = root.join("cover.png");
    write_test_raster_image(&cover, ImageFormat::Png, 600, 900);
    let cover_size = fs::metadata(&cover)
        .expect("cover metadata should exist")
        .len();

    let mut app = App::new_at(root.clone()).expect("app should initialize");
    configure_iterm_image_support(&mut app);
    app.navigation.entries.clear();
    app.navigation.selected = 0;
    app.preview.image.selection_activation_delay = Duration::ZERO;
    app.input.frame_state.preview_media_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });
    app.input.frame_state.preview_content_area = Some(Rect {
        x: 2,
        y: 23,
        width: 48,
        height: 0,
    });
    app.preview.state.content = PreviewContent::new(PreviewKind::Document, Vec::new())
        .with_preview_visual(PreviewVisual {
            kind: PreviewVisualKind::PageImage,
            layout: PreviewVisualLayout::FullHeight,
            path: cover.clone(),
            size: cover_size,
            modified: None,
        });
    assert!(
        app.active_preview_visual_overlay_request_unchecked()
            .is_some()
    );
    app.sync_image_preview_selection_activation();
    app.refresh_static_image_preloads();
    wait_for_displayed_preview_overlay(&mut app);
    assert!(app.static_image_overlay_displayed());

    app.input.frame_state.preview_panel = Some(Rect {
        x: 1,
        y: 1,
        width: 50,
        height: 24,
    });
    app.input.frame_state.preview_body_area = Some(Rect {
        x: 2,
        y: 3,
        width: 48,
        height: 20,
    });

    // Simulate the next EPUB section switching to an inline cover + text
    // layout before the frame state has been recomputed by ratatui.
    app.preview.state.content = PreviewContent::new(
        PreviewKind::Document,
        vec![Line::from("Chapter text starts here.")],
    )
    .with_preview_visual(PreviewVisual {
        kind: PreviewVisualKind::Cover,
        layout: PreviewVisualLayout::Inline,
        path: cover,
        size: cover_size,
        modified: None,
    });

    assert!(
        !app.displayed_static_image_matches_active(),
        "current preview layout should no longer match the stale full-height cover"
    );
    assert!(
        !app.iterm_pre_draw_erase().is_empty(),
        "iTerm should erase the previous full-height cover before drawing text"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}
