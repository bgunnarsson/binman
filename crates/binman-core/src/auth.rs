//! The Auth tab: what each kind asks for, and the header it becomes.
//!
//! v1 also listed AWS Sig v4, Digest, NTLM, WSSE and a bare "OAuth 2.0". None
//! of them was implemented: each sent its field labels as literal headers — a
//! header called `Username` — which is worse than offering nothing. They are
//! not carried across.

use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthKind {
    None,
    Bearer,
    Basic,
    ApiKey,
    ClientCredentials,
}

impl AuthKind {
    pub const ALL: [AuthKind; 5] = [
        AuthKind::None,
        AuthKind::Bearer,
        AuthKind::Basic,
        AuthKind::ApiKey,
        AuthKind::ClientCredentials,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AuthKind::None => "No auth",
            AuthKind::Bearer => "Bearer token",
            AuthKind::Basic => "Basic auth",
            AuthKind::ApiKey => "API key",
            AuthKind::ClientCredentials => "OAuth2 client credentials",
        }
    }

    pub fn fields(self) -> &'static [&'static str] {
        match self {
            AuthKind::None => &[],
            AuthKind::Bearer => &["Token"],
            AuthKind::Basic => &["Username", "Password"],
            AuthKind::ApiKey => &["Header", "Value"],
            AuthKind::ClientCredentials => &["Token URL", "Client ID", "Client Secret", "Scope"],
        }
    }

    pub fn cycle(self, delta: isize) -> AuthKind {
        let position = Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0) as isize;
        let next = (position + delta).rem_euclid(Self::ALL.len() as isize);
        Self::ALL[next as usize]
    }
}

/// Whether a field holds something that should not be read over a shoulder.
pub fn is_secret(field: &str) -> bool {
    let field = field.to_ascii_lowercase();
    field.contains("secret") || field.contains("password")
}

/// The header an auth kind adds, from its field values in [`AuthKind::fields`]
/// order. Client credentials has to fetch a token before it has a header to
/// give, so it answers `None` here and is handled by the sender.
pub fn header(kind: AuthKind, values: &[String]) -> Option<(String, String)> {
    let value = |index: usize| values.get(index).map(String::as_str).unwrap_or("");
    match kind {
        AuthKind::Bearer if !value(0).is_empty() => {
            Some(("Authorization".into(), format!("Bearer {}", value(0))))
        }
        AuthKind::Basic if !(value(0).is_empty() && value(1).is_empty()) => {
            let encoded = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", value(0), value(1)));
            Some(("Authorization".into(), format!("Basic {encoded}")))
        }
        AuthKind::ApiKey if !value(0).is_empty() => {
            Some((value(0).to_string(), value(1).to_string()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(list: &[&str]) -> Vec<String> {
        list.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn each_kind_becomes_its_header() {
        assert_eq!(
            header(AuthKind::Bearer, &values(&["abc"])),
            Some(("Authorization".into(), "Bearer abc".into()))
        );
        assert_eq!(
            header(AuthKind::Basic, &values(&["alice", "secret"])),
            Some(("Authorization".into(), "Basic YWxpY2U6c2VjcmV0".into()))
        );
        assert_eq!(
            header(AuthKind::ApiKey, &values(&["X-Api-Key", "k"])),
            Some(("X-Api-Key".into(), "k".into()))
        );
    }

    #[test]
    fn empty_fields_add_nothing() {
        assert_eq!(header(AuthKind::Bearer, &values(&[""])), None);
        assert_eq!(header(AuthKind::Basic, &values(&["", ""])), None);
        assert_eq!(header(AuthKind::ClientCredentials, &values(&["u", "i", "s", ""])), None);
    }
}
