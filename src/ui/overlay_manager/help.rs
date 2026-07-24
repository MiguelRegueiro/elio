use super::{HelpMode, scrollbar::render_overlay_scrollbar};
use crate::app::FrameState;
use crate::config::{KeyBindings, KeyList};
use crate::ui::{helpers, theme::Palette};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub(super) fn render_help(
    frame: &mut Frame<'_>,
    area: Rect,
    mode: HelpMode,
    scroll_top: usize,
    state: &mut FrameState,
    palette: Palette,
) {
    let kb = crate::config::keys();
    let keys = HelpKeys::new(kb, mode);

    let navigation_entries = navigation_entries(&keys);
    let search_entries = entries([
        keys.action(&kb.zoxide, "zoxide history"),
        keys.action(&kb.search_folders, "search folders"),
        keys.action(&kb.search_files, "search files"),
        e("Ctrl+←→", "move by word"),
        e("Ctrl+Backspace", "delete previous word"),
        e("Ctrl+Del", "delete next word"),
        e("Ctrl+W / Alt+D", "fallback word delete"),
    ]);
    let clipboard_entries = clipboard_entries(&keys);
    let files_entries = entries([
        keys.action(&kb.create, "create file or folder"),
        e("/… or …/", "folder in create prompt"),
        e("Alt/Shift+Enter", "add line in create prompt"),
        keys.action(&kb.trash, "trash (delete if in trash)"),
        keys.action(&kb.delete_permanently, "delete permanently"),
        keys.action(&kb.rename, "rename (bulk if selection)"),
        keys.action(&kb.rename_in_editor, "rename in editor"),
        keys.action_with_suffix(&kb.restore_from_trash, " (in trash)", "restore from trash"),
        keys.action(&kb.extract_archive, "extract archive"),
        keys.action(&kb.create_archive, "create archive"),
        keys.action(&kb.shell, "open shell here"),
        keys.action(&kb.open, "open with default app"),
        keys.action(&kb.open_with, "open with"),
    ]);
    let quit_action = if mode.is_chooser() {
        "cancel chooser"
    } else {
        "quit"
    };
    let quit_without_cd_action = if mode.is_chooser() {
        "cancel chooser"
    } else {
        "quit without cd"
    };
    let view_entries = entries([
        keys.action(&kb.toggle_view, "toggle grid / list"),
        e("Ctrl++ / Ctrl+-", "grid zoom in / out"),
        keys.action(&kb.toggle_hidden, "toggle dotfiles"),
        keys.action(&kb.sort, "cycle sort"),
        keys.action(&kb.quit, quit_action),
        keys.action(&kb.quit_without_cd, quit_without_cd_action),
    ]);
    let preview_entries = entries([
        keys.preview_action(
            &kb.scroll_preview_up,
            &kb.scroll_preview_down,
            "step page or scroll",
        ),
        keys.preview_action(
            &kb.scroll_preview_left,
            &kb.scroll_preview_right,
            "scroll left / right",
        ),
    ]);
    let mouse_entries = vec![
        e("Click", "select item"),
        e(
            "Double-click",
            if mode.is_chooser() {
                "confirm clicked item"
            } else {
                "open item"
            },
        ),
        e("Wheel", "scroll"),
        e("Shift+Wheel", "scroll sideways"),
    ];
    let sections = vec![
        HelpSection {
            title: "Navigate",
            entries: navigation_entries,
        },
        HelpSection {
            title: "Mouse",
            entries: mouse_entries,
        },
        HelpSection {
            title: "Selection & Clipboard",
            entries: clipboard_entries,
        },
        HelpSection {
            title: "Search",
            entries: search_entries,
        },
        HelpSection {
            title: "File Actions",
            entries: files_entries,
        },
        HelpSection {
            title: "Preview",
            entries: preview_entries,
        },
        HelpSection {
            title: "View",
            entries: view_entries,
        },
    ];

    let popup_width = if area.width >= 92 {
        90
    } else {
        area.width.saturating_sub(4).clamp(44, 90)
    };
    let popup_height = area.height.saturating_sub(3).clamp(9, 37);
    let popup = helpers::centered_rect(area, popup_width, popup_height);
    state.help_panel = Some(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::new()
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "󰘳",
                    Style::default()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    mode.title(),
                    Style::default()
                        .fg(palette.accent_text)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(palette.chrome_alt).fg(palette.text))
            .border_style(Style::default().fg(palette.border)),
        popup,
    );

    let inner = helpers::inner_with_padding(popup);
    if inner.width >= 88 && inner.height >= 33 {
        render_wide_help(frame, inner, &sections, state, palette);
    } else {
        render_compact_help(frame, inner, &sections, scroll_top, state, palette);
    }
}

