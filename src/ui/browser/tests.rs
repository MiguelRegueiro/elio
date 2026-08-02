use super::super::helpers;
use super::super::theme;
#[cfg(unix)]
use super::entries::browser_symlink_target_detail;
use super::entries::render_compact_list_row;
use super::layout::resolve_body_layout;
use super::scrollbar::split_scrollbar_area;
use super::sidebar::render_sidebar;
use crate::app::{App, FrameState, SidebarItem, SidebarItemKind, SidebarRow};
use crate::config::PaneWeights;
use crate::preview::default_code_preview_line_limit;
use crate::ui;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, style::Modifier};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("elio-browser-{label}-{unique}"))
}

fn draw_ui(terminal: &mut Terminal<TestBackend>, app: &mut App) -> FrameState {
    let mut frame_state = FrameState::default();
    terminal
        .draw(|frame| ui::render(frame, app, &mut frame_state))
        .expect("ui should render");
    app.set_frame_state(frame_state.clone());
    frame_state
}

fn wait_for_directory_counts(app: &mut App) {
    for _ in 0..100 {
        let _ = app.process_directory_item_count_timer();
        let _ = app.process_background_jobs();
        let all_visible_directory_counts_loaded = app
            .navigation
            .entries
            .iter()
            .filter(|entry| entry.is_dir())
            .all(|entry| app.directory_item_count_label(entry).is_some());
        if all_visible_directory_counts_loaded {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out waiting for directory counts");
}

fn wait_for_background_preview(app: &mut App) {
    for _ in 0..200 {
        if app.process_background_jobs() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out waiting for background preview");
}

fn wait_for_search_index(app: &mut App) {
    for _ in 0..200 {
        let _ = app.process_background_jobs();
        if app.search_is_open() && !app.search_is_loading() && app.search_candidate_count() > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out waiting for search index");
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
}

fn rect_row_text(buffer: &Buffer, rect: Rect, y: u16) -> String {
    (rect.x..rect.x.saturating_add(rect.width))
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
}

fn buffer_text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| row_text(buffer, y))
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_text(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

fn rect_inside(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x.saturating_add(inner.width) <= outer.x.saturating_add(outer.width)
        && inner.y.saturating_add(inner.height) <= outer.y.saturating_add(outer.height)
}

#[test]
fn wide_browser_layout_keeps_entries_and_preview_side_by_side() {
    let root = temp_path("wide-browser-layout");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("report.txt"), "hello\nworld\n").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(140, 30)).expect("terminal should init");

    let state = draw_ui(&mut terminal, &mut app);
    let entries_panel = state
        .entries_panel
        .expect("entries panel should be rendered");
    let preview_panel = state
        .preview_panel
        .expect("preview panel should be rendered");
    let sidebar_rect = state
        .sidebar_hits
        .first()
        .map(|hit| hit.rect)
        .expect("sidebar should expose at least one hit rect");

    assert!(
        sidebar_rect.x.saturating_add(sidebar_rect.width) <= entries_panel.x,
        "wide layout should keep the sidebar to the left of the entries panel"
    );
    assert_eq!(
        entries_panel.y, preview_panel.y,
        "wide layout should align entries and preview panels on the same row"
    );
    assert_eq!(
        entries_panel.height, preview_panel.height,
        "wide layout should keep entries and preview panels at the same height"
    );
    assert!(
        entries_panel.x.saturating_add(entries_panel.width) <= preview_panel.x,
        "wide layout should place the preview panel to the right of the entries panel"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_browser_layout_keeps_entries_and_preview_side_by_side() {
    let root = temp_path("compact-browser-layout");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("report.txt"), "hello\nworld\n").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(66, 30)).expect("terminal should init");

    let state = draw_ui(&mut terminal, &mut app);
    let entries_panel = state
        .entries_panel
        .expect("entries panel should be rendered");
    let preview_panel = state
        .preview_panel
        .expect("preview panel should be rendered");

    assert!(
        entries_panel.x.saturating_add(entries_panel.width) <= preview_panel.x,
        "compact layout should keep the preview panel to the right of the entries panel"
    );
    assert_eq!(
        entries_panel.y, preview_panel.y,
        "compact layout should keep entries and preview aligned on the same row"
    );
    assert_eq!(
        entries_panel.height, preview_panel.height,
        "compact layout should keep entries and preview at the same height"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn list_view_ignores_grid_zoom_levels() {
    let root = temp_path("list-view-ignores-grid-zoom");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.navigation.view_mode = crate::app::ViewMode::List;
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    app.navigation.zoom_level = 0;
    let compact = draw_ui(&mut terminal, &mut app);

    app.navigation.zoom_level = 2;
    let zoomed = draw_ui(&mut terminal, &mut app);

    assert_eq!(compact.metrics.rows_visible, zoomed.metrics.rows_visible);
    assert_eq!(
        compact
            .entry_hits
            .first()
            .expect("row should exist")
            .rect
            .height,
        zoomed
            .entry_hits
            .first()
            .expect("row should exist")
            .rect
            .height,
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn wide_browser_layout_uses_default_sidebar_width() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 140,
            height: 20,
        },
        None,
        true,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");
    let preview = layout.preview.expect("preview should be visible");

    assert_eq!(sidebar.width, 20);
    assert_eq!(sidebar.width + entries.width + preview.width, 140);
}

#[test]
fn narrowing_horizontal_browser_layout_starts_shrinking_sidebar_early() {
    for (width, expected_sidebar_width) in [
        (104, 20),
        (100, 19),
        (96, 18),
        (88, 16),
        (80, 14),
        (72, 12),
        (68, 11),
        (64, 5),
        (56, 5),
        (46, 5),
    ] {
        let layout = resolve_body_layout(
            Rect {
                x: 0,
                y: 0,
                width,
                height: 20,
            },
            None,
            true,
            false,
        );

        assert_eq!(
            layout.sidebar.expect("sidebar should be visible").width,
            expected_sidebar_width
        );
    }
}

#[test]
fn narrow_legacy_layout_keeps_preview_without_starving_files() {
    for (width, height, expect_sidebar, expect_stacked) in [
        (54, 24, true, true),
        (45, 24, true, true),
        (39, 28, true, true),
        (46, 20, true, false),
        (45, 23, true, false),
        (20, 18, false, true),
    ] {
        let layout = resolve_body_layout(
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            None,
            true,
            false,
        );

        let entries = layout.entries.expect("entries should be visible");
        let preview = layout.preview.expect("preview should be visible");
        assert_eq!(layout.sidebar.is_some(), expect_sidebar, "width {width}");
        if let Some(sidebar) = layout.sidebar {
            assert_eq!(sidebar.width, 5, "width {width}");
        }
        if expect_stacked {
            assert_eq!(entries.x, preview.x, "width {width} should stay stacked");
            assert!(
                preview.y > entries.y,
                "width {width} should keep preview below files"
            );
            assert_eq!(entries.width, preview.width);
        } else {
            assert_eq!(
                entries.y, preview.y,
                "width {width} should remain horizontal"
            );
            assert_eq!(
                layout.sidebar.map_or(0, |sidebar| sidebar.width) + entries.width + preview.width,
                width
            );
        }
    }
}

#[test]
fn weighted_layout_splits_three_panes_across_the_available_width() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 140,
            height: 20,
        },
        Some(PaneWeights {
            places: 10,
            files: 45,
            preview: 45,
        }),
        true,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");
    let preview = layout.preview.expect("preview should be visible");

    assert!(sidebar.width >= 16);
    assert!(entries.width >= 28);
    assert!(preview.width >= 24);
    assert_eq!(sidebar.width + entries.width + preview.width, 140);
    assert_eq!(sidebar.x.saturating_add(sidebar.width), entries.x);
    assert_eq!(entries.x.saturating_add(entries.width), preview.x);
}

#[test]
fn weighted_layout_can_hide_the_sidebar() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 110,
            height: 20,
        },
        Some(PaneWeights {
            places: 0,
            files: 60,
            preview: 50,
        }),
        true,
        false,
    );

    let entries = layout.entries.expect("entries should be visible");
    let preview = layout.preview.expect("preview should be visible");

    assert_eq!(layout.sidebar, None);
    assert!(entries.width >= 28);
    assert!(preview.width >= 24);
    assert_eq!(entries.width, 60);
    assert_eq!(preview.width, 50);
    assert_eq!(entries.x.saturating_add(entries.width), preview.x);
}

