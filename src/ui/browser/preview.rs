use super::super::theme::Palette;
use super::super::{helpers, theme};
use super::scrollbar::render_preview_scrollbar;
use crate::app::{App, FrameState};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};

pub(super) fn render_preview(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    state.preview_panel = Some(area);

    let title_line = if let Some(entry) = app.selected_entry() {
        Line::from(vec![
            Span::styled(
                format!(" {} ", theme::entry_symbol(entry)),
                Style::default()
                    .fg(theme::entry_color(entry, palette))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                helpers::clamp_label(&entry.name, area.width.saturating_sub(10) as usize),
                Style::default()
                    .fg(palette.accent_text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " Preview ",
                Style::default()
                    .fg(palette.accent_text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ])
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(palette.panel).fg(palette.text))
        .border_style(Style::default().fg(palette.border));
    frame.render_widget(block, area);
    helpers::render_panel_title(frame, area, title_line);
    let inner = helpers::inner_with_padding(area);
    helpers::fill_area(frame, inner, palette.panel, palette.text);

    if app.selected_entry().is_none() {
        helpers::render_empty_state(frame, inner, "Nothing selected", palette);
        return;
    }

    if inner.height > 0 {
        render_preview_body(frame, inner, app, state, palette);
    }
}

fn render_preview_body(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &mut FrameState,
    palette: Palette,
) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    helpers::fill_area(frame, sections[0], palette.panel, palette.text);
    if sections[1].height > 0 {
        helpers::fill_area(frame, sections[1], palette.panel, palette.text);
    }
    let body = if sections[1].width >= 6 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(sections[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0)])
            .split(sections[1])
    };
    let body_area = body[0];
    let scrollbar_area = body.get(1).copied();
    state.preview_body_area = Some(sections[1]);
    let (media_area, text_area) = if let Some(media_rows) = app.preview_visual_rows(body_area) {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(media_rows), Constraint::Min(0)])
            .split(body_area);
        (Some(split[0]), split[1])
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
    let section_label = app.preview_section_label();
    let header_width = sections[0].width as usize;
    let show_section_label = section_label != "Contents";
    let header_detail_width = if show_section_label {
        header_width.saturating_sub(helpers::display_width(section_label) + 2)
    } else {
        header_width
    };
    let header_detail = app
        .preview_header_detail_for_width(visible_rows, header_detail_width)
        .as_deref()
        .map(|detail| helpers::clamp_label(detail, header_detail_width))
        .unwrap_or_default();
    let header_line = if show_section_label {
        Line::from(vec![
            Span::styled(
                section_label.to_string(),
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default().fg(palette.muted)),
            Span::styled(header_detail, Style::default().fg(palette.muted)),
        ])
    } else {
        Line::from(Span::styled(
            header_detail,
            Style::default().fg(palette.muted),
        ))
    };

    frame.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(palette.panel).fg(palette.text)),
        sections[0],
    );

    if app.browser_wheel_burst_active() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Scrolling...",
                Style::default().fg(palette.muted),
            )))
            .style(Style::default().bg(palette.panel).fg(palette.text))
            .alignment(Alignment::Center),
            text_area,
        );
        return;
    }

    if app.preview_prefers_image_surface()
        || app.preview_will_use_static_image_surface_after_layout()
    {
        if let Some(message) = app.preview_overlay_placeholder_message() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    message,
                    Style::default().fg(palette.muted),
                )))
                .style(Style::default().bg(palette.panel).fg(palette.text))
                .alignment(Alignment::Center),
                text_area,
            );
        }
        return;
    }

    if app.preview_uses_image_overlay() {
        return;
    }

    if app.preview_wraps() {
        let wrapped_lines = app.preview_wrapped_lines(text_area.width as usize);
        frame.render_widget(
            PreviewLinesWidget::new(
                wrapped_lines.as_ref(),
                app.preview_scroll_offset(),
                app.preview_horizontal_scroll_offset(),
                Style::default().bg(palette.panel).fg(palette.text),
            ),
            text_area,
        );
    } else {
        let paragraph = Paragraph::new(app.preview_lines())
            .style(Style::default().bg(palette.panel).fg(palette.text))
            .scroll((
                app.preview_scroll_offset().min(u16::MAX as usize) as u16,
                app.preview_horizontal_scroll_offset()
                    .min(u16::MAX as usize) as u16,
            ));
        frame.render_widget(paragraph, text_area);
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

struct PreviewLinesWidget<'a> {
    lines: &'a [Line<'static>],
    scroll: usize,
    h_scroll: usize,
    style: Style,
}

impl<'a> PreviewLinesWidget<'a> {
    fn new(lines: &'a [Line<'static>], scroll: usize, h_scroll: usize, style: Style) -> Self {
        Self {
            lines,
            scroll,
            h_scroll,
            style,
        }
    }
}

impl Widget for PreviewLinesWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }

        buf.set_style(area, self.style);
        for (line, row) in self.lines.iter().skip(self.scroll).zip(area.rows()) {
            let clipped;
            let render_line: &Line = if self.h_scroll > 0 {
                clipped = skip_line_chars(line, self.h_scroll);
                &clipped
            } else {
                line
            };
            let line_width = render_line.width();
            let offset = match render_line.alignment.unwrap_or(Alignment::Left) {
                Alignment::Center => row.width.saturating_sub(line_width as u16) / 2,
                Alignment::Right => row.width.saturating_sub(line_width as u16),
                Alignment::Left => 0,
            };
            if offset >= row.width {
                continue;
            }
            let x = row.x.saturating_add(offset);
            let max_width = row.width.saturating_sub(offset);
            buf.set_line(x, row.y, render_line, max_width);
        }
    }
}

fn skip_line_chars(line: &Line<'static>, skip: usize) -> Line<'static> {
    let mut remaining = skip;
    let mut result = Vec::new();
    for span in &line.spans {
        if remaining == 0 {
            result.push(span.clone());
            continue;
        }
        let char_count = span.content.chars().count();
        if char_count <= remaining {
            remaining -= char_count;
        } else {
            let content: String = span.content.chars().skip(remaining).collect();
            if !content.is_empty() {
                result.push(Span::styled(content, span.style));
            }
            remaining = 0;
        }
    }
    let mut new_line = Line::from(result);
    new_line.alignment = line.alignment;
    new_line
}
