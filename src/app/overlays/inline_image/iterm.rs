use anyhow::{Context, Result};
use base64::Engine as _;
use ratatui::{layout::Rect, style::Color};
use std::{fs, io::Write as _, path::Path, sync::Arc};

use crate::app::FrameState;

use super::{
    geometry::intersect_rect,
    tmux::{self, TmuxPaneOrigin},
};

pub(in crate::app) fn encode_iterm_inline_payload(path: &Path) -> Option<Arc<str>> {
    let data = fs::read(path).ok()?;
    Some(Arc::<str>::from(
        base64::engine::general_purpose::STANDARD.encode(&data),
    ))
}

pub(super) fn place_terminal_image_with_iterm_protocol(
    path: &Path,
    area: Rect,
    inline_payload: Option<&str>,
) -> Result<Vec<u8>> {
    let encoded = match inline_payload {
        Some(payload) => payload.to_string(),
        None => encode_iterm_inline_payload(path)
            .map(|payload| payload.to_string())
            .context("failed to encode iTerm inline image payload")?,
    };
    if tmux::inside_tmux() {
        let origin = tmux::query_pane_origin()
            .ok_or_else(|| anyhow::anyhow!("tmux pane origin unavailable"))?;
        return Ok(build_iterm_tmux_placement_sequence(&encoded, area, origin));
    }
    Ok(build_iterm_placement_sequence(&encoded, area))
}

fn build_iterm_placement_sequence(encoded: &str, area: Rect) -> Vec<u8> {
    build_iterm_placement_sequence_at(
        encoded,
        area.y.saturating_add(1).into(),
        area.x.saturating_add(1).into(),
        area,
    )
}

fn build_iterm_tmux_placement_sequence(
    encoded: &str,
    area: Rect,
    origin: TmuxPaneOrigin,
) -> Vec<u8> {
    let (row, col) = origin.absolute_cursor_for(area);
    tmux::wrap_sequence_for_tmux(&build_iterm_placement_sequence_at(encoded, row, col, area))
}

fn build_iterm_placement_sequence_at(encoded: &str, row: u32, col: u32, area: Rect) -> Vec<u8> {
    // Move cursor to the top-left cell of the placement area, then emit the
    // OSC 1337 sequence. `width` and `height` are in terminal cells.
    format!(
        "\x1b[{};{}H\x1b]1337;File=inline=1;width={};height={};preserveAspectRatio=1:{}\x07",
        row,
        col,
        area.width.max(1),
        area.height.max(1),
        encoded
    )
    .into_bytes()
}

/// Overwrite every cell in `area` with a space colored with the panel background
/// so ghost pixels are erased without leaving black traces.
///
/// Using the exact panel color means ratatui's differential renderer can safely
/// skip those cells on the next draw — they already show the right color.
pub(super) fn erase_cells(area: Rect) -> Vec<u8> {
    let mut out = Vec::new();
    let blank_row = " ".repeat(usize::from(area.width));
    // Set background to the panel color so empty cells match the pane background.
    // Fall back to default-background reset if the theme returns a non-RGB value.
    match crate::ui::theme::palette().panel {
        Color::Rgb(r, g, b) => {
            let _ = write!(out, "\x1b[0;48;2;{r};{g};{b}m");
        }
        _ => {
            let _ = write!(out, "\x1b[0m");
        }
    }
    for row in 0..area.height {
        let _ = write!(
            out,
            "\x1b[{};{}H{}",
            area.y.saturating_add(1).saturating_add(row),
            area.x.saturating_add(1),
            blank_row
        );
    }
    let _ = write!(out, "\x1b[0m");
    out
}

pub(super) fn expand_raster_erase_area(
    frame_state: &FrameState,
    area: Rect,
    expand_right: u16,
    expand_bottom: u16,
) -> Rect {
    let safe_bounds = frame_state
        .preview_body_area
        .or(frame_state.preview_content_area)
        .unwrap_or(area);
    // Only erase inside the preview body/content, never into the pane border.
    // ratatui may skip unchanged border cells on the following draw, so erasing
    // the bottom border here can leave it blank in raster terminals like WezTerm.
    let bounds = safe_bounds;
    let clamped = intersect_rect(area, safe_bounds).unwrap_or(area);
    let right = clamped.x.saturating_add(clamped.width);
    let bottom = clamped.y.saturating_add(clamped.height);
    let bounds_right = bounds.x.saturating_add(bounds.width);
    let bounds_bottom = bounds.y.saturating_add(bounds.height);
    let extra_cols = bounds_right.saturating_sub(right).min(expand_right);
    let extra_rows = bounds_bottom.saturating_sub(bottom).min(expand_bottom);
    Rect {
        x: clamped.x,
        y: clamped.y,
        width: clamped.width.saturating_add(extra_cols),
        height: clamped.height.saturating_add(extra_rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_iterm_tmux_placement_wraps_absolute_cursor_and_inline_payload() {
        let output = String::from_utf8(build_iterm_tmux_placement_sequence(
            "YWJj",
            Rect {
                x: 10,
                y: 4,
                width: 8,
                height: 6,
            },
            TmuxPaneOrigin { top: 2, left: 3 },
        ))
        .expect("tmux iTerm placement should be utf8");

        assert!(output.starts_with("\x1bPtmux;\x1b\x1b[7;14H\x1b\x1b]1337;File=inline=1;"));
        assert!(output.contains("width=8"));
        assert!(output.contains("height=6"));
        assert!(output.contains("preserveAspectRatio=1:YWJj\x07"));
        assert!(output.ends_with("\x1b\\"));
        assert!(!output.contains("\x1b[5;11H"));
    }

    #[test]
    fn expand_raster_erase_area_can_grow_right_and_bottom_within_preview_bounds() {
        let frame_state = FrameState {
            preview_panel: Some(Rect {
                x: 10,
                y: 5,
                width: 40,
                height: 20,
            }),
            preview_body_area: Some(Rect {
                x: 12,
                y: 7,
                width: 30,
                height: 10,
            }),
            ..FrameState::default()
        };
        let area = Rect {
            x: 12,
            y: 7,
            width: 20,
            height: 8,
        };

        assert_eq!(
            expand_raster_erase_area(&frame_state, area, 1, 1),
            Rect {
                x: 12,
                y: 7,
                width: 21,
                height: 9,
            }
        );
    }

    #[test]
    fn expand_raster_erase_area_does_not_grow_into_preview_border() {
        let frame_state = FrameState {
            preview_panel: Some(Rect {
                x: 10,
                y: 5,
                width: 40,
                height: 20,
            }),
            preview_body_area: Some(Rect {
                x: 12,
                y: 7,
                width: 30,
                height: 10,
            }),
            ..FrameState::default()
        };
        let area = Rect {
            x: 12,
            y: 14,
            width: 20,
            height: 3,
        };

        assert_eq!(
            expand_raster_erase_area(&frame_state, area, 1, 2),
            Rect {
                x: 12,
                y: 14,
                width: 21,
                height: 3,
            }
        );
    }
}
