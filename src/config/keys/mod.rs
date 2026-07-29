mod defaults;
mod model;
mod parse;
mod validate;

pub(crate) use self::model::{
    Action, ChooserKeyAction, KeyBindings, KeyContext, KeyList, normalized_plain_key_char,
};
pub(super) use self::parse::KeysConfigOverride;
use self::validate::resolve_key_overrides;
use crossterm::event::KeyEvent;
#[cfg(test)]
use crossterm::event::{KeyCode, KeyModifiers};

#[cfg(test)]
pub(super) fn unknown_key_action_warning(key: &str) -> String {
    validate::unknown_key_action_warning(key)
}

impl KeyBindings {
    /// Returns the normal-browser action bound to `key`, if any.
    pub(crate) fn action_for_key(&self, key: KeyEvent) -> Option<Action> {
        self.action_for_key_in_context(key, KeyContext::Normal)
    }

    /// Returns the action bound to `key` in the active browser context, if any.
    pub(crate) fn action_for_key_in_context(
        &self,
        key: KeyEvent,
        context: KeyContext,
    ) -> Option<Action> {
        self.bindings().iter().find_map(|(keys, action)| {
            action
                .key_contexts()
                .contains(context)
                .then_some(())
                .filter(|_| keys.matches_event(key))
                .map(|_| *action)
        })
    }

    pub(crate) fn chooser_action_for_key(
        &self,
        key: KeyEvent,
        context: KeyContext,
    ) -> Option<ChooserKeyAction> {
        if self.choose.matches_event(key) {
            return Some(ChooserKeyAction::Choose);
        }
        if self.quit.matches_event(key) || self.quit_without_cd.matches_event(key) {
            return Some(ChooserKeyAction::Cancel);
        }
        self.action_for_key_in_context(key, context)
            .map(ChooserKeyAction::Normal)
    }

    pub(crate) fn open_with_reserved_shortcuts(&self) -> Vec<char> {
        self.nav_down
            .single_chars()
            .chain(self.nav_up.single_chars())
            .chain(self.open_or_enter.single_chars())
            .collect()
    }

    fn bindings(&self) -> [(&KeyList, Action); 50] {
        [
            (&self.quit, Action::Quit),
            (&self.quit_without_cd, Action::QuitWithoutCd),
            (&self.yank, Action::Yank),
            (&self.cut, Action::Cut),
            (&self.paste, Action::Paste),
            (&self.create_archive, Action::CreateArchive),
            (&self.extract_archive, Action::ExtractArchive),
            (&self.symlink_absolute, Action::SymlinkAbsolute),
            (&self.symlink_relative, Action::SymlinkRelative),
            (&self.trash, Action::Trash),
            (&self.delete_permanently, Action::DeletePermanently),
            (&self.create, Action::Create),
            (&self.rename, Action::Rename),
            (&self.rename_in_editor, Action::RenameInEditor),
            (&self.restore_from_trash, Action::RestoreFromTrash),
            (&self.copy_path, Action::CopyPath),
            (&self.search_folders, Action::SearchFolders),
            (&self.search_files, Action::SearchFiles),
            (&self.filter_directory, Action::FilterDirectory),
            (&self.find_duplicates, Action::FindDuplicates),
            (&self.zoxide, Action::Zoxide),
            (&self.shell, Action::Shell),
            (&self.open, Action::Open),
            (&self.open_with, Action::OpenWith),
            (&self.open_or_enter, Action::OpenOrEnter),
            (&self.go_to, Action::GoTo),
            (&self.toggle_selection, Action::ToggleSelection),
            (&self.cycle_places_next, Action::CyclePlacesNext),
            (&self.cycle_places_previous, Action::CyclePlacesPrevious),
            (&self.go_parent, Action::GoParent),
            (&self.page_up, Action::PageUp),
            (&self.page_down, Action::PageDown),
            (&self.jump_first, Action::JumpFirst),
            (&self.jump_last, Action::JumpLast),
            (&self.select_all, Action::SelectAll),
            (&self.history_back, Action::HistoryBack),
            (&self.history_forward, Action::HistoryForward),
            (&self.sort, Action::Sort),
            (&self.toggle_view, Action::ToggleView),
            (&self.toggle_preview, Action::TogglePreview),
            (&self.fullscreen_preview, Action::FullscreenPreview),
            (&self.toggle_hidden, Action::ToggleHidden),
            (&self.nav_left, Action::NavLeft),
            (&self.nav_down, Action::NavDown),
            (&self.nav_up, Action::NavUp),
            (&self.nav_right, Action::NavRight),
            (&self.scroll_preview_left, Action::ScrollPreviewLeft),
            (&self.scroll_preview_right, Action::ScrollPreviewRight),
            (&self.scroll_preview_up, Action::ScrollPreviewUp),
            (&self.scroll_preview_down, Action::ScrollPreviewDown),
        ]
    }

    /// Returns the action bound to `c`, if any.
    #[cfg(test)]
    pub(crate) fn action_for(&self, c: char) -> Option<Action> {
        self.action_for_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    /// Parse a full config TOML string and return only the resolved key
    /// bindings. Falls back to defaults on parse error. Used by integration
    /// tests that need a `KeyBindings` from an override string without going
    /// through the process-wide `OnceLock`.
    #[cfg(test)]
    pub(crate) fn from_toml_str(s: &str) -> Self {
        super::Config::from_str(s)
            .map(|config| config.keys)
            .unwrap_or_else(|_| Self::default())
    }
}

impl KeyBindings {
    pub(super) fn from_override(overrides: KeysConfigOverride, defaults: &Self) -> Self {
        resolve_key_overrides(overrides, defaults)
    }
}
