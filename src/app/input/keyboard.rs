use super::*;
use crate::app::text_edit::{
    char_to_byte, insert_text_at_cursor, multiline_paste_text, single_line_paste_text,
};

const HELP_FALLBACK_SCROLL_MAX: usize = 64;
const HELP_FALLBACK_PAGE_STEP: usize = 8;

impl App {
    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        let result = match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) => self.handle_paste(&text),
            Event::Resize(_, _) | Event::FocusGained | Event::FocusLost => Ok(()),
        };

        if let Err(error) = result {
            self.report_runtime_error("Action failed", &error);
        }

        Ok(())
    }

    fn handle_paste(&mut self, text: &str) -> Result<()> {
        if self.overlays.trash.is_some() || self.overlays.restore.is_some() {
            return Ok(());
        }

        if self.overlays.archive_password.is_some() {
            return self.paste_into_archive_password(text);
        }

        if self.overlays.archive_create.is_some() {
            return self.paste_into_archive_create(text);
        }

        if self.overlays.create.is_some() {
            return self.paste_into_create(text);
        }

        if self.overlays.rename.is_some() {
            return self.paste_into_rename(text);
        }

        if self.overlays.bulk_rename.is_some() {
            return self.paste_into_bulk_rename(text);
        }

        if self.overlays.editor_rename_confirm.is_some()
            || self.overlays.goto.is_some()
            || self.overlays.copy.is_some()
            || self.overlays.open_with.is_some()
        {
            return Ok(());
        }

        if self.overlays.search.is_some() {
            return self.paste_into_search(text);
        }

        if self.local_filter_is_editing() {
            return self.paste_into_local_filter(text);
        }

        Ok(())
    }

    fn paste_into_archive_password(&mut self, text: &str) -> Result<()> {
        let text = single_line_paste_text(text);
        if let Some(overlay) = &mut self.overlays.archive_password {
            insert_text_at_cursor(&mut overlay.input, &mut overlay.cursor_col, &text);
            overlay.error = None;
        }
        Ok(())
    }

    fn paste_into_archive_create(&mut self, text: &str) -> Result<()> {
        let text = single_line_paste_text(text);
        if let Some(overlay) = &mut self.overlays.archive_create {
            insert_text_at_cursor(&mut overlay.input, &mut overlay.cursor_col, &text);
            overlay.error = None;
        }
        Ok(())
    }

    fn paste_into_create(&mut self, text: &str) -> Result<()> {
        let text = multiline_paste_text(text);
        if text.is_empty() {
            return Ok(());
        }

        for ch in text.chars() {
            if ch == '\n' {
                self.create_insert_newline();
                continue;
            }
            if let Some(overlay) = &mut self.overlays.create {
                let byte = char_to_byte(&overlay.lines[overlay.cursor_line], overlay.cursor_col);
                overlay.lines[overlay.cursor_line].insert(byte, ch);
                overlay.cursor_col += 1;
                overlay.preferred_col = overlay.cursor_col;
                overlay.line_errors[overlay.cursor_line] = None;
            }
        }
        Ok(())
    }

    fn paste_into_rename(&mut self, text: &str) -> Result<()> {
        let text = single_line_paste_text(text);
        if let Some(overlay) = &mut self.overlays.rename {
            insert_text_at_cursor(&mut overlay.input, &mut overlay.cursor_col, &text);
            overlay.error = None;
        }
        Ok(())
    }

    fn paste_into_bulk_rename(&mut self, text: &str) -> Result<()> {
        let text = single_line_paste_text(text);
        if let Some(overlay) = &mut self.overlays.bulk_rename {
            insert_text_at_cursor(
                &mut overlay.new_names[overlay.cursor_line],
                &mut overlay.cursor_col,
                &text,
            );
            overlay.preferred_col = overlay.cursor_col;
            overlay.line_errors[overlay.cursor_line] = None;
        }
        Ok(())
    }

    fn paste_into_search(&mut self, text: &str) -> Result<()> {
        let text = single_line_paste_text(text);
        let previous_query = self
            .overlays
            .search
            .as_ref()
            .map(|search| search.query.clone())
            .unwrap_or_default();
        if let Some(search) = &mut self.overlays.search {
            insert_text_at_cursor(&mut search.query, &mut search.query_cursor, &text);
        }
        self.refresh_search_matches(&previous_query);
        Ok(())
    }

    fn paste_into_local_filter(&mut self, text: &str) -> Result<()> {
        let text = single_line_paste_text(text);
        insert_text_at_cursor(
            &mut self.navigation.local_filter.query,
            &mut self.navigation.local_filter.cursor,
            &text,
        );
        self.apply_local_filter_preserving_selection();
        Ok(())
    }

    fn help_scroll_max(&self) -> usize {
        if self.input.frame_state.help_rows_visible == 0 {
            HELP_FALLBACK_SCROLL_MAX
        } else {
            self.input.frame_state.help_scroll_max
        }
    }

    fn help_page_step(&self) -> usize {
        if self.input.frame_state.help_rows_visible == 0 {
            HELP_FALLBACK_PAGE_STEP
        } else {
            self.input
                .frame_state
                .help_rows_visible
                .saturating_sub(2)
                .max(1)
        }
    }

    fn has_active_cancelable_job(&self) -> bool {
        self.jobs.trash_progress.is_some()
            || self.jobs.restore_progress.is_some()
            || self.jobs.archive_create_progress.is_some()
            || self.jobs.archive_extract_progress.is_some()
            || self.jobs.paste_progress.is_some()
    }

    pub(in crate::app) fn scroll_help_by(&mut self, delta: isize) {
        let max_scroll = self.help_scroll_max();
        if delta.is_negative() {
            self.overlays.help_scroll = self
                .overlays
                .help_scroll
                .saturating_sub(delta.unsigned_abs());
        } else {
            self.overlays.help_scroll = self
                .overlays
                .help_scroll
                .saturating_add(delta as usize)
                .min(max_scroll);
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.overlays.help = false;
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => self.overlays.help = false,
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_help_by(-1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_help_by(1);
            }
            KeyCode::PageUp => {
                self.scroll_help_by(-(self.help_page_step() as isize));
            }
            KeyCode::PageDown => {
                self.scroll_help_by(self.help_page_step() as isize);
            }
            KeyCode::Home => self.overlays.help_scroll = 0,
            KeyCode::End => self.overlays.help_scroll = self.help_scroll_max(),
            _ if is_help_shortcut(key) => self.overlays.help = false,
            _ => {}
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // The kitty keyboard protocol (enabled when the terminal supports it) emits
        // Press, Repeat, and Release events. Ignore Release so each keystroke is only
        // handled once. Repeat is kept so held navigation keys continue to scroll.
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }

        if is_cancel_key(key)
            && !self.navigation.selected_paths.is_empty()
            && self.has_active_cancelable_job()
        {
            self.clear_selection();
            return Ok(());
        }

        if self.overlays.trash.is_some() {
            return self.handle_trash_key(key);
        }

        if self.overlays.restore.is_some() {
            return self.handle_restore_key(key);
        }

        if self.overlays.archive_password.is_some() {
            return self.handle_archive_password_key(key);
        }

        if self.overlays.archive_create.is_some() {
            return self.handle_archive_create_key(key);
        }

        if self.overlays.create.is_some() {
            return self.handle_create_key(key);
        }

        if self.overlays.rename.is_some() {
            return self.handle_rename_key(key);
        }

        if self.overlays.bulk_rename.is_some() {
            return self.handle_bulk_rename_key(key);
        }

        if self.overlays.editor_rename_confirm.is_some() {
            return self.handle_editor_rename_confirm_key(key);
        }

        if self.overlays.goto.is_some() {
            return self.handle_goto_key(key);
        }

        if self.overlays.copy.is_some() {
            return self.handle_copy_key(key);
        }

        if self.overlays.open_with.is_some() {
            return self.handle_open_with_key(key);
        }

        if self.overlays.help {
            return self.handle_help_key(key);
        }

        if self.overlays.duplicates.is_some() {
            return self.handle_duplicate_key(key);
        }

        if self.overlays.search.is_some() {
            return self.handle_search_key(key);
        }

        if self.local_filter_is_editing() {
            return self.handle_local_filter_key(key);
        }

        if self.should_debounce_navigation_key(key) {
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            if self.exit_fullscreen_preview() {
                return Ok(());
            }

            if let Some(prog) = &self.jobs.trash_progress {
                self.jobs.scheduler.cancel_trash(self.jobs.trash_token);
                if prog.permanent {
                    // Permanent delete can be stopped between items; clear chip immediately.
                    self.jobs.trash_progress = None;
                }
                // Non-permanent: the batch OS call is atomic and may already be
                // in flight.  Keep the chip visible; done=true will clear it.
            } else if self.jobs.restore_progress.is_some() {
                self.jobs.scheduler.cancel_restore(self.jobs.restore_token);
                self.jobs.restore_progress = None;
            } else if self.jobs.archive_create_progress.is_some() {
                self.jobs
                    .scheduler
                    .cancel_archive_create(self.jobs.archive_create_token);
                self.jobs.archive_create_progress = None;
            } else if self.jobs.archive_extract_progress.is_some() {
                self.jobs
                    .scheduler
                    .cancel_archive_extract(self.jobs.archive_extract_token);
                self.jobs.archive_extract_progress = None;
            } else if self.jobs.paste_progress.is_some() {
                self.jobs.scheduler.cancel_paste(self.jobs.paste_token);
                self.jobs.paste_progress = None;
                self.clear_queued_pastes();
            } else {
                self.clear_selection();
                self.jobs.clipboard = None;
            }
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.adjust_zoom(1);
                    return Ok(());
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    self.adjust_zoom(-1);
                    return Ok(());
                }
                _ => {}
            }
        }

        let configured_action = if self.chooser_mode {
            match crate::config::keys().chooser_action_for_key(key, self.key_context()) {
                Some(crate::config::ChooserKeyAction::Choose) => {
                    self.confirm_chooser();
                    return Ok(());
                }
                Some(crate::config::ChooserKeyAction::Cancel) => {
                    self.cancel_chooser();
                    return Ok(());
                }
                Some(crate::config::ChooserKeyAction::Normal(action)) => Some(action),
                None => None,
            }
        } else {
            crate::config::keys().action_for_key_in_context(key, self.key_context())
        };

        if self.preview_fullscreen() {
            if matches!(key.code, KeyCode::Esc) {
                self.exit_fullscreen_preview();
                return Ok(());
            }

            if is_help_shortcut(key) {
                self.clear_wheel_scroll();
                self.overlays.help_scroll = 0;
                self.overlays.help = true;
                return Ok(());
            }

            if let Some(action) = configured_action {
                match fullscreen_preview_action_policy(self, action) {
                    FullscreenPreviewActionPolicy::HandleInFullscreen => {
                        self.dispatch_action(action)?;
                    }
                    FullscreenPreviewActionPolicy::ExitThenDispatch => {
                        self.exit_fullscreen_preview();
                        self.dispatch_action(action)?;
                    }
                    FullscreenPreviewActionPolicy::DispatchThenExit => {
                        let status_before = self.status.clone();
                        self.dispatch_action(action)?;
                        if self.navigation.directory_runtime.pending_load.is_some() {
                            self.preview.exit_fullscreen_after_directory_load = true;
                        } else if self.clear_fullscreen_preview() && self.status == status_before {
                            self.status = "Exited fullscreen preview".to_string();
                        }
                    }
                    FullscreenPreviewActionPolicy::DispatchAndStayInFullscreen => {
                        self.dispatch_action(action)?;
                        if self.navigation.directory_runtime.pending_load.is_some() {
                            self.preview.exit_fullscreen_after_directory_load = true;
                        }
                    }
                    FullscreenPreviewActionPolicy::Block => {}
                }
                return Ok(());
            }

            return Ok(());
        }

        if should_use_grid_zoom_for_symlink_key(self, key, configured_action) {
            self.adjust_zoom(-1);
            return Ok(());
        }

        if self.input.wheel_profile == WheelProfile::HighFrequency
            && should_handle_high_frequency_horizontal_key(key, configured_action)
        {
            match key.code {
                KeyCode::Left if self.handle_horizontal_navigation_key(-1) => return Ok(()),
                KeyCode::Right if self.handle_horizontal_navigation_key(1) => return Ok(()),
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc => {
                if !self.navigation.selected_paths.is_empty() {
                    self.clear_selection();
                } else if let Some(prog) = &self.jobs.trash_progress {
                    self.jobs.scheduler.cancel_trash(self.jobs.trash_token);
                    if prog.permanent {
                        self.jobs.trash_progress = None;
                    }
                } else if self.jobs.restore_progress.is_some() {
                    self.jobs.scheduler.cancel_restore(self.jobs.restore_token);
                    self.jobs.restore_progress = None;
                } else if self.jobs.archive_create_progress.is_some() {
                    self.jobs
                        .scheduler
                        .cancel_archive_create(self.jobs.archive_create_token);
                    self.jobs.archive_create_progress = None;
                } else if self.jobs.archive_extract_progress.is_some() {
                    self.jobs
                        .scheduler
                        .cancel_archive_extract(self.jobs.archive_extract_token);
                    self.jobs.archive_extract_progress = None;
                } else if self.jobs.paste_progress.is_some() {
                    self.jobs.scheduler.cancel_paste(self.jobs.paste_token);
                    self.jobs.paste_progress = None;
                    self.clear_queued_pastes();
                } else {
                    self.clear_selection();
                    self.jobs.clipboard = None;
                }
            }
            _ if is_help_shortcut(key) => {
                self.clear_wheel_scroll();
                self.overlays.help_scroll = 0;
                self.overlays.help = true;
            }
            _ if configured_action.is_some() => {
                let action = configured_action.expect("action was checked above");
                self.dispatch_action(action)?;
            }
            KeyCode::Char('+') | KeyCode::Char('=')
                if self.navigation.view_mode == ViewMode::Grid =>
            {
                self.adjust_zoom(1);
            }
            KeyCode::Char('-') | KeyCode::Char('_')
                if self.navigation.view_mode == ViewMode::Grid =>
            {
                self.adjust_zoom(-1);
            }
            _ => {}
        }
        Ok(())
    }

    pub(in crate::app) fn dispatch_action(&mut self, action: crate::config::Action) -> Result<()> {
        use crate::config::Action;
        match action {
            Action::Quit => self.should_quit = true,
            Action::QuitWithoutCd => {
                self.should_change_directory_on_quit = false;
                self.should_quit = true;
            }
            Action::Yank => self.yank(),
            Action::Cut => self.cut(),
            Action::Paste => self.paste()?,
            Action::CreateArchive => self.open_archive_create_prompt(),
            Action::ExtractArchive => self.extract_focused_archive()?,
            Action::SymlinkAbsolute => self.link_yanked(false)?,
            Action::SymlinkRelative => self.link_yanked(true)?,
            Action::Trash => self.open_trash_prompt(),
            Action::DeletePermanently => self.open_delete_permanently_prompt(),
            Action::Create => self.open_create_prompt(),
            Action::Rename => {
                if !self.navigation.in_trash && !self.cwd_is_inside_trash_subfolder() {
                    if !self.navigation.selected_paths.is_empty() {
                        self.open_bulk_rename_prompt();
                    } else {
                        self.open_rename_prompt();
                    }
                }
            }
            Action::RenameInEditor => self.open_editor_bulk_rename()?,
            Action::RestoreFromTrash => {
                if self.navigation.in_trash {
                    self.open_restore_prompt();
                } else if self.cwd_is_inside_trash_subfolder() {
                    self.status = "Cannot restore from inside a trashed folder \
                                   — go up to the trash to restore the folder itself"
                        .to_string();
                }
            }
            Action::CopyPath => self.open_copy_overlay(),
            Action::SearchFolders => self.open_search_with_status(SearchScope::Folders),
            Action::SearchFiles => self.open_search_with_status(SearchScope::Files),
            Action::FilterDirectory => self.open_local_filter(),
            Action::FindDuplicates => self.open_duplicate_finder(),
            Action::Zoxide => {
                self.pending_terminal_task = Some(PendingTerminalTask::Zoxide);
                self.status.clear();
            }
            Action::Shell => {
                self.pending_terminal_task = Some(PendingTerminalTask::Shell {
                    cwd: self.navigation.cwd.clone(),
                });
                self.status.clear();
            }
            Action::Open => self.open_in_system()?,
            Action::OpenWith => self.open_open_with_overlay(),
            Action::OpenOrEnter => self.open_selected()?,
            Action::GoTo => self.open_goto_overlay(),
            Action::ToggleSelection => self.toggle_selection(),
            Action::CyclePlacesNext => self.step_sidebar_place(1)?,
            Action::CyclePlacesPrevious => self.step_sidebar_place(-1)?,
            Action::GoParent => self.go_parent()?,
            Action::PageUp => self.page(-1),
            Action::PageDown => self.page(1),
            Action::JumpFirst => self.select_index(0),
            Action::JumpLast => self.jump_last(),
            Action::SelectAll => self.select_all(),
            Action::HistoryBack => return self.go_back(),
            Action::HistoryForward => return self.go_forward(),
            Action::Sort => self.cycle_sort_mode()?,
            Action::ToggleView => self.toggle_view_mode(),
            Action::TogglePreview => self.toggle_preview_pane(),
            Action::FullscreenPreview => self.toggle_fullscreen_preview(),
            Action::ToggleHidden => self.toggle_hidden_files()?,
            Action::NavLeft => {
                if self.navigation.view_mode == ViewMode::Grid {
                    self.move_by_keyboard(-1);
                } else {
                    self.go_parent()?;
                }
            }
            Action::NavDown => self.move_vertical_keyboard(1),
            Action::NavUp => self.move_vertical_keyboard(-1),
            Action::NavRight => {
                if self.navigation.view_mode == ViewMode::Grid {
                    self.move_by_keyboard(1);
                } else if let Some(entry) = self.selected_entry().filter(|entry| entry.is_dir()) {
                    self.set_dir(entry.path.clone())?;
                }
            }
            Action::ScrollPreviewLeft => {
                let _ = self.scroll_preview_columns(-1);
            }
            Action::ScrollPreviewRight => {
                let _ = self.scroll_preview_columns(1);
            }
            Action::ScrollPreviewUp => {
                if !self.step_epub_section(-1)
                    && !self.step_comic_page(-1)
                    && !self.step_pdf_page(-1)
                {
                    let _ = self.scroll_preview_lines(-1);
                }
            }
            Action::ScrollPreviewDown => {
                if !self.step_epub_section(1) && !self.step_comic_page(1) && !self.step_pdf_page(1)
                {
                    let _ = self.scroll_preview_lines(1);
                }
            }
        }
        Ok(())
    }

    pub(in crate::app) fn key_context(&self) -> crate::config::KeyContext {
        if self.navigation.in_trash || self.cwd_is_inside_trash_subfolder() {
            crate::config::KeyContext::Trash
        } else {
            crate::config::KeyContext::Normal
        }
    }

    fn should_debounce_navigation_key(&mut self, key: KeyEvent) -> bool {
        let Some(navigation_key) = Self::navigation_repeat_key(key) else {
            return false;
        };

        let now = Instant::now();
        if self
            .input
            .last_navigation_key
            .is_some_and(|(previous_key, previous_at)| {
                previous_key == navigation_key
                    && now.duration_since(previous_at) < KEY_REPEAT_NAV_INTERVAL
            })
        {
            return true;
        }

        self.input.last_navigation_key = Some((navigation_key, now));
        false
    }

    fn navigation_repeat_key(key: KeyEvent) -> Option<NavigationRepeatKey> {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }

        match crate::config::keys().action_for_key(key) {
            Some(crate::config::Action::NavUp) => Some(NavigationRepeatKey::Up),
            Some(crate::config::Action::NavDown) => Some(NavigationRepeatKey::Down),
            Some(crate::config::Action::NavLeft) => Some(NavigationRepeatKey::Left),
            Some(crate::config::Action::NavRight) => Some(NavigationRepeatKey::Right),
            Some(crate::config::Action::PageUp) => Some(NavigationRepeatKey::PageUp),
            Some(crate::config::Action::PageDown) => Some(NavigationRepeatKey::PageDown),
            Some(crate::config::Action::JumpFirst) => Some(NavigationRepeatKey::Home),
            Some(crate::config::Action::JumpLast) => Some(NavigationRepeatKey::End),
            _ => None,
        }
    }

    pub(in crate::app) fn open_selected(&mut self) -> Result<()> {
        let Some(entry) = self.selected_entry() else {
            if !self.navigation.selected_paths.is_empty() {
                return self.dispatch_action(crate::config::Action::Open);
            }
            return Ok(());
        };
        if entry.is_dir() {
            return self.set_dir(entry.path.clone());
        }

        self.dispatch_action(crate::config::Action::Open)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FullscreenPreviewActionPolicy {
    HandleInFullscreen,
    ExitThenDispatch,
    DispatchThenExit,
    DispatchAndStayInFullscreen,
    Block,
}

fn fullscreen_preview_action_policy(
    app: &App,
    action: crate::config::Action,
) -> FullscreenPreviewActionPolicy {
    use crate::config::Action;
    match action {
        Action::FullscreenPreview
        | Action::TogglePreview
        | Action::Quit
        | Action::QuitWithoutCd
        | Action::ScrollPreviewLeft
        | Action::ScrollPreviewRight
        | Action::ScrollPreviewUp
        | Action::ScrollPreviewDown => FullscreenPreviewActionPolicy::HandleInFullscreen,
        Action::OpenOrEnter if app.selected_entry().is_some_and(|entry| entry.is_dir()) => {
            FullscreenPreviewActionPolicy::DispatchThenExit
        }
        action if fullscreen_preview_dispatches_and_stays(action) => {
            FullscreenPreviewActionPolicy::DispatchAndStayInFullscreen
        }
        Action::NavLeft | Action::NavRight if app.navigation.view_mode == ViewMode::Grid => {
            FullscreenPreviewActionPolicy::DispatchAndStayInFullscreen
        }
        action if fullscreen_preview_dispatches_then_exits(action) => {
            FullscreenPreviewActionPolicy::DispatchThenExit
        }
        Action::OpenOrEnter => FullscreenPreviewActionPolicy::ExitThenDispatch,
        action if fullscreen_preview_exits_then_dispatches(action) => {
            FullscreenPreviewActionPolicy::ExitThenDispatch
        }
        _ => FullscreenPreviewActionPolicy::Block,
    }
}

fn fullscreen_preview_dispatches_then_exits(action: crate::config::Action) -> bool {
    use crate::config::Action;
    matches!(
        action,
        Action::CyclePlacesNext
            | Action::CyclePlacesPrevious
            | Action::GoParent
            | Action::HistoryBack
            | Action::HistoryForward
            | Action::NavLeft
            | Action::NavRight
    )
}

fn fullscreen_preview_dispatches_and_stays(action: crate::config::Action) -> bool {
    use crate::config::Action;
    matches!(
        action,
        Action::ToggleSelection
            | Action::PageUp
            | Action::PageDown
            | Action::JumpFirst
            | Action::JumpLast
            | Action::NavDown
            | Action::NavUp
    )
}

fn fullscreen_preview_exits_then_dispatches(action: crate::config::Action) -> bool {
    use crate::config::Action;
    matches!(
        action,
        Action::Yank
            | Action::Cut
            | Action::Paste
            | Action::Trash
            | Action::DeletePermanently
            | Action::SymlinkAbsolute
            | Action::SymlinkRelative
            | Action::SelectAll
            | Action::CopyPath
            | Action::SearchFolders
            | Action::SearchFiles
            | Action::FilterDirectory
            | Action::FindDuplicates
            | Action::GoTo
            | Action::OpenWith
            | Action::Open
            | Action::Zoxide
            | Action::Shell
            | Action::Create
            | Action::Rename
            | Action::RenameInEditor
            | Action::CreateArchive
            | Action::ExtractArchive
            | Action::RestoreFromTrash
    )
}

fn should_handle_high_frequency_horizontal_key(
    key: KeyEvent,
    configured_action: Option<crate::config::Action>,
) -> bool {
    key.modifiers == KeyModifiers::ALT
        && matches!(
            (key.code, configured_action),
            (KeyCode::Left, Some(crate::config::Action::HistoryBack))
                | (KeyCode::Right, Some(crate::config::Action::HistoryForward))
        )
}

fn should_use_grid_zoom_for_symlink_key(
    app: &App,
    key: KeyEvent,
    configured_action: Option<crate::config::Action>,
) -> bool {
    app.navigation.view_mode == ViewMode::Grid
        && matches!(key.code, KeyCode::Char('-') | KeyCode::Char('_'))
        && matches!(
            configured_action,
            Some(crate::config::Action::SymlinkAbsolute | crate::config::Action::SymlinkRelative)
        )
        && !app
            .jobs
            .clipboard
            .as_ref()
            .is_some_and(|clipboard| clipboard.op == ClipOp::Yank && !clipboard.paths.is_empty())
}

fn is_cancel_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc)
        || key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c'))
}

