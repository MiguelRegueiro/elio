use std::path::PathBuf;

use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewMode {
    Grid,
    List,
}

impl ViewMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Grid => Self::List,
            Self::List => Self::Grid,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => "Grid",
            Self::List => "List",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClipOp {
    Yank,
    Cut,
}

#[derive(Clone, Debug, Default)]
pub struct FrameState {
    pub sidebar_hits: Vec<PathHit>,
    pub entry_hits: Vec<EntryHit>,
    pub search_hits: Vec<SearchHit>,
    pub duplicate_hits: Vec<DuplicateHit>,
    pub goto_hits: Vec<GoToHit>,
    pub copy_hits: Vec<CopyHit>,
    pub open_with_hits: Vec<OpenWithHit>,
    pub trash_panel: Option<Rect>,
    pub trash_confirm_btn: Option<Rect>,
    pub trash_cancel_btn: Option<Rect>,
    pub editor_rename_confirm_btn: Option<Rect>,
    pub editor_rename_cancel_btn: Option<Rect>,
    pub restore_panel: Option<Rect>,
    pub restore_confirm_btn: Option<Rect>,
    pub restore_cancel_btn: Option<Rect>,
    pub archive_create_panel: Option<Rect>,
    pub archive_create_list_area: Option<Rect>,
    pub archive_password_panel: Option<Rect>,
    pub archive_password_visibility_btn: Option<Rect>,
    pub create_panel: Option<Rect>,
    pub rename_panel: Option<Rect>,
    pub create_list_area: Option<Rect>,
    pub create_scroll_top: usize,
    pub bulk_rename_list_area: Option<Rect>,
    pub bulk_rename_scroll_top: usize,
    pub goto_panel: Option<Rect>,
    pub copy_panel: Option<Rect>,
    pub open_with_panel: Option<Rect>,
    pub search_panel: Option<Rect>,
    pub duplicate_panel: Option<Rect>,
    pub help_panel: Option<Rect>,
    pub help_scroll_max: usize,
    pub help_rows_visible: usize,
    pub entries_panel: Option<Rect>,
    pub preview_panel: Option<Rect>,
    pub preview_body_area: Option<Rect>,
    pub preview_media_area: Option<Rect>,
    pub preview_content_area: Option<Rect>,
    pub back_button: Option<Rect>,
    pub forward_button: Option<Rect>,
    pub parent_button: Option<Rect>,
    pub hidden_button: Option<Rect>,
    pub view_button: Option<Rect>,
    pub metrics: ViewMetrics,
    pub preview_rows_visible: usize,
    pub preview_cols_visible: usize,
    pub search_rows_visible: usize,
    pub duplicate_rows_visible: usize,
}

#[derive(Clone, Debug)]
pub struct PathHit {
    pub rect: Rect,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct EntryHit {
    pub rect: Rect,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub rect: Rect,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct DuplicateHit {
    pub rect: Rect,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct GoToHit {
    pub rect: Rect,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct CopyHit {
    pub rect: Rect,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct OpenWithHit {
    pub rect: Rect,
    pub index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ViewMetrics {
    pub cols: usize,
    pub rows_visible: usize,
}

impl Default for ViewMetrics {
    fn default() -> Self {
        Self {
            cols: 1,
            rows_visible: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SearchScope {
    Folders,
    Files,
}

impl SearchScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Folders => "Folders",
            Self::Files => "Files",
        }
    }

    pub(super) fn candidate_scope(self) -> crate::fs::search::SearchCandidateScope {
        match self {
            Self::Folders => crate::fs::search::SearchCandidateScope::Folders,
            Self::Files => crate::fs::search::SearchCandidateScope::Files,
        }
    }

    pub fn empty_label(self) -> &'static str {
        match self {
            Self::Folders => "No matching folders in this tree",
            Self::Files => "No matching files in this tree",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchRow {
    pub index: usize,
    pub path: std::path::PathBuf,
    pub name: String,
    pub relative: String,
    pub is_dir: bool,
    pub symlink: Option<crate::core::SymlinkInfo>,
    pub selected: bool,
}
