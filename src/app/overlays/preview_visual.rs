use super::super::*;
use super::images;
use super::inline_image::ImageProtocol;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

const PREVIEW_INLINE_COVER_MIN_HEIGHT: u16 = 6;
const PREVIEW_INLINE_COVER_MAX_HEIGHT: u16 = 12;
const PREVIEW_LARGE_INLINE_COVER_MAX_HEIGHT: u16 = 18;
const PREVIEW_INLINE_VIDEO_COVER_MAX_HEIGHT: u16 = 18;
const PREVIEW_INLINE_MIN_TEXT_HEIGHT: u16 = 6;
const PREVIEW_INLINE_PAGE_MIN_HEIGHT: u16 = 8;
const PREVIEW_INLINE_PAGE_MIN_TEXT_HEIGHT: u16 = 6;

impl App {
    pub(crate) fn preview_visual_rows(&self, area: Rect) -> Option<u16> {
        if !self.terminal_image_overlay_available()
            || self.preview.state.content.preview_visual.is_none()
        {
            return None;
        }
        let layout = self.current_preview_visual_layout()?;
        match layout {
            preview::PreviewVisualLayout::FullHeight => {
                let rows = (area.width >= 12 && area.height > 0).then_some(area.height)?;
                return (!self.preview_visual_failed_for_rows(area, rows)).then_some(rows);
            }
            preview::PreviewVisualLayout::Inline | preview::PreviewVisualLayout::LargeInline => {}
        }
        if self.current_preview_visual_kind() == Some(preview::PreviewVisualKind::PageImage) {
            if area.width < 12
                || area.height
                    < PREVIEW_INLINE_PAGE_MIN_HEIGHT + PREVIEW_INLINE_PAGE_MIN_TEXT_HEIGHT
            {
                return None;
            }
            let rows = area
                .height
                .saturating_sub(PREVIEW_INLINE_PAGE_MIN_TEXT_HEIGHT);
            return (!self.preview_visual_failed_for_rows(area, rows)).then_some(rows);
        }
        if area.width < 12
            || area.height < PREVIEW_INLINE_COVER_MIN_HEIGHT + PREVIEW_INLINE_MIN_TEXT_HEIGHT
        {
            return None;
        }

        let (height_divisor, max_height) = match layout {
            preview::PreviewVisualLayout::LargeInline => (2, PREVIEW_LARGE_INLINE_COVER_MAX_HEIGHT),
            preview::PreviewVisualLayout::Inline
                if self.preview.state.content.kind == preview::PreviewKind::Video =>
            {
                (2, PREVIEW_INLINE_VIDEO_COVER_MAX_HEIGHT)
            }
            preview::PreviewVisualLayout::Inline => (3, PREVIEW_INLINE_COVER_MAX_HEIGHT),
            preview::PreviewVisualLayout::FullHeight => unreachable!(),
        };
        let rows = (area.height / height_divisor)
            .clamp(PREVIEW_INLINE_COVER_MIN_HEIGHT, max_height)
            .min(area.height.saturating_sub(PREVIEW_INLINE_MIN_TEXT_HEIGHT));
        (!self.preview_visual_failed_for_rows(area, rows)).then_some(rows)
    }

    pub(in crate::app) fn active_preview_visual_overlay_request(
        &self,
    ) -> Option<images::StaticImageOverlayRequest> {
        if self.preview_uses_image_overlay() {
            return None;
        }

        self.active_preview_visual_overlay_request_unchecked()
    }

    pub(in crate::app) fn active_preview_visual_overlay_request_unchecked(
        &self,
    ) -> Option<images::StaticImageOverlayRequest> {
        if !self.preview_surface_visible_for_images() || !self.terminal_image_overlay_available() {
            return None;
        }
        if self.duplicates_is_open() && !self.duplicate_preview_rendered() {
            return None;
        }

        let area = self.current_preview_visual_area()?;
        if area.width == 0 || area.height == 0 {
            return None;
        }

        self.preview_visual_overlay_request_for_area(area)
    }

    fn current_preview_visual_kind(&self) -> Option<preview::PreviewVisualKind> {
        self.preview
            .state
            .content
            .preview_visual
            .as_ref()
            .map(|visual| visual.kind)
    }

    fn current_preview_visual_layout(&self) -> Option<preview::PreviewVisualLayout> {
        self.preview
            .state
            .content
            .preview_visual
            .as_ref()
            .map(|visual| visual.layout)
    }

    fn preview_visual_failed_for_rows(&self, area: Rect, rows: u16) -> bool {
        let request = match self.preview_visual_overlay_request_for_area(Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: rows,
        }) {
            Some(request) => request,
            None => return false,
        };
        self.preview
            .image
            .failed_images
            .contains(&images::StaticImageKey::from_request(&request))
    }

    fn preview_visual_overlay_request_for_area(
        &self,
        area: Rect,
    ) -> Option<images::StaticImageOverlayRequest> {
        let visual = self.preview.state.content.preview_visual.as_ref()?;
        Some(self.preview_visual_overlay_request_for_visual(
            self.preview.state.content.kind,
            visual,
            area,
        ))
    }

    pub(in crate::app) fn preview_visual_overlay_request_for_visual(
        &self,
        _preview_kind: preview::PreviewKind,
        visual: &preview::PreviewVisual,
        area: Rect,
    ) -> images::StaticImageOverlayRequest {
        images::StaticImageOverlayRequest {
            path: visual.path.clone(),
            size: visual.size,
            modified: visual.modified,
            area,
            target_width_px: images::image_target_width_px(area, self.cached_terminal_window()),
            target_height_px: images::image_target_height_px(area, self.cached_terminal_window()),
            mode: images::StaticImageOverlayMode::Inline,
            force_render_to_cache: self.preview_visual_force_render_to_cache(visual),
            prepare_inline_payload: self.preview.terminal_images.protocol
                == ImageProtocol::ItermInline,
        }
    }

    fn current_preview_visual_area(&self) -> Option<Rect> {
        if let Some(body_area) = self.current_preview_body_render_area() {
            let media_rows = self.preview_visual_rows(body_area)?;
            return Some(
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(media_rows), Constraint::Min(0)])
                    .split(body_area)[0],
            );
        }
        self.input.frame_state.preview_media_area
    }

    fn current_preview_body_render_area(&self) -> Option<Rect> {
        let body_area = self.input.frame_state.preview_body_area?;
        Some(if body_area.width >= 6 {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Length(1)])
                .split(body_area)[0]
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0)])
                .split(body_area)[0]
        })
    }
}

#[cfg(test)]
mod tests;
