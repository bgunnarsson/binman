//! Drives the real application against a real server and a real collection on
//! disk, and renders it to a test backend. This is the only way to assert that
//! the thing people look at is right; unit tests on the model would pass with
//! a blank screen.

#[path = "../../binman-core/tests/support/server.rs"]
mod server;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use binman::app::overlay::Overlay;
use binman::app::tab::{Outcome, Section, View};
use binman::app::tree::NodeKind;
use binman::app::{App, Message, Pane};
use binman::ui;
use binman_core::history::History;
use binman_core::{Client, Origin, Workspace};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use server::{Reply, serve};
use tokio::sync::mpsc::UnboundedReceiver;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 34;

/// One directory per test — these run concurrently in one process, so a
/// shared one would have them reading each other's files.
fn collection(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("binman-ui-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create a collection");
    root.canonicalize().expect("a real path")
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn app(root: &Path) -> (App, UnboundedReceiver<Message>) {
    // The history and the list of collections go in a hidden directory, which
    // the sidebar leaves out.
    let state = root.join(".state");
    let mut workspace =
        Workspace::load_at(state.join("collections.json"), None, None).expect("no collections");
    workspace.add_argument(root).expect("the collection");
    App::new(
        workspace,
        Client::with(Some(Duration::from_secs(5)), None),
        History::at(state.join("history.jsonl")),
    )
}

/// Applies background work until nothing is in flight, so a send has landed
/// before the test looks.
async fn settle(app: &mut App, messages: &mut UnboundedReceiver<Message>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.is_busy() && Instant::now() < deadline {
        if let Ok(Some(message)) =
            tokio::time::timeout(Duration::from_millis(100), messages.recv()).await
        {
            app.handle(message);
        }
    }
    while let Ok(message) = messages.try_recv() {
        app.handle(message);
    }
}

fn draw(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, app))
        .expect("draw the whole layout");
    terminal
}

fn render(app: &mut App) -> String {
    let terminal = draw(app, WIDTH, HEIGHT);
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(WIDTH as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The URL bar's top border, which carries the host the URL resolves to. It
/// sits under the header and the tab strip.
fn url_bar(screen: &str) -> String {
    screen.lines().nth(2).unwrap_or_default().to_string()
}

fn press(app: &mut App, code: KeyCode) {
    binman::app::keys::handle(app, KeyEvent::new(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, ch: char) {
    binman::app::keys::handle(app, KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL));
}

fn alt(app: &mut App, ch: char) {
    binman::app::keys::handle(app, KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT));
}

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch));
    }
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    binman::app::mouse::handle(
        app,
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
    );
}

/// Where `text` is first drawn, as column and row. Draws first: a click lands
/// on what was drawn last, as it does on screen.
fn locate(app: &mut App, text: &str) -> (u16, u16) {
    let screen = render(app);
    screen
        .lines()
        .enumerate()
        .find_map(|(row, line)| {
            line.find(text)
                .map(|at| (line[..at].chars().count() as u16, row as u16))
        })
        .unwrap_or_else(|| panic!("{text} is not on screen:\n{screen}"))
}

/// Clicks the first place `text` is drawn, the way a person aims at what they
/// read.
fn click(app: &mut App, text: &str) {
    let (column, row) = locate(app, text);
    mouse(app, MouseEventKind::Down(MouseButton::Left), column, row);
}

fn selected_name(app: &App) -> String {
    match app.tree.selected().map(|node| &node.kind) {
        Some(
            NodeKind::Root { name, .. }
            | NodeKind::Dir { name, .. }
            | NodeKind::File { name, .. }
            | NodeKind::Collection { name, .. }
            | NodeKind::Folder { name }
            | NodeKind::Item { name, .. }
            | NodeKind::Spec { name, .. }
            | NodeKind::Tag { name },
        ) => name.clone(),
        Some(NodeKind::Operation { route, .. }) => route.clone(),
        Some(NodeKind::Note { text, .. }) => text.clone(),
        None => String::new(),
    }
}

/// Walks the sidebar down to a row. Opening folders on the way is the
/// caller's job, as it would be at the keyboard.
fn select(app: &mut App, name: &str) {
    app.tree.select_first();
    for _ in 0..40 {
        if selected_name(app) == name {
            return;
        }
        press(app, KeyCode::Char('j'));
    }
    panic!("never reached {name}:\n{}", render(app));
}

fn open(app: &mut App, name: &str) {
    app.focus = Pane::Collections;
    select(app, name);
    press(app, KeyCode::Enter);
}

/// Goes to a section of the request pane the way the keyboard would.
fn section(app: &mut App, target: Section) {
    alt(app, 'l');
    for _ in 0..Section::ALL.len() {
        if app.tab().section == target {
            return;
        }
        press(app, KeyCode::Char(']'));
    }
    panic!("never reached {target:?}");
}

