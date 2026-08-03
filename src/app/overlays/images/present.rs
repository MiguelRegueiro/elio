use super::types::DisplayedStaticImagePreview;
use super::{
    SixelDcsKey, StaticImageKey, StaticImageOverlayMode, StaticImageOverlayPreparation,
    StaticImageOverlayRequest,
};
use crate::app::App;
use crate::app::overlays::inline_image::{
    ImageProtocol, OverlayPresentState, RenderedImageDimensions, TerminalWindowSize,
    area_pixel_size, encode_sixel_dcs, fit_image_area, place_sixel_from_dcs, place_terminal_image,
    preview_log,
};
use anyhow::Result;
use ratatui::layout::Rect;
use std::{path::Path, sync::Arc};

impl App {
    pub(in crate::app) fn present_static_image_overlay(
        &mut self,
        protocol: ImageProtocol,
        excluded: &[Rect],
        force_repaint: bool,
        out: &mut Vec<u8>,
    ) -> Result<OverlayPresentState> {
        let Some(request) = self.active_static_image_overlay_request() else {
            preview_log("present_static_image_overlay: no request");
            return Ok(OverlayPresentState::NotRequested);
        };
        preview_log(format_args!(
            "present_static_image_overlay: path={:?} area={:?}",
            request.path, request.area
        ));
        if !self.image_selection_activation_ready() {
            preview_log("present_static_image_overlay: activation not ready → Waiting");
            return Ok(OverlayPresentState::Waiting);
        }
        if self.terminal_image_resize_settling() {
            preview_log("present_static_image_overlay: resize settling → Waiting");
            return Ok(OverlayPresentState::Waiting);
        }

        let prepared = match self.prepared_static_image_for_overlay(&request) {
            StaticImageOverlayPreparation::Ready(prepared) => prepared,
            StaticImageOverlayPreparation::Pending => {
                preview_log("present_static_image_overlay: preparation Pending → Waiting");
                return Ok(OverlayPresentState::Waiting);
            }
            StaticImageOverlayPreparation::Failed => {
                preview_log("present_static_image_overlay: preparation Failed");
                self.mark_static_image_failed(&request);
                self.refresh_preview();
                return Ok(OverlayPresentState::NotRequested);
            }
        };
        let Some(window_size) = self.cached_terminal_window() else {
            preview_log("present_static_image_overlay: no cached window size → failed");
            self.mark_static_image_failed(&request);
            self.refresh_preview();
            return Ok(OverlayPresentState::NotRequested);
        };
        let placement = self.static_image_display_area(&request, prepared.dimensions, window_size);
        preview_log(format_args!(
            "present_static_image_overlay: dims={}x{} placement={:?}",
            prepared.dimensions.width_px, prepared.dimensions.height_px, placement
        ));
        let displayed = DisplayedStaticImagePreview::from_request(
            &request,
            placement,
            self.static_image_clear_area(&request),
        );
        let image_changed = self.preview.image.displayed.as_ref() != Some(&displayed);
        let excluded_changed = excluded != self.preview.image.displayed_excluded.as_slice();
        let needs_repaint = force_repaint && protocol.is_raster();
        if !image_changed && !excluded_changed && !needs_repaint {
            preview_log("present_static_image_overlay: already displayed → Displayed");
            return Ok(OverlayPresentState::Displayed);
        }
        // Only clear the old image when the image itself changed, not when only the
        // excluded rects changed, to avoid a visible flash as the image disappears
        // and then immediately reappears.
        if image_changed {
            out.extend(self.clear_preview_overlay()?);
        }
        match self.place_static_image(
            protocol,
            &prepared.display_path,
            placement,
            excluded,
            prepared.inline_payload.as_deref(),
            window_size,
        ) {
            Ok(bytes) => {
                preview_log(format_args!(
                    "present_static_image_overlay: placed {} bytes via {protocol:?}",
                    bytes.len()
                ));
                out.extend(bytes);
                if protocol == ImageProtocol::Sixel && image_changed {
                    self.queue_sixel_repaint();
                }
            }
            Err(error) => {
                preview_log(format_args!(
                    "present_static_image_overlay: place_terminal_image error: {error}"
                ));
                self.mark_static_image_failed(&request);
                self.refresh_preview();
                return Ok(OverlayPresentState::NotRequested);
            }
        }

        self.preview.image.displayed = Some(displayed);
        self.preview.image.displayed_excluded = excluded.to_vec();
        self.clear_pending_iterm_popup_restore();
        Ok(OverlayPresentState::Displayed)
    }

