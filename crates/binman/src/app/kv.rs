//! A table of name–value pairs: query parameters, headers, form fields.
//!
//! The last row is always the one that adds a pair, so there is nothing to
//! remember about how to add one: go to the end and press Enter.

use super::line::LineInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Value,
}

#[derive(Debug, Clone)]
pub struct CellEdit {
    /// The row being edited; `rows.len()` for a new one.
    pub row: usize,
    pub column: Column,
    pub name: LineInput,
    pub value: LineInput,
}

#[derive(Debug, Clone, Default)]
pub struct KvTable {
    pub rows: Vec<(String, String)>,
    /// Up to `rows.len()`, which is the row that adds one.
    pub selected: usize,
    pub offset: usize,
    pub edit: Option<CellEdit>,
}

impl KvTable {
    pub fn set_rows(&mut self, rows: Vec<(String, String)>) {
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len());
        self.edit = None;
    }

    pub fn move_selection(&mut self, delta: isize) {
        let last = self.rows.len() as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    pub fn first(&mut self) {
        self.selected = 0;
    }

    pub fn last(&mut self) {
        self.selected = self.rows.len().saturating_sub(1);
    }

    /// Starts editing the selected row at its name, or a new row on the add
    /// row.
    pub fn begin(&mut self) {
        let (name, value) = self.rows.get(self.selected).cloned().unwrap_or_default();
        self.edit = Some(CellEdit {
            row: self.selected,
            column: Column::Name,
            name: LineInput::new(name),
            value: LineInput::new(value),
        });
    }

    /// Goes to the add row and starts typing into it.
    pub fn add(&mut self) {
        self.selected = self.rows.len();
        self.begin();
    }

    /// Enter and Tab move from the name to the value, and from the value keep
    /// the row. Returns whether the rows changed.
    pub fn advance(&mut self) -> bool {
        match &mut self.edit {
            Some(edit) if edit.column == Column::Name => {
                edit.column = Column::Value;
                false
            }
            Some(_) => self.commit(),
            None => false,
        }
    }

    pub fn back(&mut self) {
        if let Some(edit) = &mut self.edit {
            edit.column = Column::Name;
        }
    }

    /// Keeps what was typed. A row left with neither name nor value is
    /// removed rather than kept empty. Returns whether the rows changed.
    pub fn commit(&mut self) -> bool {
        let Some(edit) = self.edit.take() else {
            return false;
        };
        let name = edit.name.text().trim().to_string();
        let value = edit.value.text().to_string();
        let empty = name.is_empty() && value.is_empty();

        if edit.row >= self.rows.len() {
            if empty {
                return false;
            }
            self.rows.push((name, value));
            self.selected = self.rows.len() - 1;
        } else if empty {
            self.rows.remove(edit.row);
            self.selected = self.selected.min(self.rows.len());
        } else {
            self.rows[edit.row] = (name, value);
        }
        true
    }

    pub fn cancel(&mut self) {
        self.edit = None;
    }

    pub fn delete(&mut self) -> bool {
        if self.selected >= self.rows.len() {
            return false;
        }
        self.rows.remove(self.selected);
        self.selected = self.selected.min(self.rows.len());
        true
    }

    /// The field being typed into.
    pub fn input(&mut self) -> Option<&mut LineInput> {
        let edit = self.edit.as_mut()?;
        Some(match edit.column {
            Column::Name => &mut edit.name,
            Column::Value => &mut edit.value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> KvTable {
        let mut table = KvTable::default();
        table.set_rows(vec![("Accept".into(), "*/*".into())]);
        table
    }

    #[test]
    fn the_add_row_appends_a_pair() {
        let mut table = table();
        table.add();
        table.input().unwrap().set("X-Trace");
        assert!(!table.advance(), "Enter on the name moves to the value");
        table.input().unwrap().set("abc");
        assert!(table.advance(), "Enter on the value keeps the row");
        assert_eq!(table.rows[1], ("X-Trace".to_string(), "abc".to_string()));
        assert_eq!(table.selected, 1);
    }

    #[test]
    fn emptying_a_row_removes_it() {
        let mut table = table();
        table.begin();
        table.input().unwrap().set("");
        table.advance();
        table.input().unwrap().set("");
        table.advance();
        assert!(table.rows.is_empty());
    }

    #[test]
    fn an_empty_new_row_is_not_kept() {
        let mut table = table();
        table.add();
        assert!(!table.commit());
        assert_eq!(table.rows.len(), 1);
    }

    #[test]
    fn escape_leaves_the_row_as_it_was() {
        let mut table = table();
        table.begin();
        table.input().unwrap().set("Changed");
        table.cancel();
        assert_eq!(table.rows[0].0, "Accept");
    }
}
