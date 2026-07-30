use super::*;

impl App {
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
    pub fn duplicate_partial(&self) -> bool {
        self.overlays.duplicates.as_ref().is_some_and(|d| d.partial)
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

    pub(in crate::app) fn remove_duplicate_paths(&mut self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        let removed = paths.iter().collect::<HashSet<_>>();
        let Some(overlay) = &mut self.overlays.duplicates else {
            return;
        };
        overlay
            .selected_paths
            .retain(|path| !removed.contains(path));
        if overlay
            .preview_path
            .as_ref()
            .is_some_and(|path| removed.contains(path))
        {
            overlay.preview_path = None;
        }
        overlay.groups.retain_mut(|group| {
            group.files.retain(|file| !removed.contains(&file.path));
            !group.files.is_empty()
        });
        if overlay.groups.is_empty() {
            overlay.selected = 0;
            overlay.scroll = 0;
        }
        self.queue_terminal_image_geometry_clear();
        self.sync_duplicate_scroll();
        self.refresh_duplicate_preview();
    }

    pub(in crate::app::duplicates) fn refresh_duplicate_preview(&mut self) {
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

pub(super) fn duplicate_group_file_count(
    groups: &[crate::fs::duplicates::DuplicateGroup],
) -> usize {
    groups.iter().map(|group| group.files.len()).sum()
}

pub(super) fn is_duplicate_help_shortcut(key: KeyEvent) -> bool {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return false;
    }

    matches!(key.code, KeyCode::Char('?'))
        || matches!(key.code, KeyCode::Char('/')) && key.modifiers.contains(KeyModifiers::SHIFT)
}
