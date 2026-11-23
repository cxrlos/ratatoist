//! One line of text being typed, with a cursor.

/// A one-line text field. The cursor is a byte offset into the text that
/// always sits on a character boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    text: String,
    cursor: usize,
}

impl TextField {
    /// A field holding `text`, with the cursor after its last character.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The text left of the cursor.
    pub fn before_cursor(&self) -> &str {
        self.text.get(..self.cursor).unwrap_or(&self.text)
    }

    /// The text from the cursor on.
    pub fn after_cursor(&self) -> &str {
        self.text.get(self.cursor..).unwrap_or_default()
    }

    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Inserts pasted `text` at the cursor. The field holds one line, so
    /// each line break or tab becomes a space; other control characters,
    /// which would corrupt the terminal when drawn, are dropped.
    pub fn paste(&mut self, text: &str) {
        let line: String = text
            .replace("\r\n", " ")
            .chars()
            .filter_map(|c| match c {
                '\n' | '\r' | '\t' => Some(' '),
                c if c.is_control() => None,
                c => Some(c),
            })
            .collect();
        self.text.insert_str(self.cursor, &line);
        self.cursor += line.len();
    }

    /// Deletes the character left of the cursor.
    pub fn backspace(&mut self) -> bool {
        let Some(c) = self.before_cursor().chars().next_back() else {
            return false;
        };
        self.cursor -= c.len_utf8();
        self.text.remove(self.cursor);
        true
    }

    /// Deletes the character under the cursor.
    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        self.text.remove(self.cursor);
        true
    }

    /// Deletes back to the start of the word left of the cursor, and the
    /// spaces after it, as Ctrl-w does in a shell.
    pub fn delete_word(&mut self) -> bool {
        let before = self.before_cursor();
        let trimmed = before.trim_end();
        let start = trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8());
        if start == self.cursor {
            return false;
        }
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        true
    }

    pub fn left(&mut self) -> bool {
        let Some(c) = self.before_cursor().chars().next_back() else {
            return false;
        };
        self.cursor -= c.len_utf8();
        true
    }

    pub fn right(&mut self) -> bool {
        let Some(c) = self.after_cursor().chars().next() else {
            return false;
        };
        self.cursor += c.len_utf8();
        true
    }

    pub fn home(&mut self) -> bool {
        let moved = self.cursor != 0;
        self.cursor = 0;
        moved
    }

    pub fn end(&mut self) -> bool {
        let moved = self.cursor != self.text.len();
        self.cursor = self.text.len();
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut field = TextField::new("ac");
        field.left();
        field.insert('b');
        assert_eq!(field.text(), "abc");
        assert_eq!(field.before_cursor(), "ab");
        assert_eq!(field.after_cursor(), "c");
    }

    #[test]
    fn backspace_and_delete_take_one_character_each_side() {
        let mut field = TextField::new("añb");
        field.left();
        assert!(field.backspace());
        assert_eq!(field.text(), "ab");
        assert!(field.delete());
        assert_eq!(field.text(), "a");
        assert!(!field.delete());
        field.home();
        assert!(!field.backspace());
    }

    #[test]
    fn the_cursor_moves_over_whole_characters_and_stops_at_the_ends() {
        let mut field = TextField::new("é");
        assert!(!field.right());
        assert!(field.left());
        assert_eq!(field.after_cursor(), "é");
        assert!(!field.left());
        assert!(field.end());
        assert!(!field.end());
        assert!(field.home());
        assert!(!field.home());
    }

    #[test]
    fn ctrl_w_deletes_the_word_before_the_cursor_and_its_trailing_spaces() {
        let mut field = TextField::new("call the dentist  ");
        assert!(field.delete_word());
        assert_eq!(field.text(), "call the ");
        assert!(field.delete_word());
        assert_eq!(field.text(), "call ");
        assert!(field.delete_word());
        assert_eq!(field.text(), "");
        assert!(!field.delete_word());
    }

    #[test]
    fn ctrl_w_keeps_the_text_after_the_cursor() {
        let mut field = TextField::new("buy oat milk");
        for _ in 0.."milk".len() {
            field.left();
        }
        field.delete_word();
        assert_eq!(field.text(), "buy milk");
        assert_eq!(field.before_cursor(), "buy ");
    }

    #[test]
    fn a_paste_lands_at_the_cursor_on_one_line() {
        let mut field = TextField::new("ab");
        field.left();
        field.paste("1\r\n2\n3");
        assert_eq!(field.text(), "a1 2 3b");
        assert_eq!(field.after_cursor(), "b");
    }

    #[test]
    fn a_paste_turns_tabs_into_spaces_and_drops_other_control_characters() {
        let mut field = TextField::new("");
        field.paste("a\tb\u{1b}[31mc\u{7}");
        assert_eq!(field.text(), "a b[31mc");
        assert_eq!(field.after_cursor(), "");
    }
}
