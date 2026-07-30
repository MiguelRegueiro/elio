use super::model::is_duplicate_help_shortcut;
use super::*;

impl App {
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
        if let Some(action) = crate::config::keys().action_for_key(key) {
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
                Action::DeletePermanently => {
                    self.open_duplicate_delete_permanently_prompt();
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

    pub(in crate::app::duplicates) fn move_duplicate_selection(&mut self, delta: isize) {
        let count = self.duplicate_file_count();
        if count == 0 {
            return;
        }
        let current = self.overlays.duplicates.as_ref().map_or(0, |d| d.selected) as isize;
        self.set_duplicate_selection(
            (current + delta).clamp(0, count.saturating_sub(1) as isize) as usize
        );
    }
    pub(in crate::app::duplicates) fn page_duplicate_selection(&mut self, direction: isize) {
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
    pub(in crate::app::duplicates) fn toggle_duplicate_selection(&mut self) {
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
    pub(in crate::app::duplicates) fn select_all_duplicates(&mut self) {
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
    pub(in crate::app::duplicates) fn clear_duplicate_selection_or_close(&mut self) {
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
    pub(in crate::app::duplicates) fn reveal_duplicate_focus(&mut self) -> Result<()> {
        let Some(path) = self.duplicate_focused_path() else {
            return Ok(());
        };
        self.close_duplicate_finder();
        self.reveal_path(path)?;
        Ok(())
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
    pub(in crate::app::duplicates) fn toggle_duplicate_preview(&mut self) {
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
}
