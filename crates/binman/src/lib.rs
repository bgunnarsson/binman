//! binman: an HTTP client for the terminal.
//!
//! Exposed as a library so the layout can be rendered against a test backend
//! — a TUI that is only reachable through `main` is a TUI nobody can assert on.

pub mod app;
pub mod pretty;
pub mod theme;
pub mod ui;
