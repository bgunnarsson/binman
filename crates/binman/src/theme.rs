//! The palette, and the roles binman assigns to it.
//!
//! binsql's, which binman sits beside in the same terminal, and so from the
//! same two sources. The pane and overlay vocabulary is binvim's — its chrome
//! roles, its Catppuccin Mocha values, and its rule that chrome sits on its own
//! surface so it reads as layered above the body. The header and status line
//! follow Claude Code instead: one coloured mark and otherwise quiet text.
//!
//! Nothing outside this module names a colour. Widgets ask for a role — a
//! focused border, a status code, a JSON key — so the whole UI can be retinted
//! by editing one file.

use ratatui::style::{Color, Modifier, Style};

// ── Surfaces ────────────────────────────────────────────────────────
/// The body: the URL, the request being edited, the response.
pub const BACKGROUND: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
/// Chrome: side pane, tab strip, status line, every overlay. Always a shade
/// off `BACKGROUND` so chrome reads as sitting above the body.
pub const CHROME_BG: Color = Color::Rgb(0x18, 0x18, 0x25);

// ── Chrome palette — binvim's roles ─────────────────────────────────
/// Main text on chrome.
pub const FOREGROUND: Color = Color::Rgb(0xcd, 0xd6, 0xf4);
/// Muted text: hints, counts, where a variable came from.
pub const DIM: Color = Color::Rgb(0x6c, 0x70, 0x86);
/// Active tab, overlay title, the selection bar.
pub const EMPHASIS: Color = Color::Rgb(0xb4, 0xbe, 0xfe);
/// Layered chrome: selection background, active tab background.
pub const SURFACE: Color = Color::Rgb(0x45, 0x47, 0x5a);
/// Borders and dividers.
pub const BORDER: Color = Color::Rgb(0x58, 0x5b, 0x70);
/// The prompt caret, and key names in hint text.
pub const ACCENT: Color = Color::Rgb(0xfa, 0xb3, 0x87);
/// A success: a 2xx, a save.
pub const ACCENT_SECONDARY: Color = Color::Rgb(0xa6, 0xe3, 0xa1);
pub const ERROR: Color = Color::Rgb(0xf3, 0x8b, 0xa8);
pub const WARNING: Color = Color::Rgb(0xf9, 0xe2, 0xaf);
pub const HINT: Color = Color::Rgb(0x89, 0xdc, 0xeb);

// A few values the tree, the badges and the response colour by.
const SUBTEXT: Color = Color::Rgb(0xa6, 0xad, 0xc8);
const MAUVE: Color = Color::Rgb(0xcb, 0xa6, 0xf7);
const TEAL: Color = Color::Rgb(0x94, 0xe2, 0xd5);
const MAROON: Color = Color::Rgb(0xeb, 0xa0, 0xac);
/// One step below `SURFACE`, for a selection in a pane that is not focused.
const SURFACE_LOW: Color = Color::Rgb(0x31, 0x32, 0x44);

/// binman's own mark, in Claude Code's brand coral. The header and the status
/// line follow Claude Code rather than binvim: binvim's powerline chips exist
/// to shout which *mode* you are in, and binman has none.
pub const BRAND: Color = Color::Rgb(0xd9, 0x77, 0x57);
pub const MARK: char = '✻';

// ── Icons ───────────────────────────────────────────────────────────
pub const ICON_FOLDER: char = '\u{f07b}';
pub const ICON_FOLDER_OPEN: char = '\u{f07c}';
/// A Postman collection: many requests in one file.
pub const ICON_COLLECTION: char = '\u{f1b3}';
/// An OpenAPI spec.
pub const ICON_SPEC: char = '\u{f02d}';
/// A tag inside a spec.
pub const ICON_TAG: char = '\u{f02b}';
/// The bar down the left of a selected row.
pub const SELECTION_BAR: char = '▌';

pub fn body() -> Style {
    Style::default().bg(BACKGROUND).fg(FOREGROUND)
}

pub fn chrome() -> Style {
    Style::default().bg(CHROME_BG).fg(FOREGROUND)
}

pub fn border(focused: bool) -> Style {
    Style::default().fg(if focused { EMPHASIS } else { BORDER })
}

/// A pane or overlay title, embedded in its top border.
pub fn title(focused: bool) -> Style {
    if focused {
        Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM)
    }
}

