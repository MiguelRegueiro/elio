use super::format::{
    StaticImageFormat, static_image_format_for_overlay_request, static_image_format_for_path,
};
use super::prepare::{
    static_image_can_prepare_inline, static_image_supports_iterm_source_passthrough,
};
use super::{
    StaticImageKey, StaticImageOverlayMode, StaticImageOverlayRequest, image_target_height_px,
    image_target_width_px, static_image_detail_label,
};
use crate::app::overlays::inline_image::{ImageProtocol, command_exists};
use crate::app::state::PreviewLoadState;
use crate::app::{App, Entry};
use crate::preview;
use ratatui::layout::Rect;
use std::time::{Duration, Instant};

impl App {
    fn current_page_preview_visual_active(&self) -> bool {
        self.preview
            .state
            .content
            .preview_visual
            .as_ref()
            .is_some_and(|visual| visual.kind == preview::PreviewVisualKind::PageImage)
    }

    pub(crate) fn process_image_preview_timers(&mut self) -> bool {
        let Some(ready_at) = self.preview.image.activation_ready_at else {
            return false;
        };
        if Instant::now() < ready_at {
            return false;
        }
        self.preview.image.activation_ready_at = None;
        self.active_static_image_overlay_request().is_some()
            || self.active_preview_visual_overlay_request().is_some()
    }

    pub(crate) fn pending_image_preview_timer(&self) -> Option<Duration> {
        self.preview
            .image
            .activation_ready_at
            .map(|ready_at| ready_at.saturating_duration_since(Instant::now()))
    }

    pub(in crate::app) fn preview_prefers_static_image_surface(&self) -> bool {
        let Some(request) = self.active_static_image_overlay_request() else {
            return false;
        };
        !self
            .preview
            .image
            .failed_images
            .contains(&StaticImageKey::from_request(&request))
    }

    pub(in crate::app) fn preview_surface_visible_for_images(&self) -> bool {
        self.preview.visible || self.duplicate_preview_rendered()
    }

    pub(crate) fn preview_will_use_static_image_surface_after_layout(&self) -> bool {
        self.preview_surface_visible_for_images()
            && self.terminal_image_overlay_available()
            && self
                .active_preview_entry()
                .is_some_and(|entry| static_image_detail_label(&entry).is_some())
    }

    pub(in crate::app) fn static_image_preview_header_detail(&self) -> Option<String> {
        let request = self.active_static_image_overlay_request()?;
        let dimensions = self
            .preview
            .image
            .dimensions
            .get(&StaticImageKey::from_request(&request))
            .copied()?;
        Some(format!("{}x{}", dimensions.width_px, dimensions.height_px))
    }

    pub(in crate::app) fn should_defer_static_image_preview(&self, entry: &Entry) -> bool {
        static_image_detail_label(entry).is_some() && self.preview_prefers_static_image_surface()
    }

    pub(in crate::app) fn sixel_static_image_preview_for_entry(&self, entry: &Entry) -> bool {
        self.preview.terminal_images.protocol == ImageProtocol::Sixel
            && static_image_detail_label(entry).is_some()
    }

