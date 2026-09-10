//! Plain `.http` files: the request line, headers until a blank line, then the
//! body.
//!
//! ```text
//! POST https://api.example.com/users
//! Content-Type: application/json
//!
//! { "name": "Jane" }
//! ```
//!
//! Also read the way VS Code and JetBrains write them: comments before the
//! request line, a trailing `HTTP/1.1`, a bare URL meaning GET, and `###`
//! ending the request. Only the first request in a file is read.

use crate::request::{Request, is_method};

pub fn parse(text: &str) -> Request {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut request = Request::default();

    let mut index = 0;
    while index < lines.len() && is_skipped(lines[index]) {
        index += 1;
    }
    let Some(line) = lines.get(index) else {
        return request;
    };
    let (method, url) = request_line(line.trim());
    request.method = method;
    request.url = url;
    index += 1;

    while index < lines.len() {
        let line = lines[index];
        index += 1;
        if line.trim().is_empty() {
            break;
        }
        if is_comment(line) {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            if !name.is_empty() {
                request.headers.push((name.to_string(), value.trim().to_string()));
            }
        }
    }

    request.body = lines[index.min(lines.len())..]
        .iter()
        .take_while(|line| !line.trim_start().starts_with("###"))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    request
}

fn is_comment(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with('#') || line.starts_with("//")
}

fn is_skipped(line: &str) -> bool {
    line.trim().is_empty() || is_comment(line)
}

fn request_line(line: &str) -> (String, String) {
    let (first, rest) = match line.split_once(char::is_whitespace) {
        Some((first, rest)) => (first, rest.trim()),
        None => (line, ""),
    };
    if !is_method(first) && first.contains("://") {
        return ("GET".into(), strip_version(line).to_string());
    }
    (first.to_ascii_uppercase(), strip_version(rest).to_string())
}

/// `https://x HTTP/1.1` is the URL `https://x`.
fn strip_version(url: &str) -> &str {
    match url.rsplit_once(' ') {
        Some((head, tail)) if tail.starts_with("HTTP/") => head.trim_end(),
        _ => url,
    }
}

pub fn format(request: &Request) -> String {
    let method = if request.method.is_empty() {
        "GET"
    } else {
        &request.method
    };
    let mut out = format!("{method} {}\n", request.url);
    for (name, value) in &request.headers {
        out.push_str(&format!("{name}: {value}\n"));
    }
    if !request.body.is_empty() {
        out.push('\n');
        out.push_str(&request.body);
        if !request.body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_request_line_headers_and_body() {
        let request = parse(
            "POST https://example.com/api\n\
             Content-Type: application/json\n\
             X-Custom: hello\n\
             \n\
             {\"hi\": 1}\n",
        );
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://example.com/api");
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.header("X-Custom"), Some("hello"));
        assert_eq!(request.body, "{\"hi\": 1}");
    }

    #[test]
    fn uppercases_the_method() {
        assert_eq!(parse("get https://x").method, "GET");
    }

    #[test]
    fn an_empty_file_is_an_empty_request() {
        let request = parse("");
        assert_eq!((request.method.as_str(), request.url.as_str()), ("", ""));
    }

    #[test]
    fn reads_files_written_for_other_clients() {
        let request = parse(
            "### list users\n\
             # @name list\n\
             GET https://api.example.com/users HTTP/1.1\n\
             Accept: application/json\n\
             \n\
             ###\n\
             POST https://api.example.com/other\n",
        );
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://api.example.com/users");
        assert_eq!(request.body, "", "the next request is not this one's body");

        let bare = parse("https://api.example.com/health\n");
        assert_eq!((bare.method.as_str(), bare.url.as_str()), ("GET", "https://api.example.com/health"));
    }

    #[test]
    fn round_trips_through_format() {
        let original = Request {
            method: "PUT".into(),
            url: "https://api.example.com/x".into(),
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Accept".into(), "*/*".into()),
            ],
            body: "{\"k\":1}".into(),
            ..Request::default()
        };
        let again = parse(&format(&original));
        assert_eq!(again.method, original.method);
        assert_eq!(again.url, original.url);
        assert_eq!(again.headers, original.headers, "order is kept");
        assert_eq!(again.body, original.body);
    }
}
