use crate::app::{App, FrameState};
use crate::ui::theme::Palette;
use ratatui::{Frame, layout::Rect};

mod archive_create;
mod archive_password;
mod bulk_rename;
mod copy;
mod create;
mod duplicates;
mod editor_rename_confirm;
mod goto;
mod help;
mod open_with;
mod rename;
mod restore;
mod scrollbar;
mod search;
mod trash;

const MAX_EDIT_OVERLAY_VISIBLE_ROWS: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HelpMode {
    Normal,
    Chooser,
}

pub(super) fn render_trash_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    trash::render_trash_overlay(frame, area, app, state, palette);
}

pub(super) fn render_restore_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    restore::render_restore_overlay(frame, area, app, state, palette);
}

pub(super) fn render_create_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    create::render_create_overlay(frame, area, app, state, palette);
}

pub(super) fn render_archive_create_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    archive_create::render_archive_create_overlay(frame, area, app, state, palette);
}

pub(super) fn render_archive_password_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    archive_password::render_archive_password_overlay(frame, area, app, state, palette);
}

pub(super) fn render_rename_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    rename::render_rename_overlay(frame, area, app, state, palette);
}

pub(super) fn render_bulk_rename_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    bulk_rename::render_bulk_rename_overlay(frame, area, app, state, palette);
}

pub(super) fn render_editor_rename_confirm_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    editor_rename_confirm::render_editor_rename_confirm_overlay(frame, area, app, state, palette);
}

pub(super) fn render_copy_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    copy::render_copy_overlay(frame, area, app, state, palette);
}

pub(super) fn render_open_with_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    open_with::render_open_with_overlay(frame, area, app, state, palette);
}

pub(super) fn render_goto_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    goto::render_goto_overlay(frame, area, app, state, palette);
}

pub(super) fn render_search_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    search::render_search_overlay(frame, area, app, state, palette);
}

pub(super) fn render_duplicate_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    duplicates::render_duplicate_overlay(frame, area, app, state, palette);
}

pub(super) fn render_help(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    let mode = if app.chooser_mode {
        HelpMode::Chooser
    } else {
        HelpMode::Normal
    };
    help::render_help(frame, area, mode, app.overlays.help_scroll, state, palette);
}

fn compute_scroll_top(cursor_line: usize, visible: usize) -> usize {
    if cursor_line < visible {
        0
    } else {
        cursor_line - visible + 1
    }
}

fn edit_overlay_visible_rows(area: Rect, row_count: usize, popup_chrome_height: u16) -> u16 {
    let available_rows = area
        .height
        .saturating_sub(popup_chrome_height.saturating_add(2))
        .max(1) as usize;
    row_count
        .clamp(1, MAX_EDIT_OVERLAY_VISIBLE_ROWS)
        .min(available_rows) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_overlay_visible_rows_caps_and_shrinks_to_terminal_height() {
        let area = Rect::new(0, 0, 90, 24);
        assert_eq!(edit_overlay_visible_rows(area, 40, 5), 12);

        let short_area = Rect::new(0, 0, 90, 10);
        assert_eq!(edit_overlay_visible_rows(short_area, 40, 5), 3);
        assert_eq!(edit_overlay_visible_rows(short_area, 40, 7), 1);
        assert_eq!(edit_overlay_visible_rows(short_area, 0, 5), 1);
    }
}
