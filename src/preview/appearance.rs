use crate::core::{Entry, EntryKind, FileClass};
use ratatui::style::Color;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Palette {
    pub bg: Color,
    pub chrome: Color,
    pub chrome_alt: Color,
    pub chip_text: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub surface: Color,
    pub elevated: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub accent_text: Color,
    pub selected_bg: Color,
    pub selected_border: Color,
    pub selection_bar: Color,
    pub yank_bar: Color,
    pub cut_bar: Color,
    pub grid_selection_band: Color,
    pub grid_yank_band: Color,
    pub grid_cut_band: Color,
    pub trash_bar: Color,
    pub restore_bar: Color,
    pub sidebar_active: Color,
    pub button_bg: Color,
    pub button_disabled_bg: Color,
    pub path_bg: Color,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CodePalette {
    pub fg: Color,
    pub bg: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub caret: Color,
    pub line_highlight: Color,
    pub line_number: Color,
    pub comment: Color,
    pub string: Color,
    pub constant: Color,
    pub keyword: Color,
    pub function: Color,
    pub r#type: Color,
    pub parameter: Color,
    pub tag: Color,
    pub operator: Color,
    pub r#macro: Color,
    pub invalid: Color,
}

pub(crate) type CodePreviewPalette = CodePalette;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PathAppearance<'a> {
    pub icon: &'a str,
    pub color: Color,
}

pub(crate) fn palette() -> Palette {
    let palette = crate::ui::theme::palette();
    Palette {
        bg: palette.bg,
        chrome: palette.chrome,
        chrome_alt: palette.chrome_alt,
        chip_text: palette.chip_text,
        panel: palette.panel,
        panel_alt: palette.panel_alt,
        surface: palette.surface,
        elevated: palette.elevated,
        border: palette.border,
        text: palette.text,
        muted: palette.muted,
        accent: palette.accent,
        accent_soft: palette.accent_soft,
        accent_text: palette.accent_text,
        selected_bg: palette.selected_bg,
        selected_border: palette.selected_border,
        selection_bar: palette.selection_bar,
        yank_bar: palette.yank_bar,
        cut_bar: palette.cut_bar,
        grid_selection_band: palette.grid_selection_band,
        grid_yank_band: palette.grid_yank_band,
        grid_cut_band: palette.grid_cut_band,
        trash_bar: palette.trash_bar,
        restore_bar: palette.restore_bar,
        sidebar_active: palette.sidebar_active,
        button_bg: palette.button_bg,
        button_disabled_bg: palette.button_disabled_bg,
        path_bg: palette.path_bg,
    }
}

pub(crate) fn code_palette() -> CodePalette {
    let palette = crate::ui::theme::code_preview_palette();
    CodePalette {
        fg: palette.fg,
        bg: palette.bg,
        selection_bg: palette.selection_bg,
        selection_fg: palette.selection_fg,
        caret: palette.caret,
        line_highlight: palette.line_highlight,
        line_number: palette.line_number,
        comment: palette.comment,
        string: palette.string,
        constant: palette.constant,
        keyword: palette.keyword,
        function: palette.function,
        r#type: palette.r#type,
        parameter: palette.parameter,
        tag: palette.tag,
        operator: palette.operator,
        r#macro: palette.r#macro,
        invalid: palette.invalid,
    }
}

pub(crate) fn code_preview_palette() -> CodePreviewPalette {
    code_palette()
}

pub(crate) fn resolve_path(path: &Path, kind: EntryKind) -> PathAppearance<'static> {
    let appearance = crate::ui::theme::resolve_path(path, kind);
    PathAppearance {
        icon: appearance.icon,
        color: appearance.color,
    }
}

pub(crate) fn resolve_path_with_class(
    path: &Path,
    kind: EntryKind,
    class: FileClass,
) -> PathAppearance<'static> {
    let appearance = crate::ui::theme::resolve_path_with_class(path, kind, class);
    PathAppearance {
        icon: appearance.icon,
        color: appearance.color,
    }
}

pub(crate) fn resolve_entry(entry: &Entry) -> PathAppearance<'static> {
    let appearance = crate::ui::theme::resolve_entry(entry);
    PathAppearance {
        icon: appearance.icon,
        color: appearance.color,
    }
}
