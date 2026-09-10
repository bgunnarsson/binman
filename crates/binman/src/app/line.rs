//! One line of editable text with a cursor: the URL, a header being typed, a
//! path to save to.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineInput {
    text: String,
    /// In characters, not bytes, so a multibyte character is one step.
    cursor: usize,
}

impl LineInput {
    pub fn new(text: impl Into<String>) -> LineInput {
        let mut input = LineInput::default();
        input.set(text);
        input
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replaces the text and puts the cursor at the end of it.
    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.chars().count();
    }

    fn byte_at(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(at, _)| at)
    }

    pub fn insert(&mut self, ch: char) {
        let at = self.byte_at(self.cursor);
        self.text.insert(at, ch);
        self.cursor += 1;
    }

    /// Pasted text. A line has no room for a line break, so a pasted block is
    /// joined into one: a `\` that ends a line is a shell continuation and
    /// goes with it, and the break becomes a space. That is what lets a curl
    /// command copied over several lines land whole.
    pub fn paste(&mut self, text: &str) {
        let joined = text
            .replace("\\\r\n", " ")
            .replace("\\\n", " ")
            .replace("\r\n", " ")
            .replace(['\n', '\r'], " ");
        let at = self.byte_at(self.cursor);
        self.text.insert_str(at, &joined);
        self.cursor += joined.chars().count();
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        let at = self.byte_at(self.cursor);
        self.text.remove(at);
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.text.chars().count() {
            return false;
        }
        let at = self.byte_at(self.cursor);
        self.text.remove(at);
        true
    }

    /// ⌃U, as a shell has it: everything before the cursor goes.
    pub fn clear_before(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let at = self.byte_at(self.cursor);
        self.text.replace_range(..at, "");
        self.cursor = 0;
        true
    }

    /// Applies an editing key. Returns whether the text changed, so a caller
    /// that mirrors the text somewhere else knows when to.
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
                false
            }
            KeyCode::Char('u') if ctrl => self.clear_before(),
            KeyCode::Char(ch) if !ctrl && !alt => {
                self.insert(ch);
                true
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                false
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.text.chars().count());
                false
            }
            KeyCode::Home => {
                self.cursor = 0;
                false
            }
            KeyCode::End => {
                self.cursor = self.text.chars().count();
                false
            }
            _ => false,
        }
    }

    /// The first character to show so the cursor stays inside `width`
    /// columns, scrolling as little as it can. The cursor cell needs a column
    /// of its own.
    pub fn first_visible(&self, width: usize) -> usize {
        let widths: Vec<usize> = self
            .text
            .chars()
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
            .collect();
        let cursor = self.cursor.min(widths.len());
        let mut start = 0;
        while start < cursor && widths[start..cursor].iter().sum::<usize>() + 1 > width {
            start += 1;
        }
        start
    }

    /// Puts the cursor under a click `column` display columns into the text
    /// as drawn from its `start`th character, or at the end past it.
    pub fn place_cursor(&mut self, start: usize, column: usize) {
        let mut used = 0;
        let mut cursor = start;
        for ch in self.text.chars().skip(start) {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + width > column {
                break;
            }
            used += width;
            cursor += 1;
        }
        self.cursor = cursor.min(self.text.chars().count());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn edits_at_the_cursor_a_character_at_a_time() {
        let mut input = LineInput::new("héllo");
        input.handle(key(KeyCode::Left));
        input.handle(key(KeyCode::Left));
        input.handle(key(KeyCode::Backspace));
        assert_eq!(input.text(), "hélo");
        input.handle(key(KeyCode::Char('L')));
        assert_eq!(input.text(), "héLlo");
        assert_eq!(input.cursor(), 3);
    }

    #[test]
    fn a_pasted_curl_command_lands_on_one_line() {
        let mut input = LineInput::default();
        input.paste("curl -X POST \\\n  -H 'A: 1' \\\n  https://x");
        assert_eq!(input.text(), "curl -X POST    -H 'A: 1'    https://x");
    }

    #[test]
    fn ctrl_u_clears_back_to_the_start() {
        let mut input = LineInput::new("https://example.com/users");
        input.handle(key(KeyCode::Home));
        for _ in 0..8 {
            input.handle(key(KeyCode::Right));
        }
        assert!(input.handle(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)));
        assert_eq!(input.text(), "example.com/users");
    }

    #[test]
    fn the_window_scrolls_only_as_far_as_the_cursor_needs() {
        let input = LineInput::new("abcdefghij");
        assert_eq!(input.first_visible(20), 0);
        assert_eq!(input.first_visible(5), 6, "four characters and the cursor cell");
    }

    #[test]
    fn a_click_lands_the_cursor_under_it() {
        let mut input = LineInput::new("https://example.com");
        input.place_cursor(0, 8);
        assert_eq!(input.cursor(), 8);
        input.place_cursor(4, 2);
        assert_eq!(input.cursor(), 6, "counted from the first character shown");
        input.place_cursor(0, 99);
        assert_eq!(input.cursor(), 19, "past the end is the end");
    }
}
