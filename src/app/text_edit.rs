//! Shared text editing primitives used by the create and search overlays.
pub(super) fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

pub(super) fn insert_text_at_cursor(text: &mut String, cursor: &mut usize, inserted: &str) {
    let byte = char_to_byte(text, *cursor);
    text.insert_str(byte, inserted);
    *cursor += inserted.chars().count();
}

pub(super) fn single_line_paste_text(text: &str) -> String {
    normalize_paste_newlines(text)
        .chars()
        .filter_map(|ch| match ch {
            '\n' => Some(' '),
            ch if ch.is_control() && ch != '\t' => None,
            ch => Some(ch),
        })
        .collect()
}

pub(super) fn multiline_paste_text(text: &str) -> String {
    normalize_paste_newlines(text)
        .chars()
        .filter(|ch| *ch == '\n' || !ch.is_control() || *ch == '\t')
        .collect()
}

fn normalize_paste_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Move cursor left to the start of the previous word (shell-style).
pub(super) fn previous_word_start(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = cursor.min(chars.len());
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    while i > 0 && !chars[i - 1].is_whitespace() && !is_word_char(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(chars[i - 1]) {
        i -= 1;
    }
    i
}

/// Move cursor right to the start of the next word.
pub(super) fn next_word_start(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = cursor.min(chars.len());
    while i < chars.len() && is_word_char(chars[i]) {
        i += 1;
    }
    while i < chars.len() && !is_word_char(chars[i]) {
        i += 1;
    }
    i
}

/// Start of the region that Ctrl+Backspace should delete (back to word boundary).
pub(super) fn previous_delete_start(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = cursor.min(chars.len());
    while i > 0 && !is_word_char(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(chars[i - 1]) {
        i -= 1;
    }
    i
}

/// End of the region that Ctrl+Delete should delete (forward to word boundary).
pub(super) fn next_delete_end(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = cursor.min(chars.len());
    if i >= chars.len() {
        return chars.len();
    }
    if is_word_char(chars[i]) {
        while i < chars.len() && is_word_char(chars[i]) {
            i += 1;
        }
        while i < chars.len() && !is_word_char(chars[i]) {
            i += 1;
        }
        return i;
    }
    while i < chars.len() && !is_word_char(chars[i]) {
        i += 1;
    }
    while i < chars.len() && is_word_char(chars[i]) {
        i += 1;
    }
    i
}

pub(super) fn remove_char_range(text: &mut String, start_char: usize, end_char: usize) {
    let start = char_to_byte(text, start_char);
    let end = char_to_byte(text, end_char);
    if start < end {
        text.replace_range(start..end, "");
    }
}
