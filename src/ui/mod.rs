mod browser;
mod chrome;
mod helpers;
mod overlay_manager;
pub(crate) mod theme;

use crate::{
    app::{App, FrameState},
    config,
};
use ratatui::{Frame, widgets::Block};

pub fn render(frame: &mut Frame<'_>, app: &App, state: &mut FrameState) {
    let palette = theme::palette();
    let ui_config = config::ui();

    state.sidebar_hits.clear();
    state.entry_hits.clear();
    state.search_hits.clear();
    state.goto_hits.clear();
    state.copy_hits.clear();
    state.open_with_hits.clear();
    state.trash_panel = None;
    state.trash_confirm_btn = None;
    state.trash_cancel_btn = None;
    state.restore_panel = None;
    state.restore_confirm_btn = None;
    state.restore_cancel_btn = None;
    state.archive_create_panel = None;
    state.archive_create_list_area = None;
    state.archive_password_panel = None;
    state.archive_password_visibility_btn = None;
    state.create_panel = None;
    state.rename_panel = None;
    state.create_list_area = None;
    state.create_scroll_top = 0;
    state.bulk_rename_list_area = None;
    state.bulk_rename_scroll_top = 0;
    state.goto_panel = None;
    state.copy_panel = None;
    state.open_with_panel = None;
    state.search_panel = None;
    state.help_panel = None;
    state.preview_panel = None;
    state.preview_body_area = None;
    state.preview_media_area = None;
    state.preview_content_area = None;
    state.back_button = None;
    state.forward_button = None;
    state.parent_button = None;
    state.hidden_button = None;
    state.view_button = None;
    state.preview_rows_visible = 0;
    state.preview_cols_visible = 0;

    let area = frame.area();
    frame.render_widget(
        Block::default().style(
            ratatui::style::Style::default()
                .bg(palette.bg)
                .fg(palette.text),
        ),
        area,
    );

    if ui_config.show_top_bar {
        let rows = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Length(3),
                ratatui::layout::Constraint::Min(10),
                ratatui::layout::Constraint::Length(1),
            ])
            .split(area);

        chrome::render_toolbar(frame, rows[0], app, state, palette);
        browser::render_body(frame, rows[1], app, state, palette);
        chrome::render_status(frame, rows[2], app, palette);
    } else {
        let rows = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Min(10),
                ratatui::layout::Constraint::Length(1),
            ])
            .split(area);

        browser::render_body(frame, rows[0], app, state, palette);
        chrome::render_status(frame, rows[1], app, palette);
    }

    if app.duplicates_is_open() {
        let duplicate_area = ratatui::layout::Rect {
            height: area.height.saturating_sub(1),
            ..area
        };
        overlay_manager::render_duplicate_overlay(frame, duplicate_area, app, state, palette);
        chrome::render_status(
            frame,
            ratatui::layout::Rect {
                y: area.y + area.height.saturating_sub(1),
                height: area.height.min(1),
                ..area
            },
            app,
            palette,
        );
    }

    if app.trash_is_open() {
        overlay_manager::render_trash_overlay(frame, area, app, state, palette);
    } else if app.restore_is_open() {
        overlay_manager::render_restore_overlay(frame, area, app, state, palette);
    } else if app.archive_password_is_open() {
        overlay_manager::render_archive_password_overlay(frame, area, app, state, palette);
    } else if app.archive_create_is_open() {
        overlay_manager::render_archive_create_overlay(frame, area, app, state, palette);
    } else if app.create_is_open() {
        overlay_manager::render_create_overlay(frame, area, app, state, palette);
    } else if app.rename_is_open() {
        overlay_manager::render_rename_overlay(frame, area, app, state, palette);
    } else if app.bulk_rename_is_open() {
        overlay_manager::render_bulk_rename_overlay(frame, area, app, state, palette);
    } else if app.editor_rename_confirm_is_open() {
        overlay_manager::render_editor_rename_confirm_overlay(frame, area, app, state, palette);
    } else if app.goto_is_open() {
        overlay_manager::render_goto_overlay(frame, area, app, state, palette);
    } else if app.copy_is_open() {
        overlay_manager::render_copy_overlay(frame, area, app, state, palette);
    } else if app.open_with_is_open() {
        overlay_manager::render_open_with_overlay(frame, area, app, state, palette);
    } else if app.search_is_open() {
        overlay_manager::render_search_overlay(frame, area, app, state, palette);
    } else if app.overlays.help {
        overlay_manager::render_help(frame, area, app, state, palette);
    }
}