#[test]
fn weighted_layout_hides_the_preview_when_requested() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 20,
        },
        Some(PaneWeights {
            places: 15,
            files: 85,
            preview: 0,
        }),
        true,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");

    assert_eq!(layout.preview, None);
    assert!(sidebar.width >= 16);
    assert!(entries.width >= 28);
    assert_eq!(sidebar.width + entries.width, 100);
}

#[test]
fn fullscreen_preview_uses_body_without_browser_panes() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 20,
    };
    let layout = resolve_body_layout(area, None, true, true);

    assert_eq!(layout.sidebar, None);
    assert_eq!(layout.entries, None);
    assert_eq!(layout.preview, Some(area));
}

#[test]
fn preview_toggle_hides_preview_without_collapsing_legacy_sidebar() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 20,
        },
        None,
        false,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");

    assert_eq!(layout.preview, None);
    assert_eq!(sidebar.width, 19);
    assert_eq!(sidebar.width + entries.width, 100);
}

#[test]
fn preview_toggle_keeps_icon_sidebar_at_stacked_breakpoint_width() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 21,
            height: 20,
        },
        None,
        false,
        false,
    );

    let sidebar = layout.sidebar.expect("icon sidebar should stay visible");
    let entries = layout.entries.expect("entries should be visible");

    assert_eq!(layout.preview, None);
    assert_eq!(sidebar.width, 5);
    assert_eq!(entries.width, 16);
}

#[test]
fn preview_toggle_hides_preview_without_changing_config_weights() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 20,
        },
        Some(PaneWeights {
            places: 15,
            files: 40,
            preview: 45,
        }),
        false,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");

    assert_eq!(layout.preview, None);
    assert_eq!(sidebar.width + entries.width, 100);
}

#[test]
fn weighted_layout_uses_horizontal_layout_when_visible_panes_fit_minimums() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 20,
        },
        Some(PaneWeights {
            places: 10,
            files: 45,
            preview: 45,
        }),
        true,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");
    let preview = layout.preview.expect("preview should be visible");

    assert!(sidebar.width >= 16);
    assert!(entries.width >= 28);
    assert!(preview.width >= 24);
    assert_eq!(entries.y, preview.y);
    assert_eq!(entries.height, preview.height);
    assert_eq!(sidebar.width + entries.width + preview.width, 120);
}

#[test]
fn weighted_layout_stacks_preview_when_width_is_tight_and_height_is_sufficient() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 20,
        },
        Some(PaneWeights {
            places: 10,
            files: 45,
            preview: 45,
        }),
        true,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");
    let preview = layout.preview.expect("preview should be visible");

    assert!(sidebar.width >= 16);
    assert_eq!(entries.x, preview.x);
    assert_eq!(entries.width, preview.width);
    assert_eq!(entries.height, 11);
    assert_eq!(preview.height, 9);
}

#[test]
fn weighted_stacked_layout_respects_file_and_preview_height_weights() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 60,
        },
        Some(PaneWeights {
            places: 10,
            files: 30,
            preview: 70,
        }),
        true,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");
    let preview = layout.preview.expect("preview should be visible");

    assert!(sidebar.width >= 16);
    assert_eq!(entries.x, preview.x);
    assert_eq!(entries.width, preview.width);
    assert_eq!(entries.height, 23);
    assert_eq!(preview.height, 37);
}

#[test]
fn weighted_stacked_layout_can_favor_files_over_preview() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 60,
        },
        Some(PaneWeights {
            places: 10,
            files: 70,
            preview: 30,
        }),
        true,
        false,
    );

    let entries = layout.entries.expect("entries should be visible");
    let preview = layout.preview.expect("preview should be visible");

    assert_eq!(entries.x, preview.x);
    assert_eq!(entries.width, preview.width);
    assert_eq!(entries.height, 39);
    assert_eq!(preview.height, 21);
}

#[test]
fn weighted_layout_avoids_stacking_when_height_is_too_limited() {
    let layout = resolve_body_layout(
        Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 14,
        },
        Some(PaneWeights {
            places: 10,
            files: 45,
            preview: 45,
        }),
        true,
        false,
    );

    let sidebar = layout.sidebar.expect("sidebar should be visible");
    let entries = layout.entries.expect("entries should be visible");

    assert!(sidebar.width >= 16);
    assert_eq!(layout.preview, None);
    assert_eq!(entries.y, 0);
    assert_eq!(entries.height, 14);
}

