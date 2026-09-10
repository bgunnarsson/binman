//! The query string of a URL, as the pairs the Params tab edits.
//!
//! Kept as typed rather than decoded: a value written `{{id}}` or `a%20b` comes
//! back exactly as it was, so editing one parameter never re-encodes the
//! others. v1 re-encoded the whole string on every edit, and a
//! `{{placeholder}}` in it became `%7B%7B…%7D%7D` and stopped resolving.

pub fn pairs(url: &str) -> Vec<(String, String)> {
    let (_, query, _) = split(url);
    let Some(query) = query else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (key.to_string(), value.to_string()),
            None => (pair.to_string(), String::new()),
        })
        .collect()
}

/// `url` with its query string replaced by `pairs`, fragment kept.
pub fn with_pairs(url: &str, pairs: &[(String, String)]) -> String {
    let (base, _, fragment) = split(url);
    let query: Vec<String> = pairs
        .iter()
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| format!("{}={}", escape(key, true), escape(value, false)))
        .collect();

    let mut out = base.to_string();
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query.join("&"));
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// Encodes only what would change the URL's structure — a `&` typed into a
/// value would otherwise split it into two parameters.
fn escape(text: &str, key: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            ' ' => out.push_str("%20"),
            '=' if key => out.push_str("%3D"),
            other => out.push(other),
        }
    }
    out
}

fn split(url: &str) -> (&str, Option<&str>, Option<&str>) {
    let (rest, fragment) = match url.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (url, None),
    };
    match rest.split_once('?') {
        Some((base, query)) => (base, Some(query), fragment),
        None => (rest, None, fragment),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    #[test]
    fn reads_the_query_as_typed() {
        assert_eq!(
            pairs("{{BASE}}/users?id={{ID}}&q=a%20b&flag#top"),
            vec![pair("id", "{{ID}}"), pair("q", "a%20b"), pair("flag", "")]
        );
        assert!(pairs("https://x/y").is_empty());
    }

    #[test]
    fn writing_pairs_keeps_placeholders_and_the_fragment() {
        let url = with_pairs(
            "{{BASE}}/users?old=1#top",
            &[pair("id", "{{ID}}"), pair("q", "a&b")],
        );
        assert_eq!(url, "{{BASE}}/users?id={{ID}}&q=a%26b#top");
    }

    #[test]
    fn removing_the_last_pair_drops_the_question_mark() {
        assert_eq!(with_pairs("https://x/y?a=1", &[]), "https://x/y");
    }
}
