use super::*;
use crate::app::jobs::DuplicateScanRequest;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

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

    pub(in crate::app) fn duplicate_flat_files(
        &self,
    ) -> Vec<(u64, crate::fs::duplicates::DuplicateFile)> {
        let Some(overlay) = &self.overlays.duplicates else {
            return Vec::new();
        };
        overlay
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .files
                    .iter()
                    .cloned()
                    .map(move |file| (group.id, file))
            })
            .collect()
    }

    pub(in crate::app) fn duplicate_focused_path(&self) -> Option<PathBuf> {
        let overlay = self.overlays.duplicates.as_ref()?;
        duplicate_file_at(&overlay.groups, overlay.selected).map(|file| file.path.clone())
    }

    pub fn duplicate_focused_entry(&self) -> Option<Entry> {
        let overlay = self.overlays.duplicates.as_ref()?;
        let file = duplicate_file_at(&overlay.groups, overlay.selected)?;
        Some(Entry {
            path: file.path.clone(),
            name: file.name.clone(),
            name_key: file.name.to_lowercase(),
            kind: EntryKind::File,
            symlink: None,
            size: file.size,
            modified: file.modified,
            readonly: false,
        })
    }

    pub(in crate::app) fn active_preview_entry(&self) -> Option<Entry> {
        if let Some(overlay) = self.overlays.duplicates.as_ref() {
            return overlay
                .preview_visible
                .then(|| self.duplicate_focused_entry())
                .flatten();
        }
        self.selected_entry().cloned()
    }

    pub fn duplicate_rows(&self, max_rows: usize) -> Vec<DuplicateRow> {
        let Some(overlay) = &self.overlays.duplicates else {
            return Vec::new();
        };
        let end = overlay.scroll.saturating_add(max_rows);
        let mut rows = Vec::with_capacity(max_rows);
        let mut flat_index = 0usize;

        for (group_rank, group) in overlay.groups.iter().enumerate() {
            let group_rank = group_rank + 1;
            let group_start = flat_index;
            for file in &group.files {
                if flat_index >= end {
                    return rows;
                }
                if flat_index >= overlay.scroll {
                    rows.push(DuplicateRow {
                        index: flat_index,
                        group_rank,
                        group_first: flat_index == group_start,
                        path: file.path.clone(),
                        name: file.name.clone(),
                        parent: duplicate_parent_label(&file.path, &overlay.cwd),
                        size: file.size,
                        selected: overlay.selected_paths.contains(&file.path),
                        focused: flat_index == overlay.selected,
                    });
                }
                flat_index += 1;
            }
        }
        rows
    }

    pub fn duplicate_group_count(&self) -> usize {
        self.overlays
            .duplicates
            .as_ref()
            .map_or(0, |d| d.groups.len())
    }
    pub fn duplicate_file_count(&self) -> usize {
        self.overlays
            .duplicates
            .as_ref()
            .map_or(0, |overlay| duplicate_group_file_count(&overlay.groups))
    }
    pub fn duplicate_stats(&self) -> Option<crate::fs::duplicates::DuplicateScanStats> {
        self.overlays.duplicates.as_ref().map(|d| d.stats)
    }
    pub fn duplicate_loading(&self) -> bool {
        self.overlays.duplicates.as_ref().is_some_and(|d| d.loading)
    }
    pub fn duplicate_error(&self) -> Option<&str> {
        self.overlays
            .duplicates
            .as_ref()
            .and_then(|d| d.error.as_deref())
    }
    pub fn duplicate_preview_visible(&self) -> bool {
        self.overlays
            .duplicates
            .as_ref()
            .is_some_and(|d| d.preview_visible)
    }
    pub(in crate::app) fn duplicate_preview_rendered(&self) -> bool {
        self.overlays.duplicates.is_some()
            && self.duplicate_preview_visible()
            && self.input.frame_state.preview_panel.is_some()
    }
    pub fn duplicate_cwd(&self) -> Option<&Path> {
        self.overlays.duplicates.as_ref().map(|d| d.cwd.as_path())
    }
    pub fn duplicate_scroll_top(&self) -> usize {
        self.overlays.duplicates.as_ref().map_or(0, |d| d.scroll)
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

    pub(in crate::app) fn handle_duplicate_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.clear_duplicate_selection_or_close();
            return Ok(());
        }
        if is_duplicate_help_shortcut(key) {
            self.clear_wheel_scroll();
            self.overlays.help_scroll = 0;
            self.overlays.help = true;
            return Ok(());
        }
        if let Some(action) =
            crate::config::keys().action_for_key_in_context(key, self.key_context())
        {
            use crate::config::Action;
            match action {
                Action::Quit | Action::QuitWithoutCd => {
                    self.dispatch_action(action)?;
                    return Ok(());
                }
                Action::Open => {
                    self.open_duplicate_targets()?;
                    return Ok(());
                }
                Action::OpenOrEnter => {
                    self.reveal_duplicate_focus()?;
                    return Ok(());
                }
                Action::CopyPath => {
                    self.open_copy_overlay_for_paths(self.duplicate_action_paths());
                    return Ok(());
                }
                Action::Rename => {
                    self.open_duplicate_rename();
                    return Ok(());
                }
                Action::RenameInEditor => {
                    self.open_duplicate_editor_bulk_rename()?;
                    return Ok(());
                }
                Action::Trash => {
                    self.open_duplicate_trash_prompt();
                    return Ok(());
                }
                Action::SelectAll => {
                    self.select_all_duplicates();
                    return Ok(());
                }
                Action::ToggleSelection => {
                    self.toggle_duplicate_selection();
                    return Ok(());
                }
                Action::FindDuplicates => {
                    self.close_duplicate_finder();
                    return Ok(());
                }
                Action::NavUp => {
                    self.move_duplicate_selection(-1);
                    return Ok(());
                }
                Action::NavDown => {
                    self.move_duplicate_selection(1);
                    return Ok(());
                }
                Action::PageUp => {
                    self.page_duplicate_selection(-1);
                    return Ok(());
                }
                Action::PageDown => {
                    self.page_duplicate_selection(1);
                    return Ok(());
                }
                Action::JumpFirst => {
                    self.set_duplicate_selection(0);
                    return Ok(());
                }
                Action::JumpLast => {
                    let last = self.duplicate_file_count().saturating_sub(1);
                    self.set_duplicate_selection(last);
                    return Ok(());
                }
                Action::TogglePreview => {
                    self.toggle_duplicate_preview();
                    return Ok(());
                }
                Action::ScrollPreviewUp => {
                    self.scroll_preview_lines(-1);
                    return Ok(());
                }
                Action::ScrollPreviewDown => {
                    self.scroll_preview_lines(1);
                    return Ok(());
                }
                Action::ScrollPreviewLeft => {
                    self.scroll_preview_columns(-1);
                    return Ok(());
                }
                Action::ScrollPreviewRight => {
                    self.scroll_preview_columns(1);
                    return Ok(());
                }
                _ => {}
            }
        }
        if key.modifiers == KeyModifiers::NONE && matches!(key.code, KeyCode::Esc) {
            self.clear_duplicate_selection_or_close();
        }
        Ok(())
    }

    fn move_duplicate_selection(&mut self, delta: isize) {
        let count = self.duplicate_file_count();
        if count == 0 {
            return;
        }
        let current = self.overlays.duplicates.as_ref().map_or(0, |d| d.selected) as isize;
        self.set_duplicate_selection(
            (current + delta).clamp(0, count.saturating_sub(1) as isize) as usize
        );
    }
    fn page_duplicate_selection(&mut self, direction: isize) {
        let visible = self.input.frame_state.duplicate_rows_visible.max(1) as isize;
        self.move_duplicate_selection(direction * visible);
    }
    pub(in crate::app) fn set_duplicate_selection(&mut self, index: usize) {
        let count = self.duplicate_file_count();
        if let Some(overlay) = &mut self.overlays.duplicates {
            overlay.selected = index.min(count.saturating_sub(1));
        }
        self.sync_duplicate_scroll();
        self.refresh_duplicate_preview();
    }
    pub(in crate::app) fn sync_duplicate_scroll(&mut self) -> bool {
        let count = self.duplicate_file_count();
        let Some(overlay) = &mut self.overlays.duplicates else {
            return false;
        };
        let previous_selected = overlay.selected;
        let previous_scroll = overlay.scroll;
        if count == 0 {
            overlay.selected = 0;
            overlay.scroll = 0;
            return previous_selected != overlay.selected || previous_scroll != overlay.scroll;
        }
        overlay.selected = overlay.selected.min(count - 1);
        let visible = self.input.frame_state.duplicate_rows_visible.max(1);
        if overlay.selected < overlay.scroll {
            overlay.scroll = overlay.selected;
        } else if overlay.selected >= overlay.scroll + visible {
            overlay.scroll = overlay.selected.saturating_sub(visible - 1);
        }
        overlay.scroll = overlay.scroll.min(count.saturating_sub(visible));
        previous_selected != overlay.selected || previous_scroll != overlay.scroll
    }
    fn toggle_duplicate_selection(&mut self) {
        let Some(path) = self.duplicate_focused_path() else {
            return;
        };
        let Some(overlay) = &mut self.overlays.duplicates else {
            return;
        };
        if !overlay.selected_paths.insert(path.clone()) {
            overlay.selected_paths.remove(&path);
        }
        self.status.clear();
        self.move_duplicate_selection(1);
    }
    fn select_all_duplicates(&mut self) {
        let paths = self
            .duplicate_flat_files()
            .into_iter()
            .map(|(_, file)| file.path)
            .collect::<Vec<_>>();
        let Some(overlay) = &mut self.overlays.duplicates else {
            return;
        };
        overlay.selected_paths.extend(paths);
        self.status.clear();
    }
    fn clear_duplicate_selection_or_close(&mut self) {
        if let Some(overlay) = self
            .overlays
            .duplicates
            .as_mut()
            .filter(|overlay| !overlay.selected_paths.is_empty())
        {
            overlay.selected_paths.clear();
            self.status.clear();
            return;
        }
        self.close_duplicate_finder();
    }
    fn reveal_duplicate_focus(&mut self) -> Result<()> {
        let Some(path) = self.duplicate_focused_path() else {
            return Ok(());
        };
        self.close_duplicate_finder();
        self.reveal_path(path)?;
        Ok(())
    }
    fn open_duplicate_targets(&mut self) -> Result<()> {
        let targets = self.duplicate_action_paths();
        if targets.is_empty() {
            return Ok(());
        }
        self.jobs.clipboard = None;
        self.open_paths_in_system(targets)
    }
    fn open_duplicate_rename(&mut self) {
        if self
            .overlays
            .duplicates
            .as_ref()
            .is_some_and(|overlay| !overlay.selected_paths.is_empty())
        {
            self.open_duplicate_bulk_rename();
            return;
        }
        let Some(entry) = self.duplicate_focused_entry() else {
            return;
        };
        self.overlays.rename = Some(RenameOverlay {
            is_dir: false,
            original_name: entry.name.clone(),
            input: entry.name,
            cursor_col: entry.name_key.chars().count(),
            error: None,
        });
    }
    fn open_duplicate_bulk_rename(&mut self) {
        let paths = self.duplicate_action_paths();
        if paths.is_empty() {
            return;
        }
        let items = paths
            .into_iter()
            .map(|path| BulkRenameItem {
                original_name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| path.display().to_string()),
                is_dir: false,
                path,
            })
            .collect::<Vec<_>>();
        let new_names = items
            .iter()
            .map(|item| item.original_name.clone())
            .collect::<Vec<_>>();
        let count = items.len();
        self.overlays.bulk_rename = Some(BulkRenameOverlay {
            items,
            new_names,
            root: None,
            cursor_line: 0,
            cursor_col: 0,
            preferred_col: 0,
            line_errors: vec![None; count],
        });
    }
    fn open_duplicate_editor_bulk_rename(&mut self) -> Result<()> {
        let paths = self.duplicate_action_paths();
        if paths.is_empty() {
            return Ok(());
        }
        let saved_selection = self.navigation.selected_paths.clone();
        self.navigation.selected_paths.clear();
        for path in paths {
            self.navigation.selected_paths.insert(path);
        }
        let result = self.open_editor_bulk_rename();
        self.navigation.selected_paths = saved_selection;
        result
    }
    fn open_duplicate_trash_prompt(&mut self) {
        if self.duplicate_loading() {
            self.status = "Wait for duplicate scan to finish before trashing results".to_string();
            return;
        }
        let targets = self
            .duplicate_action_paths()
            .into_iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().to_string();
                Some(TrashTarget {
                    path,
                    name,
                    is_dir: false,
                })
            })
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return;
        }
        self.open_trash_prompt_for_explicit_targets(targets, false);
    }
    fn duplicate_action_paths(&self) -> Vec<PathBuf> {
        if let Some(overlay) = self
            .overlays
            .duplicates
            .as_ref()
            .filter(|overlay| !overlay.selected_paths.is_empty())
        {
            let mut paths = overlay.selected_paths.iter().cloned().collect::<Vec<_>>();
            paths.sort();
            return paths;
        }
        self.duplicate_focused_path().into_iter().collect()
    }

    pub(in crate::app) fn confirm_duplicate_rename(
        &mut self,
        original_name: String,
        new_name: String,
    ) -> Result<()> {
        let Some(old_path) = self.duplicate_focused_path() else {
            self.overlays.rename = None;
            return Ok(());
        };
        if old_path.file_name().and_then(|name| name.to_str()) != Some(original_name.as_str()) {
            self.overlays.rename = None;
            return Ok(());
        }
        let new_path = old_path
            .parent()
            .map(|parent| parent.join(&new_name))
            .unwrap_or_else(|| PathBuf::from(&new_name));
        if new_path.exists() {
            if let Some(r) = &mut self.overlays.rename {
                r.error = Some(format!("\"{}\" already exists", new_name));
            }
            return Ok(());
        }
        if let Err(error) = fs::rename(&old_path, &new_path) {
            let msg = match error.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    format!("Permission denied renaming \"{}\"", original_name)
                }
                _ => format!("Could not rename: {error}"),
            };
            if let Some(r) = &mut self.overlays.rename {
                r.error = Some(msg);
            }
            return Ok(());
        }
        self.overlays.rename = None;
        self.apply_duplicate_rename_pairs(vec![(old_path, new_path)]);
        self.status = format!("Renamed \"{}\" → \"{}\"", original_name, new_name);
        Ok(())
    }

    pub(in crate::app) fn apply_duplicate_rename_pairs(&mut self, pairs: Vec<(PathBuf, PathBuf)>) {
        if pairs.is_empty() {
            return;
        }
        let Some(overlay) = &mut self.overlays.duplicates else {
            return;
        };
        for (old_path, new_path) in pairs {
            if overlay.selected_paths.remove(&old_path) {
                overlay.selected_paths.insert(new_path.clone());
            }
            if overlay.preview_path.as_ref() == Some(&old_path) {
                overlay.preview_path = None;
            }
            for group in &mut overlay.groups {
                for file in &mut group.files {
                    if file.path == old_path {
                        file.path = new_path.clone();
                        file.name = new_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .map(str::to_owned)
                            .unwrap_or_else(|| new_path.display().to_string());
                        file.relative = new_path
                            .strip_prefix(&overlay.cwd)
                            .unwrap_or(&new_path)
                            .to_string_lossy()
                            .replace('\\', "/");
                    }
                }
            }
        }
        self.refresh_duplicate_preview();
    }

    pub(in crate::app) fn handle_duplicate_mouse(
        &mut self,
        mouse: crossterm::event::MouseEvent,
    ) -> Result<()> {
        use crossterm::event::{MouseButton, MouseEventKind};
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(hit) = self
                    .input
                    .frame_state
                    .duplicate_hits
                    .iter()
                    .find(|hit| crate::fs::rect_contains(hit.rect, mouse.column, mouse.row))
                    .cloned()
                {
                    self.set_duplicate_selection(hit.index);
                } else if self
                    .input
                    .frame_state
                    .duplicate_panel
                    .is_none_or(|rect| !crate::fs::rect_contains(rect, mouse.column, mouse.row))
                {
                    self.close_duplicate_finder();
                }
            }
            MouseEventKind::ScrollDown => {
                if self
                    .input
                    .frame_state
                    .preview_panel
                    .is_some_and(|rect| crate::fs::rect_contains(rect, mouse.column, mouse.row))
                {
                    self.scroll_preview_lines(1);
                } else {
                    self.move_duplicate_selection(1);
                }
            }
            MouseEventKind::ScrollUp => {
                if self
                    .input
                    .frame_state
                    .preview_panel
                    .is_some_and(|rect| crate::fs::rect_contains(rect, mouse.column, mouse.row))
                {
                    self.scroll_preview_lines(-1);
                } else {
                    self.move_duplicate_selection(-1);
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn toggle_duplicate_preview(&mut self) {
        self.queue_terminal_image_geometry_clear();
        let mut hidden = false;
        if let Some(overlay) = &mut self.overlays.duplicates {
            overlay.preview_visible = !overlay.preview_visible;
            hidden = !overlay.preview_visible;
            if hidden {
                overlay.preview_path = None;
            }
        }
        if hidden {
            self.clear_image_preview_selection_activation();
        }
        self.refresh_duplicate_preview();
    }
    fn refresh_duplicate_preview(&mut self) {
        let Some(path) = self.duplicate_focused_path() else {
            return;
        };
        let should_refresh = self.overlays.duplicates.as_ref().is_some_and(|overlay| {
            overlay.preview_visible && overlay.preview_path.as_ref() != Some(&path)
        });
        if !should_refresh {
            return;
        }
        if let Some(overlay) = &mut self.overlays.duplicates {
            overlay.preview_path = Some(path);
        }
        self.refresh_preview();
    }
}

fn duplicate_file_at(
    groups: &[crate::fs::duplicates::DuplicateGroup],
    index: usize,
) -> Option<&crate::fs::duplicates::DuplicateFile> {
    let mut remaining = index;
    for group in groups {
        if remaining < group.files.len() {
            return group.files.get(remaining);
        }
        remaining = remaining.saturating_sub(group.files.len());
    }
    None
}

fn duplicate_parent_label(path: &Path, cwd: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.strip_prefix(cwd).ok())
        .map(|parent| {
            let label = parent.to_string_lossy().replace('\\', "/");
            if label.is_empty() {
                ".".to_string()
            } else {
                label
            }
        })
        .unwrap_or_else(|| {
            path.parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default()
        })
}