    pub(in crate::app) fn present_preview_visual_overlay(
        &mut self,
        protocol: ImageProtocol,
        excluded: &[Rect],
        force_repaint: bool,
        out: &mut Vec<u8>,
    ) -> Result<OverlayPresentState> {
        let Some(request) = self.active_preview_visual_overlay_request() else {
            preview_log("present_preview_visual_overlay: no request");
            return Ok(OverlayPresentState::NotRequested);
        };
        preview_log(format_args!(
            "present_preview_visual_overlay: path={:?} area={:?}",
            request.path, request.area
        ));
        if !self.image_selection_activation_ready() {
            preview_log("present_preview_visual_overlay: activation not ready → Waiting");
            return Ok(OverlayPresentState::Waiting);
        }
        if self.terminal_image_resize_settling() {
            preview_log("present_preview_visual_overlay: resize settling → Waiting");
            return Ok(OverlayPresentState::Waiting);
        }

        let prepared = match self.prepared_static_image_for_overlay(&request) {
            StaticImageOverlayPreparation::Ready(prepared) => prepared,
            StaticImageOverlayPreparation::Pending => {
                preview_log("present_preview_visual_overlay: preparation Pending → Waiting");
                return Ok(OverlayPresentState::Waiting);
            }
            StaticImageOverlayPreparation::Failed => {
                preview_log("present_preview_visual_overlay: preparation Failed");
                self.mark_static_image_failed(&request);
                return Ok(OverlayPresentState::NotRequested);
            }
        };
        let Some(window_size) = self.cached_terminal_window() else {
            preview_log("present_preview_visual_overlay: no cached window size → failed");
            self.mark_static_image_failed(&request);
            return Ok(OverlayPresentState::NotRequested);
        };
        let placement = self.static_image_display_area(&request, prepared.dimensions, window_size);
        preview_log(format_args!(
            "present_preview_visual_overlay: dims={}x{} placement={:?}",
            prepared.dimensions.width_px, prepared.dimensions.height_px, placement
        ));
        let displayed = DisplayedStaticImagePreview::from_request(
            &request,
            placement,
            self.static_image_clear_area(&request),
        );
        let image_changed = self.preview.image.displayed.as_ref() != Some(&displayed);
        let excluded_changed = excluded != self.preview.image.displayed_excluded.as_slice();
        let needs_repaint = force_repaint && protocol.is_raster();
        if !image_changed && !excluded_changed && !needs_repaint {
            preview_log("present_preview_visual_overlay: already displayed → Displayed");
            return Ok(OverlayPresentState::Displayed);
        }
        if image_changed {
            out.extend(self.clear_preview_overlay()?);
        }
        match self.place_static_image(
            protocol,
            &prepared.display_path,
            placement,
            excluded,
            prepared.inline_payload.as_deref(),
            window_size,
        ) {
            Ok(bytes) => {
                preview_log(format_args!(
                    "present_preview_visual_overlay: placed {} bytes via {protocol:?}",
                    bytes.len()
                ));
                out.extend(bytes);
                if protocol == ImageProtocol::Sixel && image_changed {
                    self.queue_sixel_repaint();
                }
            }
            Err(error) => {
                preview_log(format_args!(
                    "present_preview_visual_overlay: place_terminal_image error: {error}"
                ));
                self.mark_static_image_failed(&request);
                return Ok(OverlayPresentState::NotRequested);
            }
        }

        self.preview.image.displayed = Some(displayed);
        self.record_comic_page_image_displayed();
        self.preview.image.displayed_excluded = excluded.to_vec();
        self.clear_pending_iterm_popup_restore();
        Ok(OverlayPresentState::Displayed)
    }

    pub(super) fn active_static_image_display_target(&self) -> Option<DisplayedStaticImagePreview> {
        let request = self
            .active_static_image_overlay_request()
            .or_else(|| self.active_preview_visual_overlay_request_unchecked())?;
        let window_size = self.cached_terminal_window()?;
        let image_dimensions = self
            .preview
            .image
            .dimensions
            .get(&StaticImageKey::from_request(&request))
            .copied()?;
        Some(DisplayedStaticImagePreview::from_request(
            &request,
            self.static_image_display_area(&request, image_dimensions, window_size),
            self.static_image_clear_area(&request),
        ))
    }

    fn static_image_clear_area(&self, request: &StaticImageOverlayRequest) -> Rect {
        if self.preview.terminal_images.protocol.is_raster() {
            if request.mode == StaticImageOverlayMode::Inline {
                return request.area;
            }
            return self
                .input
                .frame_state
                .preview_content_area
                .unwrap_or(request.area);
        }
        request.area
    }

    fn static_image_display_area(
        &self,
        request: &StaticImageOverlayRequest,
        dimensions: RenderedImageDimensions,
        window_size: TerminalWindowSize,
    ) -> Rect {
        // Fit the placeholder grid as well as direct raster placements. Native
        // Kitty can preserve aspect ratio within an arbitrary grid, but an
        // aspect-correct grid also survives graphics proxies that materialize
        // Unicode placeholders as a direct placement.
        fit_image_area(
            request.area,
            window_size,
            dimensions.width_px as f32 / dimensions.height_px as f32,
        )
    }

    /// Place a static image using the active protocol.
    ///
    /// For Sixel, looks up the pre-encoded DCS buffer from cache and prepends
    /// only the cursor-move prefix (cheap).  On a cache miss it encodes fresh
    /// and stores the result so subsequent renders are fast.
    fn place_static_image(
        &mut self,
        protocol: ImageProtocol,
        display_path: &Path,
        placement: Rect,
        excluded: &[Rect],
        inline_payload: Option<&str>,
        window_size: TerminalWindowSize,
    ) -> Result<Vec<u8>> {
        if protocol == ImageProtocol::Sixel {
            let dcs_key = SixelDcsKey::new(display_path, placement, window_size);
            let dcs: Arc<[u8]> = match self.cached_sixel_dcs(&dcs_key) {
                Some(cached) => cached,
                None => {
                    let (pw, ph) = area_pixel_size(placement, window_size);
                    let dcs = encode_sixel_dcs(display_path, pw, ph)?;
                    self.remember_sixel_dcs(dcs_key, Arc::clone(&dcs));
                    dcs
                }
            };
            return place_sixel_from_dcs(&dcs, placement);
        }
        place_terminal_image(
            protocol,
            display_path,
            placement,
            excluded,
            inline_payload,
            Some(window_size),
        )
    }
}
