use super::super::{
    App,
    jobs::TrashRequest,
    state::{TrashOverlay, TrashProgress, TrashTarget},
};
use crate::fs::rect_contains;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::path::{Path, PathBuf};

impl App {
    pub(in crate::app) fn cwd_is_trash(&self) -> bool {
        self.navigation.in_trash
    }

    /// Returns `true` when the current directory is *inside* a trashed folder
    /// (i.e. a subdirectory of the trash root, but not the root itself).
    pub(in crate::app) fn cwd_is_inside_trash_subfolder(&self) -> bool {
        crate::fs::home_dir()
            .and_then(|home| crate::fs::trash_dir(&home))
            .is_some_and(|trash| {
                self.navigation.cwd != trash && self.navigation.cwd.starts_with(&trash)
            })
    }

    pub(in crate::app) fn path_is_trash(path: &Path) -> bool {
        crate::fs::home_dir()
            .and_then(|home| crate::fs::trash_dir(&home))
            .is_some_and(|trash| path == trash)
    }

    pub(in crate::app) fn path_is_inside_trash(path: &Path) -> bool {
        crate::fs::home_dir()
            .and_then(|home| crate::fs::trash_dir(&home))
            .is_some_and(|trash| path.starts_with(&trash))
    }

    pub(in crate::app) fn effective_show_hidden(&self) -> bool {
        self.navigation.show_hidden || self.navigation.in_trash
    }

    pub(in crate::app) fn effective_show_hidden_for(&self, path: &Path) -> bool {
        self.navigation.show_hidden || Self::path_is_trash(path)
    }
}

impl App {
    pub(in crate::app::create) fn selected_trash_targets(&self) -> Vec<TrashTarget> {
        if !self.navigation.selected_paths.is_empty() {
            self.selected_paths_sorted()
                .into_iter()
                .map(trash_target_from_path)
                .collect()
        } else {
            self.selected_entry()
                .map(|entry| {
                    vec![TrashTarget {
                        path: entry.path.clone(),
                        name: entry.name.clone(),
                        is_dir: entry.is_dir(),
                    }]
                })
                .unwrap_or_default()
        }
    }

    pub(in crate::app) fn open_trash_prompt(&mut self) {
        let targets = self.selected_trash_targets();

        if targets.is_empty() {
            return;
        }

        match self.trash_target_scope(&targets) {
            TrashTargetScope::Normal => self.open_trash_prompt_for_targets(targets, false),
            TrashTargetScope::Trash => self.open_trash_prompt_for_targets(targets, true),
            TrashTargetScope::Mixed => {
                self.status = "Selection mixes trash and normal files".to_string();
            }
        }
    }

    pub(in crate::app) fn open_delete_permanently_prompt(&mut self) {
        let targets = self.selected_trash_targets();

        if targets.is_empty() {
            return;
        }

        self.open_trash_prompt_for_targets(targets, true);
    }

    pub(in crate::app) fn open_trash_prompt_for_explicit_targets(
        &mut self,
        targets: Vec<TrashTarget>,
        permanent: bool,
    ) {
        self.overlays.help = false;
        self.overlays.search = None;
        self.overlays.create = None;
        self.overlays.trash = Some(TrashOverlay {
            targets,
            scroll: 0,
            confirmed: true,
            permanent,
        });
    }

    fn open_trash_prompt_for_targets(&mut self, targets: Vec<TrashTarget>, permanent: bool) {
        self.overlays.help = false;
        self.overlays.search = None;
        self.overlays.duplicates = None;
        self.overlays.create = None;
        self.overlays.trash = Some(TrashOverlay {
            targets,
            scroll: 0,
            confirmed: true,
            permanent,
        });
    }

    fn trash_target_scope(&self, targets: &[TrashTarget]) -> TrashTargetScope {
        let has_trash = targets
            .iter()
            .any(|target| self.trash_target_is_inside_trash(&target.path));
        let has_normal = targets
            .iter()
            .any(|target| !self.trash_target_is_inside_trash(&target.path));

        match (has_trash, has_normal) {
            (true, true) => TrashTargetScope::Mixed,
            (true, false) => TrashTargetScope::Trash,
            _ => TrashTargetScope::Normal,
        }
    }

    pub(in crate::app) fn trash_target_is_inside_trash(&self, path: &Path) -> bool {
        Self::path_is_inside_trash(path)
            || (self.navigation.in_trash && path.starts_with(&self.navigation.cwd))
    }

