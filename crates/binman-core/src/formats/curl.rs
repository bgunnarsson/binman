//! curl command lines, in and out.
//!
//! Import covers what people actually paste, including what a browser's "Copy
//! as cURL" writes: `-X`, `-H`, the `-d`/`--data*` family, `--json`, `-u`,
//! `-F`, `-A`, `-e`, `-b`, `--url`, and `$'…'` quoting. Everything else curl
//! understands — `--cert`, `--resolve`, `--proxy` — is passed over: the aim is
//! the request, not curl's whole behaviour.
//!
//! As curl does, a body built from `-d` fields is sent as
//! `application/x-www-form-urlencoded` unless a Content-Type says otherwise.

use base64::Engine;

use crate::error::{Error, Result};
use crate::request::{Request, header, set_header};

/// Whether `text` is a curl invocation rather than a URL.
pub fn is_curl(text: &str) -> bool {
    let text = text.trim();
    text == "curl"
        || text.starts_with("curl ")
        || text.starts_with("curl\t")
        || text.starts_with("curl\n")
        || text.starts_with("$ curl ")
}

/// Flags that take a value.
const VALUED: &[&str] = &[
    "-X",
    "--request",
    "-H",
    "--header",
    "-d",
    "--data",
    "--data-raw",
    "--data-binary",
    "--data-ascii",
    "--data-urlencode",
    "--json",
    "-u",
    "--user",
    "--url",
    "-F",
    "--form",
    "--form-string",
    "-A",
    "--user-agent",
    "-e",
    "--referer",
    "-b",
    "--cookie",
];

/// Flags that take none, so the token after one is never mistaken for its
/// value.
const SWITCHES: &[&str] = &[
    "-s",
    "--silent",
    "-S",
    "--show-error",
    "-k",
    "--insecure",
    "-L",
    "--location",
    "-i",
    "--include",
    "-v",
    "--verbose",
    "-f",
    "--fail",
    "-N",
    "--no-buffer",
    "--compressed",
    "-#",
    "--progress-bar",
    "--http1.1",
    "--http2",
    "-g",
    "--globoff",
];

/// Short flags whose value may be written straight after them: `-XPOST`.
const ATTACHABLE: [&str; 5] = ["-X", "-H", "-d", "-u", "-F"];

pub fn parse(text: &str) -> Result<Request> {
    let tokens = tokenize(text)?;
    let mut rest = tokens.as_slice();
    if rest.first().is_some_and(|token| token == "$") {
        rest = &rest[1..];
    }
    match rest.first() {
        Some(first) if first == "curl" => rest = &rest[1..],
        _ => return Err(Error::Invalid("not a curl command".into())),
    }

    let mut request = Request::default();
    let mut method = None;
    let mut data: Vec<String> = Vec::new();
    let mut form_encoded = false;
    let mut multipart = false;

    let mut index = 0;
    while index < rest.len() {
        let token = rest[index].as_str();
        index += 1;

        let (flag, attached) = match ATTACHABLE
            .iter()
            .find(|flag| token.len() > flag.len() && token.starts_with(**flag))
        {
            Some(flag) => (*flag, Some(token[flag.len()..].to_string())),
            None => (token, None),
        };

        if VALUED.contains(&flag) {
            let value = match attached {
                Some(value) => value,
                None => match rest.get(index) {
                    Some(value) => {
                        index += 1;
                        value.clone()
                    }
                    None => break,
                },
            };
            match flag {
                "-X" | "--request" => method = Some(value.to_ascii_uppercase()),
                "-H" | "--header" => {
                    if let Some((name, value)) = value.split_once(':')
                        && !name.trim().is_empty()
                    {
                        request
                            .headers
                            .push((name.trim().to_string(), value.trim().to_string()));
                    }
                }
                "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                    data.push(value);
                    form_encoded = true;
                }
                "--data-urlencode" => {
                    data.push(match value.split_once('=') {
                        Some((name, content)) => format!("{name}={}", query_escape(content)),
                        None => query_escape(&value),
                    });
                    form_encoded = true;
                }
                "--json" => {
                    data.push(value);
                    if header(&request.headers, "Content-Type").is_none() {
                        request
                            .headers
                            .push(("Content-Type".into(), "application/json".into()));
                    }
                    if header(&request.headers, "Accept").is_none() {
                        request
                            .headers
                            .push(("Accept".into(), "application/json".into()));
                    }
                }
                "-u" | "--user" => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(value);
                    set_header(
                        &mut request.headers,
                        "Authorization",
                        format!("Basic {encoded}"),
                    );
                }
                "--url" => request.url = value,
                "-F" | "--form" | "--form-string" => {
                    data.push(value);
                    multipart = true;
                }
                "-A" | "--user-agent" => set_header(&mut request.headers, "User-Agent", value),
                "-e" | "--referer" => set_header(&mut request.headers, "Referer", value),
                // Without an `=` it names a cookie file, which is not a request.
                "-b" | "--cookie" if value.contains('=') => {
                    set_header(&mut request.headers, "Cookie", value)
                }
                _ => {}
            }
            continue;
        }

        if SWITCHES.contains(&flag) {
            continue;
        }
        if token.starts_with('-') && token.len() > 1 {
            // A flag this does not know. What follows is taken for its value
            // unless it looks like another flag or the URL.
            if rest
                .get(index)
                .is_some_and(|next| !next.starts_with('-') && !looks_like_url(next))
            {
                index += 1;
            }
            continue;
        }
        if request.url.is_empty() {
            request.url = token.to_string();
        }
    }

    request.body = data.join("&");
    request.method =
        method.unwrap_or_else(|| if data.is_empty() { "GET" } else { "POST" }.to_string());
    if header(&request.headers, "Content-Type").is_none() {
        if multipart {
            request
                .headers
                .push(("Content-Type".into(), "multipart/form-data".into()));
        } else if form_encoded {
            request.headers.push((
                "Content-Type".into(),
                "application/x-www-form-urlencoded".into(),
            ));
        }
    }
    Ok(request)
}

