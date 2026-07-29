use super::scrollbar::render_overlay_scrollbar;
use crate::app::{App, DuplicateHit, FrameState};
use crate::ui::{
    helpers,
    theme::{self, Palette},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

const MIN_RESULTS_WIDTH_WITH_PREVIEW: u16 = 46;
const MIN_PREVIEW_WIDTH: u16 = 14;
const MIN_DUPLICATE_NAME_WIDTH: usize = 18;
const MIN_DUPLICATE_PARENT_WIDTH: usize = 10;

pub(super) fn render_duplicate_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    let popup = area;
    state.duplicate_panel = Some(popup);
    state.preview_panel = None;
    state.preview_body_area = None;
    state.preview_media_area = None;
    state.preview_content_area = None;
    state.preview_rows_visible = 0;
    state.preview_cols_visible = 0;
    frame.render_widget(Clear, popup);
    helpers::fill_area(frame, popup, palette.bg, palette.text);

    if popup.height < 4 || popup.width < 20 {
        return;
    }

    let show_preview = app.duplicate_preview_visible()
        && popup.width >= MIN_RESULTS_WIDTH_WITH_PREVIEW + MIN_PREVIEW_WIDTH
        && app.duplicate_file_count() > 0;
    if show_preview {
        let preview_width = duplicate_preview_width(popup.width);
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(preview_width)])
            .split(popup);
        render_duplicate_results_panel(frame, panes[0], app, state, palette);
        render_duplicate_preview(frame, panes[1], app, state, palette);
    } else {
        render_duplicate_results_panel(frame, popup, app, state, palette);
    }
}

fn render_duplicate_results_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(palette.chrome_alt).fg(palette.text))
        .border_style(Style::default().fg(palette.border));
    frame.render_widget(block, area);
    helpers::render_panel_title(
        frame,
        area,
        Line::from(vec![Span::styled(
            " Duplicate Finder ",
            Style::default()
                .fg(palette.accent_text)
                .add_modifier(Modifier::BOLD),
        )]),
    );

    let inner = helpers::inner_with_padding(area);
    if inner.height < 4 || inner.width < 20 {
        return;
    }
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    render_header(frame, sections[0], app, palette);
    render_results(frame, sections[1], app, state, palette);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App, palette: Palette) {
    let cwd = app
        .duplicate_cwd()
        .map(|p| helpers::stable_path_label(p, 32))
        .unwrap_or_default();
    let stats = app.duplicate_stats().unwrap_or_default();
    let status = if let Some(error) = app.duplicate_error() {
        format!("error: {error}")
    } else if app.duplicate_loading() {
        format!(
            "{}…  •  {} files  •  {} candidates  •  {} hashed  •  {} groups  •  {} reclaimable",
            stats.phase.label(),
            stats.scanned_files,
            stats.candidate_files,
            stats.hashed_files,
            app.duplicate_group_count(),
            crate::fs::format_size(stats.duplicate_bytes),
        )
    } else {
        format!(
            "{} files scanned  •  {} groups  •  {} reclaimable",
            stats.scanned_files,
            app.duplicate_group_count(),
            crate::fs::format_size(stats.duplicate_bytes),
        )
    };
    let line1 = Line::from(Span::styled(
        cwd,
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    ));
    let line2 = Line::from(Span::styled(status, Style::default().fg(palette.muted)));
    frame.render_widget(
        Paragraph::new(vec![line1, line2])
            .style(Style::default().bg(palette.chrome_alt).fg(palette.text)),
        area,
    );
}