fn render_wide_help(
    frame: &mut Frame<'_>,
    body: Rect,
    sections: &[HelpSection],
    state: &mut FrameState,
    palette: Palette,
) {
    state.help_scroll_max = 0;
    state.help_rows_visible = body.height as usize;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(39),
            Constraint::Length(3),
            Constraint::Length(46),
        ])
        .split(body);

    frame.render_widget(
        Paragraph::new(help_column_lines(cols[0].width, &sections[..3], palette))
            .style(Style::default().bg(palette.chrome_alt).fg(palette.text))
            .wrap(Wrap { trim: false }),
        cols[0],
    );

    let divider_lines: Vec<Line<'static>> =
        vec![
            Line::from(Span::styled(" │ ", Style::default().fg(palette.border)));
            cols[1].height as usize
        ];
    frame.render_widget(
        Paragraph::new(divider_lines).style(Style::default().bg(palette.chrome_alt)),
        cols[1],
    );

    frame.render_widget(
        Paragraph::new(help_column_lines(cols[2].width, &sections[3..], palette))
            .style(Style::default().bg(palette.chrome_alt).fg(palette.text))
            .wrap(Wrap { trim: false }),
        cols[2],
    );
}

fn render_compact_help(
    frame: &mut Frame<'_>,
    body: Rect,
    sections: &[HelpSection],
    scroll_top: usize,
    state: &mut FrameState,
    palette: Palette,
) {
    let visible = body.height as usize;
    let mut content = body;
    let mut lines = flowing_help_lines(content.width, sections, palette);
    let mut total = lines.len();
    let mut scrollbar = None;

    if total > visible.max(1) && body.width >= 6 {
        content.width = body.width.saturating_sub(1);
        scrollbar = Some(Rect {
            x: body.x + content.width,
            y: body.y,
            width: 1,
            height: body.height,
        });
        lines = flowing_help_lines(content.width, sections, palette);
        total = lines.len();
    }

    let max_scroll = total.saturating_sub(visible);
    state.help_scroll_max = max_scroll;
    state.help_rows_visible = visible;
    let scroll_top = scroll_top.min(max_scroll);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(palette.chrome_alt).fg(palette.text))
            .scroll((scroll_top as u16, 0))
            .wrap(Wrap { trim: false }),
        content,
    );
    if let Some(area) = scrollbar {
        render_overlay_scrollbar(frame, area, total, visible, scroll_top, palette);
    }
}

fn flowing_help_lines(
    width: u16,
    sections: &[HelpSection],
    palette: Palette,
) -> Vec<Line<'static>> {
    if width >= 70 && sections.len() >= 4 {
        return two_column_help_lines(width, &sections[..3], &sections[3..], palette);
    }
    help_column_lines(width, sections, palette)
}

