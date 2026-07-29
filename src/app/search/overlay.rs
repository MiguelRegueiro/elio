use super::super::*;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::{collections::HashMap, ffi::OsStr, path::PathBuf, sync::Arc};

impl App {
    pub fn search_is_open(&self) -> bool {
        self.overlays.search.is_some()
    }

    pub fn search_query(&self) -> &str {
        self.overlays
            .search
            .as_ref()
            .map(|search| search.query.as_str())
            .unwrap_or("")
    }

    pub fn search_match_count(&self) -> usize {
        self.overlays
            .search
            .as_ref()
            .map(|search| {
                let query_key = super::search_cache_key(&search.query);
                search
                    .cached_matches
                    .get(&query_key)
                    .map(|entry| entry.pool.len())
                    .unwrap_or(search.matches.len())
            })
            .unwrap_or(0)
    }

    pub fn search_candidate_count(&self) -> usize {
        self.overlays
            .search
            .as_ref()
            .and_then(|search| search.cached_matches.get("").map(|entry| entry.pool.len()))
            .unwrap_or(0)
    }

    pub fn search_scanned_count(&self) -> usize {
        let candidate_count = self.search_candidate_count();
        self.overlays
            .search
            .as_ref()
            .map(|search| search.stats.visited_nodes.max(candidate_count))
            .unwrap_or(0)
    }

    pub fn search_index_is_limited(&self) -> bool {
        self.overlays
            .search
            .as_ref()
            .is_some_and(|search| search.stats.is_limited())
    }

    pub fn search_scope(&self) -> Option<SearchScope> {
        self.overlays.search.as_ref().map(|search| search.scope)
    }

    pub fn search_is_loading(&self) -> bool {
        self.overlays
            .search
            .as_ref()
            .is_some_and(|search| search.loading)
    }

    pub fn search_error(&self) -> Option<&str> {
        self.overlays
            .search
            .as_ref()
            .and_then(|search| search.error.as_deref())
    }

    pub fn search_rows(&self, max_rows: usize) -> Vec<SearchRow> {
        let Some(search) = &self.overlays.search else {
            return Vec::new();
        };

        let end = (search.scroll + max_rows).min(search.matches.len());
        (search.scroll..end)
            .filter_map(|visible_index| {
                let candidate_index = search.matches.get(visible_index).copied()?;
                let candidate = search.candidates.get(candidate_index)?;
                Some(SearchRow {
                    index: visible_index,
                    path: candidate.path.clone(),
                    name: candidate.name.clone(),
                    relative: candidate.relative.clone(),
                    is_dir: candidate.is_dir,
                    symlink: candidate.symlink.clone(),
                    selected: visible_index == search.selected,
                })
            })
            .collect()
    }

    pub fn search_scroll_top(&self) -> usize {
        self.overlays
            .search
            .as_ref()
            .map(|search| search.scroll)
            .unwrap_or(0)
    }

    pub(in crate::app) fn open_fuzzy_finder(&mut self, scope: SearchScope) -> Result<()> {
        self.clear_wheel_scroll();
        self.overlays.help = false;
        let show_hidden = self.effective_show_hidden();
        let cached = self
            .jobs
            .search_cache
            .as_ref()
            .filter(|cache| {
                cache.cwd == self.navigation.cwd
                    && cache.scope == scope
                    && cache.show_hidden == show_hidden
                    && cache.fingerprint == self.navigation.directory_runtime.fingerprint
            })
            .map(|cache| (cache.candidates.clone(), cache.stats));
        let (candidates, stats) = cached.clone().unwrap_or_else(|| {
            (
                Arc::new(Vec::new()),
                crate::fs::search::SearchIndexStats::default(),
            )
        });
        let base_matches = (0..candidates.len()).collect::<Vec<_>>();
        let matches = base_matches
            .iter()
            .copied()
            .take(SEARCH_MATCH_LIMIT)
            .collect::<Vec<_>>();
        let loading = cached.is_none();
        if cached.is_none() {
            self.prewarm_search_index(scope);
        }
        self.overlays.search = Some(SearchOverlay {
            scope,
            query: String::new(),
            query_cursor: 0,
            candidates,
            matches,
            cached_matches: HashMap::from([(
                String::new(),
                super::build_base_search_cache_entry(base_matches),
            )]),
            selected: 0,
            scroll: 0,
            loading,
            error: None,
            stats,
        });
        self.status.clear();
        Ok(())
    }