fn looks_like_url(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://") || text.starts_with("{{")
}

/// What Go's `url.QueryEscape` does, which is what v1 did: unreserved
/// characters as they are, a space as `+`, everything else `%XX`.
fn query_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The request as a curl one-liner. `fields` is a multipart body's fields,
/// which go out as `-F` so curl builds the body and its boundary itself.
pub fn format(request: &Request, fields: Option<&[(String, String)]>) -> String {
    let mut out = String::from("curl");
    let method = if request.method.is_empty() {
        "GET"
    } else {
        request.method.as_str()
    };
    if method != "GET" {
        out.push_str(" -X ");
        out.push_str(method);
    }
    for (name, value) in &request.headers {
        if fields.is_some() && name.eq_ignore_ascii_case("Content-Type") {
            continue;
        }
        out.push_str(" -H ");
        out.push_str(&shell_quote(&format!("{name}: {value}")));
    }
    match fields {
        Some(fields) => {
            for (name, value) in fields {
                out.push_str(" -F ");
                out.push_str(&shell_quote(&format!("{name}={value}")));
            }
        }
        None if !request.body.is_empty() => {
            out.push_str(" --data-raw ");
            out.push_str(&shell_quote(&request.body));
        }
        None => {}
    }
    if !request.url.is_empty() {
        out.push(' ');
        out.push_str(&shell_quote(&request.url));
    }
    out
}