    pub fn trash_is_open(&self) -> bool {
        self.overlays.trash.is_some()
    }

    /// Returns `(completed, total, permanent)` for an in-progress
    /// trash/delete, or `None` when idle.
    pub fn trash_progress(&self) -> Option<(usize, usize, bool)> {
        self.jobs
            .trash_progress
            .as_ref()
            .map(|p| (p.completed, p.total, p.permanent))
    }

    pub fn trash_title(&self) -> String {
        let Some(t) = &self.overlays.trash else {
            return String::new();
        };
        let verb = if t.permanent {
            "Delete permanently"
        } else {
            "Trash"
        };
        match t.targets.len() {
            0 => String::new(),
            1 => {
                let kind = if t.targets[0].is_dir {
                    "folder"
                } else {
                    "file"
                };
                format!("{verb} 1 selected {kind}?")
            }
            _ => {
                let files = t.targets.iter().filter(|target| !target.is_dir).count();
                let dirs = t.targets.iter().filter(|target| target.is_dir).count();
                let desc = match (files, dirs) {
                    (f, 0) => format!("{f} file{}", if f == 1 { "" } else { "s" }),
                    (0, d) => format!("{d} folder{}", if d == 1 { "" } else { "s" }),
                    (f, d) => format!(
                        "{f} file{} and {d} folder{}",
                        if f == 1 { "" } else { "s" },
                        if d == 1 { "" } else { "s" }
                    ),
                };
                format!("{verb} {desc}?")
            }
        }
    }

    pub fn trash_scroll(&self) -> usize {
        self.overlays.trash.as_ref().map_or(0, |t| t.scroll)
    }

    pub fn trash_target_count(&self) -> usize {
        self.overlays.trash.as_ref().map_or(0, |t| t.targets.len())
    }

    pub fn trash_visible_rows(&self) -> usize {
        self.trash_target_count().min(8)
    }

    pub fn trash_target_label_at(&self, index: usize) -> Option<String> {
        let target = self
            .overlays
            .trash
            .as_ref()
            .and_then(|t| t.targets.get(index))?;

        if self.trash_targets_need_path_labels() {
            Some(target.path.display().to_string())
        } else {
            Some(target.name.clone())
        }
    }

    fn trash_targets_need_path_labels(&self) -> bool {
        self.overlays.trash.as_ref().is_some_and(|t| {
            t.targets
                .iter()
                .any(|target| target.path.parent() != Some(self.navigation.cwd.as_path()))
        })
    }

    pub fn trash_target_path_at(&self, index: usize) -> Option<&std::path::Path> {
        self.overlays
            .trash
            .as_ref()
            .and_then(|t| t.targets.get(index))
            .map(|target| target.path.as_path())
    }

    pub fn trash_target_is_dir_at(&self, index: usize) -> bool {
        self.overlays
            .trash
            .as_ref()
            .and_then(|t| t.targets.get(index))
            .is_some_and(|target| target.is_dir)
    }

    pub fn trash_confirmed(&self) -> bool {
        self.overlays.trash.as_ref().is_some_and(|t| t.confirmed)
    }