fn render_results(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    helpers::fill_area(frame, area, palette.chrome_alt, palette.text);
    let visible = area.height as usize;
    state.duplicate_rows_visible = visible.max(1);
    if let Some(error) = app.duplicate_error() {
        helpers::render_empty_state_with_bg(frame, area, error, palette, palette.chrome_alt);
        return;
    }
    if app.duplicate_file_count() == 0 {
        let message = if app.duplicate_loading() {
            app.duplicate_stats()
                .map(|stats| format!("{} for exact duplicates…", stats.phase.label()))
                .unwrap_or_else(|| "Scanning for exact duplicates…".to_string())
        } else {
            "No exact duplicate files found".to_string()
        };
        helpers::render_empty_state_with_bg(frame, area, &message, palette, palette.chrome_alt);
        return;
    }

    let rows = app.duplicate_rows(visible);
    let group_rank_width = (app.duplicate_group_count().max(1).ilog10() as usize + 1).max(3);
    let size_width = rows
        .iter()
        .map(|row| helpers::display_width(&crate::fs::format_size(row.size)))
        .max()
        .unwrap_or(4)
        .max(8);
    let max_name_width = rows
        .iter()
        .map(|row| helpers::display_width(&row.name))
        .max()
        .unwrap_or(MIN_DUPLICATE_NAME_WIDTH)
        .max(MIN_DUPLICATE_NAME_WIDTH);
    let max_parent_width = rows
        .iter()
        .map(|row| helpers::display_width(&row.parent))
        .max()
        .unwrap_or(MIN_DUPLICATE_PARENT_WIDTH)
        .max(MIN_DUPLICATE_PARENT_WIDTH);
    for (offset, row) in rows.iter().enumerate() {
        let rect = Rect {
            x: area.x,
            y: area.y + offset as u16,
            width: area.width,
            height: 1,
        };
        let bg = if row.focused {
            palette.selected_bg
        } else {
            palette.chrome_alt
        };
        frame.render_widget(
            Block::default().style(Style::default().bg(palette.chrome_alt)),
            rect,
        );
        let marker_color = if row.selected {
            palette.selection_bar
        } else if row.focused {
            palette.selected_border
        } else {
            palette.chrome_alt
        };
        let group_label = if row.group_first {
            duplicate_group_label(row.group_rank, group_rank_width)
        } else {
            " ".repeat(group_rank_width + 1)
        };
        let group_gutter_width = (1 + UnicodeWidthStr::width(group_label.as_str()) + 1) as u16;
        let file_rect = Rect {
            x: rect.x + group_gutter_width.min(rect.width),
            width: rect.width.saturating_sub(group_gutter_width),
            ..rect
        };
        let icon = theme::path_symbol_with_symlink(&row.path, false, None);
        let icon_color = theme::path_color_with_symlink(&row.path, false, None, palette);
        let size = crate::fs::format_size(row.size);
        let prefix_width = 1 + UnicodeWidthStr::width(icon) + 1;
        let available = (file_rect.width as usize).saturating_sub(prefix_width);
        let fixed_suffix_width = 2 + size_width + 2;
        let content_width = available.saturating_sub(fixed_suffix_width);
        let (name_width, parent_width) =
            duplicate_text_column_widths(content_width, max_name_width, max_parent_width);
        let name = pad_to_width(helpers::clamp_label(&row.name, name_width), name_width);
        let parent = helpers::truncate_middle(&row.parent, parent_width);
        let size_padding = " ".repeat(size_width.saturating_sub(helpers::display_width(&size)));
        let marker = if row.selected || row.focused {
            "▌"
        } else {
            " "
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(group_label, Style::default().fg(palette.muted)),
                Span::raw(" "),
            ]))
            .style(Style::default().bg(palette.chrome_alt).fg(palette.text)),
            Rect {
                width: group_gutter_width.min(rect.width),
                ..rect
            },
        );
        frame.render_widget(Block::default().style(Style::default().bg(bg)), file_rect);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(marker_color)),
                Span::styled(
                    icon,
                    Style::default().fg(icon_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    name,
                    Style::default()
                        .fg(palette.text)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(size_padding, Style::default().fg(palette.muted)),
                Span::styled(size, Style::default().fg(palette.muted)),
                Span::raw("  "),
                Span::styled(parent, Style::default().fg(palette.muted)),
            ]))
            .style(Style::default().bg(bg).fg(palette.text)),
            file_rect,
        );
        state.duplicate_hits.push(DuplicateHit {
            rect,
            index: row.index,
        });
    }
    if app.duplicate_file_count() > visible && area.width > 2 {
        let scrollbar = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y,
            width: 1,
            height: area.height,
        };
        render_overlay_scrollbar(
            frame,
            scrollbar,
            app.duplicate_file_count(),
            visible,
            app.duplicate_scroll_top(),
            palette,
        );
    }
}

