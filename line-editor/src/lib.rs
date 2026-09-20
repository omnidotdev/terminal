//! Single-line text editor with a cursor and readline-style operations
//! used by the native frontend's tab-rename input (the WASM frontend mirrors
//! the same behavior directly on its DOM input)

/// A single-line editable buffer with a byte-index cursor
#[derive(Debug, Clone)]
pub struct LineEditor {
    text: String,
    cursor: usize,
    all_selected: bool,
}

impl LineEditor {
    /// Create an editor seeded with `initial`, cursor at end, whole text selected
    /// so the first insert or paste replaces it
    pub fn new(initial: impl Into<String>) -> Self {
        let text = initial.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            all_selected: true,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn all_selected(&self) -> bool {
        self.all_selected
    }

    /// Drop the select-all state without touching the text
    pub fn clear_selection(&mut self) {
        self.all_selected = false;
    }

    fn take_selection(&mut self) {
        if self.all_selected {
            self.text.clear();
            self.cursor = 0;
            self.all_selected = false;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.take_selection();
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Paste external text, sanitized to a single line (control chars dropped)
    pub fn paste(&mut self, s: &str) {
        let cleaned: String = s.chars().filter(|c| !c.is_control()).collect();
        if cleaned.is_empty() {
            return;
        }
        self.take_selection();
        self.text.insert_str(self.cursor, &cleaned);
        self.cursor += cleaned.len();
    }

    pub fn backspace(&mut self) {
        if self.all_selected {
            self.take_selection();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let prev = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn kill_to_start(&mut self) {
        self.take_selection();
        self.text.replace_range(..self.cursor, "");
        self.cursor = 0;
    }

    pub fn kill_to_end(&mut self) {
        self.take_selection();
        self.text.truncate(self.cursor);
    }

    pub fn delete_word_before(&mut self) {
        self.take_selection();
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end_matches(' ');
        let start = match trimmed.rfind(' ') {
            Some(i) => i + 1,
            None => 0,
        };
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    pub fn move_left(&mut self) {
        self.all_selected = false;
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
    }

    pub fn move_right(&mut self) {
        self.all_selected = false;
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .chars()
            .next()
            .map(|c| self.cursor + c.len_utf8())
            .unwrap_or(self.text.len());
        self.cursor = next;
    }

    pub fn move_start(&mut self) {
        self.all_selected = false;
        self.cursor = 0;
    }
    pub fn move_end(&mut self) {
        self.all_selected = false;
        self.cursor = self.text.len();
    }

    pub fn select_all(&mut self) {
        self.all_selected = true;
        self.cursor = self.text.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_places_cursor_at_end_and_selects_all() {
        let e = LineEditor::new("hello");
        assert_eq!(e.text(), "hello");
        assert_eq!(e.cursor(), 5);
        assert!(e.all_selected());
    }

    #[test]
    fn typing_while_all_selected_replaces() {
        let mut e = LineEditor::new("hello");
        e.insert_char('x');
        assert_eq!(e.text(), "x");
        assert_eq!(e.cursor(), 1);
        assert!(!e.all_selected());
    }

    #[test]
    fn insert_char_inserts_at_cursor() {
        let mut e = LineEditor::new("ac");
        e.clear_selection();
        e.move_left();
        e.insert_char('b');
        assert_eq!(e.text(), "abc");
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn backspace_deletes_char_before_cursor() {
        let mut e = LineEditor::new("abc");
        e.clear_selection();
        e.backspace();
        assert_eq!(e.text(), "ab");
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn backspace_while_all_selected_clears_all() {
        let mut e = LineEditor::new("abc");
        e.backspace();
        assert_eq!(e.text(), "");
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn kill_to_start_ctrl_u() {
        let mut e = LineEditor::new("hello world");
        e.clear_selection();
        e.move_start();
        e.move_right();
        e.move_right();
        e.kill_to_start();
        assert_eq!(e.text(), "llo world");
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn kill_to_end_ctrl_k() {
        let mut e = LineEditor::new("hello world");
        e.clear_selection();
        e.move_start();
        for _ in 0..5 {
            e.move_right();
        }
        e.kill_to_end();
        assert_eq!(e.text(), "hello");
        assert_eq!(e.cursor(), 5);
    }

    #[test]
    fn delete_word_before_ctrl_w() {
        let mut e = LineEditor::new("foo bar baz");
        e.clear_selection();
        e.delete_word_before();
        assert_eq!(e.text(), "foo bar ");
        e.delete_word_before();
        assert_eq!(e.text(), "foo ");
    }

    #[test]
    fn move_left_right_respect_utf8_boundaries() {
        let mut e = LineEditor::new("aé");
        e.clear_selection();
        e.move_left();
        assert_eq!(e.cursor(), 1);
        e.move_left();
        assert_eq!(e.cursor(), 0);
        e.move_left();
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn paste_sanitizes_to_single_line() {
        let mut e = LineEditor::new("");
        e.paste("multi\nline\r\ntext\twith\u{7}ctrl");
        assert_eq!(e.text(), "multilinetextwithctrl");
    }

    #[test]
    fn paste_while_all_selected_replaces() {
        let mut e = LineEditor::new("old");
        e.paste("new");
        assert_eq!(e.text(), "new");
    }
}
