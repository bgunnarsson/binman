//! The OAuth 2.0 client credentials grant: just enough to fetch a token before
//! sending the request it authorises.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::Value;

use crate::client::describe;
use crate::error::{Error, Result};

/// A token this close to expiring is fetched again, so a request never leaves
/// authorised and arrives expired.
const SKEW: Duration = Duration::from_secs(30);

/// For a response that does not say how long its token lasts. Without one, an
/// absent `expires_in` would read as zero and every request would fetch anew.
const DEFAULT_LIFETIME: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Grant {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
}

type Key = (String, String, String);

/// Tokens by endpoint, client and scope, reused until shortly before they
/// expire. A failed fetch is never cached.
#[derive(Default)]
pub struct TokenCache {
    tokens: Mutex<HashMap<Key, (String, Instant)>>,
}

impl TokenCache {
    pub async fn fetch(&self, http: &reqwest::Client, grant: &Grant) -> Result<String> {
        if grant.token_url.trim().is_empty() || grant.client_id.trim().is_empty() {
            return Err(Error::OAuth(
                "a token URL and a client ID are both needed".into(),
            ));
        }
        let key = (
            grant.token_url.clone(),
            grant.client_id.clone(),
            grant.scope.clone(),
        );
        if let Some(token) = self.cached(&key) {
            return Ok(token);
        }

        let form = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("grant_type", "client_credentials")
                .append_pair("client_id", &grant.client_id)
                .append_pair("client_secret", &grant.client_secret);
            // An empty scope is not the same as none: some providers refuse
            // it, others issue a token that can do nothing.
            if !grant.scope.is_empty() {
                form.append_pair("scope", &grant.scope);
            }
            form.finish()
        };

        let response = http
            .post(&grant.token_url)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(ACCEPT, "application/json")
            .body(form)
            .send()
            .await
            .map_err(|error| {
                Error::OAuth(format!("reaching the token endpoint: {}", describe(&error)))
            })?;
        let status = response.status();
        let text = response.text().await.map_err(|error| {
            Error::OAuth(format!("reading the token response: {}", describe(&error)))
        })?;
        if !status.is_success() {
            // The body belongs in the message: invalid_client and
            // invalid_scope arrive with the same status and want different
            // fixes.
            return Err(Error::OAuth(format!(
                "the token endpoint answered {status}: {}",
                text.trim()
            )));
        }

        let parsed: TokenResponse = serde_json::from_str(&text).map_err(|error| {
            Error::OAuth(format!("could not decode the token response: {error}"))
        })?;
        let token = parsed
            .access_token
            .filter(|token| !token.is_empty())
            .ok_or_else(|| Error::OAuth("the token response had no access_token".into()))?;
        let lifetime = parsed
            .expires_in
            .as_ref()
            .and_then(seconds)
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_LIFETIME);

        self.tokens
            .lock()
            .expect("the token cache lock")
            .insert(key, (token.clone(), Instant::now() + lifetime));
        Ok(token)
    }

    fn cached(&self, key: &Key) -> Option<String> {
        let tokens = self.tokens.lock().expect("the token cache lock");
        let (token, expires) = tokens.get(key)?;
        (Instant::now() + SKEW < *expires).then(|| token.clone())
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    expires_in: Option<Value>,
}

/// `expires_in` as a number, or as the string some providers send instead.
fn seconds(value: &Value) -> Option<u64> {
    let seconds = match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    };
    seconds.filter(|seconds| *seconds > 0)
}