fn two_column_help_lines(
    width: u16,
    left_sections: &[HelpSection],
    right_sections: &[HelpSection],
    palette: Palette,
) -> Vec<Line<'static>> {
    let gap_width = 3usize;
    let left_width = ((width as usize).saturating_sub(gap_width)) / 2;
    let right_width = (width as usize).saturating_sub(left_width + gap_width);
    let left = help_column_lines(left_width as u16, left_sections, palette);
    let right = help_column_lines(right_width as u16, right_sections, palette);
    let rows = left.len().max(right.len());
    let mut lines = Vec::with_capacity(rows);

    for index in 0..rows {
        let left_line = left.get(index).cloned().unwrap_or_default();
        let right_line = right.get(index).cloned().unwrap_or_default();
        let left_line_width = line_width(&left_line);
        let mut spans = left_line.spans;
        spans.push(Span::raw(
            " ".repeat(left_width.saturating_sub(left_line_width) + gap_width),
        ));
        spans.extend(right_line.spans);
        lines.push(Line::from(spans));
    }

    lines
}

fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

impl HelpMode {
    fn is_chooser(self) -> bool {
        matches!(self, Self::Chooser)
    }

    fn title(self) -> &'static str {
        match self {
            Self::Normal => " Keyboard and mouse controls ",
            Self::Chooser => " Chooser controls ",
        }
    }
}

struct HelpKeys<'a> {
    kb: &'a KeyBindings,
    mode: HelpMode,
}

impl<'a> HelpKeys<'a> {
    fn new(kb: &'a KeyBindings, mode: HelpMode) -> Self {
        Self { kb, mode }
    }

    fn action(&self, keys: &KeyList, action: &'static str) -> HelpEntry {
        self.entry(self.key(keys), action)
    }

    fn action_with_suffix(
        &self,
        keys: &KeyList,
        suffix: &'static str,
        action: &'static str,
    ) -> HelpEntry {
        let mut key = self.key(keys);
        if !key.is_empty() {
            key.push_str(suffix);
        }
        self.entry(key, action)
    }

    fn pair_action(&self, first: &KeyList, second: &KeyList, action: &'static str) -> HelpEntry {
        self.entry(self.pair(first, second), action)
    }

    fn preview_action(&self, low: &KeyList, high: &KeyList, action: &'static str) -> HelpEntry {
        self.entry(self.preview_pair(low, high), action)
    }

    fn key(&self, keys: &KeyList) -> String {
        self.effective_keys(keys).to_string()
    }

    fn pair(&self, first: &KeyList, second: &KeyList) -> String {
        format_key_pair(&self.effective_keys(first), &self.effective_keys(second))
    }

    fn preview_pair(&self, low: &KeyList, high: &KeyList) -> String {
        let low = self.effective_keys(low);
        let high = self.effective_keys(high);
        format_key_pair(&low, &high)
    }

    fn effective_keys(&self, keys: &KeyList) -> KeyList {
        if self.mode.is_chooser() {
            keys.without(&self.kb.choose)
        } else {
            keys.clone()
        }
    }

    fn entry(&self, key: String, action: &'static str) -> HelpEntry {
        entry(key, action)
    }
}

fn navigation_entries(keys: &HelpKeys<'_>) -> Vec<HelpEntry> {
    let kb = keys.kb;
    entries([
        keys.action(&kb.nav_up, "move up"),
        keys.action(&kb.nav_down, "move down"),
        keys.pair_action(&kb.nav_left, &kb.go_parent, "parent folder"),
        keys.action(&kb.nav_right, "enter folder"),
        keys.action(&kb.open_or_enter, "enter folder / open"),
        keys.action(&kb.go_to, "go-to menu"),
        keys.action(&kb.filter_directory, "filter current dir"),
        keys.action(&kb.jump_first, "first item"),
        keys.action(&kb.jump_last, "last item"),
        keys.pair_action(&kb.page_up, &kb.page_down, "page up / down"),
        keys.pair_action(
            &kb.cycle_places_next,
            &kb.cycle_places_previous,
            "cycle places",
        ),
        keys.pair_action(&kb.history_back, &kb.history_forward, "back / forward"),
    ])
}

