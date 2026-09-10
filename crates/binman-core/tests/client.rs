//! The HTTP client against a real socket.

#[path = "support/server.rs"]
mod server;

use std::time::{Duration, Instant};

use binman_core::{Client, Error, Prepared};
use server::{Reply, serve};
use tokio_util::sync::CancellationToken;

fn get(url: &str) -> Prepared {
    Prepared {
        method: "GET".into(),
        url: url.into(),
        ..Prepared::default()
    }
}

fn client() -> Client {
    Client::with(Some(Duration::from_secs(5)), None)
}

#[tokio::test]
async fn sends_the_request_and_reads_the_response() {
    let server = serve(|_, _| Reply::json(201, r#"{"ok":true}"#)).await;
    let prepared = Prepared {
        method: "POST".into(),
        url: format!("{}/items?x=1", server.url),
        headers: vec![("X-Custom".into(), "hello".into())],
        body: b"payload".to_vec(),
    };

    let exchange = client()
        .send(&prepared, &CancellationToken::new(), |_| {})
        .await
        .expect("sends");

    assert_eq!(exchange.status, 201);
    assert_eq!(exchange.reason, "Created");
    assert_eq!(exchange.version, "HTTP/1.1");
    assert_eq!(exchange.text(), r#"{"ok":true}"#);
    assert_eq!(exchange.header("Content-Type"), Some("application/json"));
    assert!(!exchange.streamed);

    let seen = server.last().expect("the server was reached");
    assert_eq!(seen.method, "POST");
    assert_eq!(seen.path, "/items?x=1");
    assert_eq!(seen.header("x-custom"), Some("hello"));
    assert_eq!(seen.text(), "payload");
    assert!(
        seen.header("user-agent")
            .is_some_and(|agent| agent.starts_with("binman/")),
        "{:?}",
        seen.header("user-agent")
    );
}

#[tokio::test]
async fn the_trace_is_about_this_request() {
    let server = serve(|_, _| Reply::new(204)).await;

    let exchange = client()
        .send(&get(&server.url), &CancellationToken::new(), |_| {})
        .await
        .expect("sends");
    assert_eq!(
        exchange.trace.dns, None,
        "an IP address has nothing to resolve"
    );
    assert!(
        exchange.trace.connect.is_some(),
        "the new connection was timed"
    );
    assert!(exchange.trace.first_byte > Duration::ZERO);
    assert!(exchange.trace.total >= exchange.trace.first_byte);

    let by_name = server.url.replace("127.0.0.1", "localhost");
    let exchange = client()
        .send(&get(&by_name), &CancellationToken::new(), |_| {})
        .await
        .expect("sends");
    assert!(
        exchange.trace.dns.is_some(),
        "a name was resolved and timed"
    );
}

#[tokio::test]
async fn cookies_are_kept_and_sent_back() {
    let server = serve(|seen, _| match seen.path.as_str() {
        "/login" => Reply::new(204).header("Set-Cookie", "session=abc; Path=/; HttpOnly"),
        _ => Reply::new(200),
    })
    .await;
    let client = client();

    let login = client
        .send(
            &get(&format!("{}/login", server.url)),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .expect("sends");
    assert_eq!(login.set_cookies.len(), 1);
    assert_eq!(login.set_cookies[0].name, "session");
    assert_eq!(login.set_cookies[0].value, "abc");
    assert!(
        login.set_cookies[0]
            .attributes
            .contains(&"HttpOnly".to_string())
    );
    assert_eq!(login.jar, vec![("session".to_string(), "abc".to_string())]);

    client
        .send(
            &get(&format!("{}/me", server.url)),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .expect("sends");
    assert_eq!(server.last().unwrap().header("cookie"), Some("session=abc"));
}

#[tokio::test]
async fn cancelling_stops_a_request_that_is_waiting() {
    let server = serve(|_, _| Reply::new(200).delayed(Duration::from_secs(5))).await;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        trigger.cancel();
    });

    let started = Instant::now();
    let result = client().send(&get(&server.url), &cancel, |_| {}).await;
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn an_event_stream_arrives_an_event_at_a_time() {
    let server = serve(|_, _| {
        Reply::new(200)
            .header("Content-Type", "text/event-stream")
            .chunk(Duration::ZERO, "data: one\n\n")
            .chunk(Duration::from_millis(30), "data: two\n\n")
    })
    .await;

    let mut events = Vec::new();
    let exchange = client()
        .send(&get(&server.url), &CancellationToken::new(), |event| {
            events.push(event)
        })
        .await
        .expect("streams");
    assert!(exchange.streamed);
    assert_eq!(events, vec!["data: one\n\n", "data: two\n\n"]);
    assert_eq!(exchange.text(), "data: one\n\ndata: two\n\n");
}

#[tokio::test]
async fn a_timeout_says_where_it_is_set() {
    let server = serve(|_, _| Reply::new(200).delayed(Duration::from_secs(3))).await;
    let client = Client::with(Some(Duration::from_millis(200)), None);
    let error = client
        .send(&get(&server.url), &CancellationToken::new(), |_| {})
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("no response within 200ms"), "{error}");
    assert!(error.contains("TIMEOUT"), "{error}");
}

#[tokio::test]
async fn what_cannot_be_sent_is_refused_before_it_leaves() {
    let error = client()
        .send(&get("{{BASE}}/users"), &CancellationToken::new(), |_| {})
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)), "{error}");

    let mut bad_header = get("http://127.0.0.1:9/");
    bad_header
        .headers
        .push(("X-Bad".into(), "line\nbreak".into()));
    let error = client()
        .send(&bad_header, &CancellationToken::new(), |_| {})
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)), "{error}");
}

#[tokio::test]
async fn a_refused_connection_says_why() {
    let error = client()
        .send(
            &get("http://127.0.0.1:9/"),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap_err();
    let Error::Http(message) = &error else {
        panic!("expected an HTTP failure, got {error:?}");
    };
    assert!(message.to_lowercase().contains("refused"), "{message}");
}
