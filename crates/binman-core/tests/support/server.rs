//! A throwaway HTTP/1.1 server for tests: one listener, a closure that decides
//! each reply, and a record of what it was sent.
//!
//! Shared with the front end's tests through `#[path]`, so there is one copy.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body read as a form.
    pub fn form(&self) -> Vec<(String, String)> {
        binman_core::body::parse_form(&self.text())
    }
}

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Written after the headers, each after its pause, with no length: the
    /// connection closing ends the body. How a stream arrives.
    pub chunks: Vec<(Duration, Vec<u8>)>,
    /// Before anything is written, for a server that is slow to answer.
    pub delay: Duration,
}

impl Reply {
    pub fn new(status: u16) -> Reply {
        Reply {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            chunks: Vec::new(),
            delay: Duration::ZERO,
        }
    }

    pub fn json(status: u16, body: &str) -> Reply {
        Reply::new(status)
            .header("Content-Type", "application/json")
            .body(body)
    }

    pub fn header(mut self, name: &str, value: &str) -> Reply {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn body(mut self, body: &str) -> Reply {
        self.body = body.as_bytes().to_vec();
        self
    }

    pub fn delayed(mut self, delay: Duration) -> Reply {
        self.delay = delay;
        self
    }

    pub fn chunk(mut self, pause: Duration, text: &str) -> Reply {
        self.chunks.push((pause, text.as_bytes().to_vec()));
        self
    }
}

pub struct Server {
    pub url: String,
    requests: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    pub fn hits(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn last(&self) -> Option<Seen> {
        self.requests.lock().unwrap().last().cloned()
    }

    pub fn all(&self) -> Vec<Seen> {
        self.requests.lock().unwrap().clone()
    }
}

/// Starts a server on a free port. `handler` gets each request and how many
/// have arrived so far, counting this one.
pub async fn serve(handler: impl Fn(&Seen, usize) -> Reply + Send + Sync + 'static) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind a port");
    let url = format!("http://{}", listener.local_addr().expect("an address"));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(handler);

    let store = requests.clone();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let handler = handler.clone();
            let store = store.clone();
            tokio::spawn(async move {
                answer(socket, handler.as_ref(), &store).await;
            });
        }
    });

    Server { url, requests }
}

async fn answer(
    mut socket: TcpStream,
    handler: &(dyn Fn(&Seen, usize) -> Reply + Send + Sync),
    store: &Mutex<Vec<Seen>>,
) {
    let Some(seen) = read_request(&mut socket).await else {
        return;
    };
    let count = {
        let mut store = store.lock().unwrap();
        store.push(seen.clone());
        store.len()
    };
    let reply = handler(&seen, count);
    if !reply.delay.is_zero() {
        tokio::time::sleep(reply.delay).await;
    }

    let mut head = format!("HTTP/1.1 {} {}\r\n", reply.status, reason(reply.status));
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if reply.chunks.is_empty() {
        head.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
    }
    head.push_str("Connection: close\r\n\r\n");

    if socket.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    let _ = socket.write_all(&reply.body).await;
    for (pause, chunk) in &reply.chunks {
        tokio::time::sleep(*pause).await;
        if socket.write_all(chunk).await.is_err() {
            return;
        }
        let _ = socket.flush().await;
    }
    let _ = socket.shutdown().await;
}

async fn read_request(socket: &mut TcpStream) -> Option<Seen> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
    };

    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split(' ');
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect();

    let length: usize = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let mut body = buffer[head_end..].to_vec();
    while body.len() < length {
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }

    Some(Seen {
        method,
        path,
        headers,
        body,
    })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    }
}