#[tokio::test]
async fn opens_a_request_and_shows_its_response() {
    let server =
        serve(|_, _| Reply::json(200, r#"{"artists":[{"name":"Portishead","founded":1991}]}"#))
            .await;
    let root = collection("send");
    write(&root.join(".env"), &format!("BASE={}\n", server.url));
    write(
        &root.join("users").join("list.http"),
        "GET {{BASE}}/users\nAccept: application/json\n",
    );

    let (mut app, mut messages) = app(&root);
    select(&mut app, "users");
    press(&mut app, KeyCode::Char('l'));
    open(&mut app, "list.http");
    assert_eq!(app.tab().title, "list.http");

    // The tab strip names the request, so the header does not; its right end
    // is the environment picker, and the URL bar says where the request goes.
    let screen = render(&mut app);
    let header = screen.lines().next().unwrap().to_string();
    assert!(
        header.contains("binman") && !header.contains("list.http"),
        "{header}"
    );
    assert!(
        header.ends_with("default ▾"),
        "the environment picker: {header}"
    );
    assert!(
        screen.lines().nth(1).unwrap().contains("list.http"),
        "{screen}"
    );
    let url_bar = url_bar(&screen);
    assert!(
        url_bar.contains("http://127.0.0.1"),
        "the host it resolves to: {url_bar}"
    );

    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert!(
        screen.contains("200 OK"),
        "the status is missing:\n{screen}"
    );
    assert!(
        screen.contains("\"name\": \"Portishead\""),
        "the JSON should be re-indented:\n{screen}"
    );
    assert!(screen.contains("\"founded\": 1991"), "{screen}");

    let seen = server.last().expect("the server was reached");
    assert_eq!(seen.path, "/users");
    assert_eq!(seen.header("accept"), Some("application/json"));
}

#[tokio::test]
async fn the_chosen_environment_decides_where_requests_go() {
    let first = serve(|_, _| Reply::new(200).body("first")).await;
    let second = serve(|_, _| Reply::new(200).body("second")).await;
    let root = collection("envs");
    write(&root.join(".env"), &format!("BASE={}\n", first.url));
    write(
        &root.join(".env.staging"),
        &format!("BASE={}\n", second.url),
    );
    write(&root.join("ping.http"), "GET {{BASE}}/ping\n");
    write(&root.join("other.http"), "GET {{BASE}}/other\n");

    let (mut app, mut messages) = app(&root);
    open(&mut app, "ping.http");
    assert_eq!(
        app.tab().env_source().map(|s| s.label.as_str()),
        Some("default")
    );

    ctrl(&mut app, 'e');
    let screen = render(&mut app);
    assert!(screen.contains("Environments"), "{screen}");
    assert!(screen.contains("staging"), "{screen}");
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);

    let header = render(&mut app).lines().next().unwrap().to_string();
    assert!(header.contains("staging"), "{header}");

    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;
    assert_eq!(second.hits(), 1, "staging's server should have been asked");
    assert_eq!(first.hits(), 0);

    // The choice carries to the next request opened.
    open(&mut app, "other.http");
    assert_eq!(
        app.tab().env_source().map(|s| s.label.as_str()),
        Some("staging")
    );
}

#[tokio::test]
async fn a_token_extracted_later_reaches_a_request_opened_earlier() {
    // v1 filled the Vars tab with every variable's value when a request was
    // opened and let all of them override. A request opened before the login
    // held an empty token that beat the one extracted afterwards.
    let server = serve(|seen, _| match seen.path.as_str() {
        "/login" => Reply::json(200, r#"{"access_token":"tok-1"}"#),
        _ => Reply::json(200, "{}"),
    })
    .await;
    let root = collection("chain");
    write(&root.join(".env"), &format!("BASE={}\n", server.url));
    write(
        &root.join("me.http"),
        "GET {{BASE}}/me\nAuthorization: Bearer {{token}}\n",
    );
    write(&root.join("login.http"), "POST {{BASE}}/login\n");

    let (mut app, mut messages) = app(&root);
    open(&mut app, "me.http");
    section(&mut app, Section::Vars);
    let screen = render(&mut app);
    assert!(screen.contains("token"), "{screen}");
    assert!(
        screen.contains("not set"),
        "nothing defines the token yet:\n{screen}"
    );

    // Log in from a second tab, pulling the token out of the response.
    ctrl(&mut app, 't');
    open(&mut app, "login.http");
    assert_eq!(app.tabs.len(), 2);
    section(&mut app, Section::Scripts);
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "token = json .access_token");
    press(&mut app, KeyCode::Esc);
    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;
    assert_eq!(
        app.extracted.get("token").map(String::as_str),
        Some("tok-1")
    );

    // Back in the first tab, the token that did not exist when it was opened
    // is the one it sends.
    alt(&mut app, '1');
    let screen = render(&mut app);
    assert!(
        screen.contains("extracted"),
        "the Vars section names the layer:\n{screen}"
    );
    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;
    assert_eq!(
        server.last().unwrap().header("authorization"),
        Some("Bearer tok-1")
    );
}

#[tokio::test]
async fn a_typed_override_beats_the_environment() {
    let server = serve(|_, _| Reply::new(204)).await;
    let root = collection("override");
    write(&root.join(".env"), &format!("BASE={}\nID=1\n", server.url));
    write(&root.join("item.http"), "GET {{BASE}}/items/{{ID}}\n");

    let (mut app, mut messages) = app(&root);
    open(&mut app, "item.http");
    section(&mut app, Section::Vars);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    ctrl(&mut app, 'u');
    typed(&mut app, "42");
    press(&mut app, KeyCode::Enter);

    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;
    assert_eq!(server.last().unwrap().path, "/items/42");
}

#[tokio::test]
async fn a_variable_being_typed_into_looks_like_a_field() {
    // The cursor was the selected row's own colour, so a variable being
    // edited looked exactly like one merely selected.
    let root = collection("typing");
    write(&root.join(".env"), "ID=1\n");
    write(&root.join("item.http"), "GET https://x/items/{{ID}}\n");
    let (mut app, _messages) = app(&root);
    open(&mut app, "item.http");
    section(&mut app, Section::Vars);

    let caret = Color::Rgb(0xfa, 0xb3, 0x87);
    let well = Color::Rgb(0x18, 0x18, 0x25);
    let row = locate(&mut app, "Params").1 + 1;
    // Inside the request pane only: the sidebar is chrome, the well's colour.
    let colours = |app: &mut App| -> Vec<Color> {
        let terminal = draw(app, WIDTH, HEIGHT);
        (49..WIDTH - 1)
            .map(|x| terminal.backend().buffer()[(x, row)].bg)
            .collect()
    };

    let selected = colours(&mut app);
    assert!(!selected.contains(&caret) && !selected.contains(&well));

    press(&mut app, KeyCode::Enter);
    let typing = colours(&mut app);
    assert!(
        typing.contains(&caret),
        "the caret shows on the selected row"
    );
    assert!(
        typing.iter().filter(|colour| **colour == well).count() > 10,
        "the value sits in a field"
    );
    let status = render(&mut app).lines().last().unwrap().to_string();
    assert!(status.contains("Esc leaves it as it was"), "{status}");

    press(&mut app, KeyCode::Esc);
    assert!(!colours(&mut app).contains(&caret));
}

#[tokio::test]
async fn a_pasted_curl_command_becomes_the_request() {
    let root = collection("curl");
    let (mut app, _messages) = app(&root);

    alt(&mut app, 'k');
    app.paste(
        "curl -X POST 'https://api.example.com/users' \\\n  -H 'Content-Type: application/json' \\\n  --data-raw '{\"name\":\"Jane\"}'",
    );
    press(&mut app, KeyCode::Enter);

    let tab = app.tab();
    assert_eq!(tab.method, "POST");
    assert_eq!(tab.url.text(), "https://api.example.com/users");
    assert_eq!(tab.body_kind, binman_core::BodyKind::Json);
    assert_eq!(tab.body_text(), r#"{"name":"Jane"}"#);
    assert_eq!(
        tab.header_rows(),
        vec![("Content-Type".to_string(), "application/json".to_string())]
    );

    // And out again, to the clipboard and the screen.
    ctrl(&mut app, 'y');
    let copied = app.clipboard.clone().expect("handed to the clipboard");
    assert!(copied.starts_with("curl -X POST"), "{copied}");
    assert!(render(&mut app).contains("cURL"));
}

#[tokio::test]
async fn saving_writes_the_request_back_to_its_file() {
    let root = collection("save");
    let path = root.join("list.http");
    write(&path, "GET https://old.example.com/users\n");

    let (mut app, _messages) = app(&root);
    open(&mut app, "list.http");
    alt(&mut app, 'k');
    press(&mut app, KeyCode::End);
    typed(&mut app, "?page=2");
    assert!(app.tab().is_dirty());
    assert!(
        render(&mut app).contains("list.http •"),
        "the tab shows it has edits"
    );
    assert_eq!(
        app.tab().params.rows,
        vec![("page".to_string(), "2".to_string())]
    );

    ctrl(&mut app, 's');
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "GET https://old.example.com/users?page=2\n"
    );
    assert!(!app.tab().is_dirty());
}

#[tokio::test]
async fn a_request_with_no_file_is_saved_where_it_is_asked_to_be() {
    let root = collection("save-new");
    write(
        &root.join("users").join("list.http"),
        "GET https://x/users\n",
    );
    let (mut app, _messages) = app(&root);
    select(&mut app, "users");
    press(&mut app, KeyCode::Char('l'));

    alt(&mut app, 'k');
    typed(&mut app, "https://example.com/users/42");
    ctrl(&mut app, 's');
    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert!(screen.contains("Save the request to"), "{screen}");

    ctrl(&mut app, 'u');
    typed(&mut app, "users/get");
    press(&mut app, KeyCode::Enter);

    let path = root.join("users").join("get.http");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "GET https://example.com/users/42\n"
    );
    assert!(app.overlay.is_none());
    assert_eq!(app.tab().title, "get.http");
    assert_eq!(app.tab().origin, Some(Origin::File(path.clone())));
    assert!(!app.tab().is_dirty());
    assert_eq!(selected_name(&app), "get.http", "the tree shows the file");
    let screen = render(&mut app);
    assert!(
        screen.contains("list.http"),
        "the folder stays open:\n{screen}"
    );

    // The tab has a file now, so ⌃S writes to it without asking.
    alt(&mut app, 'k');
    typed(&mut app, "?v=2");
    ctrl(&mut app, 's');
    assert!(app.overlay.is_none());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "GET https://example.com/users/42?v=2\n"
    );
}

