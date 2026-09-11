//! binman — an HTTP client for the terminal.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use binman_core::history::History;
use binman_core::{Client, Config, Workspace};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream, MouseEventKind,
};
use futures_util::StreamExt;

use binman::app::{self, App, keys, mouse};
use binman::{send, ui};

const HELP: &str = "\
binman — an HTTP client for the terminal

USAGE
    binman              open your collections, and the project's
    binman <path>...    open these too, for this run only
    binman send <file>  send one request and print the response

OPTIONS
    -h, --help          show this
    -V, --version       show the version

SEND
    --env NAME          the environment to use; the first found when left out
    --var NAME=VALUE    a value for a variable, above every other
    -i, --include       the status line and headers ahead of the body

COLLECTIONS
    A collection is a directory of requests, a Postman collection or an
    OpenAPI spec. They are listed in two files:

    ~/.config/binman/collections.json   yours, in every project
    .binman.json                        a project's: the nearest at or above
                                        where binman starts; commit it

    In the sidebar, a adds a collection, and e and d edit and remove the one
    selected. The form's Saved in moves one between the two files.

CONFIG
    ~/.config/binman/config, or $XDG_CONFIG_HOME/binman/config:

    HTTP_FILES  = /path/to/collections    listed as one more collection
    TIMEOUT     = 30s                     0 for none; 30s when left out
    CLIENT_CERT = /path/to/client.crt     mTLS, together with CLIENT_KEY
    CLIENT_KEY  = /path/to/client.key
";

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut opened = Vec::new();
    let mut sending = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("binman {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "send" if opened.is_empty() => {
                sending = Some(send::Options::parse(args.by_ref())?);
                break;
            }
            other if other.starts_with('-') => bail!("Unknown option {other}. Try --help."),
            other => opened.push(PathBuf::from(other)),
        }
    }

    let config = Config::load()?;
    let mut workspace = Workspace::load(&config).context("reading the collections")?;
    for path in &opened {
        workspace.add_argument(path)?;
    }
    let client = Client::new(&config)?;
    let history = History::at(History::default_path());

    if let Some(options) = sending {
        return send::run(
            &options,
            &workspace,
            &client,
            &history,
            &mut std::io::stdout(),
            &mut std::io::stderr(),
        )
        .await;
    }

    let (mut app, mut messages) = App::new(workspace, client, history);
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
        let _ = crossterm::execute!(
            std::io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste
        );
        restore(info);
    }));
    let result = run(&mut terminal, &mut app, &mut messages).await;
    let _ = crossterm::execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
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
                // The pointer moving changes nothing on screen unless it is
                // pulling a seam; drawing for every cell it crosses would be
                // waste.
                Some(Ok(Event::Mouse(event)))
                    if event.kind == MouseEventKind::Moved
                        || (matches!(event.kind, MouseEventKind::Drag(_))
                            && app.dragging.is_none()) =>
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