fn duplicate_group_label(rank: usize, rank_width: usize) -> String {
    format!("G{rank:0rank_width$}")
}

fn duplicate_preview_width(total_width: u16) -> u16 {
    let available = total_width.saturating_sub(MIN_RESULTS_WIDTH_WITH_PREVIEW);
    let threshold = MIN_RESULTS_WIDTH_WITH_PREVIEW + MIN_PREVIEW_WIDTH;
    let target = MIN_PREVIEW_WIDTH + total_width.saturating_sub(threshold) / 3;
    target.min(available)
}

fn duplicate_text_column_widths(
    content_width: usize,
    max_name_width: usize,
    max_parent_width: usize,
) -> (usize, usize) {
    if content_width == 0 {
        return (0, 0);
    }
    if max_name_width.saturating_add(max_parent_width) <= content_width {
        return (max_name_width, max_parent_width);
    }

    let min_name_width = MIN_DUPLICATE_NAME_WIDTH.min(content_width);
    let min_parent_width =
        MIN_DUPLICATE_PARENT_WIDTH.min(content_width.saturating_sub(min_name_width));
    let name_width = max_name_width.min(
        content_width
            .saturating_sub(min_parent_width)
            .max(min_name_width),
    );
    let parent_width = content_width
        .saturating_sub(name_width)
        .min(max_parent_width);
    (name_width, parent_width)
}

fn pad_to_width(mut text: String, width: usize) -> String {
    let padding = width.saturating_sub(helpers::display_width(&text));
    if padding > 0 {
        text.push_str(&" ".repeat(padding));
    }
    text
}

fn render_duplicate_preview(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    state.preview_panel = Some(area);
    let title = app
        .duplicate_focused_entry()
        .map(|entry| {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", theme::entry_symbol(&entry)),
                    Style::default()
                        .fg(theme::entry_color(&entry, palette))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    helpers::clamp_label(&entry.name, area.width.saturating_sub(10) as usize),
                    Style::default()
                        .fg(palette.accent_text)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .unwrap_or_else(|| Line::from(" Preview "));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(palette.panel).fg(palette.text))
        .border_style(Style::default().fg(palette.border));
    frame.render_widget(block, area);
    helpers::render_panel_title(frame, area, title);
    let inner = helpers::inner_with_padding(area);
    helpers::fill_area(frame, inner, palette.panel, palette.text);
    if inner.height < 2 {
        return;
    }
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    state.preview_body_area = Some(split[1]);
    let body = if split[1].width >= 6 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(split[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0)])
            .split(split[1])
    };
    let body_area = body[0];
    let scrollbar_area = body.get(1).copied();
    let (media_area, text_area) = if let Some(media_rows) = app.preview_visual_rows(body_area) {
        let body = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(media_rows), Constraint::Min(0)])
            .split(body_area);
        (Some(body[0]), body[1])
    } else {
        (None, body_area)
    };
    state.preview_media_area = media_area;
    state.preview_content_area = Some(text_area);
    if let Some(media_area) = media_area {
        helpers::fill_area(frame, media_area, palette.panel, palette.text);
    }
    helpers::fill_area(frame, text_area, palette.panel, palette.text);
    if let Some(scrollbar_area) = scrollbar_area {
        helpers::fill_area(frame, scrollbar_area, palette.panel, palette.border);
    }
    let visible_rows = text_area.height as usize;
    state.preview_rows_visible = visible_rows;
    state.preview_cols_visible = text_area.width as usize;
    let detail = app
        .preview_header_detail_for_width(visible_rows, split[0].width as usize)
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(detail).style(Style::default().bg(palette.panel).fg(palette.muted)),
        split[0],
    );
    if app.preview_prefers_image_surface()
        || app.preview_will_use_static_image_surface_after_layout()
    {
        if let Some(message) = app.preview_overlay_placeholder_message() {
            frame.render_widget(
                Paragraph::new(message).style(Style::default().bg(palette.panel).fg(palette.muted)),
                text_area,
            );
        }
        return;
    }
    if app.preview_uses_image_overlay() {
        return;
    }
    if app.preview_wraps() {
        let lines = app.preview_wrapped_lines(text_area.width as usize);
        frame.render_widget(
            Paragraph::new(lines.as_ref().to_vec())
                .style(Style::default().bg(palette.panel).fg(palette.text))
                .scroll((app.preview_scroll_offset().min(u16::MAX as usize) as u16, 0)),
            text_area,
        );
    } else {
        frame.render_widget(
            Paragraph::new(app.preview_lines())
                .style(Style::default().bg(palette.panel).fg(palette.text))
                .scroll((
                    app.preview_scroll_offset().min(u16::MAX as usize) as u16,
                    app.preview_horizontal_scroll_offset()
                        .min(u16::MAX as usize) as u16,
                )),
            text_area,
        );
    }
    if let Some(scrollbar_area) = scrollbar_area
        && text_area.height > 0
    {
        render_preview_scrollbar(
            frame,
            scrollbar_area,
            app,
            visible_rows,
            text_area.width as usize,
            palette,
        );
    }
}