#[tokio::test]
async fn a_new_file_goes_nowhere_it_should_not() {
    let root = collection("save-refused");
    write(&root.join("a.http"), "GET https://a\n");
    let (mut app, _messages) = app(&root);
    alt(&mut app, 'k');
    typed(&mut app, "https://b");
    ctrl(&mut app, 's');

    ctrl(&mut app, 'u');
    typed(&mut app, "a.http");
    press(&mut app, KeyCode::Enter);
    let screen = render(&mut app);
    assert!(screen.contains("a.http is already there"), "{screen}");

    let error = |app: &App| match &app.overlay {
        Some(Overlay::SaveRequest(prompt)) => prompt.error.clone().unwrap_or_default(),
        _ => panic!("the prompt closed"),
    };
    for (name, complaint) in [
        ("../outside", "outside the collections"),
        ("q.graphql", "does not write them"),
        ("users/.hidden", "would be hidden"),
    ] {
        ctrl(&mut app, 'u');
        typed(&mut app, name);
        press(&mut app, KeyCode::Enter);
        assert!(error(&app).contains(complaint), "{name}: {}", error(&app));
    }
    assert_eq!(
        std::fs::read_to_string(root.join("a.http")).unwrap(),
        "GET https://a\n"
    );
    assert!(!root.parent().unwrap().join("outside.http").exists());

    ctrl(&mut app, 'u');
    typed(&mut app, "b.bru");
    press(&mut app, KeyCode::Enter);
    let saved =
        binman_core::formats::bru::parse(&std::fs::read_to_string(root.join("b.bru")).unwrap());
    assert_eq!(
        (saved.method.as_str(), saved.url.as_str()),
        ("GET", "https://b")
    );
}