    fn close_search_overlay(&mut self) {
        self.overlays.search = None;
        self.jobs.search_loading = false;
        self.jobs.search_token = self.jobs.search_token.wrapping_add(1);
        self.jobs.scheduler.cancel_search();
        self.clear_wheel_scroll();
    }

    pub(in crate::app) fn handle_search_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.close_search_overlay();
            self.status.clear();
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.close_search_overlay();
                self.status.clear();
            }
            KeyCode::Enter => self.confirm_search_selection()?,
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_search_cursor_to_previous_word()
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_search_cursor_to_next_word()
            }
            KeyCode::Left => self.move_search_cursor(-1),
            KeyCode::Right => self.move_search_cursor(1),
            KeyCode::Up => self.move_search_selection(-1),
            KeyCode::Down => self.move_search_selection(1),
            KeyCode::PageUp => self.page_search(-1),
            KeyCode::PageDown => self.page_search(1),
            KeyCode::Home => self.move_search_cursor_to(0),
            KeyCode::End => self.move_search_cursor_to_end(),
            _ if search_key_deletes_previous_word(key) => {
                let previous_query = self
                    .overlays
                    .search
                    .as_ref()
                    .map(|search| search.query.clone())
                    .unwrap_or_default();
                if let Some(search) = &mut self.overlays.search {
                    remove_word_before_cursor(&mut search.query, &mut search.query_cursor);
                }
                self.refresh_search_matches(&previous_query);
            }
            KeyCode::Backspace => {
                let previous_query = self
                    .overlays
                    .search
                    .as_ref()
                    .map(|search| search.query.clone())
                    .unwrap_or_default();
                if let Some(search) = &mut self.overlays.search {
                    remove_char_before_cursor(&mut search.query, &mut search.query_cursor);
                }
                self.refresh_search_matches(&previous_query);
            }
            _ if search_key_deletes_next_word(key) => {
                let previous_query = self
                    .overlays
                    .search
                    .as_ref()
                    .map(|search| search.query.clone())
                    .unwrap_or_default();
                if let Some(search) = &mut self.overlays.search {
                    remove_word_at_cursor(&mut search.query, search.query_cursor);
                }
                self.refresh_search_matches(&previous_query);
            }
            KeyCode::Delete => {
                let previous_query = self
                    .overlays
                    .search
                    .as_ref()
                    .map(|search| search.query.clone())
                    .unwrap_or_default();
                if let Some(search) = &mut self.overlays.search {
                    remove_char_at_cursor(&mut search.query, search.query_cursor);
                }
                self.refresh_search_matches(&previous_query);
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let previous_query = self
                    .overlays
                    .search
                    .as_ref()
                    .map(|search| search.query.clone())
                    .unwrap_or_default();
                if let Some(search) = &mut self.overlays.search {
                    insert_char_at_cursor(&mut search.query, &mut search.query_cursor, ch);
                }
                self.refresh_search_matches(&previous_query);
            }
            _ => {}
        }
        Ok(())
    }

    pub(in crate::app) fn handle_search_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(hit) = self
                    .input
                    .frame_state
                    .search_hits
                    .iter()
                    .find(|hit| rect_contains(hit.rect, mouse.column, mouse.row))
                    .cloned()
                {
                    self.select_search_index(hit.index);
                    self.confirm_search_selection()?;
                } else if self
                    .input
                    .frame_state
                    .search_panel
                    .is_none_or(|rect| !rect_contains(rect, mouse.column, mouse.row))
                {
                    self.close_search_overlay();
                    self.status.clear();
                }
            }
            MouseEventKind::ScrollDown => self.queue_search_wheel(1),
            MouseEventKind::ScrollUp => self.queue_search_wheel(-1),
            _ => {}
        }
        Ok(())
    }

    pub(in crate::app) fn move_search_selection(&mut self, delta: isize) {
        let Some(search) = &mut self.overlays.search else {
            return;
        };
        if search.matches.is_empty() {
            search.selected = 0;
            search.scroll = 0;
            return;
        }

        let max_index = search.matches.len().saturating_sub(1) as isize;
        search.selected = (search.selected as isize + delta).clamp(0, max_index) as usize;
        self.sync_search_scroll();
    }

    pub fn search_query_cursor(&self) -> usize {
        self.overlays
            .search
            .as_ref()
            .map(|search| search.query_cursor.min(search.query.chars().count()))
            .unwrap_or(0)
    }

    fn move_search_cursor(&mut self, delta: isize) {
        let Some(search) = &mut self.overlays.search else {
            return;
        };
        let max = search.query.chars().count() as isize;
        search.query_cursor = (search.query_cursor as isize + delta).clamp(0, max) as usize;
    }

    fn move_search_cursor_to_previous_word(&mut self) {
        let Some(search) = &mut self.overlays.search else {
            return;
        };
        search.query_cursor = previous_word_start(&search.query, search.query_cursor);
    }

    fn move_search_cursor_to_next_word(&mut self) {
        let Some(search) = &mut self.overlays.search else {
            return;
        };
        search.query_cursor = next_word_start(&search.query, search.query_cursor);
    }

    fn move_search_cursor_to(&mut self, index: usize) {
        let Some(search) = &mut self.overlays.search else {
            return;
        };
        search.query_cursor = index.min(search.query.chars().count());
    }

    fn move_search_cursor_to_end(&mut self) {
        let Some(search) = &mut self.overlays.search else {
            return;
        };
        search.query_cursor = search.query.chars().count();
    }

    fn page_search(&mut self, direction: isize) {
        let visible = self.input.frame_state.search_rows_visible.max(1) as isize;
        self.move_search_selection(direction * visible);
    }

    fn select_search_index(&mut self, index: usize) {
        let Some(search) = &mut self.overlays.search else {
            return;
        };
        if search.matches.is_empty() {
            search.selected = 0;
            search.scroll = 0;
            return;
        }
        search.selected = index.min(search.matches.len().saturating_sub(1));
        self.sync_search_scroll();
    }

    pub(in crate::app::search) fn confirm_search_selection(&mut self) -> Result<()> {
        let Some(path) = self.overlays.search.as_ref().and_then(|search| {
            search
                .matches
                .get(search.selected)
                .copied()
                .and_then(|index| search.candidates.get(index))
                .map(|candidate| candidate.path.clone())
        }) else {
            return Ok(());
        };

        self.reveal_path(path)?;
        self.close_search_overlay();
        Ok(())
    }

    pub(in crate::app) fn sync_search_scroll(&mut self) -> bool {
        let Some(search) = &mut self.overlays.search else {
            return false;
        };
        if search.matches.is_empty() {
            let changed = search.scroll != 0;
            search.scroll = 0;
            return changed;
        }

        let previous = search.scroll;
        let rows_visible = self.input.frame_state.search_rows_visible.max(1);
        if search.selected < search.scroll {
            search.scroll = search.selected;
        } else if search.selected >= search.scroll + rows_visible {
            search.scroll = search.selected + 1 - rows_visible;
        }
        let max_scroll = search.matches.len().saturating_sub(rows_visible);
        search.scroll = search.scroll.min(max_scroll);
        previous != search.scroll
    }

    pub(in crate::app) fn reveal_path(&mut self, path: PathBuf) -> Result<()> {
        if path.is_dir() {
            return self.set_dir_transition(
                path,
                DirectoryHistoryMode::PushCurrent,
                None,
                DirectoryLoadCompletion::Status("Opened folder from search".to_string()),
            );
        }

        let Some(parent) = path.parent() else {
            return Ok(());
        };

        let file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .map(str::to_string)
            .unwrap_or_default();
        self.set_dir_transition(
            parent.to_path_buf(),
            DirectoryHistoryMode::PushCurrent,
            Some(path),
            DirectoryLoadCompletion::Status(format!("Located {}", file_name)),
        )
    }
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn insert_char_at_cursor(text: &mut String, cursor: &mut usize, ch: char) {
    let byte_index = char_to_byte_index(text, *cursor);
    text.insert(byte_index, ch);
    *cursor += 1;
}