fn is_help_shortcut(key: KeyEvent) -> bool {
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
    use crate::config::Action;

    #[test]
    fn high_frequency_horizontal_emulation_only_uses_default_history_actions() {
        assert!(should_handle_high_frequency_horizontal_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::ALT),
            Some(Action::HistoryBack),
        ));
        assert!(should_handle_high_frequency_horizontal_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::ALT),
            Some(Action::HistoryForward),
        ));
        assert!(!should_handle_high_frequency_horizontal_key(
            KeyEvent::new(KeyCode::Left, KeyModifiers::ALT),
            Some(Action::Open),
        ));
        assert!(!should_handle_high_frequency_horizontal_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::ALT),
            Some(Action::Open),
        ));
        assert!(!should_handle_high_frequency_horizontal_key(
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT),
            Some(Action::HistoryBack),
        ));
    }

    #[test]
    fn fullscreen_preview_explicit_commands_exit_before_dispatch() {
        for action in [
            Action::Yank,
            Action::Cut,
            Action::Paste,
            Action::Trash,
            Action::DeletePermanently,
            Action::SymlinkAbsolute,
            Action::SymlinkRelative,
            Action::SelectAll,
            Action::CopyPath,
            Action::SearchFolders,
            Action::SearchFiles,
            Action::FilterDirectory,
            Action::GoTo,
            Action::OpenWith,
            Action::Open,
            Action::Zoxide,
            Action::Shell,
            Action::Create,
            Action::Rename,
            Action::RenameInEditor,
            Action::CreateArchive,
            Action::ExtractArchive,
            Action::RestoreFromTrash,
        ] {
            assert!(
                fullscreen_preview_exits_then_dispatches(action),
                "{action:?} should be allowed from fullscreen preview"
            );
        }
    }

    #[test]
    fn fullscreen_preview_navigation_dispatches_before_exit() {
        for action in [
            Action::CyclePlacesNext,
            Action::CyclePlacesPrevious,
            Action::GoParent,
            Action::HistoryBack,
            Action::HistoryForward,
            Action::NavLeft,
            Action::NavRight,
        ] {
            assert!(
                fullscreen_preview_dispatches_then_exits(action),
                "{action:?} should dispatch before leaving fullscreen preview"
            );
        }
    }

    #[test]
    fn fullscreen_preview_same_directory_navigation_stays_fullscreen() {
        for action in [
            Action::ToggleSelection,
            Action::PageUp,
            Action::PageDown,
            Action::JumpFirst,
            Action::JumpLast,
            Action::NavDown,
            Action::NavUp,
        ] {
            assert!(
                fullscreen_preview_dispatches_and_stays(action),
                "{action:?} should dispatch without leaving fullscreen preview"
            );
        }
    }

    #[test]
    fn fullscreen_preview_keeps_hidden_browser_mutations_blocked() {
        for action in [Action::Sort, Action::ToggleView, Action::ToggleHidden] {
            assert!(
                !fullscreen_preview_exits_then_dispatches(action),
                "{action:?} should stay blocked from fullscreen preview"
            );
        }
    }
}