#[tokio::test]
async fn a_bru_file_s_auth_opens_sends_and_saves_back() {
    let server = serve(|_, _| Reply::new(204)).await;
    let root = collection("bru-auth");
    write(&root.join(".env"), "token=tok-1\n");
    let path = root.join("me.bru");
    let original = format!(
        "get {{\n  url: {}/me\n  body: none\n  auth: bearer\n}}\n\nauth:bearer {{\n  token: {{{{token}}}}\n}}\n",
        server.url
    );
    write(&path, &original);

    let (mut app, mut messages) = app(&root);
    open(&mut app, "me.bru");
    section(&mut app, Section::Auth);
    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert!(screen.contains("Bearer token"), "{screen}");
    assert!(screen.contains("{{token}}"), "{screen}");

    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;
    assert_eq!(
        server.last().unwrap().header("authorization"),
        Some("Bearer tok-1")
    );

    ctrl(&mut app, 's');
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

    press(&mut app, KeyCode::Enter);
    ctrl(&mut app, 'u');
    typed(&mut app, "{{other}}");
    press(&mut app, KeyCode::Enter);
    assert!(
        app.tab().is_dirty(),
        "auth is saved, so it counts as an edit"
    );
    ctrl(&mut app, 's');
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        original.replace("{{token}}", "{{other}}")
    );
}

#[tokio::test]
async fn ctrl_c_cancels_a_request_in_flight() {
    let server = serve(|_, _| Reply::new(200).delayed(Duration::from_secs(5))).await;
    let root = collection("cancel");
    write(
        &root.join("slow.http"),
        &format!("GET {}/slow\n", server.url),
    );

    let (mut app, mut messages) = app(&root);
    open(&mut app, "slow.http");
    ctrl(&mut app, 'r');

    let screen = render(&mut app);
    assert!(
        screen.contains("Sending"),
        "no sign of the request:\n{screen}"
    );
    assert!(
        screen.contains("⌃C"),
        "the way out is not on screen:\n{screen}"
    );

    ctrl(&mut app, 'c');
    settle(&mut app, &mut messages).await;
    assert!(matches!(app.tab().response.outcome, Outcome::Cancelled));
    assert!(!app.tab().is_sending());
    assert!(render(&mut app).contains("Cancelled"));
}