fn clipboard_entries(keys: &HelpKeys<'_>) -> Vec<HelpEntry> {
    let kb = keys.kb;
    let mut entries = Vec::new();
    if keys.mode.is_chooser() {
        entries.push(e(&kb.choose.to_string(), "confirm selection"));
    }
    entries.extend([
        keys.action(&kb.toggle_selection, "toggle selection"),
        keys.action(&kb.select_all, "select all"),
        e("Esc", "clear selection"),
        keys.action(&kb.yank, "yank (copy)"),
        keys.action(&kb.copy_path, "copy path details"),
        keys.action(&kb.cut, "cut"),
        keys.action(&kb.paste, "paste"),
        keys.action(&kb.symlink_absolute, "symlink absolute"),
        keys.action(&kb.symlink_relative, "symlink relative"),
    ]);
    entries
}

fn format_key_pair(first: &crate::config::KeyList, second: &crate::config::KeyList) -> String {
    match (first.to_string(), second.to_string()) {
        (first, second) if first.is_empty() => second,
        (first, second) if second.is_empty() => first,
        (first, second) => format!("{first} / {second}"),
    }
}

struct HelpEntry {
    key: String,
    action: &'static str,
}

/// Convenience constructor — accepts anything that converts to a `String` for
/// the key so call sites can pass `&str`, `String`, or `&String` uniformly.
fn e(key: &str, action: &'static str) -> HelpEntry {
    entry(key.to_string(), action)
}

fn entry(key: String, action: &'static str) -> HelpEntry {
    HelpEntry { key, action }
}

fn entries(items: impl IntoIterator<Item = HelpEntry>) -> Vec<HelpEntry> {
    items.into_iter().collect()
}

struct HelpSection {
    title: &'static str,
    entries: Vec<HelpEntry>,
}

fn help_column_lines(width: u16, sections: &[HelpSection], palette: Palette) -> Vec<Line<'static>> {
    let content_width = width.max(1) as usize;
    let max_key_width = sections
        .iter()
        .flat_map(|section| section.entries.iter())
        .map(|entry| UnicodeWidthStr::width(entry.key.as_str()))
        .max()
        .unwrap_or(0);
    let gap_width = 2usize;
    let mut key_width = max_key_width.min(17);
    let min_action_width = 14usize.min(content_width.saturating_sub(gap_width + 1));
    if key_width + gap_width + min_action_width > content_width {
        key_width = content_width.saturating_sub(gap_width + min_action_width);
    }
    key_width = key_width
        .max(4)
        .min(content_width.saturating_sub(gap_width + 1));
    let action_width = content_width.saturating_sub(key_width + gap_width).max(1);

    let mut lines = Vec::new();
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        lines.push(help_section_title(section.title, palette));
        for entry in &section.entries {
            lines.extend(help_entry_lines(entry, key_width, action_width, palette));
        }
    }
    lines
}

fn help_section_title(title: &str, palette: Palette) -> Line<'static> {
    Line::from(vec![Span::styled(
        title.to_string(),
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    )])
}

fn help_entry_lines(
    entry: &HelpEntry,
    key_width: usize,
    action_width: usize,
    palette: Palette,
) -> Vec<Line<'static>> {
    let mut wrapped_action = wrap_help_action(entry.action, action_width);
    if wrapped_action.is_empty() {
        wrapped_action.push(String::new());
    }

    let key = helpers::clamp_label(&entry.key, key_width);
    let key_padding = " ".repeat(key_width.saturating_sub(UnicodeWidthStr::width(key.as_str())));
    let continuation = " ".repeat(key_width + 2);
    let mut lines = Vec::with_capacity(wrapped_action.len());

    let key_style = Style::default()
        .fg(palette.accent_text)
        .add_modifier(Modifier::BOLD);
    let separator_style = Style::default().fg(palette.muted);
    let key_len = key.chars().count();
    let mut spans: Vec<_> = key
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let style = if ch == '/' && i > 0 && i + 1 < key_len {
                separator_style
            } else {
                key_style
            };
            Span::styled(ch.to_string(), style)
        })
        .collect();
    spans.extend([
        Span::raw(key_padding),
        Span::raw("  "),
        Span::styled(wrapped_action.remove(0), Style::default().fg(palette.muted)),
    ]);
    lines.push(Line::from(spans));

    for line in wrapped_action {
        lines.push(Line::from(vec![
            Span::raw(continuation.clone()),
            Span::styled(line, Style::default().fg(palette.muted)),
        ]));
    }

    lines
}

