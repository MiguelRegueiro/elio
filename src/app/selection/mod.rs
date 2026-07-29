use super::*;
use std::path::{Path, PathBuf};

impl App {
    pub fn is_selected(&self, path: &std::path::Path) -> bool {
        self.navigation.selected_paths.contains(path)
    }

    pub fn selection_count(&self) -> usize {
        if let Some(overlay) = &self.overlays.duplicates {
            return overlay.selected_paths.len();
        }
        self.navigation.selected_paths.len()
    }

    pub(in crate::app) fn selected_paths_sorted(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.navigation.selected_paths.iter().cloned().collect();
        paths.sort();
        paths
    }

    #[cfg(unix)]
    pub(in crate::app) fn selected_paths_in_selection_order(&self) -> Vec<PathBuf> {
        self.navigation.selected_paths.ordered().cloned().collect()
    }

    pub(in crate::app) fn current_directory_escape_for_paths(
        &self,
        paths: &[PathBuf],
    ) -> Option<PathBuf> {
        paths
            .iter()
            .filter(|path| self.navigation.cwd == **path || self.navigation.cwd.starts_with(path))
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .min_by_key(|path| path.components().count())
    }

    pub(in crate::app) fn toggle_selection(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let path = entry.path.clone();
        if self.navigation.selected_paths.remove(&path) {
            self.status.clear();
            if self.navigation.view_mode == ViewMode::List && !self.preview_fullscreen() {
                self.move_vertical(1);
            }
            return;
        }

        if self.has_selection_nesting_conflict(&path) {
            self.status = "Cannot select nested paths".to_string();
            return;
        }

        self.navigation.selected_paths.insert(path);
        self.status.clear();
        if self.navigation.view_mode == ViewMode::List && !self.preview_fullscreen() {
            self.move_vertical(1);
        }
    }

    pub(in crate::app) fn select_all(&mut self) {
        let mut blocked = false;
        for path in self
            .navigation
            .entries
            .iter()
            .map(|entry| entry.path.clone())
        {
            if self.has_selection_nesting_conflict(&path) {
                blocked = true;
                continue;
            }
            self.navigation.selected_paths.insert(path);
        }
        if blocked {
            self.status = "Cannot select nested paths".to_string();
        } else {
            self.status.clear();
        }
    }

    pub(in crate::app) fn clear_selection(&mut self) {
        if !self.navigation.selected_paths.is_empty() {
            self.navigation.selected_paths.clear();
            self.status.clear();
        }
    }

    pub(crate) fn enable_chooser_mode(&mut self) {
        self.chooser_mode = true;
        self.status = "Chooser mode".to_string();
    }

    pub(crate) fn take_chooser_exit(&mut self) -> Option<ChooserExit> {
        self.chooser_exit.take()
    }

    pub(in crate::app) fn confirm_chooser(&mut self) {
        if !self.chooser_mode {
            return;
        }
        self.chooser_exit = Some(ChooserExit::Confirmed(self.chooser_selection_paths()));
        self.should_quit = true;
    }

    pub(in crate::app) fn confirm_chooser_path(&mut self, path: &Path) {
        if !self.chooser_mode {
            return;
        }
        self.chooser_exit = Some(ChooserExit::Confirmed(vec![
            self.absolute_chooser_path(path),
        ]));
        self.should_quit = true;
    }

    pub(in crate::app) fn cancel_chooser(&mut self) {
        if !self.chooser_mode {
            return;
        }
        self.chooser_exit = Some(ChooserExit::Cancelled);
        self.should_change_directory_on_quit = false;
        self.should_quit = true;
    }

    fn chooser_selection_paths(&self) -> Vec<PathBuf> {
        if self.navigation.selected_paths.is_empty() {
            return self
                .selected_entry()
                .map(|entry| vec![self.absolute_chooser_path(&entry.path)])
                .unwrap_or_default();
        }

        let mut paths: Vec<PathBuf> = self
            .selected_paths_sorted()
            .iter()
            .map(|path| self.absolute_chooser_path(path))
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    fn absolute_chooser_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.navigation.cwd.join(path)
        }
    }

    fn has_selection_nesting_conflict(&self, path: &Path) -> bool {
        self.navigation.selected_paths.has_nesting_conflict(path)
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::SelectedPaths;
    use std::path::PathBuf;

    #[test]
    fn selected_paths_tracks_nested_conflicts_without_scanning_selected_siblings() {
        let mut selected = SelectedPaths::default();
        let siblings: Vec<PathBuf> = (0..1_000)
            .map(|index| PathBuf::from(format!("/tmp/elio-selection/file-{index}")))
            .collect();

        for path in siblings.iter().cloned() {
            assert!(selected.insert(path));
        }
        assert_eq!(selected.len(), siblings.len());
        assert!(selected.has_nesting_conflict(PathBuf::from("/tmp/elio-selection").as_path()));
        assert!(!selected.has_nesting_conflict(PathBuf::from("/tmp/other").as_path()));

        assert!(selected.remove(&siblings[0]));
        assert!(!selected.contains(&siblings[0]));
        assert_eq!(selected.len(), siblings.len() - 1);
        assert!(selected.has_nesting_conflict(PathBuf::from("/tmp/elio-selection").as_path()));

        selected.clear();
        assert!(selected.is_empty());
        assert!(!selected.has_nesting_conflict(PathBuf::from("/tmp/elio-selection").as_path()));
    }

    #[test]
    fn selected_paths_rejects_parent_child_mixes() {
        let mut selected = SelectedPaths::default();
        let parent = PathBuf::from("/tmp/elio-selection");
        let child = parent.join("child");
        let sibling = PathBuf::from("/tmp/other");

        assert!(selected.insert(child.clone()));
        assert!(!selected.insert(parent.clone()));
        assert!(selected.insert(sibling));

        assert!(selected.remove(&child));
        assert!(selected.insert(parent.clone()));
        assert!(!selected.insert(parent.join("nested")));
    }
}
