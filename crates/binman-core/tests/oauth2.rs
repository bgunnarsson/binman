//! The client credentials grant against a real token endpoint.

#[path = "support/server.rs"]
mod server;

use std::time::Duration;

use binman_core::Client;
use binman_core::oauth2::Grant;
use server::{Reply, serve};
use tokio_util::sync::CancellationToken;

fn grant(url: &str, scope: &str) -> Grant {
    Grant {
        token_url: url.to_string(),
        client_id: "id".into(),
        client_secret: "secret".into(),
        scope: scope.to_string(),
    }
}

fn client() -> Client {
    Client::with(Some(Duration::from_secs(5)), None)
}

async fn fetch(client: &Client, grant: &Grant) -> binman_core::Result<String> {
    client
        .client_credentials(grant, &CancellationToken::new())
        .await
}

#[tokio::test]
async fn sends_the_grant_as_a_form() {
    let server = serve(|_, _| {
        Reply::json(200, r#"{"access_token":"tok-1","token_type":"Bearer","expires_in":3600}"#)
    })
    .await;

    let token = fetch(&client(), &grant(&server.url, "read write"))
        .await
        .expect("a token");
    assert_eq!(token, "tok-1");

    // RFC 6749 §4.4.2: a form POST, not JSON.
    let seen = server.last().unwrap();
    assert_eq!(seen.method, "POST");
    assert!(
        seen.header("content-type")
            .is_some_and(|value| value.starts_with("application/x-www-form-urlencoded"))
    );
    assert_eq!(seen.header("accept"), Some("application/json"));
    let form = seen.form();
    for (key, value) in [
        ("grant_type", "client_credentials"),
        ("client_id", "id"),
        ("client_secret", "secret"),
        ("scope", "read write"),
    ] {
        assert!(
            form.contains(&(key.to_string(), value.to_string())),
            "{key}={value} missing from {form:?}"
        );
    }
}

#[tokio::test]
async fn an_empty_scope_is_not_sent() {
    let server = serve(|_, _| Reply::json(200, r#"{"access_token":"tok","expires_in":60}"#)).await;
    fetch(&client(), &grant(&server.url, "")).await.expect("a token");
    let form = server.last().unwrap().form();
    assert!(
        !form.iter().any(|(key, _)| key == "scope"),
        "scope was sent: {form:?}"
    );
}

#[tokio::test]
async fn a_token_is_reused_until_it_nears_expiry() {
    let server = serve(|_, call| {
        Reply::json(200, &format!(r#"{{"access_token":"tok-{call}","expires_in":3600}}"#))
    })
    .await;
    let client = client();
    let grant = grant(&server.url, "");

    assert_eq!(fetch(&client, &grant).await.unwrap(), "tok-1");
    assert_eq!(fetch(&client, &grant).await.unwrap(), "tok-1");
    assert_eq!(server.hits(), 1, "the second call should have used the cache");
}

#[tokio::test]
async fn a_token_inside_the_skew_window_is_fetched_again() {
    // Valid for ten more seconds, which is inside the thirty-second margin:
    // handing it out would mean a request that leaves authorised and arrives
    // expired.
    let server = serve(|_, call| {
        Reply::json(200, &format!(r#"{{"access_token":"tok-{call}","expires_in":10}}"#))
    })
    .await;
    let client = client();
    let grant = grant(&server.url, "");

    fetch(&client, &grant).await.unwrap();
    assert_eq!(fetch(&client, &grant).await.unwrap(), "tok-2");
    assert_eq!(server.hits(), 2);
}

#[tokio::test]
async fn a_token_without_an_expiry_gets_an_hour() {
    let server = serve(|_, _| Reply::json(200, r#"{"access_token":"tok"}"#)).await;
    let client = client();
    let grant = grant(&server.url, "");

    fetch(&client, &grant).await.unwrap();
    fetch(&client, &grant).await.unwrap();
    // An absent expires_in read as zero would make the cache useless and
    // fetch once per request.
    assert_eq!(server.hits(), 1);
}

#[tokio::test]
async fn an_expiry_sent_as_a_string_is_honoured() {
    let server = serve(|_, _| Reply::json(200, r#"{"access_token":"tok","expires_in":"3600"}"#)).await;
    let client = client();
    let grant = grant(&server.url, "");
    fetch(&client, &grant).await.unwrap();
    fetch(&client, &grant).await.unwrap();
    assert_eq!(server.hits(), 1);
}

#[tokio::test]
async fn a_refusal_is_reported_whole_and_not_cached() {
    let server = serve(|_, call| {
        if call == 1 {
            Reply::json(401, r#"{"error":"invalid_client"}"#)
        } else {
            Reply::json(200, r#"{"access_token":"tok","expires_in":3600}"#)
        }
    })
    .await;
    let client = client();
    let grant = grant(&server.url, "");

    let error = fetch(&client, &grant).await.unwrap_err().to_string();
    assert!(error.contains("401"), "{error}");
    assert!(error.contains("invalid_client"), "{error}");

    assert_eq!(fetch(&client, &grant).await.unwrap(), "tok", "the retry reached the endpoint");
}

#[tokio::test]
async fn a_success_without_a_token_is_an_error() {
    let server = serve(|_, _| Reply::json(200, r#"{"token_type":"Bearer","expires_in":3600}"#)).await;
    assert!(fetch(&client(), &grant(&server.url, "")).await.is_err());
}

#[tokio::test]
async fn a_body_that_is_not_json_says_so() {
    let server = serve(|_, _| Reply::new(200).body("<html>login required</html>")).await;
    let error = fetch(&client(), &grant(&server.url, "")).await.unwrap_err().to_string();
    assert!(error.contains("decode"), "{error}");
}

#[tokio::test]
async fn missing_credentials_fail_before_any_request() {
    let server = serve(|_, _| Reply::json(200, r#"{"access_token":"tok"}"#)).await;
    let client = client();

    let mut no_url = grant(&server.url, "");
    no_url.token_url.clear();
    let mut no_id = grant(&server.url, "");
    no_id.client_id.clear();

    assert!(fetch(&client, &no_url).await.is_err());
    assert!(fetch(&client, &no_id).await.is_err());
    assert_eq!(server.hits(), 0);
}

#[tokio::test]
async fn a_cancelled_fetch_produces_no_token() {
    let server = serve(|_, _| Reply::json(200, r#"{"access_token":"tok"}"#)).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = client()
        .client_credentials(&grant(&server.url, ""), &cancel)
        .await;
    assert!(result.is_err());
}