fn duplicate_group_file_count(groups: &[crate::fs::duplicates::DuplicateGroup]) -> usize {
    groups.iter().map(|group| group.files.len()).sum()
}

fn is_duplicate_help_shortcut(key: KeyEvent) -> bool {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return false;
    }

    matches!(key.code, KeyCode::Char('?'))
        || matches!(key.code, KeyCode::Char('/')) && key.modifiers.contains(KeyModifiers::SHIFT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::overlays::inline_image::{ImageProtocol, TerminalIdentity};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "elio-duplicates-overlay-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn opening_duplicate_finder_clears_browser_selection() {
        let root = temp_path("clears-browser-selection");
        fs::create_dir_all(&root).expect("failed to create temp root");
        fs::write(root.join("alpha.txt"), "same").expect("failed to write alpha");
        fs::write(root.join("beta.txt"), "same").expect("failed to write beta");

        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.toggle_selection();
        assert_eq!(app.selection_count(), 1);

        app.open_duplicate_finder();

        assert!(app.duplicates_is_open());
        assert_eq!(app.selection_count(), 0);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_finder_binding_closes_duplicate_overlay() {
        let root = temp_path("binding-closes-overlay");
        fs::create_dir_all(&root).expect("failed to create temp root");
        fs::write(root.join("alpha.txt"), "same").expect("failed to write alpha");
        fs::write(root.join("beta.txt"), "same").expect("failed to write beta");

        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        assert!(app.duplicates_is_open());
        app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT))
            .expect("Alt+D should close duplicate finder");

        assert!(!app.duplicates_is_open());
        assert!(!app.trash_is_open());

        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_finder_help_shortcut_opens_help_overlay_on_top() {
        let root = temp_path("help-on-top");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT))
            .expect("? should open help over duplicate finder");

        assert!(app.duplicates_is_open());
        assert!(app.overlays.help);

        app.handle_event(crossterm::event::Event::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )))
        .expect("Esc should close help overlay");

        assert!(app.duplicates_is_open());
        assert!(!app.overlays.help);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_finder_help_overlay_consumes_mouse_wheel() {
        let root = temp_path("help-mouse-wheel");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(
            42,
            10,
            &["alpha.txt", "beta.txt", "gamma.txt"],
        )];
        overlay.loading = false;
        overlay.selected = 1;
        app.overlays.help = true;
        app.overlays.help_scroll = 0;

        app.handle_event(crossterm::event::Event::Mouse(
            crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::ScrollDown,
                column: 1,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        ))
        .expect("mouse wheel should be handled by help overlay");

        assert!(app.duplicates_is_open());
        assert!(app.overlays.help);
        assert_eq!(app.overlays.help_scroll, 2);
        assert_eq!(app.overlays.duplicates.as_ref().unwrap().selected, 1);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_rows_render_display_rank_not_stable_group_id() {
        let root = temp_path("display-rank");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let groups = vec![
            duplicate_group(42, 10, &["big-a.txt", "big-b.txt", "big-c.txt"]),
            duplicate_group(7, 5, &["small-a.txt", "small-b.txt"]),
        ];
        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = groups;
        overlay.loading = false;

        let rows = app.duplicate_rows(10);

        assert_eq!(rows[0].group_rank, 1);
        assert!(rows[0].group_first);
        assert_eq!(rows[1].group_rank, 1);
        assert!(!rows[1].group_first);
        assert_eq!(rows[2].group_rank, 1);
        assert!(!rows[2].group_first);
        assert_eq!(rows[3].group_rank, 2);
        assert!(rows[3].group_first);
        assert_eq!(rows[4].group_rank, 2);
        assert!(!rows[4].group_first);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_rows_can_start_inside_group_without_losing_group_context() {
        let root = temp_path("rows-inside-group");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![
            duplicate_group(1, 10, &["a.txt", "b.txt", "c.txt"]),
            duplicate_group(2, 20, &["d.txt", "e.txt"]),
        ];
        overlay.loading = false;
        overlay.scroll = 1;
        overlay.selected = 3;

        let rows = app.duplicate_rows(3);

        assert_eq!(
            rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
            vec!["b.txt", "c.txt", "d.txt"]
        );
        assert_eq!(rows[0].index, 1);
        assert_eq!(rows[0].group_rank, 1);
        assert!(!rows[0].group_first);
        assert_eq!(rows[2].group_rank, 2);
        assert!(rows[2].group_first);
        assert!(rows[2].focused);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn selection_summary_tracks_duplicate_focus() {
        let root = temp_path("footer-focus");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![
            duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
            duplicate_group(7, 5, &["gamma.txt", "delta.txt"]),
        ];
        overlay.loading = false;

        assert_eq!(app.selection_summary(), "1/4  alpha.txt");
        assert_eq!(app.selection_count(), 0);
        app.toggle_duplicate_selection();
        assert_eq!(app.selection_count(), 1);
        app.set_duplicate_selection(2);
        assert_eq!(app.selection_summary(), "3/4  gamma.txt");

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_trash_prompt_opens_on_top_of_duplicate_overlay() {
        let root = temp_path("trash-on-top");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])];
        overlay.loading = false;

        app.open_duplicate_trash_prompt();

        assert!(app.duplicates_is_open());
        assert!(app.trash_is_open());

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_select_all_selects_every_duplicate_row() {
        let root = temp_path("select-all");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![
            duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
            duplicate_group(7, 5, &["gamma.txt", "delta.txt"]),
        ];
        overlay.loading = false;

        app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL))
            .expect("Ctrl+A should select every duplicate row");

        assert_eq!(app.selection_count(), 4);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_batch_append_keeps_existing_focus_stable() {
        let root = temp_path("batch-focus-stable");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        app.apply_duplicate_batch(crate::fs::duplicates::DuplicateScanBatch {
            groups: vec![duplicate_group(1, 10, &["small-a.txt", "small-b.txt"])],
            stats: crate::fs::duplicates::DuplicateScanStats::default(),
        });
        assert_eq!(
            app.duplicate_focused_path(),
            Some(PathBuf::from("small-a.txt"))
        );

        app.apply_duplicate_batch(crate::fs::duplicates::DuplicateScanBatch {
            groups: vec![duplicate_group(2, 10_000, &["large-a.txt", "large-b.txt"])],
            stats: crate::fs::duplicates::DuplicateScanStats::default(),
        });

        assert_eq!(
            app.duplicate_focused_path(),
            Some(PathBuf::from("small-a.txt")),
            "streamed batches should not resort the list and steal preview focus"
        );

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_final_result_resets_focus_to_top_after_reorder() {
        let root = temp_path("final-result-focus-top");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(
            1,
            10,
            &["small-a.txt", "small-b.txt", "small-c.txt"],
        )];
        overlay.selected = 2;
        overlay.scroll = 2;
        overlay.selected_paths.insert(PathBuf::from("small-c.txt"));
        overlay.preview_path = Some(PathBuf::from("small-c.txt"));

        app.apply_duplicate_result(Ok(crate::fs::duplicates::DuplicateScanResult {
            groups: vec![
                duplicate_group(2, 10_000, &["large-a.txt", "large-b.txt"]),
                duplicate_group(1, 10, &["small-a.txt", "small-b.txt", "small-c.txt"]),
            ],
            stats: crate::fs::duplicates::DuplicateScanStats::default(),
        }));

        let overlay = app
            .overlays
            .duplicates
            .as_ref()
            .expect("duplicate overlay should stay open");
        assert_eq!(overlay.selected, 0);
        assert_eq!(overlay.scroll, 0);
        assert_eq!(app.duplicate_scroll_top(), 0);
        assert_eq!(
            app.duplicate_focused_path(),
            Some(PathBuf::from("large-a.txt"))
        );
        assert_eq!(overlay.preview_path, Some(PathBuf::from("large-a.txt")));
        assert!(
            overlay
                .selected_paths
                .contains(&PathBuf::from("small-c.txt"))
        );

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_actions_use_selected_rows_before_focus() {
        let root = temp_path("selected-action-paths");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![
            duplicate_group(42, 10, &["alpha.txt", "beta.txt"]),
            duplicate_group(7, 5, &["gamma.txt", "delta.txt"]),
        ];
        overlay.loading = false;
        overlay.selected_paths.insert(PathBuf::from("beta.txt"));
        overlay.selected_paths.insert(PathBuf::from("delta.txt"));

        let paths = app.duplicate_action_paths();

        assert_eq!(
            paths,
            vec![PathBuf::from("beta.txt"), PathBuf::from("delta.txt")]
        );

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_rename_while_loading_patches_paths_without_restarting_scan() {
        let root = temp_path("rename-loading-patches-without-restart");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();
        let old_token = app.jobs.duplicate_token;

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(42, 10, &["alpha.txt", "beta.txt"])];
        overlay.loading = true;
        overlay.preview_path = Some(PathBuf::from("alpha.txt"));
        overlay.selected_paths.insert(PathBuf::from("beta.txt"));

        app.apply_duplicate_rename_pairs(vec![
            (
                PathBuf::from("alpha.txt"),
                PathBuf::from("renamed-alpha.txt"),
            ),
            (PathBuf::from("beta.txt"), PathBuf::from("renamed-beta.txt")),
        ]);

        let overlay = app
            .overlays
            .duplicates
            .as_ref()
            .expect("duplicate overlay should stay open");
        assert_eq!(app.jobs.duplicate_token, old_token);
        assert!(overlay.loading);
        assert_eq!(
            overlay.groups[0].files[0].path,
            PathBuf::from("renamed-alpha.txt")
        );
        assert_eq!(overlay.groups[0].files[0].name, "renamed-alpha.txt");
        assert!(
            overlay
                .selected_paths
                .contains(&PathBuf::from("renamed-beta.txt"))
        );
        assert!(!overlay.selected_paths.contains(&PathBuf::from("beta.txt")));

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn hidden_duplicate_preview_layout_keeps_content_target_but_no_image_surface() {
        let root = temp_path("hidden-preview-layout");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.set_terminal_image_protocol_for_tests(
            ImageProtocol::KittyGraphics,
            TerminalIdentity::Other,
        );
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(42, 10, &["alpha.webp", "beta.webp"])];
        overlay.loading = false;
        overlay.preview_visible = true;
        app.preview.visible = false;

        app.input.frame_state.preview_panel = None;
        assert_eq!(
            app.active_preview_entry().map(|entry| entry.name),
            Some("alpha.webp".to_string())
        );
        assert!(!app.preview_surface_visible_for_images());

        app.input.frame_state.preview_panel = Some(ratatui::layout::Rect {
            x: 40,
            y: 0,
            width: 20,
            height: 10,
        });
        assert!(app.preview_surface_visible_for_images());
        assert_eq!(
            app.active_preview_entry().map(|entry| entry.name),
            Some("alpha.webp".to_string())
        );
        app.input.frame_state.preview_content_area = Some(ratatui::layout::Rect {
            x: 40,
            y: 2,
            width: 20,
            height: 8,
        });
        assert!(
            app.active_static_image_overlay_request().is_some(),
            "Duplicate Finder image preview should not depend on browser preview visibility"
        );

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_scroll_clamps_when_visible_rows_increase() {
        let root = temp_path("scroll-clamp");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let names = (0..84)
            .map(|index| format!("file-{index:02}.txt"))
            .collect::<Vec<_>>();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(1, 10, &refs)];
        overlay.loading = false;
        overlay.selected = 83;
        overlay.scroll = 73;

        app.input.frame_state.duplicate_rows_visible = 30;

        assert!(app.sync_duplicate_scroll());
        assert_eq!(app.duplicate_scroll_top(), 54);
        assert_eq!(app.duplicate_rows(30).len(), 30);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_finder_shift_nav_does_not_move_file_focus() {
        let root = temp_path("shift-nav-no-focus-move");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(1, 10, &["alpha.webp", "beta.webp"])];
        overlay.loading = false;
        overlay.selected = 0;

        app.handle_duplicate_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT))
            .expect("Shift+Down should not move Duplicate Finder focus");

        assert_eq!(
            app.overlays
                .duplicates
                .as_ref()
                .expect("duplicate overlay should remain open")
                .selected,
            0
        );

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn duplicate_finder_shift_v_toggles_preview_without_moving_focus() {
        let root = temp_path("shift-v-preview-toggle");
        fs::create_dir_all(&root).expect("failed to create temp root");
        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.open_duplicate_finder();

        let overlay = app
            .overlays
            .duplicates
            .as_mut()
            .expect("duplicate overlay should be open");
        overlay.groups = vec![duplicate_group(1, 10, &["alpha.webp", "beta.webp"])];
        overlay.loading = false;
        overlay.selected = 0;
        overlay.preview_visible = true;

        app.handle_duplicate_key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT))
            .expect("Shift+V should toggle Duplicate Finder preview");

        let overlay = app
            .overlays
            .duplicates
            .as_ref()
            .expect("duplicate overlay should remain open");
        assert_eq!(overlay.selected, 0);
        assert!(!overlay.preview_visible);

        app.close_duplicate_finder();
        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    fn duplicate_group(
        id: u64,
        size: u64,
        names: &[&str],
    ) -> crate::fs::duplicates::DuplicateGroup {
        crate::fs::duplicates::DuplicateGroup {
            id,
            size,
            files: names
                .iter()
                .map(|name| crate::fs::duplicates::DuplicateFile {
                    path: PathBuf::from(name),
                    name: (*name).to_string(),
                    relative: (*name).to_string(),
                    size,
                    modified: None,
                })
                .collect(),
        }
    }
}