fn search_key_deletes_previous_word(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Backspace) && key.modifiers.contains(KeyModifiers::CONTROL)
        || matches!(key.code, KeyCode::Char('h' | 'w'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT)
}

fn search_key_deletes_next_word(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Delete) && key.modifiers.contains(KeyModifiers::CONTROL)
        || matches!(key.code, KeyCode::Char('d'))
            && key.modifiers.contains(KeyModifiers::ALT)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
}

fn is_search_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn previous_word_start(text: &str, cursor: usize) -> usize {
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = cursor.min(chars.len());

    while index > 0 && chars[index - 1].is_whitespace() {
        index -= 1;
    }
    while index > 0 && !chars[index - 1].is_whitespace() && !is_search_word_char(chars[index - 1]) {
        index -= 1;
    }
    while index > 0 && is_search_word_char(chars[index - 1]) {
        index -= 1;
    }

    index
}

fn next_word_start(text: &str, cursor: usize) -> usize {
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = cursor.min(chars.len());

    while index < chars.len() && is_search_word_char(chars[index]) {
        index += 1;
    }
    while index < chars.len() && !is_search_word_char(chars[index]) {
        index += 1;
    }

    index
}

fn remove_char_range(text: &mut String, start_char: usize, end_char: usize) {
    let start = char_to_byte_index(text, start_char);
    let end = char_to_byte_index(text, end_char);
    if start >= end {
        return;
    }
    text.replace_range(start..end, "");
}