    pub(in crate::app) fn handle_trash_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.overlays.trash = None;
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => {
                self.overlays.trash = None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(t) = &mut self.overlays.trash {
                    t.scroll = t.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(t) = &mut self.overlays.trash {
                    let visible = t.targets.len().min(8);
                    let max_scroll = t.targets.len().saturating_sub(visible);
                    t.scroll = (t.scroll + 1).min(max_scroll);
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(t) = &mut self.overlays.trash {
                    t.confirmed = true;
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(t) = &mut self.overlays.trash {
                    t.confirmed = false;
                }
            }
            KeyCode::Tab => {
                if let Some(t) = &mut self.overlays.trash {
                    t.confirmed = !t.confirmed;
                }
            }
            KeyCode::Enter => {
                if self.overlays.trash.as_ref().is_some_and(|t| t.confirmed) {
                    self.confirm_trash()?;
                } else {
                    self.overlays.trash = None;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(in crate::app) fn handle_trash_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let inside = self
                    .input
                    .frame_state
                    .trash_panel
                    .is_some_and(|panel| rect_contains(panel, mouse.column, mouse.row));
                if !inside {
                    self.overlays.trash = None;
                    return Ok(());
                }
                if self
                    .input
                    .frame_state
                    .trash_confirm_btn
                    .is_some_and(|rect| rect_contains(rect, mouse.column, mouse.row))
                {
                    self.confirm_trash()?;
                } else if self
                    .input
                    .frame_state
                    .trash_cancel_btn
                    .is_some_and(|rect| rect_contains(rect, mouse.column, mouse.row))
                {
                    self.overlays.trash = None;
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(t) = &mut self.overlays.trash {
                    t.scroll = t.scroll.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(t) = &mut self.overlays.trash {
                    let visible = t.targets.len().min(8);
                    let max_scroll = t.targets.len().saturating_sub(visible);
                    t.scroll = (t.scroll + 1).min(max_scroll);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(in crate::app::create) fn confirm_trash(&mut self) -> Result<()> {
        if let Some(prog) = &self.jobs.trash_progress {
            self.status = if prog.permanent {
                "Delete in progress — press Esc to cancel".to_string()
            } else {
                // Batched trash is a single atomic OS call that cannot be
                // reliably interrupted once started.
                "Trash in progress".to_string()
            };
            self.overlays.trash = None;
            return Ok(());
        }
        let Some(t) = self.overlays.trash.take() else {
            return Ok(());
        };
        let had_duplicate_overlay = self.overlays.duplicates.is_some();
        if t.targets.is_empty() {
            return Ok(());
        }
        self.navigation.selected_paths.clear();
        if had_duplicate_overlay {
            self.overlays.duplicates = None;
        }
        let target_paths: Vec<PathBuf> =
            t.targets.iter().map(|target| target.path.clone()).collect();
        let source_cwd = self.queue_directory_escape_for_paths(&target_paths)?;

        // Compute which entry to land on after deletion: first surviving entry
        // at or after the current cursor, falling back to the last surviving
        // entry before it.
        let deleted_paths: std::collections::HashSet<_> =
            t.targets.iter().map(|tgt| &tgt.path).collect();
        let next_selection = self
            .navigation
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !deleted_paths.contains(&e.path))
            .find(|(i, _)| *i >= self.navigation.selected)
            .or_else(|| {
                self.navigation
                    .entries
                    .iter()
                    .enumerate()
                    .rfind(|(_, e)| !deleted_paths.contains(&e.path))
            })
            .map(|(_, e)| e.path.clone());

        let token = self.jobs.trash_token.wrapping_add(1);
        self.jobs.trash_token = token;
        self.jobs.trash_progress = Some(TrashProgress {
            completed: 0,
            total: t.targets.len(),
            permanent: t.permanent,
            next_selection,
        });
        self.jobs.trash_source_cwd = Some(source_cwd.clone());

        // Best-effort cross-device detection: if the source appears to be on a
        // different device than the home data dir (where the trash usually lives),
        // the trash crate will fall back to a copy+delete instead of a fast
        // rename.  Show a more informative status in that case.  This is UI-only
        // — behaviour is unchanged regardless of the heuristic's outcome.
        #[cfg(unix)]
        if !t.permanent && likely_cross_device_trash(&t.targets) {
            self.status = "Copying to trash…".to_string();
        }

        self.jobs.scheduler.submit_trash(TrashRequest {
            token,
            targets: t.targets,
            permanent: t.permanent,
        });

        Ok(())
    }
}

fn trash_target_from_path(path: PathBuf) -> TrashTarget {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| path.display().to_string());
    let is_dir = path.is_dir();
    TrashTarget { path, name, is_dir }
}

enum TrashTargetScope {
    Normal,
    Trash,
    Mixed,
}

/// Returns `true` when the first trash target appears to be on a different
/// device than `dirs::data_dir()` (i.e. `~/.local/share` on Linux), which is
/// where the home trash typically lives.
///
/// This is a best-effort heuristic.  The freedesktop trash spec may use a
/// per-mount `.Trash-UID` directory instead of the home trash, so false
/// positives are possible — in that case the user sees "Copying to trash…"
/// briefly before the fast rename completes.  The heuristic is UI-only and
/// never affects behaviour.
#[cfg(unix)]
fn likely_cross_device_trash(targets: &[crate::app::state::TrashTarget]) -> bool {
    use std::os::unix::fs::MetadataExt;
    let source_dev = targets
        .first()
        .and_then(|t| std::fs::metadata(&t.path).ok())
        .map(|m| m.dev());
    let data_dev = dirs::data_dir()
        .and_then(|d| std::fs::metadata(&d).ok())
        .map(|m| m.dev());
    match (source_dev, data_dev) {
        (Some(s), Some(d)) => s != d,
        _ => false,
    }
}
