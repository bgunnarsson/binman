//! Sending a request, and what came back.
//!
//! Every send opens its own connection rather than borrowing a pooled one. It
//! costs a handshake per request, which an interactive client can afford, and
//! buys a trace that is always about this request: DNS and connect are timed on
//! the connection this request used, so two tabs sending at once cannot trade
//! timings. The cookie jar is the one thing shared between sends, so a login
//! followed by a call behaves as it would in a browser.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Once};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use reqwest::cookie::{CookieStore, Jar};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{HeaderName, HeaderValue};
use tokio_util::sync::CancellationToken;
use tower_layer::Layer;
use tower_service::Service;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::oauth2::{Grant, TokenCache};
use crate::request::header;

const USER_AGENT: &str = concat!("binman/", env!("CARGO_PKG_VERSION"));

/// A request with every variable resolved and its body encoded: exactly what
/// goes on the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prepared {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Exchange {
    pub status: u16,
    pub reason: String,
    pub version: String,
    /// Where the response came from, after any redirects.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// What this response asked to be stored.
    pub set_cookies: Vec<SetCookie>,
    /// What the jar now holds for this host, and will send next time.
    pub jar: Vec<(String, String)>,
    pub trace: Trace,
    /// Arrived as server-sent events, a piece at a time.
    pub streamed: bool,
}

impl Exchange {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        header(&self.headers, name)
    }
}

/// How long each phase of one send took.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Trace {
    /// `None` when there was nothing to resolve, as for an IP address.
    pub dns: Option<Duration>,
    /// The TCP connection and, for HTTPS, the TLS handshake on top of it. The
    /// HTTP library makes those one step, so they are timed as one.
    pub connect: Option<Duration>,
    /// From sending to the response's headers.
    pub first_byte: Duration,
    pub total: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCookie {
    pub name: String,
    pub value: String,
    pub attributes: Vec<String>,
}

pub struct Client {
    timeout: Option<Duration>,
    /// The client certificate and key, PEM, one after the other.
    identity: Option<Vec<u8>>,
    jar: Arc<Jar>,
    tokens: TokenCache,
}

impl Client {
    /// The client the config describes. A client certificate is read and
    /// checked here, so a bad one is reported at startup rather than on the
    /// first request that needs it.
    pub fn new(config: &Config) -> Result<Client> {
        let identity = match &config.client_identity {
            Some((cert, key)) => {
                let mut pem = std::fs::read(cert).map_err(|error| {
                    Error::Config(format!("reading CLIENT_CERT {}: {error}", cert.display()))
                })?;
                pem.push(b'\n');
                pem.extend(std::fs::read(key).map_err(|error| {
                    Error::Config(format!("reading CLIENT_KEY {}: {error}", key.display()))
                })?);
                install_crypto();
                reqwest::Identity::from_pem(&pem).map_err(|error| {
                    Error::Config(format!(
                        "CLIENT_CERT and CLIENT_KEY do not make a client identity: {}",
                        describe(&error)
                    ))
                })?;
                Some(pem)
            }
            None => None,
        };
        Ok(Client::with(config.timeout, identity))
    }

    pub fn with(timeout: Option<Duration>, identity: Option<Vec<u8>>) -> Client {
        install_crypto();
        Client {
            timeout,
            identity,
            jar: Arc::new(Jar::default()),
            tokens: TokenCache::default(),
        }
    }

