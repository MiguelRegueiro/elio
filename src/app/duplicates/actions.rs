use super::*;

impl App {
    pub(in crate::app::duplicates) fn open_duplicate_targets(&mut self) -> Result<()> {
        let targets = self.duplicate_action_paths();
        if targets.is_empty() {
            return Ok(());
        }
        self.jobs.clipboard = None;
        self.open_paths_in_system(targets)
    }
    pub(in crate::app::duplicates) fn open_duplicate_rename(&mut self) {
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
    pub(in crate::app::duplicates) fn open_duplicate_bulk_rename(&mut self) {
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
    pub(in crate::app::duplicates) fn open_duplicate_editor_bulk_rename(&mut self) -> Result<()> {
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
    pub(in crate::app::duplicates) fn open_duplicate_trash_prompt(&mut self) {
        let Some(targets) = self.duplicate_trash_targets_or_status(false) else {
            return;
        };
        let has_trash = targets
            .iter()
            .any(|target| self.trash_target_is_inside_trash(&target.path));
        let has_normal = targets
            .iter()
            .any(|target| !self.trash_target_is_inside_trash(&target.path));
        match (has_trash, has_normal) {
            (true, true) => {
                self.status = "Selection mixes trash and normal files".to_string();
            }
            (true, false) => self.open_trash_prompt_for_explicit_targets(targets, true),
            _ => self.open_trash_prompt_for_explicit_targets(targets, false),
        }
    }
    pub(in crate::app::duplicates) fn open_duplicate_delete_permanently_prompt(&mut self) {
        if let Some(targets) = self.duplicate_trash_targets_or_status(true) {
            self.open_trash_prompt_for_explicit_targets(targets, true);
        }
    }
    pub(in crate::app::duplicates) fn duplicate_trash_targets_or_status(
        &mut self,
        permanent: bool,
    ) -> Option<Vec<TrashTarget>> {
        if self.duplicate_loading() {
            self.status = if permanent {
                "Wait for duplicate scan to finish before deleting results".to_string()
            } else {
                "Wait for duplicate scan to finish before trashing results".to_string()
            };
            return None;
        }
        let action_paths = self.duplicate_action_paths();
        let targets = action_paths
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
            return None;
        }
        Some(targets)
    }
    pub(in crate::app::duplicates) fn duplicate_action_paths(&self) -> Vec<PathBuf> {
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
}