/// The counter binvim puts at the right end of a popup's top border.
pub fn counter() -> Style {
    Style::default().fg(DIM)
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

pub fn muted() -> Style {
    Style::default().fg(SUBTEXT)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn text() -> Style {
    Style::default().fg(FOREGROUND)
}

/// The product mark, and nothing else.
pub fn brand() -> Style {
    Style::default().fg(BRAND)
}

/// How far the layout recedes behind an open modal. Far enough that the eye
/// goes to the modal, near enough that the shape of what is behind it survives
/// as context.
pub const SCRIM: f32 = 0.62;

/// Blends a colour toward the body background. `amount` of 0 leaves it alone,
/// 1 erases it entirely.
///
/// `Reset` means "whatever the terminal uses", which is already the background
/// as far as binman is concerned, so it passes through.
pub fn recede(colour: Color, amount: f32) -> Color {
    let (Color::Rgb(r, g, b), Color::Rgb(br, bg, bb)) = (colour, BACKGROUND) else {
        return colour;
    };
    let amount = amount.clamp(0.0, 1.0);
    let blend = |from: u8, to: u8| {
        (f32::from(from) * (1.0 - amount) + f32::from(to) * amount).round() as u8
    };
    Color::Rgb(blend(r, br), blend(g, bg), blend(b, bb))
}

pub fn success() -> Style {
    Style::default().fg(ACCENT_SECONDARY)
}

pub fn warning() -> Style {
    Style::default().fg(WARNING)
}

pub fn danger() -> Style {
    Style::default().fg(ERROR)
}

/// The background of a selected row. The `▌` bar carries the emphasis; the
/// background only says which row.
pub fn selection(focused: bool) -> Style {
    Style::default().bg(if focused { SURFACE } else { SURFACE_LOW })
}

pub fn selection_bar(focused: bool) -> Style {
    Style::default()
        .fg(if focused { EMPHASIS } else { BORDER })
        .bg(if focused { SURFACE } else { SURFACE_LOW })
}

/// Where typing lands in a field that has focus.
pub fn cursor() -> Style {
    Style::default().bg(SURFACE)
}

pub fn status_bar() -> Style {
    Style::default().bg(CHROME_BG).fg(SUBTEXT)
}

/// The environment picker in the header, as v1 coloured its dropdown: lit when
/// an environment is in use, quiet when requests go out under none.
pub fn env_picker(active: bool) -> Style {
    if active {
        Style::default()
            .bg(SURFACE)
            .fg(TEAL)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(SURFACE_LOW).fg(SUBTEXT)
    }
}

pub fn tab(active: bool) -> Style {
    if active {
        Style::default()
            .bg(SURFACE)
            .fg(EMPHASIS)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(CHROME_BG).fg(DIM)
    }
}

/// A section's name in a pane's top border: lit when it is the one showing,
/// brightest when the pane has focus.
pub fn section(active: bool, focused: bool) -> Style {
    match (active, focused) {
        (true, true) => Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD),
        (true, false) => Style::default().fg(SUBTEXT).add_modifier(Modifier::BOLD),
        (false, _) => Style::default().fg(DIM),
    }
}

pub fn overlay() -> Style {
    Style::default().bg(CHROME_BG).fg(FOREGROUND)
}

/// A key name in help text and the status bar.
pub fn key() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

// ── Tree ────────────────────────────────────────────────────────────

pub fn node_dir() -> Style {
    Style::default().fg(EMPHASIS)
}

pub fn node_file() -> Style {
    Style::default().fg(FOREGROUND)
}

pub fn node_collection() -> Style {
    Style::default().fg(WARNING)
}

pub fn node_spec() -> Style {
    Style::default().fg(HINT)
}

pub fn node_tag() -> Style {
    Style::default().fg(TEAL)
}

// ── HTTP ────────────────────────────────────────────────────────────

/// One colour per method, so a DELETE never passes for a GET at a glance.
pub fn method(method: &str) -> Style {
    let colour = match method {
        "GET" => ACCENT_SECONDARY,
        "POST" => ACCENT,
        "PUT" => HINT,
        "PATCH" => MAUVE,
        "DELETE" => ERROR,
        "HEAD" => TEAL,
        _ => SUBTEXT,
    };
    Style::default().fg(colour).add_modifier(Modifier::BOLD)
}

/// A status code by its class.
pub fn status(code: u16) -> Style {
    let colour = match code {
        200..=299 => ACCENT_SECONDARY,
        100..=199 | 300..=399 => HINT,
        400..=499 => WARNING,
        500..=599 => ERROR,
        _ => SUBTEXT,
    };
    Style::default().fg(colour).add_modifier(Modifier::BOLD)
}

/// A `{{variable}}` in the URL: set, or about to be sent as written.
pub fn variable(resolved: bool) -> Style {
    Style::default().fg(if resolved { HINT } else { WARNING })
}

/// A header's name in the response.
pub fn header_name() -> Style {
    Style::default().fg(EMPHASIS)
}

// ── JSON ────────────────────────────────────────────────────────────

pub fn json_key() -> Style {
    Style::default().fg(EMPHASIS)
}

pub fn json_string() -> Style {
    Style::default().fg(ACCENT_SECONDARY)
}

pub fn json_number() -> Style {
    Style::default().fg(ACCENT)
}

pub fn json_bool() -> Style {
    Style::default().fg(MAROON)
}

/// null is dimmed and italic, as binsql shows NULL, so it never reads as a
/// string that happens to say "null".
pub fn json_null() -> Style {
    Style::default().fg(DIM).add_modifier(Modifier::ITALIC)
}

pub fn json_punctuation() -> Style {
    Style::default().fg(DIM)
}