    fn http(&self, phases: Option<&Arc<Mutex<Phases>>>) -> Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .cookie_provider(self.jar.clone());
        if let Some(timeout) = self.timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(pem) = &self.identity {
            let identity = reqwest::Identity::from_pem(pem)
                .map_err(|error| Error::Config(describe(&error)))?;
            builder = builder.identity(identity);
        }
        if let Some(phases) = phases {
            builder = builder
                .dns_resolver(Arc::new(TimedResolver {
                    phases: phases.clone(),
                }))
                .connector_layer(TimedLayer {
                    phases: phases.clone(),
                });
        }
        builder.build().map_err(|error| {
            Error::Config(format!("building the HTTP client: {}", describe(&error)))
        })
    }

    /// Sends a request. A server-sent event stream is handed to `on_event` a
    /// complete event at a time as it arrives, and collected into the body as
    /// well. `cancel` stops the send wherever it has got to.
    pub async fn send(
        &self,
        prepared: &Prepared,
        cancel: &CancellationToken,
        mut on_event: impl FnMut(String),
    ) -> Result<Exchange> {
        let phases = Arc::new(Mutex::new(Phases::default()));
        let http = self.http(Some(&phases))?;
        let request = build(&http, prepared)?;
        let started = Instant::now();

        let exchange = async {
            let mut response = http
                .execute(request)
                .await
                .map_err(|error| self.failure(&error))?;
            let first_byte = started.elapsed();

            let status = response.status();
            let headers: Vec<(String, String)> = response
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_string(),
                        String::from_utf8_lossy(value.as_bytes()).into_owned(),
                    )
                })
                .collect();
            let set_cookies: Vec<SetCookie> = response.cookies().map(set_cookie).collect();
            let version = format!("{:?}", response.version());
            let url = response.url().to_string();
            let streamed = header(&headers, "content-type").is_some_and(|value| {
                value
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("text/event-stream")
            });

            let mut body = Vec::new();
            if streamed {
                let mut pending: Vec<u8> = Vec::new();
                while let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|error| self.failure(&error))?
                {
                    body.extend_from_slice(&chunk);
                    pending.extend_from_slice(&chunk);
                    while let Some(end) = event_end(&pending) {
                        let event: Vec<u8> = pending.drain(..end).collect();
                        on_event(String::from_utf8_lossy(&event).into_owned());
                    }
                }
                if !pending.is_empty() {
                    on_event(String::from_utf8_lossy(&pending).into_owned());
                }
            } else {
                body = response
                    .bytes()
                    .await
                    .map_err(|error| self.failure(&error))?
                    .to_vec();
            }

            let total = started.elapsed();
            let trace = phases
                .lock()
                .expect("the trace lock")
                .trace(first_byte, total);
            Ok(Exchange {
                status: status.as_u16(),
                reason: status.canonical_reason().unwrap_or("").to_string(),
                version,
                jar: self.cookies_for(&url),
                url,
                headers,
                body,
                set_cookies,
                trace,
                streamed,
            })
        };

        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(Error::Cancelled),
            result = exchange => result,
        }
    }

    /// A token for the client credentials grant, from the cache when it still
    /// has a good one.
    pub async fn client_credentials(
        &self,
        grant: &Grant,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let http = self.http(None)?;
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(Error::Cancelled),
            result = self.tokens.fetch(&http, grant) => result,
        }
    }

    /// The cookies the jar will send to `url`.
    pub fn cookies_for(&self, url: &str) -> Vec<(String, String)> {
        let Ok(url) = reqwest::Url::parse(url) else {
            return Vec::new();
        };
        let Some(value) = self.jar.cookies(&url) else {
            return Vec::new();
        };
        value
            .to_str()
            .unwrap_or_default()
            .split("; ")
            .filter_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                Some((name.to_string(), value.to_string()))
            })
            .collect()
    }

    fn failure(&self, error: &reqwest::Error) -> Error {
        if error.is_timeout()
            && let Some(timeout) = self.timeout
        {
            return Error::Http(format!(
                "no response within {} — TIMEOUT in the config sets the limit",
                human(timeout)
            ));
        }
        Error::Http(describe(error))
    }
}

fn build(http: &reqwest::Client, prepared: &Prepared) -> Result<reqwest::Request> {
    let method = reqwest::Method::from_bytes(prepared.method.as_bytes())
        .map_err(|_| Error::Invalid(format!("{} is not an HTTP method", prepared.method)))?;
    let url = reqwest::Url::parse(&prepared.url)
        .map_err(|error| Error::Invalid(format!("{} is not a URL: {error}", prepared.url)))?;
    let mut builder = http.request(method, url);
    for (name, value) in &prepared.headers {
        let header = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::Invalid(format!("{name:?} is not a header name")))?;
        let value = HeaderValue::from_str(value).map_err(|_| {
            Error::Invalid(format!(
                "the {name} header holds a character a header cannot carry"
            ))
        })?;
        builder = builder.header(header, value);
    }
    if !prepared.body.is_empty() {
        builder = builder.body(prepared.body.clone());
    }
    builder
        .build()
        .map_err(|error| Error::Invalid(describe(&error)))
}