    pub(in crate::app) fn static_image_preview_detail(
        &self,
        entry: &Entry,
    ) -> Option<&'static str> {
        static_image_detail_label(entry)
    }

    pub(in crate::app) fn static_image_overlay_placeholder_message(&self) -> Option<String> {
        if !self.preview_prefers_static_image_surface() || self.preview_uses_image_overlay() {
            return None;
        }

        let request = self.active_static_image_overlay_request()?;
        let key = StaticImageKey::from_request(&request);
        if self.preview.image.failed_images.contains(&key) {
            return Some("Image preview unavailable".to_string());
        }
        if !self.image_selection_activation_ready() {
            return None;
        }
        if self.static_image_can_display_directly_now(&request) {
            return None;
        }
        None
    }

    pub(in crate::app) fn active_static_image_overlay_request(
        &self,
    ) -> Option<StaticImageOverlayRequest> {
        let entry = self.active_preview_entry()?;
        self.static_image_overlay_request_for_entry(&entry)
    }

    pub(in crate::app) fn clear_failed_static_image_state_if_needed(&mut self) {
        if let Some(entry) = self.active_preview_entry()
            && static_image_detail_label(&entry).is_none()
        {
            self.preview.image.failed_images.clear();
        }
    }

    pub(in crate::app) fn sync_image_preview_selection_activation(&mut self) {
        self.preview.image.activation_ready_at = self
            .active_static_image_overlay_request()
            .or_else(|| self.active_preview_visual_overlay_request())
            .and_then(|_| {
                let ready_at = self.input.last_selection_change_at
                    + self.preview.image.selection_activation_delay;
                (Instant::now() < ready_at).then_some(ready_at)
            });
    }

    pub(in crate::app) fn clear_image_preview_selection_activation(&mut self) {
        self.preview.image.activation_ready_at = None;
    }

    pub(in crate::app) fn mark_static_image_failed(&mut self, request: &StaticImageOverlayRequest) {
        self.preview
            .image
            .failed_images
            .insert(StaticImageKey::from_request(request));
    }

    pub(super) fn static_image_can_display_directly_now(
        &self,
        request: &StaticImageOverlayRequest,
    ) -> bool {
        matches!(
            self.preview.terminal_images.protocol,
            ImageProtocol::KittyGraphics | ImageProtocol::KittyDirectGraphics
        ) && !request.force_render_to_cache
            && static_image_format_for_overlay_request(request) == Some(StaticImageFormat::Png)
    }

    pub(super) fn static_image_can_use_source_path(
        &self,
        request: &StaticImageOverlayRequest,
    ) -> bool {
        match self.preview.terminal_images.protocol {
            ImageProtocol::KittyGraphics | ImageProtocol::KittyDirectGraphics => {
                self.static_image_can_display_directly_now(request)
            }
            ImageProtocol::ItermInline => static_image_supports_iterm_source_passthrough(request),
            // Sixel requires decoding and re-encoding the image, so the source path
            // can never be used directly — always go through the prepare pipeline.
            ImageProtocol::Sixel | ImageProtocol::None => false,
        }
    }

    pub(super) fn static_image_requires_prepare(
        &self,
        request: &StaticImageOverlayRequest,
    ) -> bool {
        request.prepare_inline_payload || !self.static_image_can_display_directly_now(request)
    }

    pub(super) fn magick_available(&mut self) -> bool {
        *self
            .preview
            .image
            .magick_available
            .get_or_insert_with(|| command_exists("magick"))
    }

    pub(super) fn resvg_available(&mut self) -> bool {
        *self
            .preview
            .image
            .resvg_available
            .get_or_insert_with(|| command_exists("resvg"))
    }

    pub(super) fn ffmpeg_available(&mut self) -> bool {
        *self
            .preview
            .image
            .ffmpeg_available
            .get_or_insert_with(|| command_exists("ffmpeg"))
    }

    #[cfg(test)]
    pub(in crate::app) fn set_ffmpeg_available_for_tests(&mut self, available: bool) {
        self.preview.image.ffmpeg_available = Some(available);
    }

    pub(in crate::app) fn image_selection_activation_ready(&self) -> bool {
        self.preview.image.activation_ready_at.is_none()
    }

    pub(in crate::app) fn static_image_overlay_displayed(&self) -> bool {
        self.preview.image.displayed.is_some()
    }

    pub(in crate::app) fn displayed_static_image_clear_area(&self) -> Option<Rect> {
        self.preview
            .image
            .displayed
            .as_ref()
            .map(|displayed| displayed.clear_area)
    }

    pub(in crate::app) fn displayed_static_image_mode(&self) -> Option<StaticImageOverlayMode> {
        self.preview
            .image
            .displayed
            .as_ref()
            .map(|displayed| displayed.mode)
    }

    pub(in crate::app) fn clear_displayed_static_image(&mut self) {
        self.preview.image.displayed = None;
        self.preview.image.displayed_excluded.clear();
    }

    pub(in crate::app) fn preview_visual_force_render_to_cache(
        &self,
        visual: &preview::PreviewVisual,
    ) -> bool {
        if visual.kind != preview::PreviewVisualKind::PageImage {
            return false;
        }

        let Some(format) = static_image_format_for_path(&visual.path) else {
            return true;
        };
        let ffmpeg_available = self
            .preview
            .image
            .ffmpeg_available
            .unwrap_or_else(|| command_exists("ffmpeg"));
        !static_image_can_prepare_inline(visual.size, format, ffmpeg_available)
            || self.uses_iterm_inline_protocol_inside_tmux()
    }

    pub(in crate::app) fn displayed_static_image_matches_active(&self) -> bool {
        self.active_static_image_display_target()
            .as_ref()
            .zip(self.preview.image.displayed.as_ref())
            .is_some_and(|(active, displayed)| active == displayed)
    }

    pub(in crate::app) fn keep_displayed_static_image_overlay_while_pending(&self) -> bool {
        let Some(displayed) = self.preview.image.displayed.as_ref() else {
            return false;
        };
        match displayed.mode {
            StaticImageOverlayMode::Inline => {
                let loading_current_page_preview = self.current_page_preview_loading_active();
                if !self.current_page_preview_visual_active() && !loading_current_page_preview {
                    return false;
                }

                if loading_current_page_preview {
                    // Keep the stale overlay only when the displayed page
                    // belongs to the same comic (i.e. page-stepping within one
                    // file).  For cross-file navigation, clear the old image
                    // immediately so the new entry's loading state is shown.
                    return self.displayed_comic_page_belongs_to_current_session();
                }

                let Some(request) = self.active_preview_visual_overlay_request_unchecked() else {
                    return false;
                };
                // Same guard for the image-prepare stage: once stage-1 has
                // completed but the page image is still being prepared, do not
                // keep a stale page from a different comic.
                if !self.displayed_comic_page_belongs_to_current_session() {
                    return false;
                }
                self.keep_displayed_static_image_request_while_pending(&request)
            }
            StaticImageOverlayMode::FullPane => self
                .active_static_image_overlay_request()
                .is_some_and(|request| {
                    self.keep_displayed_static_image_request_while_pending(&request)
                }),
        }
    }

    pub(in crate::app) fn displayed_static_image_replaces_preview(&self) -> bool {
        self.preview
            .image
            .displayed
            .as_ref()
            .is_some_and(|displayed| displayed.mode == StaticImageOverlayMode::FullPane)
            && self.displayed_static_image_matches_active()
    }

    pub(super) fn static_image_overlay_request_for_entry(
        &self,
        entry: &Entry,
    ) -> Option<StaticImageOverlayRequest> {
        if !self.preview_surface_visible_for_images() || !self.terminal_image_overlay_available() {
            return None;
        }
        static_image_detail_label(entry)?;

        let area = self.input.frame_state.preview_content_area?;
        if area.width == 0 || area.height == 0 {
            return None;
        }

        Some(StaticImageOverlayRequest {
            path: entry.path.clone(),
            size: entry.size,
            modified: entry.modified,
            area,
            target_width_px: image_target_width_px(area, self.cached_terminal_window()),
            target_height_px: image_target_height_px(area, self.cached_terminal_window()),
            mode: StaticImageOverlayMode::FullPane,
            force_render_to_cache: self.uses_iterm_inline_protocol_inside_tmux(),
            prepare_inline_payload: self.preview.terminal_images.protocol
                == ImageProtocol::ItermInline,
        })
    }

    fn current_page_preview_loading_active(&self) -> bool {
        self.preview
            .state
            .load_state
            .as_ref()
            .is_some_and(|load_state| {
                let loading_path = match load_state {
                    PreviewLoadState::Placeholder(path) | PreviewLoadState::Refreshing(path) => {
                        path
                    }
                };
                self.selected_entry()
                    .is_some_and(|entry| entry.path == *loading_path)
                    && (self.comic_preview_wheel_capture_active()
                        || self.epub_preview_wheel_capture_active())
            })
    }

    fn keep_displayed_static_image_request_while_pending(
        &self,
        request: &StaticImageOverlayRequest,
    ) -> bool {
        let key = StaticImageKey::from_request(request);
        if self.preview.image.failed_images.contains(&key) {
            return false;
        }
        if !self.image_selection_activation_ready() {
            return true;
        }

        self.preview.image.pending_prepares.contains(&key)
            || (self.static_image_requires_prepare(request)
                && !self.preview.image.dimensions.contains_key(&key))
    }
}