#[tokio::test]
async fn an_event_stream_shows_every_event() {
    let server = serve(|_, _| {
        Reply::new(200)
            .header("Content-Type", "text/event-stream")
            .chunk(Duration::ZERO, "data: one\n\n")
            .chunk(Duration::from_millis(30), "data: two\n\n")
    })
    .await;
    let root = collection("stream");
    write(
        &root.join("events.http"),
        &format!("GET {}/events\n", server.url),
    );

    let (mut app, mut messages) = app(&root);
    open(&mut app, "events.http");
    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    assert!(screen.contains("data: one"), "{screen}");
    assert!(screen.contains("data: two"), "{screen}");
}

#[tokio::test]
async fn collections_and_specs_open_as_trees_of_requests() {
    let root = collection("formats");
    write(
        &root.join("api.postman_collection.json"),
        r#"{"variable":[{"key":"BASE","value":"https://api.example.com"}],
            "item":[{"name":"Auth","item":[{"name":"Login","request":{"method":"POST","url":"{{BASE}}/login"}}]}]}"#,
    );
    write(
        &root.join("openapi.yaml"),
        "openapi: 3.0.0\nservers:\n  - url: https://pets.example.com\npaths:\n  /pets/{id}:\n    get:\n      tags: [pets]\n      summary: Get a pet\n",
    );

    let (mut app, _messages) = app(&root);
    select(&mut app, "api.postman_collection.json");
    press(&mut app, KeyCode::Char('l'));
    open(&mut app, "Login");
    assert_eq!(app.tab().method, "POST");
    assert_eq!(app.tab().url.text(), "{{BASE}}/login");
    let url_bar = url_bar(&render(&mut app));
    assert!(
        url_bar.contains("https://api.example.com"),
        "the collection's own variables resolve: {url_bar}"
    );

    select(&mut app, "openapi.yaml");
    press(&mut app, KeyCode::Char('l'));
    open(&mut app, "/pets/{id}");
    assert_eq!(app.tab().url.text(), "https://pets.example.com/pets/{{id}}");
    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert!(
        screen.contains("GET    /pets/{id}"),
        "the operation sits in the tree under its tag:\n{screen}"
    );
    assert!(
        screen.contains("pets  1"),
        "the tag counts its operations:\n{screen}"
    );
}

#[tokio::test]
async fn find_opens_a_request_from_anywhere() {
    let root = collection("find");
    write(&root.join("a").join("list.http"), "GET https://x/list\n");
    write(
        &root.join("b").join("create.http"),
        "POST https://x/create\n",
    );

    let (mut app, _messages) = app(&root);
    ctrl(&mut app, 'f');
    assert!(render(&mut app).contains("Find a request"));
    typed(&mut app, "create");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.tab().title, "create.http");
    assert_eq!(app.tab().method, "POST");
}

#[tokio::test]
async fn history_sends_a_request_again() {
    let server = serve(|_, _| Reply::new(200).body("pong")).await;
    let root = collection("history");
    write(
        &root.join("ping.http"),
        &format!("GET {}/ping\n", server.url),
    );

    let (mut app, mut messages) = app(&root);
    open(&mut app, "ping.http");
    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;

    ctrl(&mut app, 'h');
    let screen = render(&mut app);
    assert!(screen.contains("History"), "{screen}");
    assert!(screen.contains("/ping"), "{screen}");
    press(&mut app, KeyCode::Enter);
    settle(&mut app, &mut messages).await;
    assert_eq!(server.hits(), 2);
}

#[tokio::test]
async fn ctrl_q_always_quits() {
    // Raw mode disables ISIG, so ⌃C is not a way out and ⌃Q is the only one.
    // A modal that swallowed it would trap people inside the program.
    let root = collection("quit");
    write(&root.join(".env"), "A=1\n");
    write(&root.join("a.http"), "GET https://x\n");
    let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);

    let openers = [
        (
            "nothing open",
            KeyEvent::new(KeyCode::Null, KeyModifiers::NONE),
        ),
        (
            "command palette",
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        ),
        ("help", KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)),
        (
            "search",
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
        ),
        (
            "environments",
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
        ),
    ];
    for (what, opener) in openers {
        let (mut app, _messages) = app(&root);
        binman::app::keys::handle(&mut app, opener);
        binman::app::keys::handle(&mut app, ctrl_q);
        assert!(app.should_quit, "⌃Q did not quit with {what} open");
    }
}