/// Where the first complete event in `bytes` ends: after the blank line that
/// closes it.
fn event_end(bytes: &[u8]) -> Option<usize> {
    let lf = bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|at| at + 2);
    let crlf = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| at + 4);
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn set_cookie(cookie: reqwest::cookie::Cookie<'_>) -> SetCookie {
    let mut attributes = Vec::new();
    if let Some(path) = cookie.path() {
        attributes.push(format!("Path={path}"));
    }
    if let Some(domain) = cookie.domain() {
        attributes.push(format!("Domain={domain}"));
    }
    if let Some(expires) = cookie.expires() {
        let expires: chrono::DateTime<chrono::Utc> = expires.into();
        attributes.push(format!(
            "Expires={}",
            expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        ));
    }
    if let Some(max_age) = cookie.max_age() {
        attributes.push(format!("Max-Age={}", max_age.as_secs()));
    }
    if cookie.http_only() {
        attributes.push("HttpOnly".into());
    }
    if cookie.secure() {
        attributes.push("Secure".into());
    }
    if cookie.same_site_strict() {
        attributes.push("SameSite=Strict".into());
    } else if cookie.same_site_lax() {
        attributes.push("SameSite=Lax".into());
    }
    SetCookie {
        name: cookie.name().to_string(),
        value: cookie.value().to_string(),
        attributes,
    }
}

/// An error and everything underneath it. The HTTP library's own message is
/// "error sending request"; the reason — refused, reset, a certificate — is
/// three sources down.
pub(crate) fn describe(error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(inner) = source {
        let text = inner.to_string();
        if !parts.iter().any(|part| part.contains(&text)) {
            parts.push(text);
        }
        source = inner.source();
    }
    parts.join(": ")
}

fn human(duration: Duration) -> String {
    let millis = duration.as_millis();
    if !millis.is_multiple_of(1000) {
        return format!("{millis}ms");
    }
    let seconds = duration.as_secs();
    if seconds >= 60 && seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

/// reqwest needs a crypto provider chosen before its first client is built;
/// this one is ring, as binsql uses. Another part of the process may have
/// chosen first, which is fine — reqwest takes whichever is installed.
fn install_crypto() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[derive(Debug, Default)]
struct Phases {
    dns: Option<Duration>,
    /// The whole connector, which resolves before it connects.
    connector: Option<Duration>,
}

impl Phases {
    fn trace(&self, first_byte: Duration, total: Duration) -> Trace {
        Trace {
            dns: self.dns,
            connect: self
                .connector
                .map(|connector| connector.saturating_sub(self.dns.unwrap_or_default())),
            first_byte,
            total,
        }
    }
}

fn add(slot: &mut Option<Duration>, elapsed: Duration) {
    *slot = Some(slot.unwrap_or_default() + elapsed);
}

/// Resolves the way the system does, and says how long it took.
struct TimedResolver {
    phases: Arc<Mutex<Phases>>,
}

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let phases = self.phases.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            let started = Instant::now();
            let addresses: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            add(
                &mut phases.lock().expect("the trace lock").dns,
                started.elapsed(),
            );
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

/// Wraps the connector, which resolves, connects and handshakes, and times it.
#[derive(Clone)]
struct TimedLayer {
    phases: Arc<Mutex<Phases>>,
}

impl<S> Layer<S> for TimedLayer {
    type Service = Timed<S>;

    fn layer(&self, inner: S) -> Timed<S> {
        Timed {
            inner,
            phases: self.phases.clone(),
        }
    }
}

#[derive(Clone)]
struct Timed<S> {
    inner: S,
    phases: Arc<Mutex<Phases>>,
}

impl<S, R> Service<R> for Timed<S>
where
    S: Service<R>,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = std::result::Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::result::Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let started = Instant::now();
        let future = self.inner.call(request);
        let phases = self.phases.clone();
        Box::pin(async move {
            let result = future.await;
            add(
                &mut phases.lock().expect("the trace lock").connector,
                started.elapsed(),
            );
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_ends_at_its_blank_line() {
        assert_eq!(event_end(b"data: one\n\ndata: two"), Some(11));
        assert_eq!(event_end(b"data: one\r\n\r\n"), Some(13));
        assert_eq!(event_end(b"data: partial"), None);
    }

    #[test]
    fn timeouts_read_as_written() {
        assert_eq!(human(Duration::from_secs(30)), "30s");
        assert_eq!(human(Duration::from_secs(120)), "2m");
        assert_eq!(human(Duration::from_millis(1500)), "1500ms");
    }
}
