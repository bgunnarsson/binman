//! A response body as lines to show: JSON re-indented and coloured, anything
//! else as it came.
//!
//! JSON is re-indented from its own text rather than parsed and printed again,
//! as v1 did with Go's `json.Indent`: parsing would reorder the keys, and
//! print `1.0` as `1` and a twenty-digit id as a float. What the server sent is
//! what is shown, with whitespace added and nothing else changed.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme;

/// Beyond this the body is shown plain. Colouring a few megabytes costs more
/// than it is worth to anyone scrolling through it.
const HIGHLIGHT_LIMIT: usize = 4 * 1024 * 1024;

pub fn render(text: &str) -> Vec<Line<'static>> {
    if text.len() <= HIGHLIGHT_LIMIT
        && let Some(lines) = json(text)
    {
        return lines;
    }
    plain(text)
}

/// Text a line at a time, with the characters that would break a terminal
/// line — tabs, carriage returns, other controls — made visible or harmless.
pub fn plain(text: &str) -> Vec<Line<'static>> {
    text.split('\n')
        .map(|line| Line::from(Span::styled(clean(line), theme::text())))
        .collect()
}

fn clean(line: &str) -> String {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '\t' => out.push_str("    "),
            ch if ch.is_control() => out.push('·'),
            ch => out.push(ch),
        }
    }
    out
}

fn json(text: &str) -> Option<Vec<Line<'static>>> {
    let trimmed = text.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
    Some(Printer::default().print(trimmed))
}

#[derive(Default)]
struct Printer {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    depth: usize,
}

impl Printer {
    fn push(&mut self, text: impl Into<String>, style: Style) {
        self.current.push(Span::styled(text.into(), style));
    }

    fn newline(&mut self) {
        let spans = std::mem::take(&mut self.current);
        self.lines.push(Line::from(spans));
        if self.depth > 0 {
            self.current.push(Span::raw("  ".repeat(self.depth)));
        }
    }

    /// `text` has already been checked to be valid JSON, so this only has to
    /// tell tokens apart, not police them.
    fn print(mut self, text: &str) -> Vec<Line<'static>> {
        let chars: Vec<char> = text.chars().collect();
        let mut index = 0;

        while index < chars.len() {
            let ch = chars[index];
            match ch {
                '{' | '[' => {
                    let close = if ch == '{' { '}' } else { ']' };
                    let next = skip_space(&chars, index + 1);
                    if chars.get(next) == Some(&close) {
                        self.push(format!("{ch}{close}"), theme::json_punctuation());
                        index = next + 1;
                        continue;
                    }
                    self.push(ch.to_string(), theme::json_punctuation());
                    self.depth += 1;
                    self.newline();
                }
                '}' | ']' => {
                    self.depth = self.depth.saturating_sub(1);
                    self.newline();
                    self.push(ch.to_string(), theme::json_punctuation());
                }
                ',' => {
                    self.push(",", theme::json_punctuation());
                    self.newline();
                }
                ':' => self.push(": ", theme::json_punctuation()),
                '"' => {
                    let end = string_end(&chars, index);
                    let literal: String = chars[index..end].iter().collect();
                    let is_key = chars.get(skip_space(&chars, end)) == Some(&':');
                    let style = if is_key {
                        theme::json_key()
                    } else {
                        theme::json_string()
                    };
                    self.push(literal, style);
                    index = end;
                    continue;
                }
                ch if ch.is_whitespace() => {}
                _ => {
                    let end = (index..chars.len())
                        .find(|&at| {
                            matches!(chars[at], ',' | '}' | ']' | ':') || chars[at].is_whitespace()
                        })
                        .unwrap_or(chars.len());
                    let word: String = chars[index..end].iter().collect();
                    let style = match word.as_str() {
                        "true" | "false" => theme::json_bool(),
                        "null" => theme::json_null(),
                        _ => theme::json_number(),
                    };
                    self.push(word, style);
                    index = end;
                    continue;
                }
            }
            index += 1;
        }

        if !self.current.is_empty() {
            self.newline();
        }
        self.lines
    }
}

fn skip_space(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }
    index
}

/// Just past the closing quote of the string that opens at `start`.
fn string_end(chars: &[char], start: usize) -> usize {
    let mut index = start + 1;
    while index < chars.len() {
        match chars[index] {
            '\\' => index += 2,
            '"' => return index + 1,
            _ => index += 1,
        }
    }
    chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn reindents_without_reordering_or_reformatting() {
        let lines = render(
            r#"{"zeta":1.0,"alpha":[true,null],"id":12345678901234567890,"empty":{},"s":"a\"b"}"#,
        );
        assert_eq!(
            text(&lines),
            vec![
                "{",
                "  \"zeta\": 1.0,",
                "  \"alpha\": [",
                "    true,",
                "    null",
                "  ],",
                "  \"id\": 12345678901234567890,",
                "  \"empty\": {},",
                "  \"s\": \"a\\\"b\"",
                "}",
            ]
        );
    }

    #[test]
    fn keys_and_values_are_told_apart() {
        let lines = render(r#"{"name":"Jane"}"#);
        let spans = &lines[1].spans;
        let key = spans
            .iter()
            .find(|span| span.content == "\"name\"")
            .unwrap();
        let value = spans
            .iter()
            .find(|span| span.content == "\"Jane\"")
            .unwrap();
        assert_eq!(key.style, theme::json_key());
        assert_eq!(value.style, theme::json_string());
    }

    #[test]
    fn what_is_not_json_is_shown_as_it_came() {
        assert_eq!(
            text(&render("<html>\n\t<b>hi</b>\r\n</html>")),
            vec!["<html>", "    <b>hi</b>", "</html>"]
        );
        assert_eq!(text(&render("{not json")), vec!["{not json"]);
    }
}