#[tokio::test]
async fn help_closes_on_any_key_as_it_claims() {
    let root = collection("help");
    let (mut app, _messages) = app(&root);
    press(&mut app, KeyCode::F(1));
    assert!(render(&mut app).contains("Any key closes this"));
    press(&mut app, KeyCode::Char('x'));
    assert!(!render(&mut app).contains("Any key closes this"));
}

#[tokio::test]
async fn the_splash_greets_and_any_key_dismisses_it() {
    let root = collection("splash");
    let (mut app, _messages) = app(&root);
    app.show_splash();

    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert!(
        screen.contains("an HTTP client for the terminal"),
        "{screen}"
    );
    assert!(
        screen.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))),
        "{screen}"
    );
    assert!(screen.contains("Collections  binman-ui-splash"), "{screen}");
    assert!(screen.contains("Any key to begin"), "{screen}");

    press(&mut app, KeyCode::Char('x'));
    assert!(app.overlay.is_none());
}

#[tokio::test]
async fn the_splash_falls_back_in_a_narrow_terminal() {
    let root = collection("splash-narrow");
    let (mut app, _messages) = app(&root);
    app.show_splash();

    let terminal = draw(&mut app, 40, 24);
    let rows: Vec<String> = terminal
        .backend()
        .buffer()
        .content()
        .chunks(40)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect();
    assert!(rows.iter().any(|row| row.contains("binman")));
    assert!(
        !rows.iter().any(|row| row.contains('\u{2588}')),
        "the block wordmark should not be drawn when it does not fit:\n{}",
        rows.join("\n")
    );
}

#[tokio::test]
async fn the_panels_sit_where_v1_put_them() {
    let root = collection("layout");
    write(&root.join(".env"), "BASE=http://127.0.0.1:65000\n");
    write(&root.join("list.http"), "GET {{BASE}}/users\n");
    let (mut app, _messages) = app(&root);
    open(&mut app, "list.http");

    let screen = render(&mut app);
    println!("\n{screen}\n");
    let rows: Vec<Vec<char>> = screen.lines().map(|line| line.chars().collect()).collect();
    let row_of = |needle: &str| {
        screen
            .lines()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not drawn:\n{screen}"))
    };

    // The URL bar runs the whole width, above the collections.
    let url = row_of("GET {{BASE}}/users") - 1;
    assert_eq!(rows[url].first(), Some(&'╭'), "{screen}");
    assert_eq!(rows[url].get(WIDTH as usize - 1), Some(&'╮'), "{screen}");
    let collections = row_of("Collections");
    assert_eq!(
        collections,
        url + 3,
        "the collections start under the URL bar:\n{screen}"
    );

    // The collections are v1's 48 wide, beside the request over the response.
    assert_eq!(rows[collections].get(47), Some(&'╮'), "{screen}");
    assert_eq!(rows[collections].get(48), Some(&'╭'), "{screen}");
    let request = row_of("Params");
    let response = row_of("Cookies");
    assert_eq!(request, collections, "{screen}");
    let request_rows = (response - request) as f64;
    let response_rows = (HEIGHT as usize - 1 - response) as f64;
    assert!(
        (response_rows / request_rows - 2.5).abs() < 0.3,
        "request and response should split two to five:\n{screen}"
    );
}

#[tokio::test]
async fn the_header_keeps_the_environment_picker_when_squeezed() {
    let root = collection("narrow");
    write(&root.join(".env"), "BASE=http://127.0.0.1:65000\n");
    let (mut app, _messages) = app(&root);

    let terminal = draw(&mut app, 22, 12);
    let header: String = (0..22)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_string())
        .collect();
    assert!(header.starts_with(" ✻"), "[{header}]");
    assert!(
        header.trim_end().ends_with("default ▾"),
        "the name gives way, not the picker: [{header}]"
    );
}

#[tokio::test]
async fn the_mouse_reaches_what_the_keys_do() {
    let first = serve(|_, _| Reply::new(200).body("first")).await;
    let second = serve(|_, _| Reply::new(200).body("second")).await;
    let root = collection("mouse");
    write(&root.join(".env"), &format!("BASE={}\n", first.url));
    write(
        &root.join(".env.staging"),
        &format!("BASE={}\n", second.url),
    );
    write(
        &root.join("users").join("list.http"),
        "GET {{BASE}}/users\nAccept: */*\n",
    );

    let (mut app, mut messages) = app(&root);
    click(&mut app, "users");
    click(&mut app, "list.http");
    assert_eq!(app.tab().title, "list.http", "a click in the tree opens it");

    click(&mut app, "Headers");
    assert_eq!(app.focus, Pane::Request);
    assert_eq!(app.tab().section, Section::Headers);

    click(&mut app, "default ▾");
    assert!(
        matches!(app.overlay, Some(Overlay::Picker(_))),
        "the picker drops its list"
    );
    click(&mut app, "staging");
    assert_eq!(
        app.tab().env_source().map(|s| s.label.as_str()),
        Some("staging")
    );

    click(&mut app, " Send ");
    settle(&mut app, &mut messages).await;
    assert_eq!(second.hits(), 1, "staging's server should have been asked");
    assert_eq!(first.hits(), 0);

    click(&mut app, "Cookies");
    assert_eq!(app.tab().response.view, View::Cookies);

    click(&mut app, "+");
    assert_eq!(app.tabs.len(), 2);
    click(&mut app, "list.http");
    assert_eq!(app.active, 0, "a click on a tab goes to it");
}