#[test]
fn sidebar_clamps_long_labels_when_width_is_tight() {
    let root = temp_path("sidebar-clamp");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.navigation.sidebar = vec![SidebarRow::Item(SidebarItem::new(
        SidebarItemKind::Downloads,
        "Downloads Directory",
        "D",
        root.clone(),
        root.clone(),
    ))];

    let mut terminal = Terminal::new(TestBackend::new(14, 5)).expect("terminal should init");
    let mut frame_state = FrameState::default();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                &app,
                &mut frame_state,
                theme::palette(),
            );
        })
        .expect("sidebar should render");

    let rendered = buffer_text(terminal.backend().buffer());
    assert!(
        rendered.contains("Downloa…"),
        "expected the narrow sidebar to clamp long labels, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sidebar_uses_icons_only_at_icon_width() {
    let root = temp_path("sidebar-icon-only");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.navigation.sidebar = vec![
        SidebarRow::Section { title: "Devices" },
        SidebarRow::Item(SidebarItem::new(
            SidebarItemKind::Downloads,
            "Downloads Directory",
            "D",
            root.clone(),
            root.clone(),
        )),
    ];

    let mut terminal = Terminal::new(TestBackend::new(5, 5)).expect("terminal should init");
    let mut frame_state = FrameState::default();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                &app,
                &mut frame_state,
                theme::palette(),
            );
        })
        .expect("sidebar should render");

    let rendered = buffer_text(terminal.backend().buffer());
    assert!(rendered.contains("D"));
    assert!(!rendered.contains("Places"));
    assert!(!rendered.contains("Devices"));
    assert!(!rendered.contains("Down"));
    assert_eq!(frame_state.sidebar_hits.len(), 1);
    assert_eq!(frame_state.sidebar_hits[0].path, root);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn sidebar_sections_render_without_creating_click_targets() {
    let root = temp_path("sidebar-sections");
    let drive = root.join("usb");
    fs::create_dir_all(&drive).expect("failed to create temp dirs");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.navigation.sidebar = vec![
        SidebarRow::Section { title: "Devices" },
        SidebarRow::Item(SidebarItem::new(
            SidebarItemKind::Device { removable: true },
            "Vacation",
            "U",
            drive.clone(),
            drive.clone(),
        )),
    ];

    let mut terminal = Terminal::new(TestBackend::new(18, 6)).expect("terminal should init");
    let mut frame_state = FrameState::default();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                &app,
                &mut frame_state,
                theme::palette(),
            );
        })
        .expect("sidebar should render");

    let rendered = buffer_text(terminal.backend().buffer());
    assert!(rendered.contains("Devices"));
    assert!(
        row_text(terminal.backend().buffer(), 1).contains("│ Devices"),
        "section labels should align with the panel title, got: {rendered:?}"
    );
    assert_eq!(frame_state.sidebar_hits.len(), 1);
    assert_eq!(frame_state.sidebar_hits[0].path, drive);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn sidebar_marks_symlinked_place_active_by_identity_path() {
    use std::os::unix::fs::symlink;

    let root = temp_path("sidebar-symlink-active");
    let target = root.join("target");
    let linked = root.join("linked");
    fs::create_dir_all(&target).expect("failed to create target dir");
    symlink(&target, &linked).expect("failed to create sidebar symlink");

    let target_identity = target.canonicalize().expect("target should canonicalize");
    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.navigation.cwd = target_identity.clone();
    app.navigation.sidebar = vec![SidebarRow::Item(SidebarItem::new(
        SidebarItemKind::Custom,
        "Linked",
        "L",
        linked.clone(),
        target_identity,
    ))];

    let mut terminal = Terminal::new(TestBackend::new(18, 4)).expect("terminal should init");
    let mut frame_state = FrameState::default();
    terminal
        .draw(|frame| {
            render_sidebar(
                frame,
                frame.area(),
                &app,
                &mut frame_state,
                theme::palette(),
            );
        })
        .expect("sidebar should render");

    assert!(
        row_text(terminal.backend().buffer(), 1).contains("▌"),
        "symlinked sidebar place should render as active"
    );
    assert_eq!(frame_state.sidebar_hits.len(), 1);
    assert_eq!(frame_state.sidebar_hits[0].path, linked);

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn split_scrollbar_area_only_reserves_a_column_when_width_allows() {
    let tight = Rect {
        x: 3,
        y: 4,
        width: 5,
        height: 7,
    };
    let (content, scrollbar) = split_scrollbar_area(tight);
    assert_eq!(content, tight);
    assert_eq!(scrollbar, None);

    let roomy = Rect {
        x: 8,
        y: 2,
        width: 6,
        height: 9,
    };
    let (content, scrollbar) = split_scrollbar_area(roomy);
    let scrollbar = scrollbar.expect("wide enough areas should reserve a scrollbar column");
    assert_eq!(content.width, 5);
    assert_eq!(scrollbar.width, 1);
    assert_eq!(content.height, roomy.height);
    assert_eq!(scrollbar.height, roomy.height);
    assert_eq!(scrollbar.x, content.x.saturating_add(content.width));
}

#[test]
fn grid_view_keeps_entry_hits_inside_the_entries_panel() {
    let root = temp_path("grid-layout-hits");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for index in 0..12 {
        fs::write(root.join(format!("item-{index:02}.txt")), "content\n")
            .expect("failed to write temp file");
    }

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.navigation.view_mode = crate::app::ViewMode::Grid;
    let mut terminal = Terminal::new(TestBackend::new(140, 30)).expect("terminal should init");

    let state = draw_ui(&mut terminal, &mut app);
    let entries_panel = state
        .entries_panel
        .expect("entries panel should be rendered");

    assert!(
        state.metrics.cols >= 2,
        "wide grid layouts should expose multiple columns through view metrics"
    );
    assert!(
        !state.entry_hits.is_empty(),
        "grid rendering should expose hit rects for visible entries"
    );
    for hit in &state.entry_hits {
        assert!(
            rect_inside(entries_panel, hit.rect),
            "entry hit {:?} should stay inside the entries panel {:?}",
            hit.rect,
            entries_panel
        );
    }

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn create_overlay_uses_themed_bold_icon_for_live_json_names() {
    let root = temp_path("create-overlay-json-icon");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('a'))))
        .expect("create overlay should open");
    for ch in "i.json".chars() {
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char(ch))))
            .expect("typing into create overlay should succeed");
    }

    let state = draw_ui(&mut terminal, &mut app);
    let list_area = state
        .create_list_area
        .expect("create list area should be rendered");
    let icon_cell = &terminal.backend().buffer()[(list_area.x, list_area.y)];

    assert_eq!(
        icon_cell.symbol(),
        "",
        "create overlay should resolve the JSON icon while typing",
    );
    assert!(
        icon_cell.modifier.contains(Modifier::BOLD),
        "create overlay icon should use the same bold styling as other file icon surfaces",
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn create_overlay_scrolls_to_keep_the_active_line_visible() {
    let root = temp_path("create-overlay-scroll");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('a'))))
        .expect("create overlay should open");
    for index in 0..14 {
        for ch in format!("file-{index:02}.txt").chars() {
            app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char(ch))))
                .expect("typing create line should succeed");
        }
        if index < 13 {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('j'),
                KeyModifiers::CONTROL,
            )))
            .expect("inserting another create line should succeed");
        }
    }

    let state = draw_ui(&mut terminal, &mut app);
    let list_area = state
        .create_list_area
        .expect("create overlay should render a list area");

    assert_eq!(
        state.create_scroll_top, 2,
        "create overlay should scroll once the cursor moves past the twelfth visible line"
    );
    assert!(
        rect_row_text(terminal.backend().buffer(), list_area, list_area.y).contains("file-02.txt"),
        "expected the first visible create row to track the computed scroll top"
    );
    assert!(
        rect_row_text(
            terminal.backend().buffer(),
            list_area,
            list_area
                .y
                .saturating_add(list_area.height.saturating_sub(1)),
        )
        .contains("file-13.txt"),
        "expected the active create line to remain visible at the bottom of the list"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn bulk_rename_overlay_scrolls_to_keep_the_active_row_visible() {
    let root = temp_path("bulk-rename-overlay-scroll");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for index in 0..14 {
        fs::write(root.join(format!("file-{index:02}.txt")), "content")
            .expect("failed to write test file");
    }

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.navigation.view_mode = crate::app::ViewMode::List;
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    for _ in 0..14 {
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char(' '))))
            .expect("selection toggle should succeed");
    }
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('r'))))
        .expect("bulk rename overlay should open");
    for _ in 0..13 {
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Down)))
            .expect("bulk rename cursor movement should succeed");
    }

    let state = draw_ui(&mut terminal, &mut app);
    let list_area = state
        .bulk_rename_list_area
        .expect("bulk rename overlay should render a list area");

    assert!(
        state.rename_panel.is_some(),
        "bulk rename overlay should keep using the shared rename panel slot"
    );
    assert_eq!(
        state.bulk_rename_scroll_top, 2,
        "bulk rename overlay should scroll once the active row moves past the twelfth visible line"
    );
    assert!(
        rect_row_text(terminal.backend().buffer(), list_area, list_area.y).contains("file-02.txt"),
        "expected the first visible bulk rename row to match the computed scroll top"
    );
    assert!(
        rect_row_text(
            terminal.backend().buffer(),
            list_area,
            list_area
                .y
                .saturating_add(list_area.height.saturating_sub(1)),
        )
        .contains("file-13.txt"),
        "expected the active bulk rename row to remain visible at the bottom of the list"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn archive_create_overlay_adapts_contents_to_short_terminals() {
    let root = temp_path("archive-create-short-terminal");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("source.txt"), "alpha").expect("failed to write source");

    for (height, shows_contents) in [(8, false), (10, true)] {
        let mut app = App::new_at(root.clone()).expect("app should load temp directory");
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('C'))))
            .expect("archive create overlay should open");
        let mut terminal =
            Terminal::new(TestBackend::new(94, height)).expect("terminal should init");

        let state = draw_ui(&mut terminal, &mut app);
        let panel = state
            .archive_create_panel
            .expect("archive create panel should render");

        assert_eq!(
            state.archive_create_list_area.is_some(),
            shows_contents,
            "contents visibility should track whether a complete row fits at height {height}"
        );
        assert!(
            app.collect_popup_rects().contains(&panel),
            "archive creation popup should mask terminal image previews"
        );
        if !shows_contents {
            assert_eq!(panel.height, 6, "compact popup should not leave dead rows");
        }
    }

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn copy_overlay_renders_expected_labels_and_hit_rects() {
    let root = temp_path("copy-overlay-render");
    fs::create_dir_all(root.join("docs")).expect("failed to create docs dir");
    fs::write(root.join("docs/report.final.md"), "hello\n").expect("failed to write temp file");

    let mut app = App::new_at(root.join("docs")).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('c'))))
        .expect("copy overlay should open");

    let state = draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        state.copy_panel.is_some(),
        "copy overlay should render a popup panel"
    );
    assert_eq!(
        state.copy_hits.len(),
        4,
        "copy overlay should expose one hit rect per visible row"
    );
    assert!(
        rendered.contains("Copy to clipboard"),
        "expected copy overlay title to be rendered, got: {rendered:?}"
    );
    assert!(
        rendered.contains("c -> file name"),
        "expected copy overlay to render the file-name shortcut row, got: {rendered:?}"
    );
    assert!(
        rendered.contains("d -> directory path"),
        "expected copy overlay to render the directory-path shortcut row, got: {rendered:?}"
    );
    assert!(
        rendered.contains("p -> file path"),
        "expected copy overlay to render the file-path shortcut row, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn goto_overlay_renders_expected_labels_and_hit_rects() {
    let root = temp_path("goto-overlay-render");
    fs::create_dir_all(root.join("docs")).expect("failed to create docs dir");
    fs::write(root.join("docs/report.final.md"), "hello\n").expect("failed to write temp file");

    let mut app = App::new_at(root.join("docs")).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(110, 24)).expect("terminal should init");

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('g'))))
        .expect("goto overlay should open");

    let state = draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        state.goto_panel.is_some(),
        "goto overlay should render a popup panel"
    );
    assert_eq!(
        state.goto_hits.len(),
        5,
        "goto overlay should expose one hit rect per visible shortcut"
    );
    assert!(
        rendered.contains("Go to"),
        "expected goto overlay title to be rendered, got: {rendered:?}"
    );
    assert!(
        rendered.contains("g -> top"),
        "expected goto overlay to render the top shortcut row, got: {rendered:?}"
    );
    // The label is ".config" on Linux/BSD, "Application Support" on macOS,
    // and "AppData" on Windows — check for "c ->" which is present on all.
    assert!(
        rendered.contains("c ->"),
        "expected goto overlay to render the config shortcut row, got: {rendered:?}"
    );
    assert!(
        rendered.contains("t -> trash"),
        "expected goto overlay to render the trash shortcut row, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn open_with_overlay_renders_expected_hits() {
    let root = temp_path("open-with-overlay-render");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("document.txt"), "hello\n").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    // Wait for the directory to load so the file entry is visible.
    for _ in 0..100 {
        let _ = app.process_background_jobs();
        if !app.navigation.entries.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    app.inject_open_with_for_test("Text Editor", "/usr/bin/true", vec![], false);

    let state = draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        state.open_with_panel.is_some(),
        "open-with overlay should render a popup panel"
    );
    assert!(
        !state.open_with_hits.is_empty(),
        "open-with overlay should expose at least one hit rect"
    );
    assert!(
        rendered.contains("Open With"),
        "expected open-with title to be rendered, got: {rendered:?}"
    );
    // Shortcut '1' always maps to the first row when apps are found.
    assert!(
        rendered.contains("1 ->"),
        "expected first shortcut row to be rendered, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn open_with_overlay_draws_scrollbar_only_when_rows_overflow() {
    let root = temp_path("open-with-overlay-scrollbar");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("document.txt"), "hello\n").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).expect("terminal should init");

    app.inject_open_with_for_test("Text Editor", "/usr/bin/true", vec![], false);
    let state = draw_ui(&mut terminal, &mut app);
    let panel = state
        .open_with_panel
        .expect("open-with panel should render");
    let inner = helpers::inner_with_padding(panel);
    let right_column = (inner.y..inner.y + inner.height)
        .map(|y| terminal.backend().buffer()[(inner.x + inner.width - 1, y)].symbol())
        .collect::<String>();
    assert!(
        !right_column.contains('┃'),
        "single-row open-with overlay should not draw a scrollbar"
    );

    app.inject_open_with_rows_for_test(
        (0..20)
            .map(|index| {
                (
                    format!("App {index}"),
                    "/usr/bin/true".to_string(),
                    Vec::new(),
                    false,
                )
            })
            .collect(),
    );
    let state = draw_ui(&mut terminal, &mut app);
    let panel = state
        .open_with_panel
        .expect("open-with panel should render");
    let inner = helpers::inner_with_padding(panel);
    let right_column = (inner.y..inner.y + inner.height)
        .map(|y| terminal.backend().buffer()[(inner.x + inner.width - 1, y)].symbol())
        .collect::<String>();

    assert!(
        right_column.contains('┃'),
        "overflowing open-with overlay should draw a scrollbar thumb"
    );
    assert!(
        right_column.contains('│'),
        "overflowing open-with overlay should draw a scrollbar track"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn trash_overlay_tabs_focus_between_confirm_and_cancel_buttons() {
    let root = temp_path("trash-overlay-focus");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("draft.txt"), "hello\n").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");
    let palette = theme::palette();

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('d'))))
        .expect("trash overlay should open");
    let initial_state = draw_ui(&mut terminal, &mut app);
    let confirm_rect = initial_state
        .trash_confirm_btn
        .expect("trash confirm button should be rendered");
    let cancel_rect = initial_state
        .trash_cancel_btn
        .expect("trash cancel button should be rendered");

    let confirm_cell = &terminal.backend().buffer()[(
        confirm_rect.x.saturating_add(confirm_rect.width / 2),
        confirm_rect.y,
    )];
    let cancel_cell = &terminal.backend().buffer()[(
        cancel_rect.x.saturating_add(cancel_rect.width / 2),
        cancel_rect.y,
    )];
    assert_eq!(
        confirm_cell.bg, palette.selected_bg,
        "confirm button should start focused"
    );
    assert_eq!(
        cancel_cell.bg, palette.chrome_alt,
        "cancel button should start unfocused"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Tab)))
        .expect("focus toggle should succeed");
    let toggled_state = draw_ui(&mut terminal, &mut app);
    let confirm_cell = &terminal.backend().buffer()[(
        toggled_state
            .trash_confirm_btn
            .expect("confirm button should remain rendered")
            .x
            .saturating_add(confirm_rect.width / 2),
        confirm_rect.y,
    )];
    let cancel_cell = &terminal.backend().buffer()[(
        toggled_state
            .trash_cancel_btn
            .expect("cancel button should remain rendered")
            .x
            .saturating_add(cancel_rect.width / 2),
        cancel_rect.y,
    )];
    assert_eq!(
        confirm_cell.bg, palette.chrome_alt,
        "confirm button should lose focus after tabbing"
    );
    assert_eq!(
        cancel_cell.bg, palette.selected_bg,
        "cancel button should receive focus after tabbing"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn search_overlay_scrolls_selected_results_and_tracks_hit_rects() {
    let root = temp_path("search-overlay-scroll");
    fs::create_dir_all(&root).expect("failed to create temp root");
    for index in 0..12 {
        fs::create_dir_all(root.join(format!("folder-{index:02}")))
            .expect("failed to create search folder");
    }

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");
    let palette = theme::palette();

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('f'))))
        .expect("search overlay should open");
    wait_for_search_index(&mut app);

    let initial_state = draw_ui(&mut terminal, &mut app);
    let search_panel = initial_state
        .search_panel
        .expect("search overlay should render a popup panel");
    assert!(
        initial_state.search_rows_visible > 0,
        "search overlay should expose the visible row budget through frame state"
    );
    let inner = helpers::inner_with_padding(search_panel);
    let results_area = Rect {
        x: inner.x,
        y: inner.y + 4,
        width: inner.width,
        height: inner.height.saturating_sub(4),
    };
    let bottom_row = rect_row_text(
        terminal.backend().buffer(),
        Rect {
            x: results_area.x,
            y: results_area.y + results_area.height.saturating_sub(1),
            width: results_area.width,
            height: 1,
        },
        results_area.y + results_area.height.saturating_sub(1),
    );
    assert!(
        !bottom_row.contains("Enter open")
            && !bottom_row.contains("Esc close")
            && !bottom_row.contains("move"),
        "search results bottom row should not render generic key hints, got: {bottom_row:?}"
    );
    let right_column = (results_area.y..results_area.y + results_area.height)
        .map(|y| terminal.backend().buffer()[(results_area.x + results_area.width - 1, y)].symbol())
        .collect::<String>();
    assert!(
        right_column.contains('┃'),
        "overflowing search overlay should draw a scrollbar thumb"
    );
    assert!(
        right_column.contains('│'),
        "overflowing search overlay should draw a scrollbar track"
    );

    for _ in 0..8 {
        app.handle_event(Event::Key(KeyEvent::from(KeyCode::Down)))
            .expect("search selection movement should succeed");
    }

    let state = draw_ui(&mut terminal, &mut app);
    let visible_rows = app.search_rows(state.search_rows_visible);
    let selected_offset = visible_rows
        .iter()
        .position(|row| row.selected)
        .expect("search overlay should keep one visible row selected");
    let selected_rect = state
        .search_hits
        .get(selected_offset)
        .expect("search overlay should expose hit rects for visible rows")
        .rect;
    let selected_cell =
        &terminal.backend().buffer()[(selected_rect.x.saturating_add(2), selected_rect.y)];

    assert!(
        visible_rows.first().is_some_and(|row| row.index > 0),
        "search overlay should scroll once the selected result moves past the visible window"
    );
    assert_eq!(
        state.search_hits.len(),
        visible_rows.len(),
        "search hit rects should stay aligned with the rendered visible rows"
    );
    assert_eq!(
        state.search_hits[selected_offset].index, visible_rows[selected_offset].index,
        "search hit rect indexes should stay aligned with the visible search rows"
    );
    assert_eq!(
        selected_cell.bg, palette.selected_bg,
        "selected search rows should keep the focused row background after scrolling"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn preview_title_row_is_cleared_when_switching_to_shorter_names() {
    let root = temp_path("preview-title");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(
        root.join("a-this-is-a-very-long-preview-marker-name.txt"),
        "first\n",
    )
    .expect("failed to write long file");
    fs::write(root.join("b.txt"), "second\n").expect("failed to write short file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    let initial_state = draw_ui(&mut terminal, &mut app);
    let preview_panel = initial_state
        .preview_panel
        .expect("preview panel should be rendered");
    let initial_title = rect_row_text(terminal.backend().buffer(), preview_panel, preview_panel.y);
    assert!(
        initial_title.contains("a-this-is-a-very"),
        "expected initial preview title row to show the long file name, got: {initial_title:?}"
    );

    app.handle_event(Event::Key(KeyEvent::from(KeyCode::Down)))
        .expect("selection change should succeed");
    let second_state = draw_ui(&mut terminal, &mut app);
    let second_preview_panel = second_state
        .preview_panel
        .expect("preview panel should still be rendered");
    let second_title = rect_row_text(
        terminal.backend().buffer(),
        second_preview_panel,
        second_preview_panel.y,
    );

    assert!(
        second_title.contains("b.txt"),
        "expected second preview title row to show the shorter file name, got: {second_title:?}"
    );
    assert!(
        !second_title.contains("a-this-is-a-very"),
        "stale preview title text remained after rerender: {second_title:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
#[cfg(not(windows))] // Windows rejects filenames containing \r
fn filenames_with_control_characters_are_rendered_safely() {
    let root = temp_path("control-char-name");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("bad\rname.c"), "int main(void) { return 0; }\n")
        .expect("failed to write control-char file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());
    assert!(
        rendered.contains("bad^Mname.c"),
        "expected control characters to be sanitized in the UI, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn preview_panel_does_not_repeat_generic_metadata() {
    let root = temp_path("preview-details");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("report.txt"), "hello\n").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        !rendered.contains("Type     "),
        "preview panel should not repeat generic type metadata, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("Size     "),
        "preview panel should not repeat generic size metadata, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("Modified "),
        "preview panel should not repeat generic modified metadata, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn help_overlay_keeps_controls_readable_and_drops_auto_reload_row() {
    let root = temp_path("help-overlay-format");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.overlays.help = true;
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("terminal should init");

    draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        rendered.contains("Double-click"),
        "expected help overlay to keep the double-click label readable, got: {rendered:?}"
    );
    assert!(
        rendered.contains("open item"),
        "expected help overlay to keep the action text readable, got: {rendered:?}"
    );
    assert!(
        rendered.contains("Ctrl+F"),
        "expected help overlay to keep the file search shortcut visible, got: {rendered:?}"
    );
    assert!(
        rendered.contains("zoxide history"),
        "expected help overlay to list the zoxide shortcut, got: {rendered:?}"
    );
    assert!(
        rendered.contains("/… or …/") && rendered.contains("folder in create prompt"),
        "expected help overlay to show the create prompt folder-name hint, got: {rendered:?}"
    );
    assert!(
        rendered.contains("Alt/Shift+Enter"),
        "expected help overlay to show the current create prompt newline hint, got: {rendered:?}"
    );
    assert!(
        rendered.contains("delete permanently"),
        "expected help overlay to list the permanent delete shortcut, got: {rendered:?}"
    );
    assert!(
        rendered.contains("Wheel              scroll"),
        "expected help overlay to describe wheel routing accurately, got: {rendered:?}"
    );
    assert!(
        rendered.contains("Preview"),
        "expected help overlay to include the Preview section header, got: {rendered:?}"
    );
    assert!(
        rendered.contains("K/Shift+↑") && rendered.contains("J/Shift+↓"),
        "expected help overlay to list the vertical preview scroll keys, got: {rendered:?}"
    );
    assert!(
        rendered.contains("H/Shift+←") && rendered.contains("L/Shift+→"),
        "expected help overlay to list uppercase and Shift+arrow horizontal preview scroll keys, got: {rendered:?}"
    );
    assert!(
        rendered.contains("symlink relative"),
        "expected help overlay to keep the final clipboard entry visible without clipping, got: {rendered:?}"
    );
    assert!(
        rendered.contains("View"),
        "expected help overlay to include the View section header, got: {rendered:?}"
    );
    assert!(
        rendered.contains("toggle grid / list"),
        "expected help overlay to keep View entries visible, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("Double clickopen"),
        "help overlay fused the key and action labels together: {rendered:?}"
    );
    assert!(
        !rendered.contains("current folder reloads itself"),
        "help overlay should not list auto-reload as a control: {rendered:?}"
    );
    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_help_overlay_scrolls_instead_of_truncating_small_terminals() {
    let root = temp_path("compact-help-overlay");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.overlays.help = true;
    let mut terminal = Terminal::new(TestBackend::new(60, 18)).expect("terminal should init");

    let frame_state = draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        rendered.contains("Navigation"),
        "compact help should start with the first section, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("? / Esc") && !rendered.contains("close help"),
        "compact help should not render an obvious close footer, got: {rendered:?}"
    );
    assert!(
        rendered.contains("┃"),
        "scrollable compact help should draw a right-side scrollbar, got: {rendered:?}"
    );
    assert!(
        frame_state.help_scroll_max > 0,
        "compact help should publish its real scroll limit"
    );
    let max_scroll = frame_state.help_scroll_max;
    let page_step = frame_state.help_rows_visible.saturating_sub(2).max(1);
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::End)))
        .expect("End should jump to the bottom of help");
    assert_eq!(
        app.overlays.help_scroll, max_scroll,
        "End should use the real scroll limit instead of an arbitrary sentinel"
    );
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::PageUp)))
        .expect("PageUp should move by a viewport-sized step");
    assert_eq!(
        app.overlays.help_scroll,
        max_scroll.saturating_sub(page_step),
        "PageUp should keep context instead of jumping by a fixed magic number"
    );
    app.handle_event(Event::Key(KeyEvent::from(KeyCode::End)))
        .expect("End should return to the bottom of help");
    draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        rendered.contains("File Actions") && rendered.contains("open with"),
        "compact help should keep late sections reachable, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("?/Esc") && !rendered.contains("? / Esc"),
        "compact help should not keep the obvious close hint visible, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn medium_help_overlay_uses_two_columns_before_scrolling() {
    let root = temp_path("medium-help-overlay");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.overlays.help = true;
    let mut terminal = Terminal::new(TestBackend::new(92, 37)).expect("terminal should init");

    draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        rendered.contains("Navigation") && rendered.contains("File Actions"),
        "medium help should use both sides instead of a sparse single column, got: {rendered:?}"
    );
    assert!(
        rendered.contains("create file or folder"),
        "right-side actions should be visible before scrolling, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn chooser_help_overlay_uses_chooser_actions_without_changing_esc() {
    let root = temp_path("chooser-help-overlay");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    app.enable_chooser_mode();
    app.overlays.help = true;
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("terminal should init");

    draw_ui(&mut terminal, &mut app);
    let rendered = buffer_text(terminal.backend().buffer());

    assert!(
        rendered.contains("Chooser controls"),
        "expected chooser help title, got: {rendered:?}"
    );
    assert!(
        rendered.contains("choose"),
        "expected chooser help to list choose action, got: {rendered:?}"
    );
    assert!(
        rendered.contains("enter folder / choose"),
        "expected chooser double-click help to fit on one line, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("enter folder / open"),
        "expected chooser help to hide the fully shadowed open-or-enter action, got: {rendered:?}"
    );
    assert!(
        rendered.contains("cancel chooser"),
        "expected chooser help to relabel quit as cancellation, got: {rendered:?}"
    );
    assert!(
        rendered.contains("Esc"),
        "expected chooser help to keep normal Esc behavior visible, got: {rendered:?}"
    );
    assert!(
        rendered.contains("clear selection"),
        "expected chooser help to keep Esc as clear selection, got: {rendered:?}"
    );
    assert!(
        rendered.contains("copy path details"),
        "expected chooser help to keep copy path visible, got: {rendered:?}"
    );
    assert!(
        rendered.contains("cut"),
        "expected chooser help to keep cut visible, got: {rendered:?}"
    );
    assert!(
        rendered.contains("paste"),
        "expected chooser help to keep paste visible, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("quit without cd"),
        "chooser help should not describe quit_without_cd as normal quit, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn entries_and_preview_panels_keep_top_border_segments() {
    let root = temp_path("panel-top-borders");
    fs::create_dir_all(&root).expect("failed to create temp root");
    fs::write(root.join("report.txt"), "hello\nworld\n").expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    let state = draw_ui(&mut terminal, &mut app);
    let entries_panel = state
        .entries_panel
        .expect("entries panel should be rendered");
    let preview_panel = state
        .preview_panel
        .expect("preview panel should be rendered");

    let entries_top = row_text(terminal.backend().buffer(), entries_panel.y);
    let preview_top = row_text(terminal.backend().buffer(), preview_panel.y);

    assert!(
        entries_top.contains("─"),
        "expected entries panel to keep top border segments, got: {entries_top:?}"
    );
    assert!(
        preview_top.contains("─"),
        "expected preview panel to keep top border segments, got: {preview_top:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn preview_header_detail_uses_compact_labels_before_final_clamp() {
    let root = temp_path("preview-header-clamp");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let total_lines = default_code_preview_line_limit() + 40;
    // Short lines so all total_lines fit within 64 KiB (source_line_count is
    // immediately known), but total_lines exceeds the 800-line cap so
    // line truncation fires and the header shows the semantic coverage.
    let contents = (1..=total_lines)
        .map(|index| format!("line {index} {}", "word ".repeat(3)))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join("report.txt"), contents).expect("failed to write temp file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(64, 28)).expect("terminal should init");
    wait_for_background_preview(&mut app);

    let state = draw_ui(&mut terminal, &mut app);
    let preview_panel = state
        .preview_panel
        .expect("preview panel should be rendered");
    let header_row = row_text(terminal.backend().buffer(), preview_panel.y + 1);

    assert!(
        header_row.contains("Text"),
        "expected preview header row to contain the section label, got: {header_row:?}"
    );
    let expected_line_coverage = format!("{} / {total_lines}", default_code_preview_line_limit());
    assert!(
        header_row.contains(&expected_line_coverage),
        "expected preview header row to show semantic line coverage, got: {header_row:?}"
    );
    assert!(
        !header_row.contains(&format!("{}-line cap", default_code_preview_line_limit())),
        "expected preview header row to avoid internal cap wording, got: {header_row:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn directory_preview_header_drops_contents_label_before_item_count() {
    let root = temp_path("directory-preview-header-tight");
    let folder = root.join("folder");
    fs::create_dir_all(&folder).expect("failed to create folder");
    for index in 0..27 {
        fs::write(folder.join(format!("child-{index}.txt")), "x")
            .expect("failed to write child file");
    }

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut narrow = Terminal::new(TestBackend::new(60, 24)).expect("terminal should init");
    wait_for_background_preview(&mut app);

    let state = draw_ui(&mut narrow, &mut app);
    let preview_panel = state
        .preview_panel
        .expect("preview panel should be rendered");
    let narrow_header = row_text(narrow.backend().buffer(), preview_panel.y + 1);
    assert!(
        narrow_header.contains("27 items"),
        "expected tight directory header to keep the item count, got: {narrow_header:?}"
    );
    assert!(
        !narrow_header.contains("Contents"),
        "expected tight directory header to drop the less useful section label, got: {narrow_header:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn visible_directory_rows_show_cached_item_counts() {
    let root = temp_path("directory-counts");
    let photos = root.join("photos");
    fs::create_dir_all(&photos).expect("failed to create folder");
    fs::write(photos.join("one.jpg"), "a").expect("failed to write first file");
    fs::write(photos.join("two.jpg"), "b").expect("failed to write second file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    draw_ui(&mut terminal, &mut app);
    wait_for_directory_counts(&mut app);
    draw_ui(&mut terminal, &mut app);

    let rendered = buffer_text(terminal.backend().buffer());
    assert!(
        rendered.contains("2 items"),
        "expected visible directory rows to show cached item counts, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_keep_metadata_visible_for_wide_names() {
    let root = temp_path("wide-list-metadata");
    let series = root.join("北斗の拳究極版北斗の拳究極版北斗の拳究極版北斗の拳究極版");
    fs::create_dir_all(&series).expect("failed to create series folder");
    for index in 0..10 {
        fs::write(series.join(format!("chapter-{index}.txt")), "x")
            .expect("failed to write child file");
    }

    let epub_path = root.join("北斗の拳究極版北斗の拳究極版北斗の拳究極版北斗の拳究極版13.epub");
    let epub = fs::File::create(&epub_path).expect("failed to create epub");
    epub.set_len(13_000_000).expect("failed to size epub");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");

    draw_ui(&mut terminal, &mut app);
    wait_for_directory_counts(&mut app);
    let state = draw_ui(&mut terminal, &mut app);
    let entries_panel = state
        .entries_panel
        .expect("entries panel should be rendered");

    let rows = (entries_panel.y..entries_panel.y.saturating_add(entries_panel.height))
        .map(|y| rect_row_text(terminal.backend().buffer(), entries_panel, y))
        .collect::<Vec<_>>();
    let rendered = rows.join("\n");
    let folder_row = rows
        .iter()
        .find(|row| row.contains("10 items"))
        .expect("folder row should keep its item count visible");
    let epub_row = rows
        .iter()
        .find(|row| row.contains("13 MB"))
        .expect("epub row should keep its size visible");

    assert!(
        rendered.contains("10 items") && rendered.contains("13 MB"),
        "expected wide-name rows to keep full metadata visible, got: {rendered:?}"
    );
    assert!(
        folder_row.contains("10 items"),
        "expected wide directory rows to keep the item count visible, got: {folder_row:?}"
    );
    assert!(
        epub_row.contains("13 MB"),
        "expected wide epub rows to keep the file size visible, got: {epub_row:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_do_not_push_metadata_into_a_far_right_column() {
    let root = temp_path("wide-list-column-spacing");
    let file_path = root.join("north-star-chronicles-deluxe-edition-volume-13.cbz");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file = fs::File::create(&file_path).expect("failed to create test file");
    file.set_len(13_000_000).expect("failed to size test file");

    let app = App::new_at(root.clone()).expect("app should load temp directory");
    let entry = app
        .navigation
        .entries
        .first()
        .expect("entry should be present");
    let rendered = line_text(&render_compact_list_row(
        &app,
        entry,
        true,
        220,
        theme::palette(),
    ));
    let detail_index = rendered
        .find("13 MB")
        .expect("wide compact row should keep the size metadata visible");
    let detail_column = helpers::display_width(&rendered[..detail_index]);
    let trailing_gap = 220usize.saturating_sub(helpers::display_width(&rendered));

    assert!(
        detail_column > 90,
        "expected wide compact-row metadata to move toward the right edge, got: {rendered:?}"
    );
    assert!(
        trailing_gap <= 1,
        "expected wide compact-row metadata to stay near the right edge, got trailing_gap={trailing_gap} row={rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_hide_metadata_early_on_tight_widths() {
    let root = temp_path("compact-list-priority");
    let file_path = root.join("north-star-chronicles-deluxe-edition.cbz");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file = fs::File::create(&file_path).expect("failed to create test file");
    file.set_len(13_000_000).expect("failed to size test file");

    let app = App::new_at(root.clone()).expect("app should load temp directory");
    let entry = app
        .navigation
        .entries
        .first()
        .expect("entry should be present");
    let rendered = line_text(&render_compact_list_row(
        &app,
        entry,
        true,
        24,
        theme::palette(),
    ));

    assert!(
        rendered.contains("north"),
        "expected the compact row to preserve the file name, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("13 MB"),
        "expected the compact row to hide size metadata first, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("ago"),
        "expected the compact row to hide modified metadata first, got: {rendered:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn symlink_list_rows_expose_target_detail_inline() {
    use std::os::unix::fs::symlink;

    let root = temp_path("symlink-list-detail");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let target_label = PathBuf::from("target.txt");
    let target = root.join(&target_label);
    let linked = root.join("linked.txt");
    fs::write(&target, "hello").expect("failed to write target");
    symlink(&target_label, &linked).expect("failed to create symlink");

    let app = App::new_at(root.clone()).expect("app should load temp directory");
    let entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.name == "linked.txt")
        .expect("linked entry should be visible");

    assert!(entry.is_symlink());
    let expected_detail = format!("-> {}", target_label.display());
    assert_eq!(
        browser_symlink_target_detail(entry).as_deref(),
        Some(expected_detail.as_str())
    );
    let line = render_compact_list_row(&app, entry, true, 80, theme::palette());
    let rendered = line_text(&line);
    assert!(
        rendered.contains("linked.txt -> target.txt"),
        "expected compact row to show the symlink target inline, got: {rendered:?}"
    );
    assert_eq!(
        line.spans
            .iter()
            .find(|span| span.content.as_ref() == " -> target.txt")
            .and_then(|span| span.style.fg),
        Some(theme::palette().muted),
        "expected compact row to dim the symlink target"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn symlink_list_rows_sanitize_link_names_and_targets() {
    use std::os::unix::fs::symlink;

    let root = temp_path("symlink-list-sanitized");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let target_label = PathBuf::from("bad\rtarget.txt");
    let target = root.join(&target_label);
    let link_name = "bad\u{1b}link.txt";
    let linked = root.join(link_name);
    fs::write(&target, "hello").expect("failed to write target");
    symlink(&target_label, &linked).expect("failed to create symlink");

    let app = App::new_at(root.clone()).expect("app should load temp directory");
    let entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.name == link_name)
        .expect("linked entry should be visible");

    assert_eq!(
        browser_symlink_target_detail(entry).as_deref(),
        Some("-> bad^Mtarget.txt")
    );
    let rendered = line_text(&render_compact_list_row(
        &app,
        entry,
        true,
        80,
        theme::palette(),
    ));
    assert!(
        rendered.contains("bad^[link.txt -> bad^Mtarget.txt"),
        "expected compact row to sanitize symlink names and targets, got: {rendered:?}"
    );
    assert!(!rendered.contains('\r'));
    assert!(!rendered.contains('\u{1b}'));

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[cfg(unix)]
#[test]
fn broken_symlink_list_rows_expose_broken_target_detail() {
    use std::os::unix::fs::symlink;

    let root = temp_path("broken-symlink-list-detail");
    fs::create_dir_all(&root).expect("failed to create temp root");
    let missing = PathBuf::from("../real/code/missing.rs");
    symlink(&missing, root.join("broken_link_with_known_ext.rs"))
        .expect("failed to create symlink");

    let app = App::new_at(root.clone()).expect("app should load temp directory");
    let entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.name == "broken_link_with_known_ext.rs")
        .expect("broken symlink entry should be visible");

    assert!(entry.is_broken_symlink());
    let expected_detail = format!("broken -> {}", missing.display());
    assert_eq!(
        browser_symlink_target_detail(entry).as_deref(),
        Some(expected_detail.as_str())
    );
    let line = render_compact_list_row(&app, entry, true, 80, theme::palette());
    let rendered = line_text(&line);
    assert!(
        rendered.contains("broken_link_with_known_ext.rs -> ../real/code/missing.rs"),
        "expected compact row to show the broken symlink target inline, got: {rendered:?}"
    );
    assert_eq!(
        line.spans
            .iter()
            .find(|span| span.content.as_ref() == " -> ../real/code/missing.rs")
            .and_then(|span| span.style.fg),
        Some(theme::palette().muted),
        "expected compact row to dim the broken symlink target"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_hide_file_metadata_at_consistent_widths() {
    let root = temp_path("compact-list-file-thresholds");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let small = root.join("small.bin");
    let small_file = fs::File::create(&small).expect("failed to create small file");
    small_file.set_len(512).expect("failed to size small file");

    let large = root.join("large.cbz");
    let large_file = fs::File::create(&large).expect("failed to create large file");
    large_file
        .set_len(13_000_000)
        .expect("failed to size large file");

    let app = App::new_at(root.clone()).expect("app should load temp directory");
    let small_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == small)
        .expect("small entry should be present");
    let large_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == large)
        .expect("large entry should be present");

    let small_row = line_text(&render_compact_list_row(
        &app,
        small_entry,
        true,
        29,
        theme::palette(),
    ));
    let large_row = line_text(&render_compact_list_row(
        &app,
        large_entry,
        true,
        29,
        theme::palette(),
    ));

    assert!(
        !small_row.contains("512 B") && !large_row.contains("13 MB"),
        "expected file metadata to hide consistently at the same narrow width, got small={small_row:?} large={large_row:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_hide_directory_metadata_at_consistent_widths() {
    let root = temp_path("compact-list-directory-thresholds");
    let short = root.join("short-count");
    let long = root.join("long-count");
    fs::create_dir_all(&short).expect("failed to create short-count dir");
    fs::create_dir_all(&long).expect("failed to create long-count dir");

    for index in 0..10 {
        fs::write(short.join(format!("child-{index}.txt")), "x")
            .expect("failed to write short-count child");
    }
    for index in 0..100 {
        fs::write(long.join(format!("child-{index}.txt")), "x")
            .expect("failed to write long-count child");
    }

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");
    draw_ui(&mut terminal, &mut app);
    wait_for_directory_counts(&mut app);

    let short_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == short)
        .expect("short-count entry should be present");
    let long_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == long)
        .expect("long-count entry should be present");

    let short_row = line_text(&render_compact_list_row(
        &app,
        short_entry,
        true,
        32,
        theme::palette(),
    ));
    let long_row = line_text(&render_compact_list_row(
        &app,
        long_entry,
        true,
        32,
        theme::palette(),
    ));

    assert!(
        !short_row.contains("10 items") && !long_row.contains("100 items"),
        "expected directory counts to hide consistently at the same narrow width, got short={short_row:?} long={long_row:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_align_file_and_directory_metadata_columns() {
    let root = temp_path("compact-list-alignment");
    let folder = root.join("folder");
    let file_path = root.join("movie.mkv");
    fs::create_dir_all(&folder).expect("failed to create folder");
    for index in 0..10 {
        fs::write(folder.join(format!("child-{index}.txt")), "x")
            .expect("failed to write folder child");
    }
    fs::create_dir_all(&root).expect("failed to create temp root");
    let file = fs::File::create(&file_path).expect("failed to create file");
    file.set_len(13_000_000).expect("failed to size file");

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");
    draw_ui(&mut terminal, &mut app);
    wait_for_directory_counts(&mut app);

    let folder_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == folder)
        .expect("folder entry should be present");
    let file_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == file_path)
        .expect("file entry should be present");

    let folder_row = line_text(&render_compact_list_row(
        &app,
        folder_entry,
        true,
        90,
        theme::palette(),
    ));
    let file_row = line_text(&render_compact_list_row(
        &app,
        file_entry,
        true,
        90,
        theme::palette(),
    ));

    let folder_modified_index = folder_row
        .find("ago")
        .expect("folder row should show modified metadata");
    let file_modified_index = file_row
        .find("ago")
        .expect("file row should show modified metadata");
    let folder_modified_column = helpers::display_width(&folder_row[..folder_modified_index]);
    let file_modified_column = helpers::display_width(&file_row[..file_modified_index]);

    assert_eq!(
        folder_modified_column, file_modified_column,
        "expected file and directory modified columns to align, got folder={folder_row:?} file={file_row:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_align_directory_count_nouns_for_singular_and_plural() {
    let root = temp_path("compact-list-directory-count-alignment");
    let single = root.join("single");
    let many = root.join("many");
    fs::create_dir_all(&single).expect("failed to create single dir");
    fs::create_dir_all(&many).expect("failed to create many dir");
    fs::write(single.join("child-0.txt"), "x").expect("failed to write single child");
    for index in 0..10 {
        fs::write(many.join(format!("child-{index}.txt")), "x")
            .expect("failed to write many child");
    }

    let mut app = App::new_at(root.clone()).expect("app should load temp directory");
    let mut terminal = Terminal::new(TestBackend::new(90, 24)).expect("terminal should init");
    draw_ui(&mut terminal, &mut app);
    wait_for_directory_counts(&mut app);

    let single_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == single)
        .expect("single entry should be present");
    let many_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == many)
        .expect("many entry should be present");

    let single_row = line_text(&render_compact_list_row(
        &app,
        single_entry,
        true,
        90,
        theme::palette(),
    ));
    let many_row = line_text(&render_compact_list_row(
        &app,
        many_entry,
        true,
        90,
        theme::palette(),
    ));

    let single_item_index = single_row
        .find("item")
        .expect("single row should show singular item count");
    let many_item_index = many_row
        .find("items")
        .expect("many row should show plural item count");
    let single_item_column = helpers::display_width(&single_row[..single_item_index]);
    let many_item_column = helpers::display_width(&many_row[..many_item_index]);

    assert_eq!(
        single_item_column, many_item_column,
        "expected singular and plural directory counts to align, got single={single_row:?} many={many_row:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}

#[test]
fn compact_list_rows_align_file_size_units_for_small_and_large_sizes() {
    let root = temp_path("compact-list-file-size-alignment");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let small_path = root.join("small.bin");
    let large_path = root.join("large.bin");
    let small_file = fs::File::create(&small_path).expect("failed to create small file");
    small_file.set_len(68).expect("failed to size small file");
    let large_file = fs::File::create(&large_path).expect("failed to create large file");
    large_file
        .set_len(3_720)
        .expect("failed to size large file");

    let app = App::new_at(root.clone()).expect("app should load temp directory");
    let small_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == small_path)
        .expect("small entry should be present");
    let large_entry = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == large_path)
        .expect("large entry should be present");

    let small_row = line_text(&render_compact_list_row(
        &app,
        small_entry,
        true,
        90,
        theme::palette(),
    ));
    let large_row = line_text(&render_compact_list_row(
        &app,
        large_entry,
        true,
        90,
        theme::palette(),
    ));

    let small_unit_index = small_row
        .rfind(" B ")
        .map(|index| index + 1)
        .expect("small row should show byte unit");
    let large_unit_index = large_row
        .rfind(" kB")
        .map(|index| index + 1)
        .expect("large row should show kilobyte unit");
    let small_unit_column = helpers::display_width(&small_row[..small_unit_index]);
    let large_unit_column = helpers::display_width(&large_row[..large_unit_index]);

    assert_eq!(
        small_unit_column, large_unit_column,
        "expected small and large file size units to align, got small={small_row:?} large={large_row:?}"
    );

    fs::remove_dir_all(root).expect("failed to remove temp root");
}