/// Single-quotes anything a shell would read as more than text. v1 left `&`
/// bare, so a URL with two query parameters was cut in half by the shell.
fn shell_quote(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=@%+,".contains(ch));
    if plain {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Just enough of a POSIX shell to read a pasted command: single and double
/// quotes, `$'…'`, backslash escapes, and `\` at the end of a line.
fn tokenize(text: &str) -> Result<Vec<String>> {
    let unterminated = || Error::Invalid("the curl command has an unterminated quote".into());
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return Err(unterminated()),
                    }
                }
            }
            '"' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.peek() {
                            Some('"' | '\\' | '$' | '`') => {
                                current.push(chars.next().unwrap_or_default())
                            }
                            Some('\n') => {
                                chars.next();
                            }
                            _ => current.push('\\'),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err(unterminated()),
                    }
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_token = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => current.push('\n'),
                            Some('t') => current.push('\t'),
                            Some('r') => current.push('\r'),
                            Some(other) => current.push(other),
                            None => return Err(unterminated()),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err(unterminated()),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') => {}
                Some('\r') => {
                    chars.next_if_eq(&'\n');
                }
                Some(escaped) => {
                    current.push(escaped);
                    in_token = true;
                }
                None => {}
            },
            ch if ch.is_whitespace() => {
                if in_token {
                    out.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            ch => {
                current.push(ch);
                in_token = true;
            }
        }
    }
    if in_token {
        out.push(current);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_a_curl_command() {
        for (text, expected) in [
            ("curl https://x", true),
            ("  curl https://x", true),
            ("curl", true),
            ("$ curl https://x", true),
            ("wget https://x", false),
            ("https://example.com", false),
        ] {
            assert_eq!(is_curl(text), expected, "{text}");
        }
    }

    #[test]
    fn a_bare_url_is_a_get() {
        let request = parse("curl https://example.com/x").unwrap();
        assert_eq!(
            (request.method.as_str(), request.url.as_str()),
            ("GET", "https://example.com/x")
        );
    }

    #[test]
    fn reads_method_headers_and_data() {
        let request = parse(
            r#"curl -X POST -H "Content-Type: application/json" -H 'Authorization: Bearer abc' --data-raw '{"a":1}' https://example.com/api"#,
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://example.com/api");
        assert_eq!(request.header("Content-Type"), Some("application/json"));
        assert_eq!(request.header("Authorization"), Some("Bearer abc"));
        assert_eq!(request.body, r#"{"a":1}"#);
    }

    #[test]
    fn data_implies_a_form_post() {
        let request = parse(r#"curl -d "foo=bar" https://x"#).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.header("Content-Type"),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[test]
    fn keeps_every_field_of_a_client_credentials_request() {
        let request = parse(
            "curl -s -X POST 'https://login.microsoftonline.com/tid/oauth2/v2.0/token' \
             -d 'grant_type=client_credentials' \
             -d 'client_id=abc' \
             -d 'scope=https://graph.microsoft.com/.default' \
             --data-urlencode 'client_secret=sec.ret~with/chars'",
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        for expected in [
            "grant_type=client_credentials",
            "client_id=abc",
            "scope=https://graph.microsoft.com/.default",
            "client_secret=sec.ret~with%2Fchars",
        ] {
            assert!(
                request.body.contains(expected),
                "{expected} missing from {}",
                request.body
            );
        }
    }

    #[test]
    fn user_becomes_basic_auth() {
        let request = parse("curl -u alice:secret https://x").unwrap();
        assert_eq!(
            request.header("Authorization"),
            Some("Basic YWxpY2U6c2VjcmV0")
        );
    }

    #[test]
    fn follows_line_continuations() {
        let request = parse("curl -X POST \\\n  -H 'A: 1' \\\n  https://x").unwrap();
        assert_eq!(request.url, "https://x");
        assert_eq!(request.header("A"), Some("1"));
    }

    #[test]
    fn reads_what_a_browser_copies() {
        let request = parse(
            "curl 'https://api.example.com/graphql' \
             -H 'accept: */*' \
             -b 'session=abc; theme=dark' \
             --data-raw $'{\"q\":\"it\\'s\"}' \
             --compressed",
        )
        .unwrap();
        assert_eq!(request.url, "https://api.example.com/graphql");
        assert_eq!(request.header("Cookie"), Some("session=abc; theme=dark"));
        assert_eq!(request.body, r#"{"q":"it's"}"#);
        assert_eq!(request.method, "POST");
    }

    #[test]
    fn reads_values_written_against_their_flag() {
        let request = parse("curl -XPUT -H'X-A: 1' https://x").unwrap();
        assert_eq!(request.method, "PUT");
        assert_eq!(request.header("X-A"), Some("1"));
    }

    #[test]
    fn an_unterminated_quote_is_an_error() {
        assert!(parse("curl 'https://x").is_err());
    }

    #[test]
    fn exports_what_the_shell_will_read_back() {
        let request = Request {
            method: "POST".into(),
            url: "https://x/search?a=1&b=2".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: r#"{"q":"it's"}"#.into(),
            ..Request::default()
        };
        let command = format(&request, None);
        assert_eq!(
            command,
            r#"curl -X POST -H 'Content-Type: application/json' --data-raw '{"q":"it'\''s"}' 'https://x/search?a=1&b=2'"#
        );
        let again = parse(&command).unwrap();
        assert_eq!(again.url, request.url);
        assert_eq!(again.body, request.body);
    }

    #[test]
    fn a_multipart_body_exports_as_fields() {
        let request = Request {
            method: "POST".into(),
            url: "https://x/upload".into(),
            headers: vec![(
                "Content-Type".into(),
                "multipart/form-data; boundary=b".into(),
            )],
            ..Request::default()
        };
        let fields = vec![("file".to_string(), "@/tmp/a.png".to_string())];
        assert_eq!(
            format(&request, Some(&fields)),
            "curl -X POST -F file=@/tmp/a.png https://x/upload"
        );
    }
}