fn remove_word_before_cursor(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let start = previous_word_delete_start(text, *cursor);
    remove_char_range(text, start, *cursor);
    *cursor = start;
}

fn remove_char_before_cursor(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let start = char_to_byte_index(text, cursor.saturating_sub(1));
    let end = char_to_byte_index(text, *cursor);
    text.replace_range(start..end, "");
    *cursor -= 1;
}

fn remove_char_at_cursor(text: &mut String, cursor: usize) {
    let start = char_to_byte_index(text, cursor);
    if start >= text.len() {
        return;
    }
    let end = char_to_byte_index(text, cursor + 1);
    text.replace_range(start..end, "");
}

fn previous_word_delete_start(text: &str, cursor: usize) -> usize {
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = cursor.min(chars.len());

    while index > 0 && !is_search_word_char(chars[index - 1]) {
        index -= 1;
    }
    while index > 0 && is_search_word_char(chars[index - 1]) {
        index -= 1;
    }

    index
}

fn remove_word_at_cursor(text: &mut String, cursor: usize) {
    let end = next_word_delete_end(text, cursor);
    remove_char_range(text, cursor, end);
}

fn next_word_delete_end(text: &str, cursor: usize) -> usize {
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = cursor.min(chars.len());
    if index >= chars.len() {
        return chars.len();
    }

    if is_search_word_char(chars[index]) {
        while index < chars.len() && is_search_word_char(chars[index]) {
            index += 1;
        }
        while index < chars.len() && !is_search_word_char(chars[index]) {
            index += 1;
        }
        return index;
    }

    while index < chars.len() && !is_search_word_char(chars[index]) {
        index += 1;
    }
    while index < chars.len() && is_search_word_char(chars[index]) {
        index += 1;
    }

    index
}
