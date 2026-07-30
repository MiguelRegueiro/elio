use super::model::duplicate_group_file_count;
use super::*;

impl App {
    pub fn duplicates_is_open(&self) -> bool {
        self.overlays.duplicates.is_some()
    }

    pub(in crate::app) fn open_duplicate_finder(&mut self) {
        self.clear_selection();
        self.queue_terminal_image_geometry_clear();
        self.clear_wheel_scroll();
        self.overlays.help = false;
        self.overlays.search = None;
        self.jobs.duplicate_token = self.jobs.duplicate_token.wrapping_add(1);
        let cwd = self.navigation.cwd.clone();
        let show_hidden = self.effective_show_hidden();
        self.overlays.duplicates = Some(DuplicateFinderOverlay {
            cwd: cwd.clone(),
            groups: Vec::new(),
            stats: crate::fs::duplicates::DuplicateScanStats::default(),
            selected: 0,
            scroll: 0,
            selected_paths: HashSet::new(),
            loading: true,
            error: None,
            preview_visible: true,
            preview_path: None,
        });
        let request = DuplicateScanRequest {
            token: self.jobs.duplicate_token,
            cwd,
            show_hidden,
        };
        let submitted = self.jobs.scheduler.submit_duplicate_scan(request);
        if let Some(overlay) = self.overlays.duplicates.as_mut().filter(|_| !submitted) {
            overlay.loading = false;
            overlay.error = Some("Duplicate worker unavailable".to_string());
        }
        self.refresh_duplicate_preview();
    }

    pub(in crate::app) fn close_duplicate_finder(&mut self) {
        self.queue_terminal_image_geometry_clear();
        self.jobs.duplicate_token = self.jobs.duplicate_token.wrapping_add(1);
        self.jobs.scheduler.cancel_duplicate_scan();
        self.overlays.duplicates = None;
        self.refresh_preview();
        self.clear_wheel_scroll();
    }

    pub(in crate::app) fn apply_duplicate_batch(
        &mut self,
        batch: crate::fs::duplicates::DuplicateScanBatch,
    ) {
        let mut became_non_empty = false;
        if let Some(overlay) = &mut self.overlays.duplicates {
            let had_files = duplicate_group_file_count(&overlay.groups) > 0;
            overlay.stats = batch.stats;
            overlay.error = None;
            overlay.groups.extend(batch.groups);
            became_non_empty = !had_files && duplicate_group_file_count(&overlay.groups) > 0;
        }
        if became_non_empty {
            self.queue_terminal_image_geometry_clear();
        }
        self.sync_duplicate_scroll();
        if became_non_empty {
            self.refresh_duplicate_preview();
        }
    }

    pub(in crate::app) fn apply_duplicate_result(
        &mut self,
        result: Result<crate::fs::duplicates::DuplicateScanResult, String>,
    ) {
        let had_files = self.duplicate_file_count() > 0;
        if let Some(overlay) = &mut self.overlays.duplicates {
            overlay.loading = false;
            match result {
                Ok(result) => {
                    overlay.groups = result.groups;
                    overlay.stats = result.stats;
                    overlay.selected = 0;
                    overlay.scroll = 0;
                    overlay.error = None;
                }
                Err(error) => {
                    overlay.groups.clear();
                    overlay.stats = crate::fs::duplicates::DuplicateScanStats::default();
                    overlay.error = Some(error);
                }
            }
        }
        if had_files != (self.duplicate_file_count() > 0) {
            self.queue_terminal_image_geometry_clear();
        }
        self.sync_duplicate_scroll();
        self.refresh_duplicate_preview();
    }
}
