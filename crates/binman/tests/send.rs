//! `binman send`, against a real server and a real collection on disk.

#[path = "../../binman-core/tests/support/server.rs"]
mod server;

use std::path::{Path, PathBuf};
use std::time::Duration;

use binman::send::{self, Options};
use binman_core::history::History;
use binman_core::{Client, Workspace};
use server::{Reply, serve};

/// One directory per test — these run concurrently in one process.
fn collection(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("binman-send-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create a collection");
    root.canonicalize().expect("a real path")
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| arg.to_string()).collect()
}

fn history(root: &Path) -> History {
    History::at(root.join(".state").join("history.jsonl"))
}

struct Sent {
    result: anyhow::Result<()>,
    out: String,
    err: String,
}

async fn send(root: &Path, list: &[&str]) -> Sent {
    let options = Options::parse(args(list)).expect("the arguments parse");
    let client = Client::with(Some(Duration::from_secs(5)), None);
    let mut collections =
        Workspace::load_at(root.join(".state").join("collections.json"), None, None)
            .expect("no collections");
    collections.add_argument(root).expect("the collection");
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let result = send::run(
        &options,
        &collections,
        &client,
        &history(root),
        &mut out,
        &mut err,
    )
    .await;
    Sent {
        result,
        out: String::from_utf8(out).unwrap(),
        err: String::from_utf8(err).unwrap(),
    }
}

#[tokio::test]
async fn sends_with_the_chosen_environment_and_the_values_given() {
    let server = serve(|seen, _| Reply::new(200).body(&format!("hello {}", seen.path))).await;
    let root = collection("env");
    write(&root.join(".env"), "BASE=http://127.0.0.1:1\nID=1\n");
    write(
        &root.join(".env.staging"),
        &format!("BASE={}\nID=2\n", server.url),
    );
    let file = root.join("items").join("get.http");
    write(&file, "GET {{BASE}}/items/{{ID}}\nX-Who: {{WHO}}\n");

    let sent = send(
        &root,
        &[
            file.to_str().unwrap(),
            "--env",
            "staging",
            "--var",
            "ID=42",
            "--var",
            "WHO=me",
        ],
    )
    .await;
    sent.result.expect("sent");

    let seen = server.last().expect("the server was reached");
    assert_eq!(seen.path, "/items/42", "--var outranks the environment");
    assert_eq!(seen.header("x-who"), Some("me"));
    assert_eq!(sent.out, "hello /items/42", "only the body is on stdout");
    assert!(sent.err.starts_with("HTTP/1.1 200 OK"), "{}", sent.err);
    assert_eq!(
        history(&root).load(10).unwrap().len(),
        1,
        "it is in the history"
    );
}

#[tokio::test]
async fn include_puts_the_status_and_headers_ahead_of_the_body() {
    let server = serve(|_, _| Reply::new(201).header("X-Trace", "abc").body("made")).await;
    let root = collection("include");
    let file = root.join("make.http");
    write(&file, &format!("POST {}/things\n", server.url));

    let sent = send(&root, &[file.to_str().unwrap(), "-i"]).await;
    sent.result.expect("sent");
    assert!(
        sent.out.starts_with("HTTP/1.1 201 Created\n"),
        "{}",
        sent.out
    );
    assert!(
        sent.out.to_lowercase().contains("\nx-trace: abc\n"),
        "{}",
        sent.out
    );
    assert!(sent.out.ends_with("\n\nmade"), "{}", sent.out);
    assert_eq!(sent.err, "");
}

#[tokio::test]
async fn an_unset_variable_sends_nothing() {
    let server = serve(|_, _| Reply::new(200)).await;
    let root = collection("unset");
    let file = root.join("me.http");
    write(
        &file,
        &format!(
            "GET {}/me\nAuthorization: Bearer {{{{token}}}}\n",
            server.url
        ),
    );

    let error = send(&root, &[file.to_str().unwrap()])
        .await
        .result
        .unwrap_err()
        .to_string();
    assert!(error.contains("token is not set"), "{error}");
    assert_eq!(server.hits(), 0);
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_is_an_error() {
    let root = collection("unreachable");
    let file = root.join("down.http");
    write(&file, "GET http://127.0.0.1:1/down\n");
    assert!(send(&root, &[file.to_str().unwrap()]).await.result.is_err());
}

#[tokio::test]
async fn an_unknown_environment_names_the_ones_there_are() {
    let root = collection("unknown-env");
    write(&root.join(".env"), "A=1\n");
    write(&root.join(".env.staging"), "A=2\n");
    let file = root.join("a.http");
    write(&file, "GET https://example.com\n");

    let error = send(&root, &[file.to_str().unwrap(), "--env", "prod"])
        .await
        .result
        .unwrap_err()
        .to_string();
    assert!(error.contains("default, staging"), "{error}");

    let bare = collection("no-env");
    let file = bare.join("a.http");
    write(&file, "GET https://example.com\n");
    let error = send(&bare, &[file.to_str().unwrap(), "--env", "prod"])
        .await
        .result
        .unwrap_err()
        .to_string();
    assert!(error.contains("There are no environments"), "{error}");
}

#[test]
fn the_arguments_are_read_and_checked() {
    let options = Options::parse(args(&[
        "a.http",
        "--env",
        "dev",
        "--var",
        "Q=a=b",
        "--include",
    ]))
    .unwrap();
    assert_eq!(options.file, PathBuf::from("a.http"));
    assert_eq!(options.env.as_deref(), Some("dev"));
    assert_eq!(options.vars["Q"], "a=b", "only the first = splits");
    assert!(options.include);

    for (list, complaint) in [
        (&[][..], "needs a request file"),
        (&["a.http", "--var", "Q"][..], "NAME=VALUE"),
        (&["a.http", "--env"][..], "--env needs"),
        (&["a.http", "--bogus"][..], "--bogus"),
        (&["a.http", "b.http"][..], "one request file"),
    ] {
        let error = Options::parse(args(list)).unwrap_err().to_string();
        assert!(error.contains(complaint), "{list:?}: {error}");
    }
}

#[test]
fn the_binary_says_what_send_needs() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_binman"))
        .arg("send")
        .output()
        .expect("binman runs");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("needs a request file"), "{stderr}");

    let help = std::process::Command::new(env!("CARGO_BIN_EXE_binman"))
        .arg("--help")
        .output()
        .expect("binman runs");
    assert!(String::from_utf8_lossy(&help.stdout).contains("binman send <file>"));
}