#[tokio::test]
async fn a_click_on_the_selected_row_edits_it_and_a_click_away_keeps_it() {
    let root = collection("mouse-rows");
    write(
        &root.join("a.http"),
        "GET https://example.com/users\nAccept: */*\n",
    );
    let (mut app, _messages) = app(&root);
    open(&mut app, "a.http");

    click(&mut app, "Headers");
    click(&mut app, "Accept");
    assert!(
        app.tab().headers.edit.is_some(),
        "the selected row, clicked, is edited"
    );
    typed(&mut app, "-Language");
    click(&mut app, "Params");
    assert!(app.tab().headers.edit.is_none());
    assert_eq!(app.tab().headers.rows[0].0, "Accept-Language");
    assert_eq!(app.tab().section, Section::Params);
}

#[tokio::test]
async fn a_click_in_the_url_puts_the_cursor_there() {
    let root = collection("mouse-url");
    write(&root.join("a.http"), "GET https://example.com/users\n");
    let (mut app, _messages) = app(&root);
    open(&mut app, "a.http");

    click(&mut app, "/users");
    assert_eq!(app.focus, Pane::Url);
    assert_eq!(app.tab().url.cursor(), "https://example.com".len());
}

#[tokio::test]
async fn the_wheel_scrolls_what_is_under_it() {
    let body: String = (1..=80).map(|n| format!("line {n}\n")).collect();
    let server = serve(move |_, _| Reply::new(200).body(&body)).await;
    let root = collection("wheel");
    write(
        &root.join("long.http"),
        &format!("GET {}/long\n", server.url),
    );
    let (mut app, mut messages) = app(&root);
    open(&mut app, "long.http");
    ctrl(&mut app, 'r');
    settle(&mut app, &mut messages).await;

    let (column, row) = locate(&mut app, "line 1");
    assert_eq!(
        app.focus,
        Pane::Collections,
        "the response does not have focus"
    );
    mouse(&mut app, MouseEventKind::ScrollDown, column, row);
    assert_eq!(app.tab().response.scroll, 3);
}

#[tokio::test]
async fn a_click_away_closes_a_list_and_a_click_on_an_entry_runs_it() {
    let root = collection("mouse-lists");
    let (mut app, _messages) = app(&root);

    ctrl(&mut app, 'k');
    click(&mut app, "quit");
    assert!(app.overlay.is_none(), "a click away closes the list");
    assert!(!app.should_quit, "the quit hint is not a button");

    ctrl(&mut app, 'k');
    click(&mut app, "New tab");
    assert_eq!(app.tabs.len(), 2);
}

#[tokio::test]
async fn the_environment_list_drops_from_the_picker() {
    let root = collection("dropdown");
    write(&root.join(".env"), "A=1\n");
    write(&root.join(".env.staging"), "A=2\n");
    let (mut app, _messages) = app(&root);
    assert!(
        render(&mut app)
            .lines()
            .next()
            .unwrap()
            .ends_with("default ▾")
    );

    ctrl(&mut app, 'e');
    let screen = render(&mut app);
    println!("\n{screen}\n");
    let rows: Vec<&str> = screen.lines().collect();
    assert!(
        rows[1].contains("Environments"),
        "the list hangs under the header:\n{screen}"
    );
    assert!(
        rows[1].ends_with("╮"),
        "flush with the picker's right end:\n{screen}"
    );
}

