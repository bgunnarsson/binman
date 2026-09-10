//! binman — an HTTP client for the terminal.

use std::io::Write;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use binman_core::history::History;
use binman_core::{Client, Config};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream, MouseEventKind,
};
use futures_util::StreamExt;

use binman::app::{self, App, keys, mouse};
use binman::ui;

const HELP: &str = "\
binman — an HTTP client for the terminal

USAGE
    binman              open the collections in HTTP_FILES

OPTIONS
    -h, --help          show this
    -V, --version       show the version

CONFIG
    ~/.config/binman/config, or $XDG_CONFIG_HOME/binman/config:

    HTTP_FILES  = /path/to/collections    required
    TIMEOUT     = 30s                     0 for none; 30s when left out
    CLIENT_CERT = /path/to/client.crt     mTLS, together with CLIENT_KEY
    CLIENT_KEY  = /path/to/client.key
";

#[tokio::main]
async fn main() -> Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("binman {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other if other.starts_with('-') => bail!("Unknown option {other}. Try --help."),
            other => bail!(
                "Unexpected argument {other}: binman reads its collections from HTTP_FILES in {}. Try --help.",
                Config::path().display()
            ),
        }
    }

    let config = Config::load()?;
    let root = config.root.canonicalize().with_context(|| {
        format!(
            "the collections directory {} (HTTP_FILES in {})",
            config.root.display(),
            Config::path().display()
        )
    })?;
    if !root.is_dir() {
        bail!(
            "{} is not a directory (HTTP_FILES in {})",
            root.display(),
            Config::path().display()
        );
    }
    let client = Client::new(&config)?;

    let (mut app, mut messages) = App::new(root, client, History::at(History::default_path()));
    app.show_splash();

    let mut terminal = ratatui::init();
    // Bracketed paste, so a curl command copied over several lines arrives as
    // one paste rather than as keystrokes whose line breaks press Enter. Mouse
    // capture, so a click reaches binman; the terminal's own selection is
    // still there with Shift held — Option in iTerm2.
    let _ = crossterm::execute!(std::io::stdout(), EnableBracketedPaste, EnableMouseCapture);
    // ratatui's panic hook restores the screen but knows nothing of these,
    // and a shell left with mouse capture on prints every movement.
    let restore = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture, DisableBracketedPaste);
        restore(info);
    }));
    let result = run(&mut terminal, &mut app, &mut messages).await;
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture, DisableBracketedPaste);
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    messages: &mut tokio::sync::mpsc::UnboundedReceiver<app::Message>,
) -> Result<()> {
    let mut events = EventStream::new();
    // Moves the elapsed time on screen while something is in flight; idle,
    // nothing redraws until something happens.
    let mut tick = tokio::time::interval(Duration::from_millis(250));

    let mut redraw = true;

    loop {
        if redraw {
            terminal.draw(|frame| ui::draw(frame, app))?;
            if let Some(text) = app.clipboard.take() {
                copy_to_clipboard(&text);
            }
        }
        redraw = true;

        tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => keys::handle(app, key),
                Some(Ok(Event::Paste(text))) => app.paste(&text),
                // The pointer moving, button held or not, changes nothing on
                // screen; drawing for every cell it crosses would be waste.
                Some(Ok(Event::Mouse(event)))
                    if matches!(event.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) =>
                {
                    redraw = false;
                }
                Some(Ok(Event::Mouse(event))) => mouse::handle(app, event),
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error.into()),
                // stdin closed; there is no way to drive the UI any more.
                None => break,
            },
            message = messages.recv() => match message {
                Some(message) => app.handle(message),
                None => break,
            },
            _ = tick.tick(), if app.is_busy() => {}
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Hands text to the terminal's clipboard with OSC 52, which works over SSH
/// and needs no clipboard library — in the terminals that allow it. Those that
/// do not simply ignore it, and the text is on screen to select either way.
fn copy_to_clipboard(text: &str) {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{encoded}\x07");
    let _ = out.flush();
}
