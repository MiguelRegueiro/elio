use super::super::App;
use super::super::text_edit::{
    char_to_byte, next_delete_end, next_word_start, previous_delete_start, previous_word_start,
    remove_char_range,
};

impl App {
    pub(in crate::app) fn create_insert_newline(&mut self) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        let tail = {
            let byte = char_to_byte(&c.lines[c.cursor_line], c.cursor_col);
            c.lines[c.cursor_line].split_off(byte)
        };
        c.cursor_line += 1;
        c.lines.insert(c.cursor_line, tail);
        c.line_errors.insert(c.cursor_line, None);
        c.cursor_col = 0;
        c.preferred_col = 0;
    }

    pub(in crate::app::create) fn create_move_horizontal(&mut self, delta: isize) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        if delta < 0 {
            if c.cursor_col > 0 {
                c.cursor_col -= 1;
            } else if c.cursor_line > 0 {
                c.cursor_line -= 1;
                c.cursor_col = c.lines[c.cursor_line].chars().count();
            }
        } else {
            let len = c.lines[c.cursor_line].chars().count();
            if c.cursor_col < len {
                c.cursor_col += 1;
            } else if c.cursor_line + 1 < c.lines.len() {
                c.cursor_line += 1;
                c.cursor_col = 0;
            }
        }
        c.preferred_col = c.cursor_col;
    }

    pub(in crate::app::create) fn create_move_word(&mut self, direction: isize) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        let line = &c.lines[c.cursor_line];
        let new_col = if direction < 0 {
            previous_word_start(line, c.cursor_col)
        } else {
            next_word_start(line, c.cursor_col)
        };
        c.cursor_col = new_col;
        c.preferred_col = new_col;
    }

    pub(in crate::app::create) fn create_move_vertical(&mut self, delta: isize) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        let new_line =
            (c.cursor_line as isize + delta).clamp(0, c.lines.len() as isize - 1) as usize;
        if new_line == c.cursor_line {
            return;
        }
        c.cursor_line = new_line;
        let max_col = c.lines[c.cursor_line].chars().count();
        c.cursor_col = c.preferred_col.min(max_col);
    }

    pub(in crate::app::create) fn create_backspace(&mut self) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        if c.cursor_col > 0 {
            let start = char_to_byte(&c.lines[c.cursor_line], c.cursor_col - 1);
            let end = char_to_byte(&c.lines[c.cursor_line], c.cursor_col);
            c.lines[c.cursor_line].replace_range(start..end, "");
            c.cursor_col -= 1;
            c.preferred_col = c.cursor_col;
            c.line_errors[c.cursor_line] = None;
        } else if c.cursor_line > 0 {
            let removed = c.lines.remove(c.cursor_line);
            c.line_errors.remove(c.cursor_line);
            c.cursor_line -= 1;
            c.cursor_col = c.lines[c.cursor_line].chars().count();
            c.preferred_col = c.cursor_col;
            c.lines[c.cursor_line].push_str(&removed);
            c.line_errors[c.cursor_line] = None;
        }
    }

    pub(in crate::app::create) fn create_delete(&mut self) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        let len = c.lines[c.cursor_line].chars().count();
        if c.cursor_col < len {
            let start = char_to_byte(&c.lines[c.cursor_line], c.cursor_col);
            let end = char_to_byte(&c.lines[c.cursor_line], c.cursor_col + 1);
            c.lines[c.cursor_line].replace_range(start..end, "");
            c.line_errors[c.cursor_line] = None;
        } else if c.cursor_line + 1 < c.lines.len() {
            let next = c.lines.remove(c.cursor_line + 1);
            c.line_errors.remove(c.cursor_line + 1);
            c.lines[c.cursor_line].push_str(&next);
            c.line_errors[c.cursor_line] = None;
        }
    }

    pub(in crate::app::create) fn create_delete_word_back(&mut self) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        if c.cursor_col == 0 {
            return;
        }
        let line = &mut c.lines[c.cursor_line];
        let start = previous_delete_start(line, c.cursor_col);
        remove_char_range(line, start, c.cursor_col);
        c.cursor_col = start;
        c.preferred_col = start;
        c.line_errors[c.cursor_line] = None;
    }

    pub(in crate::app::create) fn create_delete_word_forward(&mut self) {
        let Some(c) = &mut self.overlays.create else {
            return;
        };
        let line = &mut c.lines[c.cursor_line];
        let end = next_delete_end(line, c.cursor_col);
        remove_char_range(line, c.cursor_col, end);
        c.line_errors[c.cursor_line] = None;
    }
}
