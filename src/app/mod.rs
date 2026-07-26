mod actions;
mod clipboard;
mod constants;
mod create;
mod directory_counts;
mod drag;
mod git;
mod input;
mod jobs;
mod local_filter;
mod open_rules;
mod open_with;
mod overlays;
use crate::{config, preview};
mod search;
mod selection;
mod state;
mod text_edit;
mod types;

#[cfg(test)]
use self::jobs::SchedulerMetricsSnapshot;
use self::jobs::{
    ArchiveExtractRequest, PreviewLineCountRequest, PreviewPriority, PreviewRequest, SearchRequest,
};
use self::{constants::*, state::*};
use anyhow::Result;
#[cfg(test)]
use ratatui::layout::Rect;
use ratatui::text::Line;
use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

pub use self::state::App;
#[cfg(test)]
pub use self::state::PreviewMetricsSnapshot;
pub(crate) use self::state::{ChooserExit, PendingTerminalTask};
pub(crate) use crate::core::FileClass;
pub(crate) use crate::fs::{
    format_item_count, format_size, format_size_parts, format_time_ago, rect_contains,
    sanitize_terminal_text,
};

pub(crate) use self::types::ClipOp;
pub use self::types::{
    CopyHit, EntryHit, FrameState, GoToHit, OpenWithHit, PathHit, SearchHit, SearchRow,
    SearchScope, ViewMetrics, ViewMode,
};
#[cfg(test)]
pub use crate::core::SidebarItem;
pub use crate::core::{Entry, EntryKind, SidebarItemKind, SidebarRow, SortMode};

impl App {
    pub fn set_frame_state(&mut self, mut frame_state: FrameState) -> bool {
        let previous_code_line_limit = self.selected_entry().map(|entry| {
            self.preview_code_line_limit_for_entry_with_rows(
                entry,
                self.input.frame_state.preview_rows_visible,
            )
        });
        if self.preview_fullscreen() && frame_state.entries_panel.is_none() {
            frame_state.metrics = self.input.frame_state.metrics;
        }
        self.input.frame_state = frame_state;
        let mut dirty = self.sync_scroll() | self.sync_search_scroll() | self.sync_preview_scroll();
        let next_code_line_limit = self.selected_entry().map(|entry| {
            self.preview_code_line_limit_for_entry_with_rows(
                entry,
                self.input.frame_state.preview_rows_visible,
            )
        });
        if previous_code_line_limit != next_code_line_limit {
            self.refresh_preview();
            dirty = true;
        }
        self.queue_visible_directory_item_counts();
        self.refresh_static_image_preloads_if_needed();
        self.remember_current_directory_view();
        dirty
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.navigation.entries.get(self.navigation.selected)
    }

    pub fn has_pending_auto_reload(&self) -> bool {
        self.navigation
            .directory_runtime
            .pending_reload_at
            .is_some()
    }

    pub fn has_pending_background_work(&self) -> bool {
        self.jobs.scheduler.has_pending_work()
    }

    pub(crate) fn browser_wheel_burst_active(&self) -> bool {
        self.input.wheel_profile == WheelProfile::HighFrequency
            && self.overlays.search.is_none()
            && self.input.last_wheel_target == Some(WheelTarget::Entries)
            && self
                .input
                .wheel_scroll
                .vertical
                .last_input_at
                .is_some_and(|at| at.elapsed() <= WHEEL_SCROLL_BURST_WINDOW)
    }

    pub(crate) fn pending_browser_wheel_timer(&self) -> Option<Duration> {
        if !self.input.browser_wheel_post_burst_pending {
            return None;
        }
        self.input
            .wheel_scroll
            .vertical
            .last_input_at
            .map(|at| WHEEL_SCROLL_BURST_WINDOW.saturating_sub(at.elapsed()))
    }

    pub(crate) fn process_browser_wheel_timers(&mut self) -> bool {
        if self.input.browser_wheel_post_burst_pending && !self.browser_wheel_burst_active() {
            self.input.browser_wheel_post_burst_pending = false;
            return true;
        }
        false
    }

    #[cfg(test)]
    pub fn scheduler_metrics(&self) -> SchedulerMetricsSnapshot {
        self.jobs.scheduler.metrics_snapshot()
    }

    #[cfg(test)]
    pub fn preview_metrics(&self) -> PreviewMetricsSnapshot {
        self.preview.state.metrics.snapshot()
    }

    pub(crate) fn preview_visible(&self) -> bool {
        self.preview.visible
    }

    pub(crate) fn preview_fullscreen(&self) -> bool {
        self.preview.visible && self.preview.fullscreen
    }

    pub(in crate::app) fn toggle_fullscreen_preview(&mut self) {
        if preview_pane_disabled_by_layout(config::layout()) {
            self.preview.visible = false;
            self.preview.fullscreen = false;
            self.status = "Preview pane disabled in config".to_string();
            return;
        }

        if !self.preview.visible {
            self.preview.visible = true;
            self.refresh_preview();
        }

        let next_fullscreen = !self.preview.fullscreen;
        if self.preview.fullscreen != next_fullscreen {
            self.queue_terminal_image_geometry_clear();
        }
        self.preview.fullscreen = next_fullscreen;
        self.status = if self.preview.fullscreen {
            "Fullscreen preview"
        } else {
            "Exited fullscreen preview"
        }
        .to_string();
    }

    pub(in crate::app) fn exit_fullscreen_preview(&mut self) -> bool {
        if self.clear_fullscreen_preview() {
            self.status = "Exited fullscreen preview".to_string();
            true
        } else {
            false
        }
    }

    pub(in crate::app) fn clear_fullscreen_preview(&mut self) -> bool {
        if self.preview.fullscreen {
            self.queue_terminal_image_geometry_clear();
            self.preview.fullscreen = false;
            self.preview.exit_fullscreen_after_directory_load = false;
            true
        } else {
            false
        }
    }

    pub(in crate::app) fn toggle_preview_pane(&mut self) {
        if preview_pane_disabled_by_layout(config::layout()) {
            self.preview.visible = false;
            self.preview.fullscreen = false;
            self.status = "Preview pane disabled in config".to_string();
            return;
        }

        self.preview.visible = !self.preview.visible;
        if self.preview.visible {
            self.refresh_preview();
            self.status = "Preview shown".to_string();
        } else {
            self.preview.fullscreen = false;
            self.status = "Preview hidden".to_string();
        }
    }

    pub fn report_runtime_error(&mut self, context: &str, error: &anyhow::Error) {
        self.status = format!("{context}: {error}");
    }
}

fn preview_pane_disabled_by_layout(layout: config::LayoutConfig) -> bool {
    layout.panes.is_some_and(|panes| panes.preview == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_pane_disabled_by_layout_when_custom_preview_weight_is_zero() {
        assert!(preview_pane_disabled_by_layout(config::LayoutConfig {
            panes: Some(config::PaneWeights {
                places: 1,
                files: 99,
                preview: 0,
            }),
        }));
        assert!(!preview_pane_disabled_by_layout(config::LayoutConfig {
            panes: Some(config::PaneWeights {
                places: 1,
                files: 98,
                preview: 1,
            }),
        }));
        assert!(!preview_pane_disabled_by_layout(config::LayoutConfig {
            panes: None
        }));
    }
}