/// Relative luminance, per WCAG.
fn luminance(colour: Color) -> f64 {
    let Color::Rgb(r, g, b) = colour else {
        return 0.0;
    };
    let channel = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(fg: Color, bg: Color) -> f64 {
    let (a, b) = (luminance(fg), luminance(bg));
    let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

#[tokio::test]
async fn the_header_and_status_line_are_legible() {
    // binsql's hint strip was once `dim` on `surface`, a contrast of about
    // 1.6: present on screen and unreadable. Text on a background dark enough
    // to hide it is a bug, not a style.
    const FLOOR: f64 = 3.0;
    let root = collection("contrast");
    write(&root.join(".env"), "BASE=http://127.0.0.1:65000\n");
    write(&root.join("list.http"), "GET {{BASE}}/users\n");
    let (mut app, _messages) = app(&root);
    open(&mut app, "list.http");

    let terminal = draw(&mut app, WIDTH, HEIGHT);
    let buffer = terminal.backend().buffer();
    for y in [0u16, HEIGHT - 1] {
        for x in 0..WIDTH {
            let cell = &buffer[(x, y)];
            if cell.symbol().trim().is_empty() {
                continue;
            }
            let ratio = contrast(cell.fg, cell.bg);
            assert!(
                ratio >= FLOOR,
                "row {y} col {x} {:?} has contrast {ratio:.2} (fg {:?} on bg {:?})",
                cell.symbol(),
                cell.fg,
                cell.bg
            );
        }
    }
}

fn distance(a: Color, b: Color) -> i32 {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return i32::MAX;
    };
    (i32::from(ar) - i32::from(br)).abs()
        + (i32::from(ag) - i32::from(bg)).abs()
        + (i32::from(ab) - i32::from(bb)).abs()
}

#[tokio::test]
async fn an_open_modal_dims_what_is_behind_it() {
    let root = collection("scrim");
    let (mut app, _messages) = app(&root);
    let colour_at = |app: &mut App| {
        let terminal = draw(app, WIDTH, HEIGHT);
        terminal.backend().buffer()[(1, 0)].fg
    };

    let bright = colour_at(&mut app);
    press(&mut app, KeyCode::F(1));
    let dimmed = colour_at(&mut app);
    let background = Color::Rgb(0x1e, 0x1e, 0x2e);
    assert_ne!(bright, dimmed, "the layout behind a modal should recede");
    assert!(distance(dimmed, background) < distance(bright, background));

    press(&mut app, KeyCode::Esc);
    assert_eq!(
        colour_at(&mut app),
        bright,
        "closing should restore the layout"
    );
}

#[tokio::test]
async fn every_modal_hugs_its_content() {
    let root = collection("hug");
    write(&root.join(".env"), "A=1\n");
    write(&root.join("a.http"), "GET https://x\n");

    let cases = [
        (
            "help",
            KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
            "Help",
        ),
        (
            "palette",
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            "Commands",
        ),
        (
            "search",
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            "Find a request",
        ),
        (
            "environments",
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
            "Environments",
        ),
    ];
    for (what, opener, title) in cases {
        let (mut app, _messages) = app(&root);
        binman::app::keys::handle(&mut app, opener);
        assert!(app.overlay.is_some(), "{what} did not open");
        let screen = render(&mut app);
        let rows: Vec<Vec<char>> = screen.lines().map(|line| line.chars().collect()).collect();
        let top = screen
            .lines()
            .position(|line| line.contains(title))
            .unwrap_or_else(|| panic!("{what} not drawn:\n{screen}"));

        // The modal's own corners, found from its title: the panes around
        // it have borders of their own in the same rows.
        let line = screen.lines().nth(top).unwrap();
        let title_at = line[..line.find(title).unwrap()].chars().count();
        let left = rows[top][..title_at]
            .iter()
            .rposition(|c| *c == '╭')
            .unwrap_or_else(|| panic!("{what} has no corner:\n{screen}"));
        let right = rows[top][left..]
            .iter()
            .position(|c| *c == '╮')
            .unwrap_or_else(|| panic!("{what} has no corner:\n{screen}"))
            + left;
        let bottom = (top + 1..rows.len())
            .find(|&row| rows[row].get(left) == Some(&'╰'))
            .unwrap_or_else(|| panic!("{what} not closed:\n{screen}"));

        let blank_rows = rows[top + 1..bottom]
            .iter()
            .filter(|row| {
                row.get(left + 1..right)
                    .is_none_or(|inside| inside.iter().all(|c| c.is_whitespace()))
            })
            .count();
        let inner_rows = bottom - top - 1;
        assert!(
            blank_rows * 3 <= inner_rows,
            "{what} is mostly empty: {blank_rows} blank of {inner_rows}:\n{screen}"
        );
        assert!(
            bottom - top + 1 < HEIGHT as usize,
            "{what} fills the screen:\n{screen}"
        );
    }
}

#[tokio::test]
async fn the_help_screen_lists_the_keys_that_work() {
    let root = collection("helpkeys");
    let (mut app, _messages) = app(&root);
    press(&mut app, KeyCode::F(1));
    let screen = render(&mut app);
    println!("\n{screen}\n");
    for binding in [
        "Send the request",
        "Environments",
        "Copy as cURL",
        "Change the method",
    ] {
        assert!(screen.contains(binding), "{binding} missing:\n{screen}");
    }
    assert!(matches!(app.overlay, Some(Overlay::Help)));
}