fn render_preview_scrollbar(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    visible_rows: usize,
    visible_cols: usize,
    palette: Palette,
) {
    let total = app.preview_total_lines(visible_cols);
    if area.height == 0 || total <= visible_rows.max(1) {
        frame.render_widget(
            Paragraph::new(" ").style(Style::default().bg(palette.panel).fg(palette.border)),
            area,
        );
        return;
    }
    let track = vec![
        Line::from(Span::styled("│", Style::default().fg(palette.border)));
        area.height as usize
    ];
    frame.render_widget(
        Paragraph::new(track).style(Style::default().bg(palette.panel)),
        area,
    );
    let thumb_height = ((visible_rows.max(1) * area.height as usize) / total)
        .max(1)
        .min(area.height as usize);
    let max_scroll = total.saturating_sub(visible_rows.max(1));
    let thumb_max_top = area.height as usize - thumb_height;
    let thumb_top = app
        .preview_scroll_offset()
        .checked_mul(thumb_max_top)
        .and_then(|offset| offset.checked_div(max_scroll))
        .unwrap_or(0);
    let thumb = Rect {
        x: area.x,
        y: area.y + thumb_top as u16,
        width: area.width,
        height: thumb_height as u16,
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "┃",
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            thumb.height as usize
        ])
        .style(Style::default().bg(palette.panel)),
        thumb,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        MIN_PREVIEW_WIDTH, MIN_RESULTS_WIDTH_WITH_PREVIEW, duplicate_group_label,
        duplicate_preview_width,
    };

    #[test]
    fn duplicate_group_labels_zero_pad_to_rank_width() {
        assert_eq!(duplicate_group_label(9, 1), "G9");
        assert_eq!(duplicate_group_label(9, 2), "G09");
        assert_eq!(duplicate_group_label(10, 2), "G10");
        assert_eq!(duplicate_group_label(100, 3), "G100");
    }

    #[test]
    fn duplicate_preview_width_scales_with_available_space() {
        let compact = MIN_RESULTS_WIDTH_WITH_PREVIEW + MIN_PREVIEW_WIDTH;
        assert_eq!(duplicate_preview_width(compact), MIN_PREVIEW_WIDTH);
        assert_eq!(duplicate_preview_width(80), 20);
        assert_eq!(duplicate_preview_width(120), 34);
        assert_eq!(duplicate_preview_width(200), 60);
    }

    #[test]
    fn duplicate_preview_width_is_monotonic_while_shrinking() {
        let first = MIN_RESULTS_WIDTH_WITH_PREVIEW + MIN_PREVIEW_WIDTH;
        let mut previous = duplicate_preview_width(first);
        for width in first + 1..220 {
            let current = duplicate_preview_width(width);
            assert!(
                current >= previous,
                "preview width regressed at total width {width}: {current} < {previous}"
            );
            previous = current;
        }
    }
}