fn wrap_help_action(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() || width == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        let separator_width = usize::from(!current.is_empty());
        if !current.is_empty() && current_width + separator_width + word_width > width {
            lines.push(current);
            current = word.to_string();
            current_width = word_width;
            continue;
        }

        if !current.is_empty() {
            current.push(' ');
            current_width += 1;
        }
        current.push_str(word);
        current_width += word_width;
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_key(entries: &[HelpEntry], action: &str) -> String {
        entries
            .iter()
            .find(|entry| entry.action == action)
            .unwrap_or_else(|| panic!("missing help entry for {action:?}"))
            .key
            .clone()
    }

    fn entry_keys(entries: &[HelpEntry], action: &str) -> Vec<String> {
        entries
            .iter()
            .filter(|entry| entry.action == action)
            .map(|entry| entry.key.clone())
            .collect()
    }

    fn has_action(entries: &[HelpEntry], action: &str) -> bool {
        entries.iter().any(|entry| entry.action == action)
    }

    #[test]
    fn help_uses_configurable_browser_control_defaults() {
        let kb = KeyBindings::default();
        let keys = HelpKeys::new(&kb, HelpMode::Normal);
        let navigation = navigation_entries(&keys);
        let clipboard = clipboard_entries(&keys);

        assert_eq!(entry_key(&navigation, "go-to menu"), "g");
        assert_eq!(entry_key(&navigation, "first item"), "Home");
        assert_eq!(entry_key(&navigation, "last item"), "G/End");
        assert_eq!(
            entry_key(&navigation, "page up / down"),
            "PageUp / PageDown"
        );
        assert_eq!(entry_key(&navigation, "cycle places"), "Tab / Shift+Tab");
        assert_eq!(entry_key(&navigation, "back / forward"), "Alt+← / Alt+→");
        assert_eq!(entry_key(&clipboard, "toggle selection"), "Space");
        assert_eq!(entry_key(&clipboard, "select all"), "Ctrl+A");
        assert!(
            !has_action(&clipboard, "confirm selection"),
            "normal help should not show chooser-only actions"
        );
    }

    #[test]
    fn help_reflects_rebound_browser_controls() {
        let kb = KeyBindings::from_toml_str(
            r#"[keys]
go_to = "u"
toggle_selection = "t"
cycle_places_next = "n"
cycle_places_previous = "P"
page_up = "<"
page_down = ">"
jump_first = "1"
jump_last = "2"
select_all = "A"
history_back = "alt+h"
history_forward = "alt+l"
"#,
        );
        let keys = HelpKeys::new(&kb, HelpMode::Normal);
        let navigation = navigation_entries(&keys);
        let clipboard = clipboard_entries(&keys);

        assert_eq!(entry_key(&navigation, "go-to menu"), "u");
        assert_eq!(entry_key(&navigation, "first item"), "1");
        assert_eq!(entry_key(&navigation, "last item"), "2");
        assert_eq!(entry_key(&navigation, "page up / down"), "< / >");
        assert_eq!(entry_key(&navigation, "cycle places"), "n / P");
        assert_eq!(entry_key(&navigation, "back / forward"), "Alt+H / Alt+L");
        assert_eq!(entry_key(&clipboard, "toggle selection"), "t");
        assert_eq!(entry_key(&clipboard, "select all"), "A");
    }

    #[test]
    fn chooser_help_adds_choose_and_relabels_quit() {
        let kb = KeyBindings::default();
        let keys = HelpKeys::new(&kb, HelpMode::Chooser);
        let clipboard = clipboard_entries(&keys);
        let view_entries = entries([
            keys.action(&kb.quit, "cancel chooser"),
            keys.action(&kb.quit_without_cd, "cancel chooser"),
        ]);

        assert_eq!(entry_key(&clipboard, "confirm selection"), "Enter");
        assert_eq!(entry_key(&view_entries, "cancel chooser"), "q");
        assert!(
            !has_action(&view_entries, "quit"),
            "chooser help should describe quit keys as chooser cancellation"
        );
    }

    #[test]
    fn chooser_help_removes_choose_key_from_normal_action_labels() {
        let kb = KeyBindings::from_toml_str(
            r#"[keys]
nav_right = []
open_or_enter = ["enter", "l", "right"]
"#,
        );

        let keys = HelpKeys::new(&kb, HelpMode::Chooser);
        let navigation = navigation_entries(&keys);
        let clipboard = clipboard_entries(&keys);

        assert_eq!(entry_key(&clipboard, "confirm selection"), "Enter");
        assert_eq!(entry_key(&navigation, "enter folder / open"), "l/→");
    }

    #[test]
    fn chooser_help_gives_choose_precedence_over_quit() {
        let kb = KeyBindings::from_toml_str(
            r#"[keys]
choose = "q"
"#,
        );
        let keys = HelpKeys::new(&kb, HelpMode::Chooser);
        let clipboard = clipboard_entries(&keys);
        let view_entries = entries([
            keys.action(&kb.quit, "cancel chooser"),
            keys.action(&kb.quit_without_cd, "cancel chooser"),
        ]);

        assert_eq!(entry_key(&clipboard, "confirm selection"), "q");
        assert_eq!(
            entry_keys(&view_entries, "cancel chooser"),
            vec![String::new(), "Q".to_string()]
        );
    }

    #[test]
    fn preview_scroll_help_drops_empty_sides() {
        let kb = KeyBindings::from_toml_str(
            r#"[keys]
choose = "K"
"#,
        );
        let keys = HelpKeys::new(&kb, HelpMode::Chooser);

        assert_eq!(
            keys.preview_pair(&kb.scroll_preview_up, &kb.scroll_preview_down),
            "[ / J/]"
        );

        let kb = KeyBindings::from_toml_str(
            r#"[keys]
choose = ["K", "["]
"#,
        );
        let keys = HelpKeys::new(&kb, HelpMode::Chooser);

        assert_eq!(
            keys.preview_pair(&kb.scroll_preview_up, &kb.scroll_preview_down),
            "J/]"
        );

        let kb = KeyBindings::from_toml_str(
            r#"[keys]
choose = ["K", "[", "J", "]"]
"#,
        );
        let keys = HelpKeys::new(&kb, HelpMode::Chooser);

        assert_eq!(
            keys.preview_pair(&kb.scroll_preview_up, &kb.scroll_preview_down),
            ""
        );
    }

    #[test]
    fn preview_scroll_help_uses_standard_uppercase_notation() {
        let kb = KeyBindings::default();
        let keys = HelpKeys::new(&kb, HelpMode::Normal);

        assert_eq!(
            keys.preview_pair(&kb.scroll_preview_up, &kb.scroll_preview_down),
            "K/[ / J/]"
        );
        assert_eq!(
            keys.preview_pair(&kb.scroll_preview_left, &kb.scroll_preview_right),
            "H / L"
        );

        let kb = KeyBindings::from_toml_str(
            r#"[keys]
scroll_preview_left = "A"
scroll_preview_right = "B"
"#,
        );
        let keys = HelpKeys::new(&kb, HelpMode::Normal);

        assert_eq!(
            keys.preview_pair(&kb.scroll_preview_left, &kb.scroll_preview_right),
            "A / B"
        );
    }
}
